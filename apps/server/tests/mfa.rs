//! Two-step sign-in, over HTTP.
//!
//! The round trip lives in [`common::two_step_lifecycle`], shared with the
//! cross-backend suite. What is here is what must not work, and the ways back in
//! for someone who has lost the authenticator.
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
use common::{
    Auth, TestApp, body_json, expect_status, finish_sign_in, session_cookie, two_step_lifecycle,
};
use tessera_server::auth::mfa::TotpSecret;

const PASSWORD: &str = "a perfectly adequate passphrase";

async fn register(app: &TestApp, username: &str) -> (String, String) {
    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": username,
                "email": format!("{username}@example.com"),
                "password": PASSWORD,
            }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let cookie = session_cookie(&response);
    let id = body_json(response).await["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    (cookie, id)
}

fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

/// Sets up and confirms an authenticator, returning its secret.
async fn turn_on(app: &TestApp, cookie: &str) -> TotpSecret {
    let setup = body_json(
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/account/mfa/totp",
                Auth::Cookie(cookie),
                &serde_json::json!({ "current_password": PASSWORD }),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    let secret = TotpSecret::from_base32(setup["secret"].as_str().expect("a secret"))
        .expect("a base32 secret");
    expect_status(
        app.json(
            Method::POST,
            "/api/v1/account/mfa/totp/confirm",
            Auth::Cookie(cookie),
            &serde_json::json!({ "code": secret.code(now()).expect("a code") }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    secret
}

async fn sign_in(app: &TestApp, username: &str) -> axum::response::Response {
    app.json(
        Method::POST,
        "/api/v1/auth/login",
        Auth::None,
        &serde_json::json!({ "identifier": username, "password": PASSWORD }),
    )
    .await
}

#[tokio::test]
async fn an_account_can_turn_two_step_sign_in_on_use_it_and_turn_it_off() {
    two_step_lifecycle(&TestApp::ready().await, "").await;
}

#[tokio::test]
async fn a_wrong_code_leaves_the_ticket_usable() {
    let app = TestApp::ready().await;
    let (cookie, _) = register(&app, "alice").await;
    let secret = turn_on(&app, &cookie).await;

    let challenge =
        body_json(expect_status(sign_in(&app, "alice").await, StatusCode::ACCEPTED).await).await;
    let ticket = challenge["ticket"].as_str().expect("a ticket");

    let refused = body_json(
        expect_status(
            finish_sign_in(&app, ticket, "not a code").await,
            StatusCode::UNAUTHORIZED,
        )
        .await,
    )
    .await;
    assert_eq!(refused["code"], "mfa_code_incorrect");
    let code = secret.code(now() + 30).expect("a code");
    expect_status(finish_sign_in(&app, ticket, &code).await, StatusCode::OK).await;
}

#[tokio::test]
async fn a_ticket_that_was_never_issued_is_refused() {
    let app = TestApp::ready().await;
    let refused = body_json(
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/auth/login/mfa",
                Auth::None,
                &serde_json::json!({ "ticket": "made up", "code": "123456" }),
            )
            .await,
            StatusCode::UNAUTHORIZED,
        )
        .await,
    )
    .await;
    assert_eq!(refused["code"], "mfa_challenge_unusable");
}

/// Setting up a second one would turn the first off until the second is
/// confirmed, without the owner having said so.
#[tokio::test]
async fn an_account_with_it_on_cannot_set_it_up_again() {
    let app = TestApp::ready().await;
    let (cookie, _) = register(&app, "alice").await;
    turn_on(&app, &cookie).await;

    let refused = body_json(
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/account/mfa/totp",
                Auth::Cookie(&cookie),
                &serde_json::json!({ "current_password": PASSWORD }),
            )
            .await,
            StatusCode::CONFLICT,
        )
        .await,
    )
    .await;
    assert_eq!(refused["code"], "mfa_already_enabled");
}

/// A borrowed, unlocked browser must not be able to tie the account to the
/// borrower's phone.
#[tokio::test]
async fn setting_it_up_needs_the_password() {
    let app = TestApp::ready().await;
    let (cookie, _) = register(&app, "alice").await;
    expect_status(
        app.json(
            Method::POST,
            "/api/v1/account/mfa/totp",
            Auth::Cookie(&cookie),
            &serde_json::json!({ "current_password": "not the password at all" }),
        )
        .await,
        StatusCode::UNAUTHORIZED,
    )
    .await;
}

