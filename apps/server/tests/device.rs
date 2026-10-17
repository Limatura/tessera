//! Signing in a program from a browser, over HTTP.
//!
//! The helpers below sit outside a `#[test]` body, where `clippy.toml`'s test
//! exemptions do not reach. A fixture that cannot read what the server sent
//! has nothing to report but a failure.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test fixtures; see module docs"
)]

mod common;

use axum::http::{Method, StatusCode};
use common::{Auth, TestApp, body_json, expect_status, session_cookie};

async fn register(app: &TestApp, username: &str) -> String {
    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": username,
                "email": format!("{username}@example.com"),
                "password": "a perfectly adequate passphrase",
            }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    session_cookie(&response)
}

/// Starts a request and returns its device code and user code.
async fn start(app: &TestApp) -> (String, String) {
    let started = body_json(
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/auth/device",
                Auth::None,
                &serde_json::json!({ "client_name": "tessera CLI on laptop" }),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    (
        started["device_code"]
            .as_str()
            .expect("a device code")
            .to_owned(),
        started["user_code"]
            .as_str()
            .expect("a user code")
            .to_owned(),
    )
}

async fn poll(app: &TestApp, device_code: &str) -> axum::response::Response {
    app.json(
        Method::POST,
        "/api/v1/auth/device/token",
        Auth::None,
        &serde_json::json!({ "device_code": device_code }),
    )
    .await
}

async fn refusal(response: axum::response::Response, status: StatusCode) -> String {
    body_json(expect_status(response, status).await).await["code"]
        .as_str()
        .expect("a code")
        .to_owned()
}

#[tokio::test]
async fn an_approved_program_gets_a_token_once() {
    let app = TestApp::ready().await;
    let cookie = register(&app, "alice").await;
    let (device_code, user_code) = start(&app).await;

    assert_eq!(
        refusal(poll(&app, &device_code).await, StatusCode::BAD_REQUEST).await,
        "device_authorization_pending"
    );

    // Typed the way a person might: lower case, no dash.
    let typed = user_code.replace('-', "").to_lowercase();
    let pending = body_json(
        expect_status(
            app.empty(
                Method::GET,
                &format!("/api/v1/auth/device/{typed}"),
                Auth::Cookie(&cookie),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(pending["client_name"], "tessera CLI on laptop");
    assert_eq!(pending["user_code"], user_code.as_str());

    expect_status(
        app.empty(
            Method::POST,
            &format!("/api/v1/auth/device/{user_code}/approve"),
            Auth::Cookie(&cookie),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;

    let issued =
        body_json(expect_status(poll(&app, &device_code).await, StatusCode::OK).await).await;
    assert_eq!(issued["name"], "tessera CLI on laptop");
    let mut permissions = issued["permissions"]
        .as_array()
        .expect("permissions")
        .iter()
        .map(|permission| permission.as_str().expect("a name").to_owned())
        .collect::<Vec<_>>();
    permissions.sort();
    assert_eq!(
        permissions,
        ["shares:create", "shares:delete", "shares:read"]
    );

    let secret = issued["secret"].as_str().expect("a secret");
    let session = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/auth/session", Auth::Bearer(secret))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(session["username"], "alice");

    assert_eq!(
        refusal(poll(&app, &device_code).await, StatusCode::BAD_REQUEST).await,
        "device_code_unusable"
    );
}

#[tokio::test]
async fn a_denied_program_is_told_so() {
    let app = TestApp::ready().await;
    let cookie = register(&app, "alice").await;
    let (device_code, user_code) = start(&app).await;

    expect_status(
        app.empty(
            Method::POST,
            &format!("/api/v1/auth/device/{user_code}/deny"),
            Auth::Cookie(&cookie),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(
        refusal(poll(&app, &device_code).await, StatusCode::FORBIDDEN).await,
        "device_authorization_denied"
    );

    // Answered once, it cannot be approved after all.
    expect_status(
        app.empty(
            Method::POST,
            &format!("/api/v1/auth/device/{user_code}/approve"),
            Auth::Cookie(&cookie),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
}

#[tokio::test]
async fn a_code_nobody_issued_is_not_found() {
    let app = TestApp::ready().await;
    let cookie = register(&app, "alice").await;
    expect_status(
        app.empty(
            Method::GET,
            "/api/v1/auth/device/BCDF-GHJK",
            Auth::Cookie(&cookie),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    assert_eq!(
        refusal(poll(&app, "made up").await, StatusCode::BAD_REQUEST).await,
        "device_code_unusable"
    );
}

#[tokio::test]
async fn an_api_token_cannot_approve_another_program() {
    let app = TestApp::ready().await;
    let cookie = register(&app, "alice").await;
    let (first, code) = start(&app).await;
    expect_status(
        app.empty(
            Method::POST,
            &format!("/api/v1/auth/device/{code}/approve"),
            Auth::Cookie(&cookie),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let issued = body_json(expect_status(poll(&app, &first).await, StatusCode::OK).await).await;
    let secret = issued["secret"].as_str().expect("a secret");

    let (_, second) = start(&app).await;
    assert_eq!(
        refusal(
            app.empty(
                Method::POST,
                &format!("/api/v1/auth/device/{second}/approve"),
                Auth::Bearer(secret),
            )
            .await,
            StatusCode::FORBIDDEN,
        )
        .await,
        "session_required"
    );
}

#[tokio::test]
async fn answering_needs_a_signed_in_browser() {
    let app = TestApp::ready().await;
    let (_, code) = start(&app).await;
    expect_status(
        app.empty(
            Method::POST,
            &format!("/api/v1/auth/device/{code}/approve"),
            Auth::None,
        )
        .await,
        StatusCode::UNAUTHORIZED,
    )
    .await;
}
