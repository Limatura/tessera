//! Accounts, sessions and API tokens, over HTTP.
//!
//! The whole lifecycle lives in [`common::account_lifecycle`] rather than here,
//! because the cross-backend suite has to run exactly the same scenario. What is
//! left in this file is the behaviour that does not depend on the dialect:
//! configuration switches, credential separation and the things that must *not*
//! work.

mod common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
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

    tessera_server::auth::bootstrap::ensure_admin(
        &app.state.users,
        &app.state.roles,
        &app.state.config().auth,
    )
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
    tessera_server::auth::bootstrap::ensure_admin(
        &app.state.users,
        &app.state.roles,
        &app.state.config().auth,
    )
    .await
    .expect("bootstrap");

    // Running again must not create a second administrator or reset the first.
    tessera_server::auth::bootstrap::ensure_admin(
        &app.state.users,
        &app.state.roles,
        &app.state.config().auth,
    )
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

    // Over the roles, not a column: `users.is_admin` is gone, and what
    // decides whether somebody administers the instance is `roles:write`.
    let roles = app.state.roles.of_user(admin.id).await.expect("roles");
    assert!(
        roles.iter().any(|role| role
            .permissions
            .allows(tessera_core::Permission::RolesWrite)),
        "the bootstrapped administrator should hold the keys"
    );
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
    tessera_server::auth::bootstrap::ensure_admin(
        &app.state.users,
        &app.state.roles,
        &app.state.config().auth,
    )
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

/// The row goes only once it is older than the history is kept for. That is the
/// same query the expiry sweep used to be, with a different bound.
#[tokio::test]
async fn the_sweep_removes_sessions_older_than_the_history_window() {
    let app = TestApp::ready().await;
    register(&app, "abandoner").await;

    let now = time::OffsetDateTime::now_utc();
    assert_eq!(
        app.state
            .sessions
            .delete_ended_before(now - time::Duration::days(90))
            .await
            .expect("sweep"),
        0,
        "a session that is still live must survive the retention sweep"
    );

    let statement = sea_orm::Statement::from_string(
        sea_orm::ConnectionTrait::get_database_backend(&app.state.db),
        format!(
            "UPDATE sessions SET expires_at = '{}'",
            (now - time::Duration::days(200))
                .format(&time::format_description::well_known::Rfc3339)
                .expect("formattable")
        ),
    );
    sea_orm::ConnectionTrait::execute_raw(&app.state.db, statement)
        .await
        .expect("age the session");

    assert_eq!(
        app.state
            .sessions
            .delete_ended_before(now - time::Duration::days(90))
            .await
            .expect("sweep"),
        1
    );
}

// ── password reset ──────────────────────────────────────────────────

/// An instance with a relay configured.
///
/// The host points nowhere on purpose: every test here asserts on what the
/// *API* does, and sending must not be what decides that. A relay that cannot
/// be reached is also the honest case to pin — the response has to be identical
/// whether the mail went out or not.
async fn app_with_mail() -> TestApp {
    TestApp::with_config(|config| {
        "smtp.invalid".clone_into(&mut config.mail.host);
        "tessera@example.com".clone_into(&mut config.mail.from);
        "https://tessera.example.com".clone_into(&mut config.mail.public_url);
    })
    .await
}

async fn ask_for_reset(app: &TestApp, email: &str) -> StatusCode {
    app.json(
        Method::POST,
        "/api/v1/auth/password-reset",
        Auth::None,
        &serde_json::json!({ "email": email }),
    )
    .await
    .status()
}

/// The property the whole endpoint exists to have: the response says nothing
/// about whether the address belongs to anybody.
///
/// Anything else turns this into "does this person have an account here",
/// answerable by anyone at the rate the throttle allows.
#[tokio::test]
async fn a_reset_request_answers_the_same_for_strangers_and_members() {
    let app = app_with_mail().await;
    register_verified(&app, "member").await;

    assert_eq!(
        ask_for_reset(&app, "member@example.com").await,
        StatusCode::ACCEPTED
    );
    assert_eq!(
        ask_for_reset(&app, "nobody@example.com").await,
        StatusCode::ACCEPTED,
        "an address with no account must be indistinguishable from one with"
    );
    assert_eq!(
        ask_for_reset(&app, "not an address at all").await,
        StatusCode::ACCEPTED,
        "even a malformed address, or that is one more way the answers differ"
    );
}

/// The relay here is unreachable, so this also pins that a send failure does not
/// change the answer — otherwise a broken relay would leak membership.
#[tokio::test]
async fn a_failed_send_is_still_accepted() {
    let app = app_with_mail().await;
    register_verified(&app, "member").await;
    assert_eq!(
        ask_for_reset(&app, "member@example.com").await,
        StatusCode::ACCEPTED
    );
}

/// Without a relay the endpoint is mounted but answers 404, the same as every
/// other switched-off feature. `/capabilities` is what a client reads.
#[tokio::test]
async fn an_instance_without_mail_offers_no_reset() {
    let app = TestApp::ready().await;
    assert_eq!(
        ask_for_reset(&app, "someone@example.com").await,
        StatusCode::NOT_FOUND
    );

    let body =
        body_json(expect_status(app.get("/api/v1/capabilities").await, StatusCode::OK).await).await;
    assert_eq!(body["password_reset"], false);
}

#[tokio::test]
async fn an_instance_with_mail_says_so() {
    let app = app_with_mail().await;
    let body =
        body_json(expect_status(app.get("/api/v1/capabilities").await, StatusCode::OK).await).await;
    assert_eq!(body["password_reset"], true);
}

/// The regression this pins was live and CI was green for it: the throttle used
/// to count only when a message was actually sent, so the fourth request
/// answered `429` for a member and `202` for a stranger — four calls to `curl`
/// and you knew who had an account here. Exactly the question the `202` exists
/// to refuse.
#[tokio::test]
async fn being_throttled_says_nothing_about_membership() {
    let app = app_with_mail().await;
    register_verified(&app, "member").await;

    // Past the limit on both, one of which has no account behind it.
    for _ in 0..4 {
        ask_for_reset(&app, "member@example.com").await;
        ask_for_reset(&app, "ghost@example.com").await;
    }

    assert_eq!(
        ask_for_reset(&app, "member@example.com").await,
        ask_for_reset(&app, "ghost@example.com").await,
        "a refusal must look the same whether or not the address is known"
    );
    assert_eq!(
        ask_for_reset(&app, "member@example.com").await,
        StatusCode::TOO_MANY_REQUESTS,
        "and both must actually be refused, or the limit does nothing"
    );
}

/// Keyed on the address, so one inbox cannot be buried.
#[tokio::test]
async fn reset_requests_are_limited_per_address() {
    let app = app_with_mail().await;
    register_verified(&app, "member").await;

    for _ in 0..3 {
        assert_eq!(
            ask_for_reset(&app, "member@example.com").await,
            StatusCode::ACCEPTED
        );
    }
    assert_eq!(
        ask_for_reset(&app, "member@example.com").await,
        StatusCode::TOO_MANY_REQUESTS
    );

    // A different address is unaffected, which is what keying on the address
    // means.
    register_verified(&app, "other").await;
    assert_eq!(
        ask_for_reset(&app, "other@example.com").await,
        StatusCode::ACCEPTED
    );
}

/// One address being at its limit must not affect another, which is what
/// keying on the address means.
#[tokio::test]
async fn one_address_at_its_limit_does_not_refuse_another() {
    let app = app_with_mail().await;
    register_verified(&app, "member").await;
    register_verified(&app, "other").await;

    for _ in 0..4 {
        ask_for_reset(&app, "member@example.com").await;
    }
    assert_eq!(
        ask_for_reset(&app, "member@example.com").await,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        ask_for_reset(&app, "other@example.com").await,
        StatusCode::ACCEPTED
    );
}

async fn confirm(app: &TestApp, token: &str, password: &str) -> StatusCode {
    app.json(
        Method::POST,
        "/api/v1/auth/password-reset/confirm",
        Auth::None,
        &serde_json::json!({ "token": token, "password": password }),
    )
    .await
    .status()
}

/// Unknown, spent and expired all answer the same way. They differ to us — which
/// is why the row survives being used — but saying which would tell somebody
/// holding a spent link that it was once real, and that is a fact about
/// another person's account.
#[tokio::test]
async fn an_unusable_link_says_only_that() {
    let app = app_with_mail().await;
    assert_eq!(
        confirm(&app, "not a real token", PASSWORD).await,
        StatusCode::BAD_REQUEST
    );
}

