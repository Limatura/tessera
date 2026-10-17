//! The web app manifest, and where the share sheet posts to.
//!
//! # Why the server writes the manifest
//!
//! An installed app takes its name from the manifest, and a self-hosted
//! instance is called whatever its operator set in `site.title`. A file in the
//! bundle would install every instance as "Tessera". So it is assembled per
//! request from the live config, like the title spliced into `index.html`
//! (see [`crate::assets`]).
//!
//! # The share target
//!
//! `share_target` makes the installed app a destination in the system share
//! sheet (Android; iOS has no equivalent). The share arrives as a multipart
//! `POST /share-target`, which the service worker (`apps/web/public/sw.js`)
//! answers without touching the network: it keeps the files in `IndexedDB` and
//! opens the upload page with them. [`share_target`] is here only for the case
//! where no worker is running (site data cleared, say), so the share opens the
//! app instead of failing; the body is not read.

use axum::{
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::json;
use tessera_core::{Mode, Theme};

/// Where the system share sheet posts. Must match `apps/web/public/sw.js`.
pub const SHARE_TARGET_PATH: &str = "/share-target";
/// Where `index.html` links the manifest from.
pub const MANIFEST_PATH: &str = "/manifest.webmanifest";

/// `GET /manifest.webmanifest`.
pub async fn manifest(State(state): State<crate::state::AppState>) -> Response {
    let config = state.config();
    let colour = background(config.appearance.theme, config.appearance.mode);
    let body = document(&config.site, colour);
    (
        [
            (header::CONTENT_TYPE, "application/manifest+json"),
            // The operator can rename the instance at any time; a browser
            // checking for manifest changes should see it.
            (header::CACHE_CONTROL, "no-cache"),
        ],
        body.to_string(),
    )
        .into_response()
}

fn document(site: &crate::config::SiteConfig, colour: &str) -> serde_json::Value {
    let mut manifest = json!({
        "id": "/",
        "name": site.title,
        "short_name": site.title,
        "start_url": "/",
        "scope": "/",
        "display": "standalone",
        "background_color": colour,
        "theme_color": colour,
        "icons": [
            { "src": "/icon-192.png", "sizes": "192x192", "type": "image/png", "purpose": "any" },
            { "src": "/icon-512.png", "sizes": "512x512", "type": "image/png", "purpose": "any" },
            { "src": "/icon-maskable-512.png", "sizes": "512x512", "type": "image/png", "purpose": "maskable" },
        ],
        "share_target": {
            "action": SHARE_TARGET_PATH,
            "method": "POST",
            "enctype": "multipart/form-data",
            "params": {
                "title": "title",
                "text": "text",
                "url": "url",
                "files": [{ "name": "files", "accept": ["*/*"] }],
            },
        },
    });
    if !site.description.trim().is_empty()
        && let Some(fields) = manifest.as_object_mut()
    {
        fields.insert("description".to_owned(), json!(site.description));
    }
    manifest
}

/// The theme's page background, for the splash screen and the title bar
/// before the page has loaded.
///
/// These mirror `--background` in `packages/ui/src/styles/globals.css`,
/// converted from oklch to hex: a manifest is read by the OS, which is not
/// guaranteed to understand oklch. A new theme needs a row here as well as its
/// CSS; the match is exhaustive, so the build says so.
///
/// `System` takes the light colour: a manifest has no media queries. Once the
/// page is up, `<meta name="theme-color">` follows the visitor's actual
/// palette (`apps/web/src/components/theme-color-sync.tsx`).
const fn background(theme: Theme, mode: Mode) -> &'static str {
    let dark = matches!(mode, Mode::Dark);
    match (theme, dark) {
        (Theme::Default, false) => "#ffffff",
        (Theme::Default, true) => "#0c0a09",
        (Theme::Graphite, false) => "#f9fafb",
        (Theme::Graphite, true) => "#0e1116",
        (Theme::Sepia, false) => "#fbf7ef",
        (Theme::Sepia, true) => "#18130f",
        (Theme::Matsu, false) => "#f1dfbe",
        (Theme::Matsu, true) => "#221d12",
        (Theme::Cosmic, false) => "#f5f5ff",
        (Theme::Cosmic, true) => "#0f0f1a",
        (Theme::Slate, false) => "#f8fafc",
        (Theme::Slate, true) => "#0f172a",
        (Theme::Vintage, false) => "#f5f1e6",
        (Theme::Vintage, true) => "#2d2621",
    }
}

/// `POST /share-target` when no service worker caught it.
///
/// 303 so the browser follows with a `GET`. The shared content is lost: the
/// worker is what keeps it, and the next visit installs it again.
pub async fn share_target() -> Response {
    (StatusCode::SEE_OTHER, [(header::LOCATION, "/")]).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SiteConfig;

    fn site(title: &str, description: &str) -> SiteConfig {
        SiteConfig {
            title: title.to_owned(),
            description: description.to_owned(),
            ..SiteConfig::default()
        }
    }

    #[test]
    fn names_the_app_after_the_instance() {
        let manifest = document(&site("Drop Box", ""), "#ffffff");
        assert_eq!(manifest["name"], "Drop Box");
        assert_eq!(manifest["short_name"], "Drop Box");
        assert!(manifest.get("description").is_none());
    }

    #[test]
    fn carries_the_description_when_set() {
        let manifest = document(&site("T", "files, briefly"), "#ffffff");
        assert_eq!(manifest["description"], "files, briefly");
    }

    #[test]
    fn share_target_posts_files_to_the_worker_path() {
        let manifest = document(&site("T", ""), "#ffffff");
        let target = &manifest["share_target"];
        assert_eq!(target["action"], SHARE_TARGET_PATH);
        assert_eq!(target["method"], "POST");
        assert_eq!(target["enctype"], "multipart/form-data");
        assert_eq!(target["params"]["files"][0]["name"], "files");
    }

    #[test]
    fn dark_mode_takes_the_dark_background() {
        assert_eq!(background(Theme::Default, Mode::Dark), "#0c0a09");
        assert_eq!(background(Theme::Default, Mode::Light), "#ffffff");
        assert_eq!(background(Theme::Default, Mode::System), "#ffffff");
    }
}
