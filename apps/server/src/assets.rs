//! The embedded SPA and its fallback routing.
//!
//! The frontend bundle is compiled into the binary by `rust-embed` (see
//! `build.rs` for how the directory is populated), so a deployment is one file
//! with no web server in front of it.

use axum::{
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
pub async fn handler(uri: Uri) -> Response {
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
        not_found()
    } else {
        serve(INDEX)
    }
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
        assert!(
            etag[1..65]
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
}