/// The end-to-end path, driven through the repository because the token only
/// ever exists in the email otherwise — which is the point of storing a digest.
#[tokio::test]
async fn a_link_sets_the_password_once_and_ends_every_session() {
    let app = app_with_mail().await;
    let cookie = register_verified(&app, "member").await;

    let user = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("member").expect("a name"))
        .await
        .expect("query")
        .expect("the account");

    let now = time::OffsetDateTime::now_utc();
    let secret = app
        .state
        .resets
        .issue(
            user.id,
            now,
            now.saturating_add(time::Duration::minutes(30)),
        )
        .await
        .expect("issue");
    let token = secret.expose();

    let new_password = "an entirely different passphrase";
    assert_eq!(
        confirm(&app, token, new_password).await,
        StatusCode::NO_CONTENT
    );

    // The session that existed before is gone: asking for a reset is usually
    // somebody saying they have lost control of the account.
    assert_eq!(
        app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&cookie))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );

    // The link is spent.
    assert_eq!(
        confirm(&app, token, "yet another passphrase").await,
        StatusCode::BAD_REQUEST
    );

    // And the new password is the one that works.
    expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/login",
            Auth::None,
            &serde_json::json!({ "identifier": "member", "password": new_password }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
}

/// A password that fails the policy must leave the link usable — otherwise a
/// typo costs somebody the whole round trip through their inbox.
#[tokio::test]
async fn a_rejected_password_does_not_burn_the_link() {
    let app = app_with_mail().await;
    register_verified(&app, "member").await;

    let user = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("member").expect("a name"))
        .await
        .expect("query")
        .expect("the account");
    let now = time::OffsetDateTime::now_utc();
    let secret = app
        .state
        .resets
        .issue(
            user.id,
            now,
            now.saturating_add(time::Duration::minutes(30)),
        )
        .await
        .expect("issue");
    let token = secret.expose();

    assert_eq!(confirm(&app, token, "short").await, StatusCode::BAD_REQUEST);
    assert_eq!(
        confirm(&app, token, "a perfectly fine replacement").await,
        StatusCode::NO_CONTENT,
        "the link must survive a password the policy refused"
    );
}

/// A second outstanding link is a key somebody may still be holding, and the
/// usual reason there are two is that the first mail went astray.
#[tokio::test]
async fn using_one_link_invalidates_the_others() {
    let app = app_with_mail().await;
    register_verified(&app, "member").await;

    let user = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("member").expect("a name"))
        .await
        .expect("query")
        .expect("the account");
    let now = time::OffsetDateTime::now_utc();
    let expires = now.saturating_add(time::Duration::minutes(30));

    let first = app
        .state
        .resets
        .issue(user.id, now, expires)
        .await
        .expect("issue");
    let second = app
        .state
        .resets
        .issue(user.id, now, expires)
        .await
        .expect("issue");

    assert_eq!(
        confirm(&app, second.expose(), "a perfectly fine replacement").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        confirm(&app, first.expose(), "something else entirely here").await,
        StatusCode::BAD_REQUEST,
        "the older link must stop working too"
    );
}

/// An expired link is refused, and says the same thing as an unknown one.
#[tokio::test]
async fn an_expired_link_is_refused() {
    let app = app_with_mail().await;
    register_verified(&app, "member").await;

    let user = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("member").expect("a name"))
        .await
        .expect("query")
        .expect("the account");
    let now = time::OffsetDateTime::now_utc();
    let secret = app
        .state
        .resets
        .issue(
            user.id,
            now - time::Duration::hours(2),
            now - time::Duration::hours(1),
        )
        .await
        .expect("issue");

    assert_eq!(
        confirm(&app, secret.expose(), "a perfectly fine replacement").await,
        StatusCode::BAD_REQUEST
    );
}

// ── registration that waits on the address ──────────────────────────

/// Asks for an account and gets no further than the `202`.
async fn ask_to_register(app: &TestApp, username: &str, email: &str) -> StatusCode {
    app.json(
        Method::POST,
        "/api/v1/auth/register",
        Auth::None,
        &serde_json::json!({
            "username": username,
            "email": email,
            "password": PASSWORD,
        }),
    )
    .await
    .status()
}

async fn confirm_registration(app: &TestApp, token: &str) -> axum::response::Response {
    app.json(
        Method::POST,
        "/api/v1/auth/register/confirm",
        Auth::None,
        &serde_json::json!({ "token": token }),
    )
    .await
}

/// Waits for the row the handler writes after it has answered.
///
/// Everything that depends on whether the address is known happens off the
/// request path on purpose — hashing a password takes about a tenth of a second
/// and a taken address skips it, which is a membership oracle anyone can read
/// with a stopwatch. The cost of that decision is paid here: a test cannot look
/// straight after the response and expect the row to be there.
// `clippy.toml`'s test exemptions only reach code inside a `#[test]` body, and
// this is a helper. Failing is the only thing a fixture that cannot find what it
// is waiting for has to report.
#[expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test fixture; a panic is how it reports"
)]
async fn wait_for_pending(
    app: &TestApp,
    email: &str,
) -> tessera_server::db::repository::pending_registration::PendingRegistration {
    let address = tessera_core::EmailAddress::parse(email).expect("an address");
    for _ in 0..600 {
        if let Some(pending) = app
            .state
            .pending
            .find_by_email(&address)
            .await
            .expect("query")
        {
            return pending;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("no registration was recorded for {email} within six seconds");
}

/// Registers and follows the link, for tests that need an account to exist on an
/// instance that sends mail.
///
/// The link is re-minted from the recorded row rather than read out of the
/// message, because the token exists nowhere else — that is what storing only a
/// digest buys. What is asserted along the way is that the handler recorded the
/// registration it was asked for, so the only part not covered here is the two
/// lines that put the secret into the letter.
#[expect(
    clippy::expect_used,
    reason = "test fixture; a panic is how it reports"
)]
async fn register_verified(app: &TestApp, username: &str) -> String {
    let email = format!("{username}@example.com");
    assert_eq!(
        ask_to_register(app, username, &email).await,
        StatusCode::ACCEPTED
    );

    let pending = wait_for_pending(app, &email).await;
    assert_eq!(pending.username.as_str(), username);

    let now = time::OffsetDateTime::now_utc();
    let secret = app
        .state
        .pending
        .start(
            tessera_server::db::repository::pending_registration::Waiting {
                username: &pending.username,
                email: &pending.email,
                password_hash: pending.password_hash.clone(),
                invite_id: None,
            },
            now,
            now.saturating_add(time::Duration::hours(24)),
        )
        .await
        .expect("issue");

    let response = expect_status(
        confirm_registration(app, secret.expose()).await,
        StatusCode::CREATED,
    )
    .await;
    session_cookie(&response)
}

/// The property the `202` exists to have. A registration form that says "that
/// address is taken" is the same membership question `POST /auth/password-reset`
/// refuses to answer, reachable through a different door — and this one needs no
/// account and no code, just the form every visitor can see.
#[tokio::test]
async fn registering_answers_the_same_for_a_free_and_a_taken_address() {
    let app = app_with_mail().await;
    register_verified(&app, "member").await;

    assert_eq!(
        ask_to_register(&app, "someone", "member@example.com").await,
        StatusCode::ACCEPTED
    );
    assert_eq!(
        ask_to_register(&app, "someone", "nobody@example.com").await,
        StatusCode::ACCEPTED,
        "an address with no account must be indistinguishable from one with"
    );
}

/// Being refused must not answer it either — the same hole that was live in the
/// reset handler, where the limit was only ever reached by addresses that had an
/// account behind them.
#[tokio::test]
async fn being_throttled_says_nothing_about_membership_either() {
    let app = app_with_mail().await;
    register_verified(&app, "member").await;

    for attempt in 0..4 {
        ask_to_register(&app, &format!("probe-a{attempt}"), "member@example.com").await;
        ask_to_register(&app, &format!("probe-b{attempt}"), "ghost@example.com").await;
    }

    assert_eq!(
        ask_to_register(&app, "probe-x", "member@example.com").await,
        ask_to_register(&app, "probe-y", "ghost@example.com").await
    );
    assert_eq!(
        ask_to_register(&app, "probe-x", "member@example.com").await,
        StatusCode::TOO_MANY_REQUESTS,
        "and both must actually be refused, or the limit does nothing"
    );
}

