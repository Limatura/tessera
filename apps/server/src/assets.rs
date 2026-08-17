//! The embedded SPA and its fallback routing.
//!
//! The frontend bundle is compiled into the binary by `rust-embed` (see
//! `build.rs` for how the directory is populated), so a deployment is one file
//! with no web server in front of it.

use axum::{
    extract::State,
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use rust_embed::{Embed, EmbeddedFile};

/// Bundle produced by the web workspace. `build.rs` guarantees the directory
/// exists and contains at least a placeholder page.
#[derive(Embed)]
#[folder = "assets/dist"]
struct Bundle;

/// Entry point of the SPA.
const INDEX: &str = "index.html";

/// Hashed build output is safe to cache forever; `index.html` is the mutable
/// pointer at it and must not be.
const IMMUTABLE_CACHE: &str = "public, max-age=31536000, immutable";
const NO_CACHE: &str = "no-cache";

/// Turns a request path into a lookup key for the embedded bundle.
///
/// Returns `None` when the path could escape the bundle root. `rust-embed` only
/// serves what was compiled in, so traversal cannot reach the filesystem — but a
/// request that *tries* is a probe, and answering it with `index.html` would
/// quietly reward the attempt.
///
/// The result never starts with `/`, never contains a `.` or `..` segment, and is
/// idempotent under a second application.
#[must_use]
pub fn normalize_asset_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim_start_matches('/');
    let mut segments = Vec::new();

    for segment in trimmed.split('/') {
        match segment {
            // Collapse `//` and `/./`, which are noise rather than an attack.
            "" | "." => {}
            ".." => return None,
            other => segments.push(other),
        }
    }

    Some(segments.join("/"))
}

/// Serves an embedded asset, falling back to the SPA entry point.
///
/// A miss on a path that looks like a file (it has an extension) is a 404; a miss
/// on anything else is a client-side route, so `index.html` is returned with a
/// 200 and the router in the browser takes over.
pub async fn handler(State(state): State<crate::state::AppState>, uri: Uri) -> Response {
    let Some(path) = normalize_asset_path(uri.path()) else {
        return not_found();
    };

    if path.is_empty() {
        return serve(INDEX);
    }

    if let Some(file) = Bundle::get(&path) {
        return respond(&path, file);
    }

    // `foo.js` missing means a broken build, not a client-side route.
    let looks_like_a_file = std::path::Path::new(&path)
        .extension()
        .is_some_and(|ext| !ext.is_empty());
    if looks_like_a_file {
        return not_found();
    }

    // A share page is the one client-side route the server knows something
    // about, and the one whose link gets pasted where something will unfurl it.
    if let Some(code) = share_code(&path) {
        return serve_share_page(&state, code).await;
    }
    serve(INDEX)
}

/// The retrieval code in `/s/{code}`, if that is what this path is.
///
/// Only the bare share page: `/s/{code}/raw` and `/s/{code}/download` are real
/// routes and never reach the fallback.
///
/// The frontend puts the locale in the path, so the same page is also
/// `/en/s/{code}`. The locale list lives in `apps/web/project.inlang`, and
/// duplicating it here would be a second copy to keep in step — so any single
/// leading segment shaped like a language tag is skipped instead. Being loose
/// costs nothing: a preview for `/nonsense/s/{code}` tells its reader only what
/// `/s/{code}` would have told them anyway, and the lookup behind it is
/// throttled the same way.
fn share_code(path: &str) -> Option<&str> {
    let rest = strip_locale_prefix(path).strip_prefix("s/")?;
    (!rest.is_empty() && !rest.contains('/')).then_some(rest)
}

/// Drops a leading `{locale}/`, if the path starts with one.
fn strip_locale_prefix(path: &str) -> &str {
    let Some((head, tail)) = path.split_once('/') else {
        return path;
    };
    let looks_like_a_locale = (2..=8).contains(&head.len())
        && head
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if looks_like_a_locale { tail } else { path }
}

/// Serves the app with the share's link preview spliced into its `<head>`.
///
/// Every failure path here falls back to the plain page. A preview is worth
/// having; it is not worth a share page that does not load.
async fn serve_share_page(state: &crate::state::AppState, code: &str) -> Response {
    use crate::api::preview::{Preview, splice_into_document};

    let Some(file) = Bundle::get(INDEX) else {
        return not_found();
    };
    let Some((share, content)) = preview_target(state, code).await else {
        return respond(INDEX, file);
    };
    let Ok(html) = std::str::from_utf8(&file.data) else {
        return respond(INDEX, file);
    };
    let tags = Preview::of(&share, &content, time::OffsetDateTime::now_utc()).to_head_tags();
    let page = splice_into_document(html, &tags);

    (
        [
            (header::CONTENT_TYPE, "text/html".to_owned()),
            (header::CACHE_CONTROL, NO_CACHE.to_owned()),
        ],
        page,
    )
        .into_response()
}

async fn preview_target(
    state: &crate::state::AppState,
    code: &str,
) -> Option<(
    crate::db::repository::share::Share,
    crate::db::repository::share::Content,
)> {
    crate::api::preview::lookup(state, code).await
}

