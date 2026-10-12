//! The SPA and its fallback routing.
//!
//! The bundle is compiled into the binary by `rust-embed` (see `build.rs` for
//! how the directory is populated), so a deployment is one file with no web
//! server in front. The admin panel is part of the same SPA, a client-side
//! route under `/admin/`, so it gets the same fallback and spliced-in colours.
//!
//! `server.assets_dir` reads from a directory instead. A development
//! convenience, and the only mode with a filesystem to get wrong; see `load`
//! for how it is confined.

use std::path::PathBuf;

use axum::{
    extract::State,
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use rust_embed::Embed;
use sha2::{Digest as _, Sha256};

/// Bundle produced by the web workspace. `build.rs` guarantees the directory
/// exists and contains at least a placeholder page.
#[derive(Embed)]
#[folder = "assets/dist"]
struct Bundle;

/// Entry point of the SPA.
const INDEX: &str = "index.html";

/// Where the frontend is read from.
///
/// # Why the root is resolved once
///
/// Confinement is decided by comparing the resolved path of the request against
/// the resolved path of the root, and a root that resolved differently per
/// request would be comparing against a moving target. Resolving it at start-up
/// also means a directory that does not exist is a start-up failure, not a 404
/// on every page.
#[derive(Debug, Clone)]
pub enum Source {
    /// The bundle compiled into this binary.
    Embedded,
    /// A directory on disk, already resolved to a canonical path.
    Directory(PathBuf),
}

impl Source {
    /// Reads `server.assets_dir`, or reports why it cannot be served from.
    ///
    /// # Errors
    ///
    /// Returns a message naming what is wrong, for a caller that is about to
    /// refuse to start.
    pub fn resolve(configured: &str) -> Result<Self, String> {
        if configured.trim().is_empty() {
            return Ok(Self::Embedded);
        }
        let root = PathBuf::from(configured.trim())
            .canonicalize()
            .map_err(|error| format!("`{configured}` cannot be read: {error}"))?;
        if !root.is_dir() {
            return Err(format!("`{configured}` is not a directory"));
        }
        // Checked here, not on the first request. An operator who mistyped the
        // path should be told while they are still looking at the
        // terminal, not by a blank page an hour later.
        if !root.join(INDEX).is_file() {
            return Err(format!("`{configured}` has no {INDEX} in it"));
        }
        Ok(Self::Directory(root))
    }
}

/// One asset, however it was found.
struct Asset {
    bytes: Bytes,
    etag: String,
}

/// Reads an asset, or `None` if this source does not have it.
///
/// # Why the directory case resolves the path again
///
/// [`normalize_asset_path`] already removes `..`, and against the embedded
/// bundle that is enough, since there is no filesystem to escape to. A
/// directory has one, and a name that survived normalisation can still be a
/// symbolic link pointing anywhere: a `logo.png` pointing at `/etc/shadow` is a
/// path with no `..` in it.
///
/// So the resolved path is compared against the resolved root, and anything
/// that lands outside is treated as absent.
fn load(source: &Source, path: &str) -> Option<Asset> {
    match source {
        Source::Embedded => embedded::<Bundle>(path),
        Source::Directory(root) => {
            let resolved = root.join(path).canonicalize().ok()?;
            if !resolved.starts_with(root) {
                return None;
            }
            let bytes = std::fs::read(&resolved).ok()?;
            let etag = quoted_etag(&Sha256::digest(&bytes).into());
            Some(Asset {
                bytes: Bytes::from(bytes),
                etag,
            })
        }
    }
}

/// Reads an asset out of a compiled-in bundle.
fn embedded<B: Embed>(path: &str) -> Option<Asset> {
    B::get(path).map(|file| Asset {
        etag: quoted_etag(&file.metadata.sha256_hash()),
        // `Cow::Borrowed` in release builds: the bytes live in `.rodata` and
        // the response body borrows them instead of copying per request.
        bytes: match file.data {
            std::borrow::Cow::Borrowed(bytes) => Bytes::from_static(bytes),
            std::borrow::Cow::Owned(bytes) => Bytes::from(bytes),
        },
    })
}

/// Hashed build output is safe to cache forever; `index.html` is the mutable
/// pointer at it and must not be.
const IMMUTABLE_CACHE: &str = "public, max-age=31536000, immutable";
const NO_CACHE: &str = "no-cache";

/// Turns a request path into a lookup key for the embedded bundle.
///
/// Returns `None` when the path could escape the bundle root.
///
/// Against the compiled-in bundle this is not the security boundary, since
/// there is no filesystem behind it, but a request that *tries* is a probe and
/// answering it with `index.html` would reward the attempt. Against a directory
/// it is the first of two checks; the second is in `load`, because a symbolic
/// link is a path with no `..` in it.
///
/// The result never starts with `/`, never contains a `.` or `..` segment, and is
/// idempotent under a second application.
#[must_use]
pub fn normalize_asset_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim_start_matches('/');
    let mut segments = Vec::new();

    for segment in trimmed.split('/') {
        match segment {
            // Collapse `//` and `/./`, which are noise, not an attack.
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
pub async fn handler(
    State(state): State<crate::state::AppState>,
    headers: axum::http::HeaderMap,
    uri: Uri,
) -> Response {
    let Some(path) = normalize_asset_path(uri.path()) else {
        return not_found();
    };

    let source = &state.assets;

    if path.is_empty() {
        return serve_index(&state);
    }

    if let Some(asset) = load(source, &path) {
        // The entry point is the one asset rewritten on the way out, so it
        // cannot take the plain path even when requested by name.
        if path == INDEX {
            return serve_index(&state);
        }
        return respond(source, &path, asset);
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
        return serve_share_page(&state, &headers, code).await;
    }
    serve_index(&state)
}

/// Serves the entry point with the instance's appearance already applied.
///
/// # Why the server writes the colours in
///
/// Otherwise the page loads, paints, and then a script finds the operator's
/// choice and repaints. On a dark instance that is a white flash on every
/// navigation, the one thing everyone notices about a theme.
///
/// Only the instance default goes in. A signed-in user's own choice is applied
/// by the frontend from local storage; doing it here would mean resolving a
/// session on the static-asset path and a per-visitor `index.html` that no
/// cache could share.
fn serve_index(state: &crate::state::AppState) -> Response {
    let Some(asset) = load(&state.assets, INDEX) else {
        return not_found();
    };
    let Ok(html) = std::str::from_utf8(&asset.bytes) else {
        return respond(&state.assets, INDEX, asset);
    };
    let config = state.config();
    let page = named(&with_appearance(html, config.appearance), &config.site);
    html_page(page)
}

/// An HTML body assembled by this server, with an entity tag over what was
/// actually sent.
///
/// [`respond`] derives its tag from the bytes on disk, which stop matching as
/// soon as anything is spliced in. Keeping that tag would be worse than none:
/// nothing here answers conditional requests, but a proxy in front would, and
/// it would serve the colours from before the operator changed them. Hashing
/// the outgoing body costs one digest over a few kilobytes.
fn html_page(body: String) -> Response {
    let etag = quoted_etag(&Sha256::digest(body.as_bytes()).into());
    (
        [
            (header::CONTENT_TYPE, "text/html".to_owned()),
            (header::CACHE_CONTROL, NO_CACHE.to_owned()),
            (header::ETAG, etag),
        ],
        body,
    )
        .into_response()
}

/// Puts the instance's name in the tab and its description under its links.
///
/// Both come from `site.*`, so the operator's name shows from first paint, and
/// a link to the instance unfurls with it instead of whatever the bundle was
/// built with.
fn named(html: &str, site: &crate::config::SiteConfig) -> String {
    let page = retitled(html, &site.title);
    if site.description.trim().is_empty() {
        return page;
    }
    let tag = format!(
        "<meta name=\"description\" content=\"{}\">",
        crate::api::preview::escape(&site.description)
    );
    crate::api::preview::splice_into_document(&page, &tag)
}

/// Replaces what is between `<title>` and `</title>`.
///
/// A document without both is returned unchanged: a name in the tab is not
/// worth a page that fails to load. The value is escaped, since an operator
/// may put anything in it.
fn retitled(html: &str, title: &str) -> String {
    let lowered = html.to_ascii_lowercase();
    let Some(open) = lowered.find("<title").and_then(|start| {
        lowered
            .get(start..)
            .and_then(|rest| rest.find('>'))
            .map(|offset| start + offset + 1)
    }) else {
        return html.to_owned();
    };
    let Some(close) = lowered
        .get(open..)
        .and_then(|rest| rest.find("</title>"))
        .map(|offset| open + offset)
    else {
        return html.to_owned();
    };
    let mut out = String::with_capacity(html.len() + title.len());
    out.push_str(html.get(..open).unwrap_or_default());
    out.push_str(&crate::api::preview::escape(title));
    out.push_str(html.get(close..).unwrap_or_default());
    out
}

/// Writes the instance's appearance onto the `<html>` element.
///
/// `data-theme` and `data-accent` select CSS blocks in `packages/ui`.
/// `class="dark"` drives the stylesheet's dark variant, so it appears only when
/// the instance chose dark outright. `data-mode` carries the choice itself,
/// since "the operator picked light" and "the operator left it to the device"
/// mean different things to the frontend and a missing class cannot tell them
/// apart.
///
/// A document with no `<html>` is returned unchanged. The colours are not worth
/// a page that fails to load.
///
/// # Why concatenation is safe here
///
/// Every value comes from [`tessera_core::Mode`], [`tessera_core::Theme`] or
/// [`tessera_core::Accent`], whose `as_str` returns one of a fixed set of
/// lowercase words. There is nothing to escape, and an operator cannot inject
/// anything else: unknown values are refused when the setting is saved.
fn with_appearance(html: &str, appearance: crate::config::AppearanceConfig) -> String {
    use tessera_core::Mode;

    let Some(tag) = html_tag(html) else {
        return html.to_owned();
    };

    let mut attributes = format!(
        r#" data-mode="{}" data-theme="{}" data-accent="{}""#,
        appearance.mode.as_str(),
        appearance.theme.as_str(),
        appearance.accent.as_str(),
    );
    if matches!(appearance.mode, Mode::Dark) {
        attributes.push_str(r#" class="dark""#);
    }

    let mut out = String::with_capacity(html.len() + attributes.len());
    out.push_str(html.get(..tag.end).unwrap_or_default());
    out.push_str(&attributes);
    out.push_str(html.get(tag.end..).unwrap_or_default());
    out
}

/// Where the opening `<html …>` tag ends, in bytes.
///
/// `end` is the offset of its `>`, so an attribute inserted there lands inside
/// the tag. `None` when the document has no such tag. The placeholder page and
/// the real bundle have one; a build that did not would lose the colours,
/// not the page.
struct HtmlTag {
    end: usize,
}

fn html_tag(html: &str) -> Option<HtmlTag> {
    let lowered = html.to_ascii_lowercase();
    let open = lowered.find("<html")?;
    // `<htmlfoo>` is not the tag. Anything else that can follow the name is
    // whitespace before an attribute, or the closing `>`.
    let after = lowered.get(open + "<html".len()..)?;
    if !after.starts_with(|c: char| c.is_ascii_whitespace() || c == '>') {
        return None;
    }
    let end = open + "<html".len() + after.find('>')?;
    Some(HtmlTag { end })
}

/// The retrieval code in `/s/{code}`, if that is what this path is.
///
/// Only the bare share page: `/s/{code}/raw` and `/s/{code}/download` are real
/// routes and never reach the fallback.
///
/// The frontend puts the locale in the path, so the same page is also
/// `/en/s/{code}`. The locale list lives in `apps/web/project.inlang` and
/// duplicating it here would be a second copy to keep in step, so any single
/// leading segment shaped like a language tag is skipped instead. Being loose
/// costs nothing: a preview for `/nonsense/s/{code}` tells its reader what
/// `/s/{code}` would have anyway, and the lookup is throttled the same way.
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
async fn serve_share_page(
    state: &crate::state::AppState,
    headers: &axum::http::HeaderMap,
    code: &str,
) -> Response {
    use crate::api::preview::{Preview, splice_into_document};

    let Some(asset) = load(&state.assets, INDEX) else {
        return not_found();
    };
    let Some((share, content)) = preview_target(state, code).await else {
        return serve_index(state);
    };
    // A link with nothing to ask redirects straight away, via the download
    // route, so that route's checks and access log are the only ones. A
    // password needs the page to ask for it. A use limit needs a human click:
    // unfurlers follow a link as soon as it is pasted and would use up a
    // one-time link before anyone opened it.
    if share.kind == crate::db::repository::share::ShareKind::Link
        && !share.needs_password()
        && share.use_limit.is_none()
        && matches!(
            share.availability_at(time::OffsetDateTime::now_utc()),
            crate::db::repository::share::Availability::Available
        )
    {
        return (
            StatusCode::FOUND,
            [
                (header::LOCATION, format!("/s/{}/download", share.code)),
                (header::CACHE_CONTROL, "no-store".to_owned()),
            ],
        )
            .into_response();
    }
    let Ok(html) = std::str::from_utf8(&asset.bytes) else {
        return respond(&state.assets, INDEX, asset);
    };
    let config = state.config();
    // The share page is the entry point with more spliced in, so it gets the
    // instance's colours like every other page. It gets the instance's name
    // but not its description: the share's own is spliced in below.
    let html = retitled(
        &with_appearance(html, config.appearance),
        &config.site.title,
    );
    let origin = crate::api::preview::origin(&config, headers);
    let tags = Preview::of(
        &share,
        &content,
        time::OffsetDateTime::now_utc(),
        config.site.language,
    )
    .to_head_tags(origin.as_deref(), code, &config.site.title);
    html_page(splice_into_document(&html, &tags))
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

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

fn respond(source: &Source, path: &str, asset: Asset) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    // Nothing from a directory is immutable, whatever it is called. The bundle
    // compiled in cannot change without the binary changing, so a hashed name
    // in it never means different bytes; a directory exists to be rewritten
    // while the server runs. Telling a browser to keep `favicon.ico` for a year
    // would silently undo the one thing this mode is for, on exactly the files
    // vite does not hash.
    let cache = if path == INDEX || matches!(source, Source::Directory(_)) {
        NO_CACHE
    } else {
        IMMUTABLE_CACHE
    };

    (
        [
            (header::CONTENT_TYPE, mime.as_ref().to_owned()),
            (header::CACHE_CONTROL, cache.to_owned()),
            (header::ETAG, asset.etag),
        ],
        asset.bytes,
    )
        .into_response()
}

/// Serves one of the fonts the card renderer uses.
///
/// # Why the browser needs these
///
/// An editor previewing a card layout runs the same renderer, and without the
/// same fonts it wraps text differently and picks different glyphs. That hurts
/// most where the operator can least check by eye: a CJK filename falling back
/// to a font the server does not have.
///
/// # Caching behind a reverse proxy
///
/// This is three and a half megabytes of fonts, so the proxy should serve them
/// itself without asking again. Two headers do that.
///
/// `immutable` with a one-year max-age is accurate here: the bytes are compiled
/// into the binary, so they change only with a new binary, which is a new
/// deployment. The hashed bundle assets get the same treatment.
///
/// `font/woff2` matters because woff2 is already compressed. Without the type,
/// this server's compression layer and whatever sits in front would each spend
/// CPU making the bytes slightly larger.
///
/// The name is matched against a list, not turned into a path, so there is no
/// traversal to get wrong and no filesystem access.
#[cfg(feature = "og-images")]
pub async fn font(axum::extract::Path(name): axum::extract::Path<String>) -> Response {
    let Some((_, bytes)) = crate::og::FACES
        .iter()
        .find(|(face, _)| *face == name.as_str())
    else {
        return not_found();
    };

    (
        [
            (header::CONTENT_TYPE, "font/woff2".to_owned()),
            (header::CACHE_CONTROL, IMMUTABLE_CACHE.to_owned()),
            (header::ETAG, quoted_etag(&Sha256::digest(bytes).into())),
        ],
        Bytes::from_static(bytes),
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
    /// reading in, so the preview has to survive the prefix. Failing is
    /// invisible: the page still loads and the unfurl is simply blank.
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
    /// eaten, which would turn every share page into a plain page.
    #[test]
    fn the_share_segment_is_not_taken_for_a_locale() {
        assert_eq!(strip_locale_prefix("s/ABC123"), "s/ABC123");
    }

    /// The attributes land inside the opening tag, after any existing ones.
    /// Anywhere else gives a page that loads but has the wrong colours, which
    /// is the failure worth testing.
    #[test]
    fn the_appearance_lands_inside_the_html_tag() {
        let page = with_appearance(
            "<!doctype html>\n<html lang=\"zh-Hant\">\n  <head></head>\n</html>",
            crate::config::AppearanceConfig {
                mode: tessera_core::Mode::Dark,
                theme: tessera_core::Theme::Sepia,
                accent: tessera_core::Accent::Rose,
            },
        );
        let expected = concat!(
            r#"<html lang="zh-Hant" data-mode="dark" data-theme="sepia""#,
            r#" data-accent="rose" class="dark">"#,
        );
        assert!(page.contains(expected), "{page}");
    }

    /// `class="dark"` drives the stylesheet's dark variant, so it must appear
    /// for a dark instance and nothing else. `data-mode` carries the choice
    /// either way, since a missing class cannot say whether the operator picked
    /// light or left it to the device.
    #[rstest]
    #[case::dark(tessera_core::Mode::Dark, true)]
    #[case::light(tessera_core::Mode::Light, false)]
    #[case::system(tessera_core::Mode::System, false)]
    fn only_a_dark_instance_gets_the_class(
        #[case] mode: tessera_core::Mode,
        #[case] expected: bool,
    ) {
        let page = with_appearance(
            "<html>",
            crate::config::AppearanceConfig {
                mode,
                ..crate::config::AppearanceConfig::default()
            },
        );
        assert_eq!(page.contains("class=\"dark\""), expected, "{page}");
        assert!(
            page.contains(&format!("data-mode=\"{}\"", mode.as_str())),
            "{page}"
        );
    }

    /// Losing the colours is acceptable; losing the page is not. Each of these
    /// comes back byte for byte.
    #[rstest]
    #[case::no_html_tag("<p>hello</p>")]
    #[case::a_word_starting_with_html("<htmlish>")]
    #[case::an_unterminated_tag("<html lang=\"en\"")]
    fn a_document_without_an_html_tag_is_untouched(#[case] html: &str) {
        assert_eq!(
            with_appearance(html, crate::config::AppearanceConfig::default()),
            html
        );
    }

    /// Every visitor of a fresh instance sees the default, so it is pinned here
    /// instead of left to whichever variant came first.
    #[test]
    fn the_default_instance_follows_the_device() {
        let page = with_appearance("<html>", crate::config::AppearanceConfig::default());
        assert!(page.contains("data-mode=\"system\""), "{page}");
        assert!(page.contains("data-theme=\"default\""), "{page}");
        assert!(page.contains("data-accent=\"violet\""), "{page}");
        assert!(!page.contains("class="), "{page}");
    }

    /// The operator's name reaches the tab, escaped: the value goes between
    /// tags on a page served from this origin.
    #[test]
    fn the_instance_name_goes_in_the_title() {
        let site = crate::config::SiteConfig {
            title: "Acme <files> & co".to_owned(),
            description: String::new(),
            ..Default::default()
        };
        let page = named("<html><head><title>Tessera</title></head></html>", &site);
        assert!(
            page.contains("<title>Acme &lt;files&gt; &amp; co</title>"),
            "{page}"
        );
        assert!(!page.contains("name=\"description\""), "{page}");
    }

    #[test]
    fn a_description_is_added_only_when_there_is_one() {
        let site = crate::config::SiteConfig {
            title: "Acme".to_owned(),
            description: "  ".to_owned(),
            ..Default::default()
        };
        let page = named("<html><head><title>t</title></head></html>", &site);
        assert!(!page.contains("description"), "{page}");

        let site = crate::config::SiteConfig {
            description: "Files, briefly".to_owned(),
            ..site
        };
        let page = named("<html><head><title>t</title></head></html>", &site);
        assert!(
            page.contains("<meta name=\"description\" content=\"Files, briefly\">"),
            "{page}"
        );
    }

    /// As with the colours: the wrong name on the tab beats a page that does
    /// not load.
    #[rstest]
    #[case::no_title("<html><head></head></html>")]
    #[case::unterminated("<html><head><title>Tessera</head></html>")]
    fn a_document_without_a_title_is_untouched(#[case] html: &str) {
        assert_eq!(retitled(html, "Acme"), html);
    }
}