/// Registration must not spend the reset budget. If it did, typing somebody's
/// address into the registration form three times would take away their way back
/// into the account — a worse outcome than the six messages an hour that two
/// counters allow.
#[tokio::test]
async fn registering_at_an_address_does_not_use_up_its_resets() {
    let app = app_with_mail().await;
    register_verified(&app, "member").await;

    for attempt in 0..5 {
        ask_to_register(&app, &format!("probe-a{attempt}"), "member@example.com").await;
    }
    assert_eq!(
        ask_to_register(&app, "probe-z", "member@example.com").await,
        StatusCode::TOO_MANY_REQUESTS
    );

    assert_eq!(
        ask_for_reset(&app, "member@example.com").await,
        StatusCode::ACCEPTED,
        "the account holder must still be able to ask for a reset"
    );
}

/// A name is answered plainly where an address is not, and the asymmetry is
/// deliberate: a name is already half public — a share page names whoever
/// uploaded it — and a form that cannot say "try another one" is a form nobody
/// can complete.
#[tokio::test]
async fn a_taken_name_is_refused_outright() {
    let app = app_with_mail().await;
    register_verified(&app, "member").await;

    assert_eq!(
        ask_to_register(&app, "member", "somebody-else@example.com").await,
        StatusCode::CONFLICT
    );
}

/// The point of the whole change: until the link is followed there is nothing to
/// sign in to, so an address somebody does not control never becomes an account.
#[tokio::test]
async fn nothing_exists_until_the_link_is_followed() {
    let app = app_with_mail().await;
    assert_eq!(
        ask_to_register(&app, "member", "member@example.com").await,
        StatusCode::ACCEPTED
    );
    wait_for_pending(&app, "member@example.com").await;

    assert!(
        app.state
            .users
            .find_by_username(&tessera_core::Username::parse("member").expect("a name"))
            .await
            .expect("query")
            .is_none(),
        "the account must not exist yet"
    );
    assert_eq!(
        app.json(
            Method::POST,
            "/api/v1/auth/login",
            Auth::None,
            &serde_json::json!({ "identifier": "member", "password": PASSWORD }),
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED,
        "and the password chosen for it must not work"
    );

    // And the name is still free, because nothing was reserved: reserving it
    // would make "register and never confirm" a way to lock every good name
    // away, with nothing an operator could do about it.
    assert_eq!(
        ask_to_register(&app, "member", "another@example.com").await,
        StatusCode::ACCEPTED
    );
}

/// Following the link is what creates the account, and it arrives verified —
/// that is the whole of what was waited for.
#[tokio::test]
async fn the_link_creates_a_verified_account_and_signs_it_in() {
    let app = app_with_mail().await;
    let cookie = register_verified(&app, "member").await;

    let user = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("member").expect("a name"))
        .await
        .expect("query")
        .expect("the account");
    assert!(user.email_verified_at.is_some(), "it was just confirmed");
    let roles = app.state.roles.of_user(user.id).await.expect("roles");
    assert!(
        roles.iter().any(|role| role
            .permissions
            .allows(tessera_core::Permission::RolesWrite)),
        "the first account to exist administers it"
    );

    expect_status(
        app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&cookie))
            .await,
        StatusCode::OK,
    )
    .await;
}

/// Unknown, spent and expired all answer the same way, for the same reason the
/// reset links do: saying which would tell somebody holding a stale link that it
/// was once real.
#[tokio::test]
async fn an_unusable_registration_link_says_only_that() {
    let app = app_with_mail().await;
    assert_eq!(
        confirm_registration(&app, "not a real token")
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );

    let now = time::OffsetDateTime::now_utc();
    let secret = app
        .state
        .pending
        .start(
            tessera_server::db::repository::pending_registration::Waiting {
                username: &tessera_core::Username::parse("member").expect("a name"),
                email: &tessera_core::EmailAddress::parse("member@example.com")
                    .expect("an address"),
                password_hash: "not a real hash".to_owned(),
                invite_id: None,
            },
            now - time::Duration::hours(48),
            now - time::Duration::hours(24),
        )
        .await
        .expect("issue");

    assert_eq!(
        confirm_registration(&app, secret.expose()).await.status(),
        StatusCode::BAD_REQUEST,
        "an expired link must say the same as an unknown one"
    );
}

/// The row is deleted when it becomes an account, so the link cannot be replayed
/// into a second one.
#[tokio::test]
async fn a_registration_link_works_once() {
    let app = app_with_mail().await;
    let now = time::OffsetDateTime::now_utc();
    let secret = app
        .state
        .pending
        .start(
            tessera_server::db::repository::pending_registration::Waiting {
                username: &tessera_core::Username::parse("member").expect("a name"),
                email: &tessera_core::EmailAddress::parse("member@example.com")
                    .expect("an address"),
                password_hash: tessera_server::auth::password::hash(
                    tessera_core::Password::parse(
                        PASSWORD.to_owned(),
                        tessera_core::PasswordPolicy::default(),
                    )
                    .expect("a password"),
                    tessera_server::config::HashingConfig::default(),
                )
                .await
                .expect("hash"),
                invite_id: None,
            },
            now,
            now.saturating_add(time::Duration::hours(24)),
        )
        .await
        .expect("issue");

    assert_eq!(
        confirm_registration(&app, secret.expose()).await.status(),
        StatusCode::CREATED
    );
    assert_eq!(
        confirm_registration(&app, secret.expose()).await.status(),
        StatusCode::BAD_REQUEST
    );
}

/// Asking again replaces the link rather than adding one. Two live links in one
/// mailbox means the older keeps working after the newer has been used, and
/// there is no reason for that to be possible.
#[tokio::test]
async fn asking_again_replaces_the_earlier_link() {
    let app = app_with_mail().await;
    let now = time::OffsetDateTime::now_utc();
    let address = tessera_core::EmailAddress::parse("member@example.com").expect("an address");
    let name = tessera_core::Username::parse("member").expect("a name");
    let expires = now.saturating_add(time::Duration::hours(24));

    let first = app
        .state
        .pending
        .start(
            tessera_server::db::repository::pending_registration::Waiting {
                username: &name,
                email: &address,
                password_hash: "first".to_owned(),
                invite_id: None,
            },
            now,
            expires,
        )
        .await
        .expect("issue");
    let second = app
        .state
        .pending
        .start(
            tessera_server::db::repository::pending_registration::Waiting {
                username: &name,
                email: &address,
                password_hash: "second".to_owned(),
                invite_id: None,
            },
            now,
            expires,
        )
        .await
        .expect("issue");

    assert_eq!(
        confirm_registration(&app, first.expose()).await.status(),
        StatusCode::BAD_REQUEST,
        "the earlier link must have stopped working"
    );
    assert_eq!(
        confirm_registration(&app, second.expose()).await.status(),
        StatusCode::CREATED
    );
}

/// Nothing was reserved while the message sat in an inbox, so the name is
/// checked against today's accounts. Somebody can be told at this point that it
/// went to another person — rare, recoverable, and a much smaller problem than
/// a registration form that hands out permanent holds on names.
#[tokio::test]
async fn a_name_taken_while_waiting_is_refused_at_confirmation() {
    let app = app_with_mail().await;
    let now = time::OffsetDateTime::now_utc();
    let secret = app
        .state
        .pending
        .start(
            tessera_server::db::repository::pending_registration::Waiting {
                username: &tessera_core::Username::parse("member").expect("a name"),
                email: &tessera_core::EmailAddress::parse("first@example.com").expect("an address"),
                password_hash: "a hash".to_owned(),
                invite_id: None,
            },
            now,
            now.saturating_add(time::Duration::hours(24)),
        )
        .await
        .expect("issue");

    register_verified(&app, "member").await;

    assert_eq!(
        confirm_registration(&app, secret.expose()).await.status(),
        StatusCode::CONFLICT
    );
}