fn serve(path: &str) -> Response {
    Bundle::get(path).map_or_else(not_found, |file| respond(path, file))
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

fn respond(path: &str, file: EmbeddedFile) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache = if path == INDEX {
        NO_CACHE
    } else {
        IMMUTABLE_CACHE
    };
    let etag = quoted_etag(&file.metadata.sha256_hash());

    // `Cow::Borrowed` is the release-build case: the bytes live in `.rodata` and
    // the response body borrows them rather than copying per request.
    let body = match file.data {
        std::borrow::Cow::Borrowed(bytes) => Bytes::from_static(bytes),
        std::borrow::Cow::Owned(bytes) => Bytes::from(bytes),
    };

    (
        [
            (header::CONTENT_TYPE, mime.as_ref().to_owned()),
            (header::CACHE_CONTROL, cache.to_owned()),
            (header::ETAG, etag),
        ],
        body,
    )
        .into_response()
}

/// Renders a digest as a quoted, hex-encoded entity tag.
fn quoted_etag(hash: &[u8; 32]) -> String {
    use std::fmt::Write as _;

    // 32 bytes hex-encoded, plus the two quotes.
    let mut out = String::with_capacity(66);
    out.push('"');
    for byte in hash {
        // Writing into a String is infallible; the result is discarded rather
        // than unwrapped so this stays panic-free.
        let _ = write!(out, "{byte:02x}");
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rstest::rstest;

    use super::*;

    #[test]
    fn strips_the_leading_slash() {
        assert_eq!(
            normalize_asset_path("/assets/app.js").as_deref(),
            Some("assets/app.js")
        );
    }

    #[test]
    fn root_normalises_to_the_empty_key() {
        assert_eq!(normalize_asset_path("/").as_deref(), Some(""));
    }

    #[test]
    fn collapses_redundant_segments() {
        assert_eq!(normalize_asset_path("//a/./b//c").as_deref(), Some("a/b/c"));
    }

    #[test]
    fn rejects_traversal() {
        assert_eq!(normalize_asset_path("/../etc/passwd"), None);
        assert_eq!(normalize_asset_path("/assets/../../etc/passwd"), None);
    }

    proptest! {
        /// Whatever a client sends, the key we hand to the bundle is confined:
        /// relative, free of traversal segments, and stable under renormalisation.
        #[test]
        fn normalized_paths_are_confined_and_idempotent(raw in "[/a-zA-Z0-9._~-]{0,64}") {
            if let Some(normalized) = normalize_asset_path(&raw) {
                prop_assert!(!normalized.starts_with('/'));
                prop_assert!(!normalized.ends_with('/'));
                for segment in normalized.split('/') {
                    prop_assert_ne!(segment, "..");
                    prop_assert_ne!(segment, ".");
                    if !normalized.is_empty() {
                        prop_assert!(!segment.is_empty());
                    }
                }
                let renormalized = normalize_asset_path(&normalized);
                prop_assert_eq!(renormalized.as_deref(), Some(normalized.as_str()));
            }
        }
    }

    #[test]
    fn etag_is_quoted_lowercase_hex() {
        let etag = quoted_etag(&[0xab; 32]);
        assert_eq!(etag.len(), 66);
        assert!(etag.starts_with('"') && etag.ends_with('"'));
        let digits = etag.trim_matches('"');
        assert_eq!(digits.len(), 64);
        assert!(
            digits
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }

    #[test]
    fn the_bundle_always_has_an_entry_point() {
        assert!(
            Bundle::get(INDEX).is_some(),
            "build.rs must guarantee an index.html"
        );
    }

    /// A share link carries whatever locale the person who copied it was
    /// reading in, so the preview has to survive the prefix. This failing is
    /// invisible — the page still loads, the unfurl is just blank.
    #[rstest]
    #[case::unprefixed("s/ABC123", Some("ABC123"))]
    #[case::english("en/s/ABC123", Some("ABC123"))]
    #[case::japanese("ja/s/ABC123", Some("ABC123"))]
    #[case::region_tagged("zh-hant/s/ABC123", Some("ABC123"))]
    fn finds_the_share_code_whatever_the_locale(
        #[case] path: &str,
        #[case] expected: Option<&str>,
    ) {
        assert_eq!(share_code(path), expected);
    }

    #[rstest]
    #[case::not_a_share_page("about")]
    #[case::no_code("s/")]
    #[case::a_real_route("s/ABC123/raw")]
    #[case::a_real_route_localised("en/s/ABC123/raw")]
    #[case::two_prefixes("en/ja/s/ABC123")]
    fn is_not_a_share_page(#[case] path: &str) {
        assert_eq!(share_code(path), None);
    }

    /// `s` is one character, so it must never be mistaken for the locale and
    /// eaten — that would turn every share page into a plain page.
    #[test]
    fn the_share_segment_is_not_taken_for_a_locale() {
        assert_eq!(strip_locale_prefix("s/ABC123"), "s/ABC123");
    }
}
