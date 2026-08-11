//! Accounts, sessions and API tokens, over HTTP.
//!
//! The whole lifecycle lives in [`common::account_lifecycle`] rather than here,
//! because the cross-backend suite has to run exactly the same scenario. What is
//! left in this file is the behaviour that does not depend on the dialect:
//! configuration switches, credential separation and the things that must *not*
//! work.

mod common;

use axum::http::{Method, StatusCode, header};
use common::{
    Auth, TestApp, account_lifecycle, body_json, expect_status, force_password_change,
    session_cookie,
};
use tessera_server::config::RegistrationPolicy;

/// Password used wherever the value itself does not matter.
const PASSWORD: &str = "a perfectly adequate passphrase";

fn registration(username: &str) -> serde_json::Value {
    serde_json::json!({
        "username": username,
        "email": format!("{username}@example.com"),
        "password": PASSWORD,
    })
}

async fn register(app: &TestApp, username: &str) -> String {
    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &registration(username),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    session_cookie(&response)
}

#[tokio::test]
async fn an_account_can_be_registered_used_and_closed_out() {
    account_lifecycle(&TestApp::ready().await, "").await;
}

#[tokio::test]
async fn an_anonymous_caller_has_no_session() {
    let app = TestApp::ready().await;
    expect_status(
        app.empty(Method::GET, "/api/v1/auth/session", Auth::None)
            .await,
        StatusCode::UNAUTHORIZED,
    )
    .await;
}

/// The session secret is what turns an XSS into a stolen account, so the
/// attributes that keep it out of JavaScript are asserted rather than assumed.
#[tokio::test]
async fn the_session_cookie_is_http_only_and_same_site_lax() {
    let app = TestApp::ready().await;
    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &registration("cookieinspector"),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;

    let raw = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("a Set-Cookie header")
        .to_str()
        .expect("ASCII");
    assert!(raw.contains("HttpOnly"), "got: {raw}");
    assert!(raw.contains("SameSite=Lax"), "got: {raw}");
    assert!(raw.contains("Path=/"), "got: {raw}");
    // The fixture's config does not claim HTTPS, and marking the cookie `Secure`
    // there would silently break login on a plain-HTTP LAN deployment.
    assert!(!raw.contains("Secure"), "got: {raw}");
}

#[tokio::test]
async fn the_first_account_to_register_administers_the_instance() {
    let app = TestApp::ready().await;
    let cookie = register(&app, "founder").await;
    let body = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&cookie))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(body["is_admin"], true);

    // And only the first: otherwise every visitor administers the instance.
    let cookie = register(&app, "latecomer").await;
    let body = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&cookie))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(body["is_admin"], false);
}

#[tokio::test]
async fn a_closed_instance_refuses_registration() {
    let app = TestApp::with_config(|config| {
        config.auth.registration = RegistrationPolicy::Closed;
    })
    .await;
    expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &registration("uninvited"),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
}

/// With accounts switched off the endpoints are still routed — the contract
/// describes the software, not the instance — but they refuse to exist.
#[tokio::test]
async fn an_instance_without_accounts_reports_the_endpoints_as_absent() {
    let app = TestApp::with_config(|config| {
        config.features.accounts = false;
    })
    .await;

    expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &registration("nobody"),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/login",
            Auth::None,
            &serde_json::json!({ "identifier": "nobody", "password": PASSWORD }),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;

    let body =
        body_json(expect_status(app.get("/api/v1/capabilities").await, StatusCode::OK).await).await;
    assert_eq!(body["accounts"], false);
}

/// A wrong password and an account that does not exist must be indistinguishable
/// from outside, or the login endpoint enumerates accounts for anyone who asks.
#[tokio::test]
async fn a_wrong_password_and_an_unknown_account_answer_identically() {
    let app = TestApp::ready().await;
    register(&app, "known").await;

    let wrong = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/login",
            Auth::None,
            &serde_json::json!({ "identifier": "known", "password": "not the passphrase" }),
        )
        .await,
        StatusCode::UNAUTHORIZED,
    )
    .await;
    let unknown = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/login",
            Auth::None,
            &serde_json::json!({ "identifier": "stranger", "password": "not the passphrase" }),
        )
        .await,
        StatusCode::UNAUTHORIZED,
    )
    .await;

    assert_eq!(body_json(wrong).await, body_json(unknown).await);
}