/// An address that gained an account while the link was waiting is answered like
/// an unusable link rather than as a conflict. The address is not this caller's
/// to be told about — and whoever they are, they have a working password for it
/// either way.
#[tokio::test]
async fn an_address_that_gained_an_account_while_waiting_is_refused() {
    let app = app_with_mail().await;
    // The account first: the pending row is keyed on the address, so seeding it
    // before the registration that uses the same one would just be overwritten.
    register_verified(&app, "member").await;

    let now = time::OffsetDateTime::now_utc();
    let secret = app
        .state
        .pending
        .start(
            tessera_server::db::repository::pending_registration::Waiting {
                username: &tessera_core::Username::parse("second").expect("a name"),
                email: &tessera_core::EmailAddress::parse("member@example.com")
                    .expect("an address"),
                password_hash: "a hash".to_owned(),
                invite_id: None,
            },
            now,
            now.saturating_add(time::Duration::hours(24)),
        )
        .await
        .expect("issue");

    assert_eq!(
        confirm_registration(&app, secret.expose()).await.status(),
        StatusCode::BAD_REQUEST
    );
}

/// Without a relay there is nothing to verify with, so registration is what it
/// always was. This is the ordinary case for the instance this was built for:
/// one person on a NAS, who is also the one who set the address.
#[tokio::test]
async fn an_instance_without_mail_creates_the_account_outright() {
    let app = TestApp::ready().await;
    register(&app, "member").await;

    assert_eq!(
        ask_to_register(&app, "second", "member@example.com").await,
        StatusCode::CONFLICT,
        "with nothing to verify against, a taken address is said plainly"
    );
    assert_eq!(
        confirm_registration(&app, "any token at all")
            .await
            .status(),
        StatusCode::NOT_FOUND,
        "and there is no link to confirm"
    );
}

/// Which of the two an instance does is not something a client can guess from a
/// `201`, so it is declared. A client that reads this wrong shows "welcome" to
/// somebody who has to go and read their mail first.
#[tokio::test]
async fn capabilities_says_whether_registration_waits_on_the_address() {
    let plain = TestApp::ready().await;
    let body =
        body_json(expect_status(plain.get("/api/v1/capabilities").await, StatusCode::OK).await)
            .await;
    assert_eq!(body["registration_requires_confirmation"], false);

    let with_mail = app_with_mail().await;
    let body =
        body_json(expect_status(with_mail.get("/api/v1/capabilities").await, StatusCode::OK).await)
            .await;
    assert_eq!(body["registration_requires_confirmation"], true);
}

/// Closed registration is refused before any of this, so a closed instance is
/// not a way to make it send mail.
#[tokio::test]
async fn closed_registration_sends_nothing() {
    let app = TestApp::with_config(|config| {
        "smtp.invalid".clone_into(&mut config.mail.host);
        "tessera@example.com".clone_into(&mut config.mail.from);
        "https://tessera.example.com".clone_into(&mut config.mail.public_url);
        config.auth.registration = RegistrationPolicy::Closed;
    })
    .await;

    assert_eq!(
        ask_to_register(&app, "member", "member@example.com").await,
        StatusCode::FORBIDDEN
    );
}

/// A registration nobody confirmed is swept, so an abandoned attempt does not
/// hold an address against a later one for ever.
#[tokio::test]
async fn abandoned_registrations_are_swept() {
    let app = app_with_mail().await;
    let now = time::OffsetDateTime::now_utc();
    app.state
        .pending
        .start(
            tessera_server::db::repository::pending_registration::Waiting {
                username: &tessera_core::Username::parse("member").expect("a name"),
                email: &tessera_core::EmailAddress::parse("member@example.com")
                    .expect("an address"),
                password_hash: "a hash".to_owned(),
                invite_id: None,
            },
            now - time::Duration::hours(48),
            now - time::Duration::hours(24),
        )
        .await
        .expect("issue");

    assert_eq!(
        app.state.pending.delete_expired(now).await.expect("sweep"),
        1
    );
}

/// The limit protects a mailbox, not an identity. `Alice@` and `alice@` stay
/// separate accounts on purpose — the local part is case-sensitive by the
/// standard — but every mail provider delivers both to the same inbox, so
/// counting them apart made the limit walkable by changing case.
///
/// Measured against a running instance before this was fixed: thirty-two
/// attempts at eight spellings of a six-letter local part were answered
/// twenty-four times, against a stated limit of three. Every extra letter in the
/// local part doubles that.
#[tokio::test]
async fn spelling_an_address_differently_does_not_buy_more_messages() {
    let app = app_with_mail().await;

    let spellings = ["victim", "Victim", "vIctim", "VICTIM"];
    let mut accepted = 0;
    let mut attempt = 0;
    for spelling in spellings {
        for _ in 0..3 {
            attempt += 1;
            if ask_to_register(
                &app,
                &format!("probe{attempt:03}"),
                &format!("{spelling}@example.com"),
            )
            .await
                == StatusCode::ACCEPTED
            {
                accepted += 1;
            }
        }
    }

    assert_eq!(
        accepted, 3,
        "twelve attempts at four spellings of one mailbox must send three messages, not twelve"
    );
}

/// The same hole on the reset endpoint, which had it first.
#[tokio::test]
async fn spelling_an_address_differently_does_not_buy_more_reset_messages() {
    let app = app_with_mail().await;

    let mut accepted = 0;
    for spelling in ["victim", "Victim", "vIctim", "VICTIM"] {
        for _ in 0..3 {
            if ask_for_reset(&app, &format!("{spelling}@example.com")).await == StatusCode::ACCEPTED
            {
                accepted += 1;
            }
        }
    }

    assert_eq!(accepted, 3);
}

/// Closing registration has to mean no more accounts. Without this check it
/// would mean "no more accounts except from the links already sent" — a day of
/// them by default, which an operator cannot see, count or cancel. Somebody
/// closing registration is usually reacting to something.
#[tokio::test]
async fn closing_registration_stops_links_that_were_already_sent() {
    let app = TestApp::with_config(|config| {
        "smtp.invalid".clone_into(&mut config.mail.host);
        "tessera@example.com".clone_into(&mut config.mail.from);
        "https://tessera.example.com".clone_into(&mut config.mail.public_url);
        // The link went out while registration was open; this is the instance
        // as it stands when somebody opens it.
        config.auth.registration = RegistrationPolicy::Closed;
    })
    .await;

    let now = time::OffsetDateTime::now_utc();
    let secret = app
        .state
        .pending
        .start(
            tessera_server::db::repository::pending_registration::Waiting {
                username: &tessera_core::Username::parse("member").expect("a name"),
                email: &tessera_core::EmailAddress::parse("member@example.com")
                    .expect("an address"),
                password_hash: "a hash".to_owned(),
                invite_id: None,
            },
            now,
            now.saturating_add(time::Duration::hours(24)),
        )
        .await
        .expect("issue");

    assert_eq!(
        confirm_registration(&app, secret.expose()).await.status(),
        StatusCode::FORBIDDEN
    );
    assert!(
        app.state
            .users
            .find_by_username(&tessera_core::Username::parse("member").expect("a name"))
            .await
            .expect("query")
            .is_none(),
        "and no account came of it"
    );
}

/// Two links asking for the same name, opened at the same moment, both read a
/// free name before either writes. The unique index is what actually decides it,
/// and the loser must be told that rather than shown a server error.
#[tokio::test]
async fn two_links_racing_for_one_name_do_not_produce_a_server_error() {
    let app = app_with_mail().await;
    let now = time::OffsetDateTime::now_utc();
    let name = tessera_core::Username::parse("contested").expect("a name");
    let expires = now.saturating_add(time::Duration::hours(24));

    let mut tokens = Vec::new();
    for address in ["first@example.com", "second@example.com"] {
        let secret = app
            .state
            .pending
            .start(
                tessera_server::db::repository::pending_registration::Waiting {
                    username: &name,
                    email: &tessera_core::EmailAddress::parse(address).expect("an address"),
                    password_hash: "a hash".to_owned(),
                    invite_id: None,
                },
                now,
                expires,
            )
            .await
            .expect("issue");
        tokens.push(secret.expose().to_owned());
    }

    let (first, second) = tokio::join!(
        confirm_registration(&app, &tokens[0]),
        confirm_registration(&app, &tokens[1])
    );
    let mut statuses = [first.status(), second.status()];
    statuses.sort_unstable();

    assert_eq!(
        statuses,
        [StatusCode::CREATED, StatusCode::CONFLICT],
        "one wins the name and the other is told so; neither may be a 5xx"
    );
}

// ── what a session remembers about where it started ─────────────────

