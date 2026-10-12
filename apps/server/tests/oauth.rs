//! Signing in with GitHub or Discord, over HTTP, against a running provider.
//!
//! What these share with `OpenID` Connect is covered by `oidc.rs`: the one-shot
//! `state`, the registration policy, username selection, linking to an existing
//! account. This covers what the two do differently: how they report who
//! signed in.
//!
//! GitHub is driven end to end, since `auth.github.base_url` exists for GitHub
//! Enterprise Server and a test can point it at a server it controls. Discord
//! has no self-hosted edition and so no such setting, and adding one for a
//! test would be a knob that misrepresents why it exists. Its adapter, where
//! everything Discord-specific lives, is exercised directly instead.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test fixture; see `oidc.rs`"
)]

mod common;

use axum::{
    body::Body,
    http::{Method, StatusCode, header},
    response::Response,
};
use common::{Auth, TestApp, expect_status, provider::FakeOauth, session_cookie};
use tessera_server::config::Config;

const CLIENT_ID: &str = "tessera";
const PUBLIC_URL: &str = "https://tessera.example";

/// An instance whose GitHub is the one this test runs.
async fn instance(github: &FakeOauth) -> TestApp {
    let base = github.base.clone();
    TestApp::with_config(move |config: &mut Config| {
        PUBLIC_URL.clone_into(&mut config.server.public_url);
        CLIENT_ID.clone_into(&mut config.auth.github.client_id);
        "a-secret".clone_into(&mut config.auth.github.client_secret);
        config.auth.github.base_url = base;
    })
    .await
}

fn location(response: &Response<Body>) -> String {
    response
        .headers()
        .get(header::LOCATION)
        .expect("a Location header")
        .to_str()
        .expect("an ASCII location")
        .to_owned()
}

fn rejection(response: &Response<Body>) -> Option<String> {
    location(response)
        .split_once("oidc_error=")
        .map(|(_page, code)| code.to_owned())
}

/// Walks the redirect out and reads the `state` it carried.
async fn leave(app: &TestApp) -> String {
    let response = app
        .empty(Method::GET, "/api/v1/auth/github/start", Auth::None)
        .await;
    assert_eq!(
        response.status(),
        StatusCode::SEE_OTHER,
        "expected to be sent to GitHub, got {:?}",
        location(&response)
    );
    let url = url::Url::parse(&location(&response)).expect("an absolute redirect");
    url.query_pairs()
        .find(|(key, _value)| key == "state")
        .map_or_else(
            || panic!("a state parameter in {url}"),
            |(_key, value)| value.into_owned(),
        )
}

async fn come_back(app: &TestApp, state: &str) -> Response<Body> {
    app.empty(
        Method::GET,
        &format!("/api/v1/auth/github/callback?code=a-code&state={state}"),
        Auth::None,
    )
    .await
}

/// The same setting for GitHub, where it matters most: with no groups to
/// narrow it, `always` opens this instance to every GitHub account, so it must
/// work exactly as written and no wider.
#[tokio::test]
async fn github_makes_accounts_only_as_its_own_setting_says() {
    use tessera_server::config::{ProviderRegistration, RegistrationPolicy};

    for (registration, github_says, makes_one) in [
        (
            RegistrationPolicy::Closed,
            ProviderRegistration::Always,
            true,
        ),
        (RegistrationPolicy::Open, ProviderRegistration::Never, false),
        (
            RegistrationPolicy::Closed,
            ProviderRegistration::Inherit,
            false,
        ),
    ] {
        let github = FakeOauth::start().await;
        let base = github.base.clone();
        let app = TestApp::with_config(move |config: &mut Config| {
            PUBLIC_URL.clone_into(&mut config.server.public_url);
            CLIENT_ID.clone_into(&mut config.auth.github.client_id);
            "a-secret".clone_into(&mut config.auth.github.client_secret);
            config.auth.github.base_url = base;
            config.auth.registration = registration;
            config.auth.github.allow_registration = github_says;
        })
        .await;
        github.is(serde_json::json!({ "id": 4_242, "login": "alice" }));
        github.addresses(serde_json::json!([
            { "email": "alice@example.com", "primary": true, "verified": true },
        ]));
        let state = leave(&app).await;
        let response = come_back(&app, &state).await;
        if makes_one {
            assert_eq!(
                location(&response),
                "/",
                "{registration:?} with {github_says:?}"
            );
        } else {
            assert_eq!(
                rejection(&response).as_deref(),
                Some("not_invited"),
                "{registration:?} with {github_says:?}"
            );
        }
    }
}