#[tokio::test]
async fn turning_it_off_needs_the_password_or_a_code() {
    let app = TestApp::ready().await;
    let (cookie, _) = register(&app, "alice").await;
    turn_on(&app, &cookie).await;

    expect_status(
        app.json(
            Method::DELETE,
            "/api/v1/account/mfa/totp",
            Auth::Cookie(&cookie),
            &serde_json::json!({}),
        )
        .await,
        StatusCode::UNAUTHORIZED,
    )
    .await;
    expect_status(
        app.json(
            Method::DELETE,
            "/api/v1/account/mfa/totp",
            Auth::Cookie(&cookie),
            &serde_json::json!({ "current_password": PASSWORD }),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    expect_status(sign_in(&app, "alice").await, StatusCode::OK).await;
}

async fn regenerate(app: &TestApp, cookie: &str) -> serde_json::Value {
    body_json(
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/account/mfa/recovery-codes",
                Auth::Cookie(cookie),
                &serde_json::json!({ "current_password": PASSWORD }),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await
}

#[tokio::test]
async fn new_recovery_codes_replace_the_old_ones() {
    let app = TestApp::ready().await;
    let (cookie, _) = register(&app, "alice").await;
    turn_on(&app, &cookie).await;

    let old = regenerate(&app, &cookie).await;
    let new = regenerate(&app, &cookie).await;

    let challenge =
        body_json(expect_status(sign_in(&app, "alice").await, StatusCode::ACCEPTED).await).await;
    let ticket = challenge["ticket"].as_str().expect("a ticket");
    let first = |codes: &serde_json::Value| {
        codes["recovery_codes"][0]
            .as_str()
            .expect("a code")
            .to_owned()
    };
    expect_status(
        finish_sign_in(&app, ticket, &first(&old)).await,
        StatusCode::UNAUTHORIZED,
    )
    .await;
    expect_status(
        finish_sign_in(&app, ticket, &first(&new)).await,
        StatusCode::OK,
    )
    .await;
}

/// The way back for somebody who lost the phone and the codes.
#[tokio::test]
async fn an_administrator_can_turn_it_off_for_someone_else() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "founder").await;
    let (cookie, id) = register(&app, "alice").await;
    turn_on(&app, &cookie).await;
    expect_status(sign_in(&app, "alice").await, StatusCode::ACCEPTED).await;
    assert_eq!(listed_mfa(&app, &admin, &id).await, Some(true));

    // Not something an account can do to another without the permission.
    expect_status(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/users/{id}/mfa"),
            Auth::Cookie(&cookie),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;

    expect_status(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/users/{id}/mfa"),
            Auth::Cookie(&admin),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    expect_status(sign_in(&app, "alice").await, StatusCode::OK).await;
    assert_eq!(listed_mfa(&app, &admin, &id).await, Some(false));
}

/// Whether the panel's list of accounts shows two-step sign-in on for `id`.
async fn listed_mfa(app: &TestApp, admin: &str, id: &str) -> Option<bool> {
    let page = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/admin/users", Auth::Cookie(admin))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    page["data"]
        .as_array()?
        .iter()
        .find(|user| user["id"] == id)?["mfa_enabled"]
        .as_bool()
}

/// A leaked token exists to upload files, not to decide who can sign in.
#[tokio::test]
async fn an_api_token_cannot_touch_it() {
    let app = TestApp::ready().await;
    let (cookie, _) = register(&app, "alice").await;
    let token = body_json(
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/tokens",
                Auth::Cookie(&cookie),
                &serde_json::json!({ "name": "script" }),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let token = token["secret"].as_str().expect("a secret");

    expect_status(
        app.empty(Method::GET, "/api/v1/account/mfa", Auth::Bearer(token))
            .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    expect_status(
        app.json(
            Method::POST,
            "/api/v1/account/mfa/totp",
            Auth::Bearer(token),
            &serde_json::json!({ "current_password": PASSWORD }),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
}
