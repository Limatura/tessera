//! Signing in with GitHub or Discord, over HTTP, against a provider that runs.
//!
//! The plumbing these share with `OpenID` Connect is covered by `oidc.rs`: the
//! one-shot `state`, the registration policy, choosing a username, attaching to
//! an account that already exists. What is here is what those two do
//! differently, which is how they answer the question of who just signed in.
//!
//! GitHub is driven end to end, because `auth.github.base_url` exists for
//! GitHub Enterprise Server and a test can point it at a server it controls.
//! Discord has no self-hosted edition and so no such setting, and inventing one
//! to make a test easier would be a configuration knob that lies about why it
//! is there. Its adapter is exercised directly instead, which is where
//! everything Discord-specific lives.
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

/// An instance whose GitHub is the one this test is running.
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

/// The same setting for GitHub, and the one that matters most there: with no
/// groups to narrow it, `always` opens this instance to every GitHub account,
/// so it has to work exactly as written and nothing wider.
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

/// The whole point.
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
/// changed, and a released one can be claimed by somebody else, so keying on it
/// would hand an account to whoever picked the name up.
#[tokio::test]
async fn a_renamed_github_account_is_still_the_same_account() {
    let github = FakeOauth::start().await;
    let app = instance(&github).await;
    github.addresses(serde_json::json!([
        { "email": "alice@example.com", "primary": true, "verified": true },
    ]));

    github.is(serde_json::json!({ "id": 4_242, "login": "alice" }));
    let first = session_cookie(&come_back(&app, &leave(&app).await).await);

    // Same person, new name, and the address changed too.
    github.is(serde_json::json!({ "id": 4_242, "login": "alice-renamed" }));
    github.addresses(serde_json::json!([
        { "email": "elsewhere@example.com", "primary": true, "verified": true },
    ]));
    let second = session_cookie(&come_back(&app, &leave(&app).await).await);

    assert_eq!(id_of(&app, &first).await, id_of(&app, &second).await);
}

/// And two different accounts that happen to share a login are two accounts.
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

/// `GET /user` answers `email: null` when the address is private, which is the
/// default for plenty of accounts. The address has to come from
/// `GET /user/emails`, and this is that.
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

/// An address somebody added and never confirmed is not theirs. Taking it would
/// let anyone claim an account by typing in its owner's address.
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

/// And neither is one that is verified but belongs to a different entry than
/// the primary. Taking the first verified one instead would pick an address the
/// person may not use.
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

/// A token without `user:email` cannot read the addresses at all. Reported as
/// a missing address, which names a scope to add, rather than as a refusal.
#[tokio::test]
async fn a_token_that_cannot_read_addresses_says_no_address() {
    let github = FakeOauth::start().await;
    let app = instance(&github).await;
    github.is(serde_json::json!({ "id": 7, "login": "alice" }));
    // What the endpoint answers with when the scope is missing: not a list.
    github.addresses(serde_json::json!({ "message": "Requires authentication" }));

    let response = come_back(&app, &leave(&app).await).await;

    assert_eq!(rejection(&response).as_deref(), Some("no_address"));
}

/// The redirect carries no nonce, because there is no token to echo one back
/// in. What ties the callback to this sign-in is the state, and the flow row
/// makes that single-use.
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

/// A GitHub session records which provider started it, so a provider that later
/// signs somebody out cannot end another one's sessions.
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
/// pointing at one, which is why this is here rather than driven through the
/// endpoint like GitHub. What is Discord-specific is entirely in these
/// answers.
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

    /// An unverified address is dropped rather than carried with a flag.
    /// Carrying it would leave every caller to remember to check, and
    /// forgetting is an account takeover.
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

    /// `email` is null for some accounts even with the scope, and `verified`
    /// is absent rather than false on others.
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
        // Not a user object, which is what an expired token gets back.
        fake.is(serde_json::json!({ "message": "401: Unauthorized" }));

        assert!(
            discord::identify(&format!("{}/api", fake.base), "a-token")
                .await
                .is_err()
        );
    }
}