/// The main path.
#[tokio::test]
async fn a_github_account_signs_in() {
    let github = FakeOauth::start().await;
    let app = instance(&github).await;
    github.is(serde_json::json!({ "id": 4_242, "login": "alice", "name": "Alice" }));
    github.addresses(serde_json::json!([
        { "email": "alice@example.com", "primary": true, "verified": true },
    ]));

    let state = leave(&app).await;
    let response = come_back(&app, &state).await;

    assert_eq!(location(&response), "/");
    let cookie = session_cookie(&response);
    let account = common::body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&cookie))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(
        common::field(&account, "username"),
        serde_json::json!("alice")
    );
    assert_eq!(
        common::field(&account, "email"),
        serde_json::json!("alice@example.com")
    );
}

/// The account is keyed on the numeric id, not the login. A GitHub login can be
/// changed and a released one claimed by another user, so keying on it would
/// hand the account to whoever took the name.
#[tokio::test]
async fn a_renamed_github_account_is_still_the_same_account() {
    let github = FakeOauth::start().await;
    let app = instance(&github).await;
    github.addresses(serde_json::json!([
        { "email": "alice@example.com", "primary": true, "verified": true },
    ]));

    github.is(serde_json::json!({ "id": 4_242, "login": "alice" }));
    let first = session_cookie(&come_back(&app, &leave(&app).await).await);

    // Same user, new name, and a new address too.
    github.is(serde_json::json!({ "id": 4_242, "login": "alice-renamed" }));
    github.addresses(serde_json::json!([
        { "email": "elsewhere@example.com", "primary": true, "verified": true },
    ]));
    let second = session_cookie(&come_back(&app, &leave(&app).await).await);

    assert_eq!(id_of(&app, &first).await, id_of(&app, &second).await);
}

/// Two different accounts that share a login are still two accounts.
#[tokio::test]
async fn two_github_ids_are_two_accounts() {
    let github = FakeOauth::start().await;
    let app = instance(&github).await;

    github.is(serde_json::json!({ "id": 1, "login": "alice" }));
    github.addresses(serde_json::json!([
        { "email": "first@example.com", "primary": true, "verified": true },
    ]));
    let first = session_cookie(&come_back(&app, &leave(&app).await).await);

    github.is(serde_json::json!({ "id": 2, "login": "alice" }));
    github.addresses(serde_json::json!([
        { "email": "second@example.com", "primary": true, "verified": true },
    ]));
    let second = session_cookie(&come_back(&app, &leave(&app).await).await);

    assert_ne!(id_of(&app, &first).await, id_of(&app, &second).await);
}

/// `GET /user` returns `email: null` when the address is private, the default
/// for many accounts. The address must come from `GET /user/emails`, which is
/// what this tests.
#[tokio::test]
async fn a_private_address_is_still_found() {
    let github = FakeOauth::start().await;
    let app = instance(&github).await;
    github.is(serde_json::json!({ "id": 7, "login": "alice", "email": null }));
    github.addresses(serde_json::json!([
        { "email": "public@example.com", "primary": false, "verified": true },
        { "email": "private@example.com", "primary": true, "verified": true },
    ]));

    let cookie = session_cookie(&come_back(&app, &leave(&app).await).await);

    let account = common::body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&cookie))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(
        common::field(&account, "email"),
        serde_json::json!("private@example.com"),
        "the primary address is the one to take"
    );
}

/// An address added but never confirmed is not the user's. Taking it would let
/// anyone claim an account by typing in its owner's address.
#[tokio::test]
async fn an_unverified_address_is_not_an_address() {
    let github = FakeOauth::start().await;
    let app = instance(&github).await;
    github.is(serde_json::json!({ "id": 7, "login": "alice" }));
    github.addresses(serde_json::json!([
        { "email": "unconfirmed@example.com", "primary": true, "verified": false },
    ]));

    let response = come_back(&app, &leave(&app).await).await;

    assert_eq!(rejection(&response).as_deref(), Some("no_address"));
}