/// The row behind a session cookie.
#[expect(
    clippy::expect_used,
    reason = "test fixture; a panic is how it reports"
)]
async fn session_behind(
    app: &TestApp,
    cookie: &str,
) -> tessera_server::db::repository::session::Session {
    let value = cookie.split_once('=').expect("a name=value pair").1;
    app.state
        .sessions
        .find_by_secret(&tessera_server::auth::secret::Secret::from_client(
            value.to_owned(),
        ))
        .await
        .expect("query")
        .expect("the session")
}

/// The account behind a name.
#[expect(
    clippy::expect_used,
    reason = "test fixture; a panic is how it reports"
)]
async fn account_named(
    app: &TestApp,
    username: &str,
) -> tessera_server::db::repository::user::User {
    app.state
        .users
        .find_by_username(&tessera_core::Username::parse(username).expect("a name"))
        .await
        .expect("query")
        .expect("the account")
}

/// Signs in with a chosen `User-Agent`, the way a particular browser would.
async fn sign_in_as(app: &TestApp, username: &str, agent: &str) -> String {
    let response = expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::USER_AGENT, agent),
            Body::from(
                serde_json::json!({ "identifier": username, "password": PASSWORD }).to_string(),
            ),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    session_cookie(&response)
}

/// Without this, a list of sessions is five rows of timestamps and no way to
/// tell which one is not yours — which is the only question the list exists to
/// answer.
#[tokio::test]
async fn a_session_remembers_the_browser_that_started_it() {
    let app = TestApp::ready().await;
    register(&app, "member").await;

    let agent = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Safari/605.1.15";
    let cookie = sign_in_as(&app, "member", agent).await;

    assert_eq!(
        session_behind(&app, &cookie).await.created_user_agent,
        Some(agent.to_owned()),
        "stored as it arrived; turning it into `Safari on macOS` belongs to whatever draws the list"
    );
}

/// The design decision this table exists to hold: written once, never updated.
///
/// A row that followed the current request would show a stolen session wearing
/// the thief's latest address — or the owner's own, if they happened to use it
/// last. What identifies a session is where it began.
#[tokio::test]
async fn later_requests_do_not_rewrite_where_a_session_began() {
    let app = TestApp::ready().await;
    register(&app, "member").await;

    let cookie = sign_in_as(&app, "member", "the browser that signed in").await;

    // The same session, presented by something claiming to be something else.
    expect_status(
        app.send(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/auth/session")
                .header(header::COOKIE, &cookie)
                .header(header::USER_AGENT, "somebody else entirely"),
            Body::empty(),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    assert_eq!(
        session_behind(&app, &cookie).await.created_user_agent,
        Some("the browser that signed in".to_owned())
    );
}

/// `unknown` is what a throttle key says when there is no address to be had. A
/// person reading a list of their sessions would take that word for a place, so
/// nothing is stored instead.
#[tokio::test]
async fn an_address_we_cannot_place_is_stored_as_nothing() {
    let app = TestApp::ready().await;
    register(&app, "member").await;

    // The test harness calls the router directly, so there is no peer address —
    // exactly the case this is about.
    let cookie = sign_in_as(&app, "member", "a browser").await;
    assert_eq!(session_behind(&app, &cookie).await.created_ip, None);
}

/// A header is as long as the caller decides. Unbounded, a sign-in is a way to
/// put as much into a row as the body limit allows.
#[tokio::test]
async fn an_enormous_user_agent_is_cut_rather_than_refused() {
    let app = TestApp::ready().await;
    register(&app, "member").await;

    let cookie = sign_in_as(&app, "member", &"u".repeat(4000)).await;
    let stored = session_behind(&app, &cookie)
        .await
        .created_user_agent
        .expect("an agent");
    assert_eq!(stored.chars().count(), 512);
}

/// A session started by something that sends no `User-Agent` — curl with the
/// header stripped, a script — is still a session.
#[tokio::test]
async fn a_caller_that_names_no_browser_still_gets_a_session() {
    let app = TestApp::ready().await;
    let cookie = register(&app, "member").await;
    assert_eq!(session_behind(&app, &cookie).await.created_user_agent, None);
}

/// A header value outside visible ASCII is dropped rather than mangled. Every
/// real browser sends ASCII, so this is a client that went out of its way — and
/// a session showing no browser is truthful, where a repaired string would not
/// be.
#[tokio::test]
async fn a_user_agent_that_is_not_ascii_is_stored_as_nothing() {
    let app = TestApp::ready().await;
    register(&app, "member").await;

    let response = expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .header(
                    header::USER_AGENT,
                    axum::http::HeaderValue::from_bytes("瀏覽器/1.0".as_bytes())
                        .expect("header values may carry these bytes"),
                ),
            Body::from(
                serde_json::json!({ "identifier": "member", "password": PASSWORD }).to_string(),
            ),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    let cookie = session_cookie(&response);
    assert_eq!(session_behind(&app, &cookie).await.created_user_agent, None);
}

/// The row is the record of a login, so presenting a session that is over must
/// not remove it. This is the moment it matters most: somebody using a cookie
/// the owner revoked is the thing the list exists to show.
#[tokio::test]
async fn presenting_a_session_that_is_over_does_not_erase_it() {
    let app = TestApp::ready().await;
    let cookie = register(&app, "member").await;
    let user = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("member").expect("a name"))
        .await
        .expect("query")
        .expect("the account");

    let now = time::OffsetDateTime::now_utc();
    let ended = app
        .state
        .sessions
        .end_all_for_user(
            user.id,
            None,
            tessera_server::db::repository::session::SessionEnding::Revoked,
            now,
        )
        .await
        .expect("end");
    assert_eq!(ended, 1);

    // The browser still has the cookie and tries it.
    assert_eq!(
        app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&cookie))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let rows = app
        .state
        .sessions
        .find_for_user(user.id)
        .await
        .expect("query");
    assert_eq!(rows.len(), 1, "the login must still be on the list");
    assert_eq!(
        rows[0].ended_reason,
        Some(tessera_server::db::repository::session::SessionEnding::Revoked)
    );
}

/// The owner is in the `WHERE` clause, not in a check beforehand. Somebody
/// else's identifier must do nothing at all — not fail differently, not fail
/// later, nothing.
#[tokio::test]
async fn revoking_reaches_only_your_own_sessions() {
    let app = TestApp::ready().await;
    register(&app, "owner").await;
    register(&app, "stranger").await;

    let owner = account_named(&app, "owner").await;
    let stranger = account_named(&app, "stranger").await;
    let theirs = app
        .state
        .sessions
        .find_for_user(stranger.id)
        .await
        .expect("query");
    assert_eq!(theirs.len(), 1);

    let now = time::OffsetDateTime::now_utc();
    assert_eq!(
        app.state
            .sessions
            .revoke(theirs[0].id, owner.id, now)
            .await
            .expect("revoke"),
        tessera_server::db::repository::session::Revocation::NotYours,
        "the owner must not be able to end a stranger's session"
    );

    let still = app
        .state
        .sessions
        .find_for_user(stranger.id)
        .await
        .expect("query");
    assert!(
        still[0].is_valid_at(now),
        "and the stranger's session must still work"
    );
    assert_eq!(still[0].ended_reason, None);
}

/// Ending one that is already over changes nothing, so a second attempt cannot
/// rewrite when it ended or why.
#[tokio::test]
async fn revoking_something_already_over_does_nothing() {
    let app = TestApp::ready().await;
    register(&app, "member").await;
    let member = account_named(&app, "member").await;

    let now = time::OffsetDateTime::now_utc();
    let mine = app
        .state
        .sessions
        .find_for_user(member.id)
        .await
        .expect("query");

    assert_eq!(
        app.state
            .sessions
            .revoke(mine[0].id, member.id, now)
            .await
            .expect("revoke"),
        tessera_server::db::repository::session::Revocation::Ended
    );
    assert_eq!(
        app.state
            .sessions
            .revoke(mine[0].id, member.id, now + time::Duration::minutes(5))
            .await
            .expect("revoke"),
        tessera_server::db::repository::session::Revocation::AlreadyOver,
        "a second attempt must be told it is already over, and must not write"
    );

    let after = app
        .state
        .sessions
        .find_for_user(member.id)
        .await
        .expect("query");
    assert_eq!(
        after[0].expires_at, now,
        "the ending time must not have moved"
    );
}