#[tokio::test]
async fn a_password_below_the_policy_is_refused_at_registration() {
    let app = TestApp::ready().await;
    expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": "brief",
                "email": "brief@example.com",
                "password": "short",
            }),
        )
        .await,
        StatusCode::BAD_REQUEST,
    )
    .await;
}

/// Names are normalised, so `Alice` and `alice` must not become two accounts a
/// person could be phished between.
#[tokio::test]
async fn usernames_are_matched_case_insensitively() {
    let app = TestApp::ready().await;
    register(&app, "alice").await;

    expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": "ALICE",
                "email": "alice-two@example.com",
                "password": PASSWORD,
            }),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;

    expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/login",
            Auth::None,
            &serde_json::json!({ "identifier": "Alice", "password": PASSWORD }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
}

/// A leaked token must not be able to take the account it belongs to.
#[tokio::test]
async fn an_api_token_cannot_change_the_password() {
    let app = TestApp::ready().await;
    let cookie = register(&app, "tokenholder").await;

    let created = body_json(
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/tokens",
                Auth::Cookie(&cookie),
                &serde_json::json!({ "name": "automation" }),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let secret = created["secret"].as_str().expect("a secret").to_owned();

    expect_status(
        app.json(
            Method::PATCH,
            "/api/v1/auth/password",
            Auth::Bearer(&secret),
            &serde_json::json!({
                "current_password": PASSWORD,
                "new_password": "an entirely different passphrase",
            }),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
}

#[tokio::test]
async fn the_wrong_current_password_does_not_change_anything() {
    let app = TestApp::ready().await;
    let cookie = register(&app, "cautious").await;

    expect_status(
        app.json(
            Method::PATCH,
            "/api/v1/auth/password",
            Auth::Cookie(&cookie),
            &serde_json::json!({
                "current_password": "not the current passphrase",
                "new_password": "an entirely different passphrase",
            }),
        )
        .await,
        StatusCode::UNAUTHORIZED,
    )
    .await;

    // The original still signs in.
    expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/login",
            Auth::None,
            &serde_json::json!({ "identifier": "cautious", "password": PASSWORD }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
}

/// One account's token must not be revocable by another, and the answer must not
/// reveal that the id exists.
#[tokio::test]
async fn a_token_cannot_be_revoked_by_someone_else() {
    let app = TestApp::ready().await;
    let owner = register(&app, "owner").await;
    let stranger = register(&app, "stranger").await;

    let created = body_json(
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/tokens",
                Auth::Cookie(&owner),
                &serde_json::json!({ "name": "private" }),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let id = created["id"].as_str().expect("an id").to_owned();

    expect_status(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/tokens/{id}"),
            Auth::Cookie(&stranger),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;

    // And the owner can still use it.
    let secret = created["secret"].as_str().expect("a secret").to_owned();
    expect_status(
        app.empty(Method::GET, "/api/v1/auth/session", Auth::Bearer(&secret))
            .await,
        StatusCode::OK,
    )
    .await;
}

/// A listing is per-account, not global.
#[tokio::test]
async fn a_token_listing_shows_only_the_callers_own() {
    let app = TestApp::ready().await;
    let first = register(&app, "first").await;
    let second = register(&app, "second").await;

    for (cookie, name) in [(&first, "first-token"), (&second, "second-token")] {
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/tokens",
                Auth::Cookie(cookie),
                &serde_json::json!({ "name": name }),
            )
            .await,
            StatusCode::CREATED,
        )
        .await;
    }

    let listed = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/tokens", Auth::Cookie(&first))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    let names: Vec<_> = listed
        .as_array()
        .expect("an array")
        .iter()
        .map(|token| token["name"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(names, vec!["first-token"]);
}

#[tokio::test]
async fn a_nonsense_token_id_is_not_found_rather_than_a_bad_request() {
    let app = TestApp::ready().await;
    let cookie = register(&app, "curious").await;

    // Telling an unparsable id apart from somebody else's would confirm which
    // identifiers exist.
    expect_status(
        app.empty(
            Method::DELETE,
            "/api/v1/tokens/not-an-identifier",
            Auth::Cookie(&cookie),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
}

/// Guessing must get expensive, and the response must say for how long.
#[tokio::test]
async fn repeated_failures_are_throttled_with_a_retry_after() {
    let app = TestApp::ready().await;
    register(&app, "guessable").await;

    let mut throttled = None;
    for _ in 0..12 {
        let response = app
            .json(
                Method::POST,
                "/api/v1/auth/login",
                Auth::None,
                &serde_json::json!({ "identifier": "guessable", "password": "wrong" }),
            )
            .await;
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            throttled = Some(response);
            break;
        }
    }

    let response = throttled.expect("repeated failures should eventually be refused");
    let retry_after = response
        .headers()
        .get(header::RETRY_AFTER)
        .expect("a Retry-After header")
        .to_str()
        .expect("ASCII");
    assert!(
        retry_after.parse::<u64>().is_ok_and(|seconds| seconds > 0),
        "Retry-After should be a positive number of seconds, got `{retry_after}`"
    );
}

/// An expired session must stop working without anybody having to sweep it.
#[tokio::test]
async fn an_expired_session_is_refused() {
    let app = TestApp::with_config(|config| {
        // The shortest a session can be configured to last. Expiry is then forced
        // by writing the row's timestamp into the past rather than by waiting.
        config.auth.session_ttl_hours = 1;
    })
    .await;
    let cookie = register(&app, "shortlived").await;

    // Moved into the past directly. The repository has no "set expiry" call, and
    // adding one only for a test would put a footgun in the production API.
    // SQLite-only SQL, which is what this binary runs against.
    let past = time::OffsetDateTime::now_utc() - time::Duration::hours(2);
    let statement = sea_orm::Statement::from_string(
        sea_orm::ConnectionTrait::get_database_backend(&app.state.db),
        format!(
            "UPDATE sessions SET expires_at = '{}'",
            past.format(&time::format_description::well_known::Rfc3339)
                .expect("formattable")
        ),
    );
    sea_orm::ConnectionTrait::execute_raw(&app.state.db, statement)
        .await
        .expect("expire the session");

    expect_status(
        app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&cookie))
            .await,
        StatusCode::UNAUTHORIZED,
    )
    .await;
}

/// The account every instance starts with must be able to sign in.
///
/// It is called `admin` by default, which is on the reserved list — so the
/// lookup path has to accept a name the registration path refuses. This is the
/// regression test for getting that backwards, which locks every operator out of
/// their own instance on first run.
#[tokio::test]
async fn the_bootstrapped_administrator_can_sign_in() {
    let app = TestApp::with_config(|config| {
        config.auth.admin_password = PASSWORD.to_owned();
    })
    .await;

    tessera_server::auth::bootstrap::ensure_admin(&app.state.users, &app.state.config.auth)
        .await
        .expect("bootstrap");

    let body = body_json(
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/auth/login",
                Auth::None,
                &serde_json::json!({ "identifier": "admin", "password": PASSWORD }),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(body["username"], "admin");
    assert_eq!(body["is_admin"], true);
    // A password the operator chose is one they already know, so there is
    // nothing to force a change of.
    assert_eq!(body["must_change_password"], false);

    // And the name is still unavailable to everyone else.
    expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &registration("admin"),
        )
        .await,
        StatusCode::BAD_REQUEST,
    )
    .await;
}

/// A generated password is printed once, so the account it belongs to must be
/// told to replace it.
#[tokio::test]
async fn a_generated_administrator_password_must_be_changed() {
    let app = TestApp::ready().await;
    tessera_server::auth::bootstrap::ensure_admin(&app.state.users, &app.state.config.auth)
        .await
        .expect("bootstrap");

    // Running again must not create a second administrator or reset the first.
    tessera_server::auth::bootstrap::ensure_admin(&app.state.users, &app.state.config.auth)
        .await
        .expect("idempotent");

    let name = tessera_core::Username::parse_privileged("admin").expect("a valid name");
    let admin = app
        .state
        .users
        .find_by_username(&name)
        .await
        .expect("query")
        .expect("the administrator should exist");

    assert!(admin.is_admin);
    assert!(
        admin.must_change_password,
        "a password nobody chose has been through a log file and must be replaced"
    );
}

/// A flag the server reports but does not enforce is a request that the client
/// please behave — and the account carrying it administers the instance.
#[tokio::test]
async fn an_account_owing_a_password_change_can_only_fix_itself() {
    // A configured password, then the flag set by hand. The bootstrap only sets
    // the flag for a password it generated, and that password exists nowhere a
    // test could read it — which is exactly why the flag has to be enforced.
    let app = TestApp::with_config(|config| {
        config.auth.admin_password = PASSWORD.to_owned();
    })
    .await;
    tessera_server::auth::bootstrap::ensure_admin(&app.state.users, &app.state.config.auth)
        .await
        .expect("bootstrap");
    force_password_change(&app, "admin").await;

    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/login",
            Auth::None,
            &serde_json::json!({ "identifier": "admin", "password": PASSWORD }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let cookie = session_cookie(&response);

    // Reading its own identity still works — a client has to be able to see why
    // it is being refused.
    let body = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&cookie))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(body["must_change_password"], true);

    // Everything else is refused, with a code the client can branch on.
    let refused = expect_status(
        app.json(
            Method::POST,
            "/api/v1/tokens",
            Auth::Cookie(&cookie),
            &serde_json::json!({ "name": "too soon" }),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_eq!(body_json(refused).await["code"], "password_change_required");
    expect_status(
        app.empty(Method::GET, "/api/v1/tokens", Auth::Cookie(&cookie))
            .await,
        StatusCode::FORBIDDEN,
    )
    .await;

    // Changing the password lifts it, in the same request.
    expect_status(
        app.json(
            Method::PATCH,
            "/api/v1/auth/password",
            Auth::Cookie(&cookie),
            &serde_json::json!({
                "current_password": PASSWORD,
                "new_password": "an entirely different passphrase",
            }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    // The change issued a fresh session; sign in again to pick one up.
    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/login",
            Auth::None,
            &serde_json::json!({
                "identifier": "admin",
                "password": "an entirely different passphrase",
            }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let cookie = session_cookie(&response);
    expect_status(
        app.json(
            Method::POST,
            "/api/v1/tokens",
            Auth::Cookie(&cookie),
            &serde_json::json!({ "name": "now allowed" }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
}

/// Sessions nobody comes back for are only removed by the sweep — without it the
/// table grows for as long as the instance lives.
#[tokio::test]
async fn the_sweep_removes_sessions_nobody_returns_for() {
    let app = TestApp::ready().await;
    register(&app, "abandoner").await;

    let past = time::OffsetDateTime::now_utc() - time::Duration::hours(1);
    let statement = sea_orm::Statement::from_string(
        sea_orm::ConnectionTrait::get_database_backend(&app.state.db),
        format!(
            "UPDATE sessions SET expires_at = '{}'",
            past.format(&time::format_description::well_known::Rfc3339)
                .expect("formattable")
        ),
    );
    sea_orm::ConnectionTrait::execute_raw(&app.state.db, statement)
        .await
        .expect("expire the session");

    let removed = app
        .state
        .sessions
        .delete_expired(time::OffsetDateTime::now_utc())
        .await
        .expect("sweep");
    assert_eq!(removed, 1);

    // And a live one is left alone.
    register(&app, "stillhere").await;
    let removed = app
        .state
        .sessions
        .delete_expired(time::OffsetDateTime::now_utc())
        .await
        .expect("sweep");
    assert_eq!(removed, 0);
}