/// Nor is a verified address that is not the primary entry. Taking the first
/// verified one instead could pick an address the user may not use.
#[tokio::test]
async fn a_verified_address_that_is_not_primary_is_not_taken() {
    let github = FakeOauth::start().await;
    let app = instance(&github).await;
    github.is(serde_json::json!({ "id": 7, "login": "alice" }));
    github.addresses(serde_json::json!([
        { "email": "old@example.com", "primary": false, "verified": true },
        { "email": "unconfirmed@example.com", "primary": true, "verified": false },
    ]));

    let response = come_back(&app, &leave(&app).await).await;

    assert_eq!(rejection(&response).as_deref(), Some("no_address"));
}

/// A token without `user:email` cannot read the addresses. Reported as a
/// missing address, which points at the scope to add, not as a refusal.
#[tokio::test]
async fn a_token_that_cannot_read_addresses_says_no_address() {
    let github = FakeOauth::start().await;
    let app = instance(&github).await;
    github.is(serde_json::json!({ "id": 7, "login": "alice" }));
    // What the endpoint returns without the scope: not a list.
    github.addresses(serde_json::json!({ "message": "Requires authentication" }));

    let response = come_back(&app, &leave(&app).await).await;

    assert_eq!(rejection(&response).as_deref(), Some("no_address"));
}

/// The redirect carries no nonce, since there is no token to echo one back.
/// The callback is tied to this sign-in by the state, which the flow row makes
/// single-use.
#[tokio::test]
async fn the_redirect_carries_no_nonce_and_the_state_works_once() {
    let github = FakeOauth::start().await;
    let app = instance(&github).await;
    github.is(serde_json::json!({ "id": 7, "login": "alice" }));
    github.addresses(serde_json::json!([
        { "email": "alice@example.com", "primary": true, "verified": true },
    ]));

    let response = app
        .empty(Method::GET, "/api/v1/auth/github/start", Auth::None)
        .await;
    let redirect = location(&response);
    assert!(!redirect.contains("nonce="), "{redirect}");
    assert!(
        redirect.contains("code_challenge_method=S256"),
        "{redirect}"
    );
    assert!(
        redirect.starts_with(&github.base),
        "it should go to the configured GitHub: {redirect}"
    );

    let url = url::Url::parse(&redirect).expect("an absolute redirect");
    let state = url
        .query_pairs()
        .find(|(key, _value)| key == "state")
        .map(|(_key, value)| value.into_owned())
        .expect("a state");

    assert!(
        come_back(&app, &state)
            .await
            .headers()
            .contains_key(header::SET_COOKIE)
    );
    let again = come_back(&app, &state).await;
    assert_eq!(rejection(&again).as_deref(), Some("stale"));
}

/// Being vouched for by GitHub is not being invited here.
#[tokio::test]
async fn a_closed_instance_does_not_register_from_github() {
    let github = FakeOauth::start().await;
    let base = github.base.clone();
    let app = TestApp::with_config(move |config: &mut Config| {
        PUBLIC_URL.clone_into(&mut config.server.public_url);
        CLIENT_ID.clone_into(&mut config.auth.github.client_id);
        "a-secret".clone_into(&mut config.auth.github.client_secret);
        config.auth.github.base_url = base;
        config.auth.registration = tessera_server::config::RegistrationPolicy::Closed;
    })
    .await;
    github.is(serde_json::json!({ "id": 7, "login": "alice" }));
    github.addresses(serde_json::json!([
        { "email": "alice@example.com", "primary": true, "verified": true },
    ]));

    let response = come_back(&app, &leave(&app).await).await;

    assert_eq!(rejection(&response).as_deref(), Some("not_invited"));
}