/// What "sign out my other devices" is made of: the browser doing the asking
/// keeps working, everything else stops.
#[tokio::test]
async fn ending_the_others_spares_the_one_you_name() {
    let app = TestApp::ready().await;
    let kept = register(&app, "member").await;
    let member = account_named(&app, "member").await;
    sign_in_as(&app, "member", "another browser").await;
    sign_in_as(&app, "member", "a third browser").await;

    let kept_id = session_behind(&app, &kept).await.id;
    let now = time::OffsetDateTime::now_utc();
    let ended = app
        .state
        .sessions
        .end_all_for_user(
            member.id,
            Some(kept_id),
            tessera_server::db::repository::session::SessionEnding::Revoked,
            now,
        )
        .await
        .expect("end");
    assert_eq!(ended, 2);

    expect_status(
        app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&kept))
            .await,
        StatusCode::OK,
    )
    .await;

    let rows = app
        .state
        .sessions
        .find_for_user(member.id)
        .await
        .expect("query");
    assert_eq!(rows.len(), 3, "all three are still on the list");
    assert_eq!(
        rows.iter().filter(|row| row.is_valid_at(now)).count(),
        1,
        "and exactly one of them still works"
    );
}

// ── seeing and ending your own sessions ─────────────────────────────

async fn list_sessions(app: &TestApp, auth: Auth<'_>) -> Vec<serde_json::Value> {
    let response = expect_status(
        app.empty(Method::GET, "/api/v1/sessions", auth).await,
        StatusCode::OK,
    )
    .await;
    body_json(response)
        .await
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// The one thing a person cannot work out for themselves before deciding what
/// to end.
#[tokio::test]
async fn the_list_says_which_one_you_are_reading_it_from() {
    let app = TestApp::ready().await;
    let here = register(&app, "member").await;
    sign_in_as(&app, "member", "another browser").await;

    let rows = list_sessions(&app, Auth::Cookie(&here)).await;
    assert_eq!(rows.len(), 2);

    let current: Vec<_> = rows.iter().filter(|row| row["current"] == true).collect();
    assert_eq!(current.len(), 1, "exactly one row is the one asking");
    assert_eq!(
        current[0]["id"],
        session_behind(&app, &here).await.id.to_string()
    );
    assert_eq!(current[0]["active"], true);
}

/// The sessions that are over stay on the list. Somebody reading it is asking
/// whether anybody else has been in, and one that was revoked last week is the
/// answer they came for.
#[tokio::test]
async fn the_list_keeps_the_ones_that_are_over() {
    let app = TestApp::ready().await;
    let here = register(&app, "member").await;
    let other = sign_in_as(&app, "member", "another browser").await;
    let other_id = session_behind(&app, &other).await.id;

    assert_eq!(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/sessions/{other_id}"),
            Auth::Cookie(&here),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );

    let rows = list_sessions(&app, Auth::Cookie(&here)).await;
    assert_eq!(rows.len(), 2, "the revoked one is still listed");
    let ended: Vec<_> = rows.iter().filter(|row| row["active"] == false).collect();
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0]["ended_reason"], "revoked");

    // And the cookie behind it stops working.
    assert_eq!(
        app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&other))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

/// Ending the others leaves the browser doing the asking signed in. Being
/// logged out by your own attempt to log everybody else out is a poor way to
/// find out it worked.
#[tokio::test]
async fn ending_the_others_leaves_you_signed_in() {
    let app = TestApp::ready().await;
    let here = register(&app, "member").await;
    let second = sign_in_as(&app, "member", "another browser").await;
    let third = sign_in_as(&app, "member", "a third browser").await;

    assert_eq!(
        app.empty(Method::DELETE, "/api/v1/sessions", Auth::Cookie(&here))
            .await
            .status(),
        StatusCode::NO_CONTENT
    );

    expect_status(
        app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&here))
            .await,
        StatusCode::OK,
    )
    .await;
    for gone in [&second, &third] {
        assert_eq!(
            app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(gone))
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }

    let rows = list_sessions(&app, Auth::Cookie(&here)).await;
    assert_eq!(rows.len(), 3, "all three are still on the list");
    assert_eq!(rows.iter().filter(|row| row["active"] == true).count(), 1);
}

/// Somebody else's identifier answers the same as one that never existed and
/// one that is not an identifier at all. Anything else makes this a way to ask
/// which sessions exist and whose they are.
#[tokio::test]
async fn ending_a_session_that_is_not_yours_says_only_that() {
    let app = TestApp::ready().await;
    let mine = register(&app, "owner").await;
    let theirs = register(&app, "stranger").await;
    let theirs_id = session_behind(&app, &theirs).await.id;

    for target in [
        theirs_id.to_string(),
        tessera_core::SessionId::generate().to_string(),
        "not-an-identifier".to_owned(),
    ] {
        assert_eq!(
            app.empty(
                Method::DELETE,
                &format!("/api/v1/sessions/{target}"),
                Auth::Cookie(&mine),
            )
            .await
            .status(),
            StatusCode::NOT_FOUND,
            "{target}"
        );
    }

    // And the stranger is still signed in.
    expect_status(
        app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&theirs))
            .await,
        StatusCode::OK,
    )
    .await;
}

/// A list that went stale between loading and clicking must not report a
/// problem where there is none.
#[tokio::test]
async fn ending_one_twice_is_not_an_error() {
    let app = TestApp::ready().await;
    let here = register(&app, "member").await;
    let other = sign_in_as(&app, "member", "another browser").await;
    let other_id = session_behind(&app, &other).await.id;

    for _ in 0..2 {
        assert_eq!(
            app.empty(
                Method::DELETE,
                &format!("/api/v1/sessions/{other_id}"),
                Auth::Cookie(&here),
            )
            .await
            .status(),
            StatusCode::NO_CONTENT
        );
    }
}

/// Stricter than the token endpoints next door, and for two reasons. Ending
/// sessions with a token would let a leaked one throw the owner off every device
/// they have. Listing them would hand it the addresses and browsers the owner
/// signs in from.
#[tokio::test]
async fn a_bearer_token_can_neither_see_nor_end_sessions() {
    let app = TestApp::ready().await;
    let cookie = register(&app, "member").await;

    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/tokens",
            Auth::Cookie(&cookie),
            &serde_json::json!({ "name": "a script" }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let secret = body_json(response).await["secret"]
        .as_str()
        .expect("a secret")
        .to_owned();

    let id = session_behind(&app, &cookie).await.id;
    for (method, uri) in [
        (Method::GET, "/api/v1/sessions".to_owned()),
        (Method::DELETE, "/api/v1/sessions".to_owned()),
        (Method::DELETE, format!("/api/v1/sessions/{id}")),
    ] {
        assert_eq!(
            app.empty(method.clone(), &uri, Auth::Bearer(&secret))
                .await
                .status(),
            StatusCode::FORBIDDEN,
            "{method} {uri}"
        );
    }

    // And the session it tried to end still works.
    expect_status(
        app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&cookie))
            .await,
        StatusCode::OK,
    )
    .await;
}

/// Signing out is an ending like any other, and says so on the list.
#[tokio::test]
async fn signing_out_shows_up_as_a_reason() {
    let app = TestApp::ready().await;
    let here = register(&app, "member").await;
    let other = sign_in_as(&app, "member", "another browser").await;

    expect_status(
        app.empty(Method::POST, "/api/v1/auth/logout", Auth::Cookie(&other))
            .await,
        StatusCode::NO_CONTENT,
    )
    .await;

    let rows = list_sessions(&app, Auth::Cookie(&here)).await;
    let ended: Vec<_> = rows
        .iter()
        .filter(|row| row["ended_reason"] == "signed_out")
        .collect();
    assert_eq!(ended.len(), 1);
}

// ── invite codes ────────────────────────────────────────────────────

/// An instance that asks for a code, with no relay so registration finishes in
/// one step.
async fn app_with_invites() -> TestApp {
    TestApp::with_config(|config| {
        config.auth.registration = RegistrationPolicy::Invite;
    })
    .await
}

#[expect(
    clippy::expect_used,
    reason = "test fixture; a panic is how it reports"
)]
async fn mint(app: &TestApp, uses: i64) -> String {
    let now = time::OffsetDateTime::now_utc();
    let (_, code) = app
        .state
        .invites
        .issue(None, uses, now, None)
        .await
        .expect("issue");
    code.expose().to_owned()
}

