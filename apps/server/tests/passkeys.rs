//! Passkeys, over HTTP.
//!
//! The round trip lives in [`common::passkey_lifecycle`], shared with the
//! cross-backend suite. What is here is what must not work.
//!
//! The helpers below sit outside a `#[test]` body, where `clippy.toml`'s test
//! exemptions do not reach. A fixture that cannot read what the server sent
//! has nothing to report but a failure.
#![expect(clippy::expect_used, reason = "test fixtures; see module docs")]

mod common;

use axum::http::{Method, StatusCode};
use common::{
    Auth, PASSKEY_ORIGIN, TestApp, add_passkey, authenticator::Authenticator, body_json,
    expect_status, passkey_lifecycle, passkey_options, passkey_sign_in, place_at, session_cookie,
};

const PASSWORD: &str = "a perfectly adequate passphrase";

/// An instance on [`PASSKEY_ORIGIN`], with one account, returning its cookie
/// and id.
async fn with_account() -> (TestApp, String, String) {
    let app = TestApp::ready().await;
    place_at(&app, PASSKEY_ORIGIN);
    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": "alice",
                "email": "alice@example.com",
                "password": PASSWORD,
            }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let cookie = session_cookie(&response);
    let id = body_json(response)
        .await
        .get("id")
        .and_then(serde_json::Value::as_str)
        .expect("an id")
        .to_owned();
    (app, cookie, id)
}

async fn refused_code(response: axum::response::Response) -> String {
    let body = body_json(expect_status(response, StatusCode::UNAUTHORIZED).await).await;
    body.get("code")
        .and_then(serde_json::Value::as_str)
        .expect("a code")
        .to_owned()
}

#[tokio::test]
async fn a_passkey_can_be_added_used_renamed_and_removed() {
    passkey_lifecycle(&TestApp::ready().await, "").await;
}