/// A GitHub session records which provider started it, so a provider signing a
/// user out later cannot end another provider's sessions.
#[tokio::test]
async fn a_github_session_records_github() {
    let github = FakeOauth::start().await;
    let app = instance(&github).await;
    github.is(serde_json::json!({ "id": 7, "login": "alice" }));
    github.addresses(serde_json::json!([
        { "email": "alice@example.com", "primary": true, "verified": true },
    ]));

    let cookie = session_cookie(&come_back(&app, &leave(&app).await).await);
    let user_id = id_of(&app, &cookie).await;

    let identities = common::body_json(
        expect_status(
            app.empty(
                Method::GET,
                "/api/v1/auth/oidc/identities",
                Auth::Cookie(&cookie),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    let listed = identities.as_array().expect("a list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["provider"], serde_json::json!("github"));
    assert!(!user_id.is_null());
}

/// Walks the redirect out with a query, and reads the `state` it carried.
async fn leave_with(app: &TestApp, query: &str) -> String {
    let response = app
        .empty(
            Method::GET,
            &format!("/api/v1/auth/github/start{query}"),
            Auth::None,
        )
        .await;
    let url = url::Url::parse(&location(&response)).expect("an absolute redirect");
    url.query_pairs()
        .find(|(key, _value)| key == "state")
        .map(|(_key, value)| value.into_owned())
        .expect("a state parameter")
}

/// Makes every session the account has look as if it signed in `minutes` ago.
async fn signed_in_ago(app: &TestApp, minutes: i64) {
    use sea_orm::ConnectionTrait as _;
    let when = time::OffsetDateTime::now_utc() - time::Duration::minutes(minutes);
    let statement = sea_orm::Statement::from_string(
        app.state.db.get_database_backend(),
        format!(
            "UPDATE sessions SET created_at = '{}'",
            when.format(&time::format_description::well_known::Rfc3339)
                .expect("formattable")
        ),
    );
    app.state
        .db
        .execute_raw(statement)
        .await
        .expect("age the sessions");
}

async fn session_of(app: &TestApp, cookie: &str) -> serde_json::Value {
    common::body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(cookie))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await
}

/// An account created through GitHub has no password to confirm changes with.
/// It confirms by having just signed in; after that it is asked to sign in
/// again, and returns to the page it left.
#[tokio::test]
async fn an_account_without_a_password_confirms_by_signing_in_again() {
    let github = FakeOauth::start().await;
    let app = instance(&github).await;
    // The first account administers the instance and cannot remove itself.
    expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": "first",
                "email": "first@example.com",
                "password": "correct horse battery staple",
            }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    github.is(serde_json::json!({ "id": 7, "login": "alice", "name": "Alice" }));
    github.addresses(serde_json::json!([
        { "email": "alice@example.com", "primary": true, "verified": true },
    ]));
    let cookie = session_cookie(&come_back(&app, &leave(&app).await).await);
    assert_eq!(session_of(&app, &cookie).await["has_password"], false);

    // Just signed in: nothing to type.
    expect_status(
        app.json(
            Method::PATCH,
            "/api/v1/account",
            Auth::Cookie(&cookie),
            &serde_json::json!({ "username": "alice2" }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    // Too long ago: every confirming action asks for a fresh sign-in.
    signed_in_ago(&app, 11).await;
    for (method, path, body) in [
        (
            Method::PATCH,
            "/api/v1/account",
            serde_json::json!({ "username": "alice3" }),
        ),
        (Method::DELETE, "/api/v1/account", serde_json::json!({})),
        (
            Method::PATCH,
            "/api/v1/auth/password",
            serde_json::json!({ "new_password": "a brand new passphrase" }),
        ),
    ] {
        let refused = common::body_json(
            expect_status(
                app.json(method, path, Auth::Cookie(&cookie), &body).await,
                StatusCode::UNAUTHORIZED,
            )
            .await,
        )
        .await;
        assert_eq!(refused["code"], "reauthentication_required", "{path}");
    }
    // Linking another provider is a browser navigation, so the refusal comes
    // as a redirect instead.
    let link = app
        .empty(
            Method::GET,
            "/api/v1/auth/github/link",
            Auth::Cookie(&cookie),
        )
        .await;
    assert_eq!(rejection(&link).as_deref(), Some("sign_in_again"));

    // Signing in again lands back on the page that asked.
    let again = come_back(&app, &leave_with(&app, "?return_to=/user").await).await;
    assert_eq!(location(&again), "/user");
    let fresh = session_cookie(&again);

    // And it sets a first password with nothing to confirm but that sign-in.
    expect_status(
        app.json(
            Method::PATCH,
            "/api/v1/auth/password",
            Auth::Cookie(&fresh),
            &serde_json::json!({ "new_password": "a brand new passphrase" }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
}

/// A recent sign-in removes an account with no password.
#[tokio::test]
async fn an_account_without_a_password_can_remove_itself() {
    let github = FakeOauth::start().await;
    let app = instance(&github).await;
    expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": "first",
                "email": "first@example.com",
                "password": "correct horse battery staple",
            }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    github.is(serde_json::json!({ "id": 8, "login": "bob", "name": "Bob" }));
    github.addresses(serde_json::json!([
        { "email": "bob@example.com", "primary": true, "verified": true },
    ]));
    let cookie = session_cookie(&come_back(&app, &leave(&app).await).await);
    expect_status(
        app.json(
            Method::DELETE,
            "/api/v1/account",
            Auth::Cookie(&cookie),
            &serde_json::json!({}),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
}

/// Only a path on this site is a valid return target. Anything a browser would
/// resolve against another host lands on the home page instead.
#[tokio::test]
async fn a_return_path_off_this_site_is_ignored() {
    let github = FakeOauth::start().await;
    let app = instance(&github).await;
    github.is(serde_json::json!({ "id": 9, "login": "carol", "name": "Carol" }));
    github.addresses(serde_json::json!([
        { "email": "carol@example.com", "primary": true, "verified": true },
    ]));
    for query in [
        "?return_to=//evil.example/",
        "?return_to=https://evil.example/",
        "?return_to=/%5Cevil.example",
        "?return_to=user",
    ] {
        let back = come_back(&app, &leave_with(&app, query).await).await;
        assert_eq!(location(&back), "/", "{query}");
    }
}

/// Which account a cookie belongs to.
async fn id_of(app: &TestApp, cookie: &str) -> serde_json::Value {
    let body = common::body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(cookie))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    common::field(&body, "id")
}

/// Discord, at the adapter. There is no self-hosted Discord and so no setting
/// to point at one, which is why this is tested here instead of through the
/// endpoint like GitHub. Everything Discord-specific is in these responses.
mod discord {
    use tessera_server::auth::oauth::discord;

    use super::FakeOauth;

    /// The snowflake, never the username. Discord's own documentation says a
    /// username is "not unique across the platform", and it can be changed.
    #[tokio::test]
    async fn the_snowflake_is_the_subject() {
        let fake = FakeOauth::start().await;
        fake.is(serde_json::json!({
            "id": "80351110224678912",
            "username": "alice",
            "global_name": "Alice",
            "email": "alice@example.com",
            "verified": true,
        }));

        let identity = discord::identify(&format!("{}/api", fake.base), "a-token")
            .await
            .expect("an identity");

        assert_eq!(identity.subject, "80351110224678912");
        assert_eq!(identity.preferred_username.as_deref(), Some("alice"));
        assert_eq!(identity.name.as_deref(), Some("Alice"));
        assert_eq!(identity.email.as_deref(), Some("alice@example.com"));
        assert!(identity.email_verified);
    }

    /// An unverified address is dropped, not carried with a flag. Carrying it
    /// would leave every caller to remember to check, and forgetting is an
    /// account takeover.
    #[tokio::test]
    async fn an_unverified_address_is_dropped() {
        let fake = FakeOauth::start().await;
        fake.is(serde_json::json!({
            "id": "1",
            "username": "alice",
            "email": "alice@example.com",
            "verified": false,
        }));

        let identity = discord::identify(&format!("{}/api", fake.base), "a-token")
            .await
            .expect("an identity");

        assert_eq!(identity.email, None);
        assert!(!identity.email_verified);
    }

    /// `email` is null for some accounts even with the scope, and `verified` is
    /// absent, not false, on others.
    #[tokio::test]
    async fn a_missing_address_is_absent_rather_than_a_failure() {
        let fake = FakeOauth::start().await;
        fake.is(serde_json::json!({ "id": "1", "username": "alice" }));

        let identity = discord::identify(&format!("{}/api", fake.base), "a-token")
            .await
            .expect("an identity");

        assert_eq!(identity.subject, "1");
        assert_eq!(identity.email, None);
        assert!(!identity.email_verified);
    }

    /// A refusal is a refusal, not an identity with empty fields.
    #[tokio::test]
    async fn a_provider_that_refuses_is_an_error() {
        let fake = FakeOauth::start().await;
        // Not a user object: what an expired token gets back.
        fake.is(serde_json::json!({ "message": "401: Unauthorized" }));

        assert!(
            discord::identify(&format!("{}/api", fake.base), "a-token")
                .await
                .is_err()
        );
    }
}