async fn register_with(app: &TestApp, username: &str, code: Option<&str>) -> StatusCode {
    let body = serde_json::json!({
        "username": username,
        "email": format!("{username}@example.com"),
        "password": PASSWORD,
        "invite_code": code,
    });
    app.json(Method::POST, "/api/v1/auth/register", Auth::None, &body)
        .await
        .status()
}

/// The hole this closes: `invite` was a documented setting that behaved exactly
/// like `closed`, so a client drawing its form from `/capabilities` produced one
/// that could never succeed.
#[tokio::test]
async fn an_invite_only_instance_accepts_a_code() {
    let app = app_with_invites().await;
    let code = mint(&app, 1).await;

    assert_eq!(
        register_with(&app, "invited", Some(&code)).await,
        StatusCode::CREATED
    );
}

/// Unknown, spent, expired and absent are one message. Anything else turns the
/// registration form into a way of testing codes.
#[tokio::test]
async fn every_unusable_code_says_the_same_thing() {
    let app = app_with_invites().await;
    let spent = mint(&app, 1).await;
    assert_eq!(
        register_with(&app, "first", Some(&spent)).await,
        StatusCode::CREATED
    );

    let now = time::OffsetDateTime::now_utc();
    let (_, expired) = app
        .state
        .invites
        .issue(
            None,
            5,
            now - time::Duration::days(2),
            Some(now - time::Duration::days(1)),
        )
        .await
        .expect("issue");

    for (label, code) in [
        ("spent", Some(spent.as_str())),
        ("expired", Some(expired.expose())),
        ("never existed", Some("0123456789abcdef")),
        ("absent", None),
        ("empty", Some("")),
    ] {
        assert_eq!(
            register_with(&app, &format!("who-{}", label.replace(' ', "-")), code).await,
            StatusCode::BAD_REQUEST,
            "{label}"
        );
    }
}

/// One code with several places is the case this exists for: something to send
/// to a group chat rather than a code per person.
#[tokio::test]
async fn a_code_admits_as_many_as_it_says_and_no_more() {
    let app = app_with_invites().await;
    let code = mint(&app, 3).await;

    for n in 0..3 {
        assert_eq!(
            register_with(&app, &format!("member{n}"), Some(&code)).await,
            StatusCode::CREATED,
            "{n}"
        );
    }
    assert_eq!(
        register_with(&app, "one-too-many", Some(&code)).await,
        StatusCode::BAD_REQUEST
    );
}

/// The condition lives in the `UPDATE`, so two people reaching for the last
/// place do not both get it.
#[tokio::test]
async fn two_people_cannot_take_the_same_last_place() {
    let app = app_with_invites().await;
    let code = mint(&app, 1).await;

    let (first, second) = tokio::join!(
        register_with(&app, "racer-one", Some(&code)),
        register_with(&app, "racer-two", Some(&code))
    );
    let mut statuses = [first, second];
    statuses.sort_unstable();
    assert_eq!(statuses, [StatusCode::CREATED, StatusCode::BAD_REQUEST]);

    let invite = app.state.invites.list().await.expect("query");
    assert_eq!(invite[0].used, 1, "the code must not have been overdrawn");
}

/// An open instance ignores a code rather than refusing it, so a client can send
/// the same body whichever kind of instance it is talking to.
#[tokio::test]
async fn an_open_instance_ignores_a_code() {
    let app = TestApp::ready().await;
    assert_eq!(
        register_with(&app, "member", Some("nonsense")).await,
        StatusCode::CREATED
    );
}

/// Closed still means closed, and says so before reading anything.
#[tokio::test]
async fn a_closed_instance_refuses_even_a_real_code() {
    let app = TestApp::with_config(|config| {
        config.auth.registration = RegistrationPolicy::Closed;
    })
    .await;
    let code = mint(&app, 1).await;
    assert_eq!(
        register_with(&app, "member", Some(&code)).await,
        StatusCode::FORBIDDEN
    );
}

/// The decision this feature turns on when mail is configured: a place is not
/// taken until an account exists. Charging at submission would let somebody burn
/// every place on a code by filling the form in five times and never opening
/// their inbox — and there would be nothing to show for it.
#[tokio::test]
async fn filling_in_the_form_does_not_spend_a_place() {
    let app = TestApp::with_config(|config| {
        "smtp.invalid".clone_into(&mut config.mail.host);
        "tessera@example.com".clone_into(&mut config.mail.from);
        "https://tessera.example.com".clone_into(&mut config.mail.public_url);
        config.auth.registration = RegistrationPolicy::Invite;
    })
    .await;
    let code = mint(&app, 2).await;

    for n in 0..5 {
        assert_eq!(
            register_with(&app, &format!("waiting{n}"), Some(&code)).await,
            StatusCode::ACCEPTED,
            "{n}"
        );
    }
    wait_for_pending(&app, "waiting4@example.com").await;

    assert_eq!(
        app.state.invites.list().await.expect("query")[0].used,
        0,
        "five submissions and nobody has confirmed, so nothing is spent"
    );
}

/// And following the link is what charges it.
#[tokio::test]
async fn following_the_link_spends_the_place() {
    let app = TestApp::with_config(|config| {
        "smtp.invalid".clone_into(&mut config.mail.host);
        "tessera@example.com".clone_into(&mut config.mail.from);
        "https://tessera.example.com".clone_into(&mut config.mail.public_url);
        config.auth.registration = RegistrationPolicy::Invite;
    })
    .await;
    let code = mint(&app, 1).await;

    assert_eq!(
        register_with(&app, "invited", Some(&code)).await,
        StatusCode::ACCEPTED
    );
    let pending = wait_for_pending(&app, "invited@example.com").await;
    assert!(
        pending.invite_id.is_some(),
        "the waiting row must remember which code it came in on"
    );

    let now = time::OffsetDateTime::now_utc();
    let secret = app
        .state
        .pending
        .start(
            tessera_server::db::repository::pending_registration::Waiting {
                username: &pending.username,
                email: &pending.email,
                password_hash: pending.password_hash.clone(),
                invite_id: pending.invite_id,
            },
            now,
            now.saturating_add(time::Duration::hours(24)),
        )
        .await
        .expect("issue");

    assert_eq!(
        confirm_registration(&app, secret.expose()).await.status(),
        StatusCode::CREATED
    );
    assert_eq!(app.state.invites.list().await.expect("query")[0].used, 1);
}