/// A passkey is a fingerprint or a face on top of the device: the password and
/// the second step together.
#[tokio::test]
async fn a_passkey_sign_in_skips_the_two_step_code() {
    let (app, cookie, _) = with_account().await;
    let mut key = Authenticator::new(PASSKEY_ORIGIN);
    add_passkey(&app, &cookie, PASSWORD, &mut key).await;

    let setup = body_json(
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/account/mfa/totp",
                Auth::Cookie(&cookie),
                &serde_json::json!({ "current_password": PASSWORD }),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    let secret = tessera_server::auth::mfa::TotpSecret::from_base32(
        setup
            .get("secret")
            .and_then(serde_json::Value::as_str)
            .expect("a secret"),
    )
    .expect("base32");
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    expect_status(
        app.json(
            Method::POST,
            "/api/v1/account/mfa/totp/confirm",
            Auth::Cookie(&cookie),
            &serde_json::json!({ "code": secret.code(now).expect("a code") }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    expect_status(passkey_sign_in(&app, &mut key).await, StatusCode::OK).await;
}

#[tokio::test]
async fn an_answer_from_another_site_is_refused() {
    let (app, cookie, _) = with_account().await;
    let mut key = Authenticator::new(PASSKEY_ORIGIN);
    add_passkey(&app, &cookie, PASSWORD, &mut key).await;

    // The right key, used from the wrong page.
    key.visit("https://files.example.evil");
    let response = passkey_sign_in(&app, &mut key).await;
    assert_eq!(refused_code(response).await, "passkey_rejected");
}

/// Touching the key is not enough: it has to have checked who touched it.
#[tokio::test]
async fn an_authenticator_that_did_not_verify_the_person_is_refused() {
    let (app, cookie, _) = with_account().await;
    let mut key = Authenticator::new(PASSKEY_ORIGIN);
    add_passkey(&app, &cookie, PASSWORD, &mut key).await;

    let options = passkey_options(
        &app,
        "/api/v1/auth/passkey/options",
        Auth::None,
        &serde_json::json!({}),
    )
    .await;
    let credential = key.sign_in_with_flags(&options, 0x01);
    let response = app
        .json(
            Method::POST,
            "/api/v1/auth/passkey",
            Auth::None,
            &serde_json::json!({ "credential": credential }),
        )
        .await;
    assert_eq!(refused_code(response).await, "passkey_rejected");
}

#[tokio::test]
async fn an_answer_works_once() {
    let (app, cookie, _) = with_account().await;
    let mut key = Authenticator::new(PASSKEY_ORIGIN);
    add_passkey(&app, &cookie, PASSWORD, &mut key).await;

    let options = passkey_options(
        &app,
        "/api/v1/auth/passkey/options",
        Auth::None,
        &serde_json::json!({}),
    )
    .await;
    let body = serde_json::json!({ "credential": key.sign_in(&options) });
    expect_status(
        app.json(Method::POST, "/api/v1/auth/passkey", Auth::None, &body)
            .await,
        StatusCode::OK,
    )
    .await;
    let again = app
        .json(Method::POST, "/api/v1/auth/passkey", Auth::None, &body)
        .await;
    assert_eq!(refused_code(again).await, "passkey_rejected");
}

/// A counter that goes backwards is a copy of the key.
#[tokio::test]
async fn a_counter_that_goes_backwards_is_refused() {
    let (app, cookie, _) = with_account().await;
    let mut key = Authenticator::new(PASSKEY_ORIGIN).with_counter(10);
    add_passkey(&app, &cookie, PASSWORD, &mut key).await;
    expect_status(passkey_sign_in(&app, &mut key).await, StatusCode::OK).await;

    key.rewind(1);
    let response = passkey_sign_in(&app, &mut key).await;
    assert_eq!(refused_code(response).await, "passkey_rejected");
}

#[tokio::test]
async fn the_same_passkey_cannot_be_added_twice() {
    let (app, cookie, _) = with_account().await;
    let mut key = Authenticator::new(PASSKEY_ORIGIN);
    add_passkey(&app, &cookie, PASSWORD, &mut key).await;

    let options = passkey_options(
        &app,
        "/api/v1/account/passkeys/options",
        Auth::Cookie(&cookie),
        &serde_json::json!({ "current_password": PASSWORD }),
    )
    .await;
    // The browser is told which to leave out.
    assert_eq!(
        options
            .get("excludeCredentials")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len),
        Some(1)
    );
    let response = app
        .json(
            Method::POST,
            "/api/v1/account/passkeys",
            Auth::Cookie(&cookie),
            &serde_json::json!({ "credential": key.register(&options) }),
        )
        .await;
    expect_status(response, StatusCode::CONFLICT).await;
}

/// A borrowed, unlocked browser must not be able to add a passkey the
/// borrower holds.
#[tokio::test]
async fn adding_one_needs_the_password() {
    let (app, cookie, _) = with_account().await;
    expect_status(
        app.json(
            Method::POST,
            "/api/v1/account/passkeys/options",
            Auth::Cookie(&cookie),
            &serde_json::json!({ "current_password": "not the password at all" }),
        )
        .await,
        StatusCode::UNAUTHORIZED,
    )
    .await;
}

/// A challenge issued for one account cannot add a passkey to another.
#[tokio::test]
async fn a_registration_answers_only_its_own_account() {
    let (app, cookie, _) = with_account().await;
    let other = session_cookie(
        &expect_status(
            app.json(
                Method::POST,
                "/api/v1/auth/register",
                Auth::None,
                &serde_json::json!({
                    "username": "mallory",
                    "email": "mallory@example.com",
                    "password": PASSWORD,
                }),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    );

    let options = passkey_options(
        &app,
        "/api/v1/account/passkeys/options",
        Auth::Cookie(&cookie),
        &serde_json::json!({ "current_password": PASSWORD }),
    )
    .await;
    let mut key = Authenticator::new(PASSKEY_ORIGIN);
    expect_status(
        app.json(
            Method::POST,
            "/api/v1/account/passkeys",
            Auth::Cookie(&other),
            &serde_json::json!({ "credential": key.register(&options) }),
        )
        .await,
        StatusCode::BAD_REQUEST,
    )
    .await;
}

/// With no password and no provider, the last passkey is the only way in.
#[tokio::test]
async fn the_last_way_in_cannot_be_removed() {
    use sea_orm::ConnectionTrait as _;

    let (app, cookie, id) = with_account().await;
    let mut key = Authenticator::new(PASSKEY_ORIGIN);
    let added = add_passkey(&app, &cookie, PASSWORD, &mut key).await;
    let passkey = added
        .get("id")
        .and_then(serde_json::Value::as_str)
        .expect("an id");

    app.state
        .db
        .execute_unprepared(&format!("DELETE FROM credentials WHERE user_id = '{id}'"))
        .await
        .expect("password removed");

    let refused = body_json(
        expect_status(
            app.empty(
                Method::DELETE,
                &format!("/api/v1/account/passkeys/{passkey}"),
                Auth::Cookie(&cookie),
            )
            .await,
            StatusCode::CONFLICT,
        )
        .await,
    )
    .await;
    assert_eq!(
        refused.get("code").and_then(serde_json::Value::as_str),
        Some("last_sign_in_method")
    );
}

#[tokio::test]
async fn another_accounts_passkey_is_not_found() {
    let (app, cookie, _) = with_account().await;
    let mut key = Authenticator::new(PASSKEY_ORIGIN);
    let added = add_passkey(&app, &cookie, PASSWORD, &mut key).await;
    let passkey = added
        .get("id")
        .and_then(serde_json::Value::as_str)
        .expect("an id");

    let other = session_cookie(
        &expect_status(
            app.json(
                Method::POST,
                "/api/v1/auth/register",
                Auth::None,
                &serde_json::json!({
                    "username": "mallory",
                    "email": "mallory@example.com",
                    "password": PASSWORD,
                }),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    );
    for method in [Method::DELETE, Method::PATCH] {
        expect_status(
            app.json(
                method,
                &format!("/api/v1/account/passkeys/{passkey}"),
                Auth::Cookie(&other),
                &serde_json::json!({ "name": "mine now" }),
            )
            .await,
            StatusCode::NOT_FOUND,
        )
        .await;
    }
}
