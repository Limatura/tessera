//! One person's own preferences, and the instance's default underneath them.

mod common;

use axum::http::{Method, StatusCode, header};
use common::{Auth, TestApp, body_json, body_string, expect_status, session_cookie};

/// The bootstrap administrator's password, for the tests that need an account.
const ADMIN_PASSWORD: &str = "correct horse battery staple";

/// An instance whose bootstrap administrator has a password these tests know.
async fn an_app_with_an_account() -> TestApp {
    TestApp::with_config(|config| {
        ADMIN_PASSWORD.clone_into(&mut config.auth.admin_password);
    })
    .await
}

/// Bootstraps an account and signs in as it.
///
/// Any account will do; these endpoints need somebody signed in and nothing
/// else. The administrator is simply the one a fresh instance already has.
///
/// See `common/mod.rs`: the test-code exemption only reaches code inside a
/// `#[test]` body, so a helper has to restate it.
#[expect(
    clippy::expect_used,
    reason = "test helper; a fixture that cannot build has nothing to report but a failure"
)]
async fn a_session(app: &TestApp) -> String {
    tessera_server::auth::bootstrap::ensure_admin(
        &app.state.users,
        &app.state.roles,
        &app.state.config().auth,
    )
    .await
    .expect("bootstrap");
    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/login",
            Auth::None,
            &serde_json::json!({ "identifier": "admin", "password": ADMIN_PASSWORD }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    session_cookie(&response)
}

async fn get_preferences(app: &TestApp, cookie: &str) -> serde_json::Value {
    let response = expect_status(
        app.empty(Method::GET, "/api/v1/me/preferences", Auth::Cookie(cookie))
            .await,
        StatusCode::OK,
    )
    .await;
    body_json(response).await
}

async fn put_preferences(
    app: &TestApp,
    cookie: &str,
    body: &serde_json::Value,
    expected: StatusCode,
) -> serde_json::Value {
    let response = expect_status(
        app.json(
            Method::PUT,
            "/api/v1/me/preferences",
            Auth::Cookie(cookie),
            body,
        )
        .await,
        expected,
    )
    .await;
    body_json(response).await
}

/// Somebody who has never opened the setting is described by an empty object,
/// not by a copy of whatever the instance's default happens to be that day.
#[tokio::test]
async fn having_chosen_nothing_reads_as_an_empty_object() {
    let app = an_app_with_an_account().await;
    let cookie = a_session(&app).await;

    assert_eq!(
        get_preferences(&app, &cookie).await,
        serde_json::json!({ "appearance": {} })
    );
}

/// The round trip the frontend actually makes: save three names, read them
/// back on the next device.
#[tokio::test]
async fn a_choice_survives_the_round_trip() {
    let app = an_app_with_an_account().await;
    let cookie = a_session(&app).await;

    let chosen = serde_json::json!({
        "appearance": { "mode": "dark", "theme": "sepia", "accent": "rose" }
    });
    assert_eq!(
        put_preferences(&app, &cookie, &chosen, StatusCode::OK).await,
        chosen
    );
    assert_eq!(get_preferences(&app, &cookie).await, chosen);
}

/// Choosing one thing must not silently choose the other two, or the instance
/// default stops reaching anybody who ever touched the setting.
#[tokio::test]
async fn choosing_one_thing_leaves_the_rest_following_the_instance() {
    let app = an_app_with_an_account().await;
    let cookie = a_session(&app).await;

    let only_accent = serde_json::json!({ "appearance": { "accent": "teal" } });
    put_preferences(&app, &cookie, &only_accent, StatusCode::OK).await;

    assert_eq!(get_preferences(&app, &cookie).await, only_accent);
}

/// The whole object is replaced, so an empty one is how somebody goes back to
/// following the instance for everything.
#[tokio::test]
async fn an_empty_object_clears_what_was_chosen() {
    let app = an_app_with_an_account().await;
    let cookie = a_session(&app).await;

    put_preferences(
        &app,
        &cookie,
        &serde_json::json!({ "appearance": { "mode": "dark" } }),
        StatusCode::OK,
    )
    .await;
    put_preferences(
        &app,
        &cookie,
        &serde_json::json!({ "appearance": {} }),
        StatusCode::OK,
    )
    .await;

    assert_eq!(
        get_preferences(&app, &cookie).await,
        serde_json::json!({ "appearance": {} })
    );
}

/// These names become attributes on the `<html>` element, so anything outside
/// the lists has to stop here rather than at the point of rendering.
#[tokio::test]
async fn a_name_outside_the_lists_is_refused() {
    let app = an_app_with_an_account().await;
    let cookie = a_session(&app).await;

    for body in [
        // A derived struct deserializer takes a sequence as its fields in
        // order, so this was a valid appearance with everything unset while
        // the contract said it was an object. Found by the contract fuzzer.
        serde_json::json!({ "appearance": [null, null] }),
        serde_json::json!({ "appearance": ["dark", "sepia", "rose"] }),
        serde_json::json!({ "appearance": "dark" }),
        serde_json::json!({ "appearance": { "accent": "chartreuse" } }),
        serde_json::json!({ "appearance": { "theme": "default\" onload=\"alert(1)" } }),
        serde_json::json!({ "appearance": { "mode": ["dark"] } }),
        serde_json::json!({ "appearance": { "colour": "rose" } }),
        serde_json::json!({ "preferences": { "accent": "rose" } }),
    ] {
        put_preferences(&app, &cookie, &body, StatusCode::BAD_REQUEST).await;
    }

    assert_eq!(
        get_preferences(&app, &cookie).await,
        serde_json::json!({ "appearance": {} }),
        "a refused write must not have stored anything"
    );
}

/// Preferences are per person, so they need somebody to be per.
#[tokio::test]
async fn anonymous_callers_are_refused() {
    let app = TestApp::ready().await;

    expect_status(
        app.empty(Method::GET, "/api/v1/me/preferences", Auth::None)
            .await,
        StatusCode::UNAUTHORIZED,
    )
    .await;
    expect_status(
        app.json(
            Method::PUT,
            "/api/v1/me/preferences",
            Auth::None,
            &serde_json::json!({ "appearance": {} }),
        )
        .await,
        StatusCode::UNAUTHORIZED,
    )
    .await;
}

/// The point of writing the colours into the page from here: a visitor who is
/// not signed in, and has no local storage to read, still gets the operator's
/// instance rather than a white flash.
#[tokio::test]
async fn the_page_carries_the_instances_appearance() {
    let app = TestApp::with_config(|config| {
        config.appearance.mode = tessera_core::Mode::Dark;
        config.appearance.theme = tessera_core::Theme::Graphite;
        config.appearance.accent = tessera_core::Accent::Amber;
    })
    .await;

    let response = expect_status(app.get("/").await, StatusCode::OK).await;
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-cache"),
        "the entry point is the mutable pointer at the build"
    );
    // The entity tag describes what was sent, not the file on disk, or a proxy
    // in front of this would keep serving the colours from before.
    let etag = response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .expect("an entity tag")
        .to_owned();

    let page = body_string(response).await;
    assert!(page.contains(r#"data-mode="dark""#), "{page}");
    assert!(page.contains(r#"data-theme="graphite""#), "{page}");
    assert!(page.contains(r#"data-accent="amber""#), "{page}");
    assert!(page.contains(r#"class="dark""#), "{page}");

    let plain = TestApp::ready().await;
    let other = expect_status(plain.get("/").await, StatusCode::OK).await;
    assert_ne!(
        other
            .headers()
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok()),
        Some(etag.as_str()),
        "two instances serving different colours must not share an entity tag"
    );
}

/// A client-side route is the same page, so it carries the same colours. This
/// is the common case in a single-page app: the first load is rarely `/`.
#[tokio::test]
async fn a_client_side_route_carries_them_too() {
    let app = TestApp::with_config(|config| {
        config.appearance.accent = tessera_core::Accent::Green;
    })
    .await;

    let page = body_string(expect_status(app.get("/some/route").await, StatusCode::OK).await).await;
    assert!(page.contains(r#"data-accent="green""#), "{page}");
}

/// The instance default is a hot setting: changing it in the panel must show
/// on the next load, with no restart and no stale cached page.
#[tokio::test]
async fn changing_the_setting_changes_the_page() {
    let app = an_app_with_an_account().await;
    let cookie = a_session(&app).await;

    let before = body_string(expect_status(app.get("/").await, StatusCode::OK).await).await;
    assert!(before.contains(r#"data-accent="violet""#), "{before}");

    expect_status(
        app.json(
            Method::PUT,
            "/api/v1/admin/settings",
            Auth::Cookie(&cookie),
            &serde_json::json!({
                "changes": { "appearance.accent": "rose", "appearance.mode": "dark" }
            }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    let after = body_string(expect_status(app.get("/").await, StatusCode::OK).await).await;
    assert!(after.contains(r#"data-accent="rose""#), "{after}");
    assert!(after.contains(r#"class="dark""#), "{after}");
}

/// The panel can rename the instance, and the empty title it refuses is
/// refused by the same rule a configuration file would be.
#[tokio::test]
async fn the_panel_can_rename_the_instance() {
    let app = an_app_with_an_account().await;
    let cookie = a_session(&app).await;

    expect_status(
        app.json(
            Method::PUT,
            "/api/v1/admin/settings",
            Auth::Cookie(&cookie),
            &serde_json::json!({ "changes": { "site.title": "Acme Drop" } }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let page = body_string(expect_status(app.get("/").await, StatusCode::OK).await).await;
    assert!(page.contains("<title>Acme Drop</title>"), "{page}");

    expect_status(
        app.json(
            Method::PUT,
            "/api/v1/admin/settings",
            Auth::Cookie(&cookie),
            &serde_json::json!({ "changes": { "site.title": "  " } }),
        )
        .await,
        StatusCode::BAD_REQUEST,
    )
    .await;
}

/// The same refusal as the per-person endpoint, on the instance's own value.
/// It reaches the same `<html>` element by a different route.
#[tokio::test]
async fn the_instance_default_refuses_a_name_outside_the_lists() {
    let app = an_app_with_an_account().await;
    let cookie = a_session(&app).await;

    expect_status(
        app.json(
            Method::PUT,
            "/api/v1/admin/settings",
            Auth::Cookie(&cookie),
            &serde_json::json!({ "changes": { "appearance.accent": "chartreuse" } }),
        )
        .await,
        StatusCode::BAD_REQUEST,
    )
    .await;
}