/// A token minted without saying what it is for may upload and nothing else.
///
/// The other default would hand every screenshot tool the ability to delete
/// everything its owner has, and nobody would notice until one leaked.
#[tokio::test]
async fn a_token_defaults_to_uploading_and_nothing_else() {
    let app = TestApp::ready().await;
    let session = register(&app, "alice").await;

    let created = body_json(
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/tokens",
                Auth::Cookie(&session),
                &serde_json::json!({ "name": "a screenshot tool" }),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    assert_eq!(
        created["permissions"],
        serde_json::json!(["shares:create"]),
        "a token is for uploading"
    );
    let secret = created["secret"].as_str().expect("a secret").to_owned();

    // Uploading works.
    assert_eq!(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=note.txt")
                .header(header::AUTHORIZATION, format!("Bearer {secret}")),
            Body::from(b"hello".to_vec()),
        )
        .await
        .status(),
        StatusCode::CREATED
    );

    // Listing what the account has uploaded does not — that is `shares:read`.
    assert_eq!(
        app.empty(Method::GET, "/api/v1/shares", Auth::Bearer(&secret))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
}

/// A token must not be a way to exceed the account that minted it.
#[tokio::test]
async fn a_token_cannot_be_given_a_permission_its_owner_lacks() {
    let app = TestApp::ready().await;
    register(&app, "first").await;
    let ordinary = register(&app, "second").await;

    let response = app
        .json(
            Method::POST,
            "/api/v1/tokens",
            Auth::Cookie(&ordinary),
            &serde_json::json!({ "name": "ambitious", "permissions": ["users:delete"] }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// Demoting somebody narrows every token they hold, at that moment.
///
/// Without the intersection at the request, a token minted while its owner was
/// an administrator would keep administering the instance after the owner
/// stopped being one — which is the state nobody would think to check.
#[tokio::test]
async fn demoting_an_owner_narrows_the_tokens_they_already_hold() {
    let app = TestApp::ready().await;
    let admin = register(&app, "first").await;

    let secret = body_json(
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/tokens",
                Auth::Cookie(&admin),
                &serde_json::json!({ "name": "wide", "permissions": ["shares:create", "shares:read"] }),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await["secret"]
        .as_str()
        .expect("a secret")
        .to_owned();

    assert_eq!(
        app.empty(Method::GET, "/api/v1/shares", Auth::Bearer(&secret))
            .await
            .status(),
        StatusCode::OK
    );

    // Take every share permission away without touching the token.
    let user = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("first").expect("a name"))
        .await
        .expect("query")
        .expect("the account");
    let narrow = app
        .state
        .roles
        .create(
            "narrow".to_owned(),
            None,
            &[tessera_core::Permission::TokensManage]
                .into_iter()
                .collect(),
            time::OffsetDateTime::now_utc(),
        )
        .await
        .expect("create the role");
    app.state
        .roles
        .set_roles_of(user.id, &[narrow.id], time::OffsetDateTime::now_utc())
        .await
        .expect("demote them");

    assert_eq!(
        app.empty(Method::GET, "/api/v1/shares", Auth::Bearer(&secret))
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "the token must not outlive the permission it was minted from"
    );
}

/// Changing the address has to un-verify it.
///
/// Carrying the old flag over would mean an address nobody has proven reachable
/// wearing a verified one's badge — and on an instance that trusts that flag,
/// that is the whole of the check.
#[tokio::test]
async fn moving_the_address_clears_the_verification() {
    let app = TestApp::ready().await;
    let session = register(&app, "alice").await;
    let user = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("alice").expect("a name"))
        .await
        .expect("query")
        .expect("the account");
    let now = time::OffsetDateTime::now_utc();
    app.state
        .users
        .rename(user.id, &user.username, &user.email, Some(now), now)
        .await
        .expect("mark the address verified");

    expect_status(
        app.json(
            Method::PATCH,
            "/api/v1/account",
            Auth::Cookie(&session),
            &serde_json::json!({
                "email": "elsewhere@example.com",
                "current_password": PASSWORD,
            }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    let moved = app
        .state
        .users
        .find_by_id(user.id)
        .await
        .expect("query")
        .expect("the account");
    assert_eq!(moved.email.as_str(), "elsewhere@example.com");
    assert!(
        moved.email_verified_at.is_none(),
        "the new address has not been proven"
    );
}

/// Renaming without touching the address keeps it verified.
#[tokio::test]
async fn a_rename_does_not_cost_the_verification() {
    let app = TestApp::ready().await;
    let session = register(&app, "alice").await;
    let user = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("alice").expect("a name"))
        .await
        .expect("query")
        .expect("the account");
    let now = time::OffsetDateTime::now_utc();
    app.state
        .users
        .rename(user.id, &user.username, &user.email, Some(now), now)
        .await
        .expect("mark the address verified");

    expect_status(
        app.json(
            Method::PATCH,
            "/api/v1/account",
            Auth::Cookie(&session),
            &serde_json::json!({ "username": "alicia", "current_password": PASSWORD }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    let renamed = app
        .state
        .users
        .find_by_id(user.id)
        .await
        .expect("query")
        .expect("the account");
    assert_eq!(renamed.username.as_str(), "alicia");
    assert!(renamed.email_verified_at.is_some());
}

/// An unlocked browser left on a desk must not be enough to take the account.
#[tokio::test]
async fn changing_the_address_needs_the_password() {
    let app = TestApp::ready().await;
    let session = register(&app, "alice").await;

    assert_eq!(
        app.json(
            Method::PATCH,
            "/api/v1/account",
            Auth::Cookie(&session),
            &serde_json::json!({
                "email": "attacker@example.com",
                "current_password": "not the password",
            }),
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
}

/// Nobody renames themselves into a reserved name.
#[tokio::test]
async fn a_rename_cannot_claim_a_reserved_name() {
    let app = TestApp::ready().await;
    register(&app, "first").await;
    let session = register(&app, "second").await;

    assert_eq!(
        app.json(
            Method::PATCH,
            "/api/v1/account",
            Auth::Cookie(&session),
            &serde_json::json!({ "username": "admin", "current_password": PASSWORD }),
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST,
        "the reserved list guards this door too, not only registration"
    );
}

/// Removing your own account ends what you sent, unless you say otherwise.
#[tokio::test]
async fn removing_your_own_account_ends_your_shares_by_default() {
    let app = TestApp::ready().await;
    register(&app, "first").await;
    let session = register(&app, "second").await;

    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/shares?filename=mine.bin")
                    .header(header::COOKIE, &session),
                Body::from(b"content".to_vec()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let code = created["code"].as_str().expect("a code").to_owned();

    expect_status(
        app.json(
            Method::DELETE,
            "/api/v1/account",
            Auth::Cookie(&session),
            &serde_json::json!({ "current_password": PASSWORD }),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;

    assert_eq!(
        app.empty(Method::GET, &format!("/s/{code}/download"), Auth::None)
            .await
            .status(),
        StatusCode::GONE
    );
    assert_eq!(
        app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&session))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

/// And keeps them when you do say otherwise.
#[tokio::test]
async fn removing_your_own_account_can_leave_your_shares_working() {
    let app = TestApp::ready().await;
    register(&app, "first").await;
    let session = register(&app, "second").await;

    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/shares?filename=mine.bin")
                    .header(header::COOKIE, &session),
                Body::from(b"content".to_vec()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let code = created["code"].as_str().expect("a code").to_owned();

    expect_status(
        app.json(
            Method::DELETE,
            "/api/v1/account",
            Auth::Cookie(&session),
            &serde_json::json!({ "current_password": PASSWORD, "delete_shares": false }),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;

    assert_eq!(
        app.empty(Method::GET, &format!("/s/{code}/download"), Auth::None)
            .await
            .status(),
        StatusCode::OK,
        "whoever holds this link did nothing wrong"
    );
}

/// The last person who can administer the instance cannot walk out.
#[tokio::test]
async fn the_only_administrator_cannot_remove_themselves() {
    let app = TestApp::ready().await;
    let session = register(&app, "first").await;

    assert_eq!(
        app.json(
            Method::DELETE,
            "/api/v1/account",
            Auth::Cookie(&session),
            &serde_json::json!({ "current_password": PASSWORD }),
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
}

/// Somebody must not be able to take the name a deletion will need.
///
/// A tombstone is `gone-<the account's own ulid>`, and identifiers are public —
/// every response that names an account carries one. So a person can read
/// somebody else's, rename themselves to the tombstone that account will one
/// day need, and the unique index then refuses the deletion.
#[tokio::test]
async fn a_rename_cannot_take_a_tombstone_that_is_not_yours() {
    let app = TestApp::ready().await;
    register(&app, "first").await;
    let session = register(&app, "second").await;

    let victim = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("first").expect("a name"))
        .await
        .expect("query")
        .expect("the account");
    let (their_tombstone, _) = tessera_core::tombstone(victim.id);

    let refused = app
        .json(
            Method::PATCH,
            "/api/v1/account",
            Auth::Cookie(&session),
            &serde_json::json!({
                "username": their_tombstone.as_str(),
                "current_password": PASSWORD,
            }),
        )
        .await;
    assert_eq!(
        refused.status(),
        StatusCode::BAD_REQUEST,
        "taking it would make that account undeletable"
    );

    // The point of refusing: with the name taken, the deletion cannot write the
    // tombstone and the account can never be removed.
    assert!(
        app.state
            .users
            .soft_delete(victim.id, victim.id, time::OffsetDateTime::now_utc())
            .await
            .is_ok(),
        "the account must still be deletable"
    );
}

/// The same door, reached by registering rather than renaming.
#[tokio::test]
async fn registration_cannot_take_a_tombstone_either() {
    let app = TestApp::ready().await;
    register(&app, "first").await;

    let victim = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("first").expect("a name"))
        .await
        .expect("query")
        .expect("the account");
    let (their_name, their_email) = tessera_core::tombstone(victim.id);

    assert_eq!(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": their_name.as_str(),
                "email": "someone@example.com",
                "password": PASSWORD,
            }),
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );

    assert_eq!(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": "someone",
                "email": their_email.as_str(),
                "password": PASSWORD,
            }),
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST,
        "the address a deletion needs must not be claimable either"
    );
}
