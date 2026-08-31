//! Signing in through an identity provider, over HTTP, against a real one.
//!
//! The provider is `common::provider::FakeProvider`: a loopback server that
//! serves a discovery document and a token endpoint. Everything else is the
//! shipped code, so what these prove is the wiring the unit tests cannot reach,
//! which is the order the steps happen in and what each refusal does to the
//! browser.
//!
//! Every outcome here is a `303`, including every refusal. See `api/oidc.rs`
//! for why. The assertions read the `Location`.
//!
//! `clippy.toml`'s test exemption only reaches code inside a `#[test]` body,
//! and the helpers below sit outside one. Same reasoning as `common/mod.rs`: a
//! fixture that cannot build what it needs has nothing to report but a failure.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test fixture; see module docs"
)]

mod common;

use axum::{
    body::Body,
    http::{Method, StatusCode, header},
    response::Response,
};
use common::{
    Auth, TestApp, expect_status,
    provider::{FakeProvider, Script},
    session_cookie,
};
use tessera_server::config::Config;
use time::OffsetDateTime;

const CLIENT_ID: &str = "tessera";
const PUBLIC_URL: &str = "https://tessera.example";

/// An instance pointed at a provider that is actually running.
async fn instance(provider: &FakeProvider) -> TestApp {
    let issuer = provider.issuer.clone();
    TestApp::with_config(move |config: &mut Config| {
        PUBLIC_URL.clone_into(&mut config.server.public_url);
        config.auth.oidc.issuer = issuer;
        CLIENT_ID.clone_into(&mut config.auth.oidc.client_id);
        "a-secret".clone_into(&mut config.auth.oidc.client_secret);
    })
    .await
}

/// Where a response says the browser should go.
fn location(response: &Response<Body>) -> String {
    response
        .headers()
        .get(header::LOCATION)
        .expect("a Location header")
        .to_str()
        .expect("an ASCII location")
        .to_owned()
}

/// The reason carried back to the sign-in page, if there is one.
fn rejection(response: &Response<Body>) -> Option<String> {
    location(response)
        .split_once("oidc_error=")
        .map(|(_page, code)| code.to_owned())
}

/// What a sign-in carried out to the provider.
struct Leaving {
    state: String,
    nonce: String,
}

/// Walks the redirect out to the provider and reads what it carried.
async fn leave(app: &TestApp, uri: &str, auth: Auth<'_>) -> Leaving {
    let response = app.empty(Method::GET, uri, auth).await;
    assert_eq!(
        response.status(),
        StatusCode::SEE_OTHER,
        "expected to be sent to the provider, got {:?}",
        location(&response)
    );
    let url = url::Url::parse(&location(&response)).expect("an absolute redirect");
    let pair = |name: &str| {
        url.query_pairs()
            .find(|(key, _value)| key == name)
            .map_or_else(
                || panic!("a {name} parameter in {url}"),
                |(_key, value)| value.into_owned(),
            )
    };
    Leaving {
        state: pair("state"),
        nonce: pair("nonce"),
    }
}

/// Answers the callback the way the provider's redirect would.
///
/// The nonce goes in first, because a provider puts the one it was given into
/// the token it issues, and refusing a token without it is the point.
async fn come_back(app: &TestApp, provider: &FakeProvider, leaving: &Leaving) -> Response<Body> {
    provider.echo_nonce(&leaving.nonce);
    app.empty(
        Method::GET,
        &format!(
            "/api/v1/auth/oidc/callback?code=an-authorisation-code&state={}",
            leaving.state
        ),
        Auth::None,
    )
    .await
}

/// Writes one claim, so a test can leave one out or put in one that must be
/// refused. Indexing a `Value` panics on the wrong shape; this does not.
fn with_claim(claims: &mut serde_json::Value, key: &str, value: serde_json::Value) {
    if let Some(fields) = claims.as_object_mut() {
        fields.insert(key.to_owned(), value);
    }
}

/// The claims a provider would release for somebody it is happy with.
fn claims_for(subject: &str, email: &str) -> serde_json::Value {
    serde_json::json!({
        "sub": subject,
        "exp": OffsetDateTime::now_utc().unix_timestamp() + 300,
        "email": email,
        "email_verified": true,
        "preferred_username": "alice",
    })
}

/// The whole point: a person nobody here has seen arrives from the provider and
/// leaves with a session.
#[tokio::test]
async fn a_stranger_the_provider_vouches_for_gets_an_account() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = instance(&provider).await;
    provider.will_say(Script {
        claims: claims_for("sub-1", "alice@example.com"),
        ..Script::default()
    });

    let state = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;
    let response = come_back(&app, &provider, &state).await;

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&response), "/");
    let cookie = session_cookie(&response);

    // The session is real, and it belongs to an account built from the claims.
    let body = common::body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&cookie))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(common::field(&body, "username"), serde_json::json!("alice"));
    assert_eq!(
        common::field(&body, "email"),
        serde_json::json!("alice@example.com")
    );
}

/// And coming back a second time signs in as the same account rather than
/// making another one, because the subject is what it is keyed on.
#[tokio::test]
async fn the_same_subject_comes_back_to_the_same_account() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = instance(&provider).await;
    provider.will_say(Script {
        claims: claims_for("sub-1", "alice@example.com"),
        ..Script::default()
    });

    let first = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;
    let first_cookie = session_cookie(&come_back(&app, &provider, &first).await);

    // The provider now calls them something else. The subject is unchanged, so
    // it is still the same person.
    provider.will_say(Script {
        claims: serde_json::json!({
            "sub": "sub-1",
            "exp": OffsetDateTime::now_utc().unix_timestamp() + 300,
            "email": "alice.renamed@example.com",
            "email_verified": true,
            "preferred_username": "someone-else",
        }),
        ..Script::default()
    });
    let second = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;
    let second_cookie = session_cookie(&come_back(&app, &provider, &second).await);

    let first_id = common::field(&session_of(&app, &first_cookie).await, "id");
    let second_id = common::field(&session_of(&app, &second_cookie).await, "id");
    assert_eq!(first_id, second_id);
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

/// A callback opened twice, or replayed out of browser history, works once.
#[tokio::test]
async fn a_callback_cannot_be_replayed() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = instance(&provider).await;
    provider.will_say(Script {
        claims: claims_for("sub-1", "alice@example.com"),
        ..Script::default()
    });

    let state = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;
    assert!(
        come_back(&app, &provider, &state)
            .await
            .headers()
            .contains_key(header::SET_COOKIE)
    );

    let again = come_back(&app, &provider, &state).await;
    assert_eq!(rejection(&again).as_deref(), Some("stale"));
    assert!(!again.headers().contains_key(header::SET_COOKIE));
}

/// A callback for a sign-in this server never started.
#[tokio::test]
async fn a_callback_nobody_asked_for_is_refused() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = instance(&provider).await;

    for uri in [
        "/api/v1/auth/oidc/callback?code=c&state=never-issued",
        "/api/v1/auth/oidc/callback?code=c",
    ] {
        let response = app.empty(Method::GET, uri, Auth::None).await;
        assert_eq!(rejection(&response).as_deref(), Some("stale"), "for {uri}");
    }
}

/// The provider refusing is reported as a refusal, and the flow row goes with
/// it rather than staying answerable.
#[tokio::test]
async fn a_provider_that_refuses_is_reported_and_the_flow_is_spent() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = instance(&provider).await;
    provider.will_say(Script {
        refuse: Some("invalid_grant".to_owned()),
        ..Script::default()
    });

    let state = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;
    let response = come_back(&app, &provider, &state).await;
    assert_eq!(rejection(&response).as_deref(), Some("refused"));

    // Spent, not left behind: the same state is now unknown.
    provider.will_say(Script {
        claims: claims_for("sub-1", "alice@example.com"),
        ..Script::default()
    });
    assert_eq!(
        rejection(&come_back(&app, &provider, &state).await).as_deref(),
        Some("stale")
    );
}

/// The provider saying no in the redirect, rather than at the token endpoint.
#[tokio::test]
async fn a_provider_that_refuses_in_the_redirect_is_reported() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = instance(&provider).await;

    let state = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;
    let response = app
        .empty(
            Method::GET,
            &format!(
                "/api/v1/auth/oidc/callback?error=access_denied&state={}",
                state.state
            ),
            Auth::None,
        )
        .await;

    assert_eq!(rejection(&response).as_deref(), Some("refused"));
}

/// A token for a different sign-in must not be usable, which is what the nonce
/// is for. The provider here answers with one it made up.
#[tokio::test]
async fn a_token_that_belongs_to_another_sign_in_is_refused() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = instance(&provider).await;
    let mut claims = claims_for("sub-1", "alice@example.com");
    with_claim(
        &mut claims,
        "nonce",
        serde_json::json!("a-nonce-from-somewhere-else"),
    );
    provider.will_say(Script {
        claims,
        ..Script::default()
    });

    let state = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;

    assert_eq!(
        rejection(&come_back(&app, &provider, &state).await).as_deref(),
        Some("refused")
    );
}

/// The `email` scope is optional in the specification, and an account here
/// needs an address. The refusal names the case rather than saying "refused".
#[tokio::test]
async fn a_provider_that_releases_no_address_is_told_so() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = instance(&provider).await;
    provider.will_say(Script {
        claims: serde_json::json!({
            "sub": "sub-1",
            "exp": OffsetDateTime::now_utc().unix_timestamp() + 300,
        }),
        ..Script::default()
    });

    let state = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;

    assert_eq!(
        rejection(&come_back(&app, &provider, &state).await).as_deref(),
        Some("no_address")
    );
}

/// A provider that keeps the address out of the token releases it from
/// userinfo instead, and that is enough.
#[tokio::test]
async fn an_address_from_userinfo_is_enough() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = instance(&provider).await;
    provider.will_say(Script {
        claims: serde_json::json!({
            "sub": "sub-1",
            "exp": OffsetDateTime::now_utc().unix_timestamp() + 300,
        }),
        userinfo: Some(serde_json::json!({
            "email": "alice@example.com",
            "email_verified": true,
        })),
        ..Script::default()
    });

    let state = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;
    let response = come_back(&app, &provider, &state).await;

    assert_eq!(location(&response), "/");
    assert!(response.headers().contains_key(header::SET_COOKIE));
}

/// With a group required, somebody outside it is refused even though the
/// provider was happy to vouch for them.
#[tokio::test]
async fn somebody_outside_the_required_group_is_refused() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let issuer = provider.issuer.clone();
    let app = TestApp::with_config(move |config: &mut Config| {
        PUBLIC_URL.clone_into(&mut config.server.public_url);
        config.auth.oidc.issuer = issuer;
        CLIENT_ID.clone_into(&mut config.auth.oidc.client_id);
        "a-secret".clone_into(&mut config.auth.oidc.client_secret);
        "tessera-users".clone_into(&mut config.auth.oidc.required_group);
    })
    .await;

    let mut outside = claims_for("sub-1", "alice@example.com");
    with_claim(&mut outside, "groups", serde_json::json!(["everyone-else"]));
    provider.will_say(Script {
        claims: outside,
        ..Script::default()
    });
    let state = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;
    assert_eq!(
        rejection(&come_back(&app, &provider, &state).await).as_deref(),
        Some("not_permitted")
    );

    // And somebody inside it is not.
    let mut inside = claims_for("sub-2", "bob@example.com");
    with_claim(
        &mut inside,
        "groups",
        serde_json::json!(["staff", "tessera-users"]),
    );
    provider.will_say(Script {
        claims: inside,
        ..Script::default()
    });
    let state = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;
    assert_eq!(location(&come_back(&app, &provider, &state).await), "/");
}

/// An instance that admits nobody new admits nobody new through the provider
/// either. Being vouched for is not being invited.
#[tokio::test]
async fn a_closed_instance_does_not_register_from_the_provider() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let issuer = provider.issuer.clone();
    let app = TestApp::with_config(move |config: &mut Config| {
        PUBLIC_URL.clone_into(&mut config.server.public_url);
        config.auth.oidc.issuer = issuer;
        CLIENT_ID.clone_into(&mut config.auth.oidc.client_id);
        "a-secret".clone_into(&mut config.auth.oidc.client_secret);
        config.auth.registration = tessera_server::config::RegistrationPolicy::Closed;
    })
    .await;
    provider.will_say(Script {
        claims: claims_for("sub-1", "alice@example.com"),
        ..Script::default()
    });

    let state = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;

    assert_eq!(
        rejection(&come_back(&app, &provider, &state).await).as_deref(),
        Some("not_invited")
    );
}

/// With no provider configured there is nothing to send anybody to, and the
/// sign-in page is told rather than left waiting.
#[tokio::test]
async fn an_instance_with_no_provider_says_so() {
    let app = TestApp::ready().await;

    let body = common::body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/auth/oidc", Auth::None)
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(common::field(&body, "enabled"), serde_json::json!(false));

    let response = app
        .empty(Method::GET, "/api/v1/auth/oidc/start", Auth::None)
        .await;
    assert_eq!(rejection(&response).as_deref(), Some("unavailable"));
}

/// Configured, and the sign-in page is told what to write on the button.
#[tokio::test]
async fn a_configured_instance_advertises_its_provider() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = instance(&provider).await;

    let body = common::body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/auth/oidc", Auth::None)
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;

    assert_eq!(common::field(&body, "enabled"), serde_json::json!(true));
    assert!(
        common::field(&body, "display_name")
            .as_str()
            .is_some_and(|name| !name.is_empty())
    );
}

/// Attaching a provider is a way into the account, so a leaked API token must
/// not be able to add one.
#[tokio::test]
async fn attaching_a_provider_needs_the_browser_credential() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = instance(&provider).await;
    let cookie = register(&app, "alice").await;

    assert_eq!(
        app.empty(Method::GET, "/api/v1/auth/oidc/link", Auth::None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    // With the cookie it leaves for the provider.
    let response = app
        .empty(Method::GET, "/api/v1/auth/oidc/link", Auth::Cookie(&cookie))
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(location(&response).starts_with(&provider.issuer));
}

/// Somebody who registered with a password attaches the provider and keeps the
/// account they already had.
#[tokio::test]
async fn a_password_account_can_attach_a_provider_and_detach_it() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = instance(&provider).await;
    let cookie = register(&app, "alice").await;
    provider.will_say(Script {
        claims: claims_for("sub-1", "someone@example.com"),
        ..Script::default()
    });

    let state = leave(&app, "/api/v1/auth/oidc/link", Auth::Cookie(&cookie)).await;
    let response = come_back(&app, &provider, &state).await;
    // Already signed in, so nothing is issued and the account page is where
    // this belongs.
    assert_eq!(location(&response), "/account");
    assert!(!response.headers().contains_key(header::SET_COOKIE));

    let attached = identities(&app, &cookie).await;
    assert_eq!(attached.as_array().map(Vec::len), Some(1));
    // A password is still there, so this is not the only way in.
    assert_eq!(
        attached[0].get("is_last_way_in"),
        Some(&serde_json::json!(false))
    );

    assert_eq!(
        app.empty(
            Method::DELETE,
            "/api/v1/auth/oidc/identities/oidc",
            Auth::Cookie(&cookie)
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        identities(&app, &cookie).await.as_array().map(Vec::len),
        Some(0)
    );
}

/// An account with no password and one identity cannot detach it, because
/// nothing would be left to sign in with.
#[tokio::test]
async fn the_last_way_into_an_account_cannot_be_detached() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = instance(&provider).await;
    provider.will_say(Script {
        claims: claims_for("sub-1", "alice@example.com"),
        ..Script::default()
    });

    let state = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;
    let cookie = session_cookie(&come_back(&app, &provider, &state).await);

    let attached = identities(&app, &cookie).await;
    assert_eq!(
        attached[0].get("is_last_way_in"),
        Some(&serde_json::json!(true)),
        "the page has to be able to grey the button out: {attached}"
    );

    let refused = app
        .empty(
            Method::DELETE,
            "/api/v1/auth/oidc/identities/oidc",
            Auth::Cookie(&cookie),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::CONFLICT);
    // And it really is still attached.
    assert_eq!(
        identities(&app, &cookie).await.as_array().map(Vec::len),
        Some(1)
    );
}

/// Detaching something that was never attached is a `404`, not a silent
/// success.
#[tokio::test]
async fn detaching_what_was_never_attached_is_not_found() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = instance(&provider).await;
    let cookie = register(&app, "alice").await;

    assert_eq!(
        app.empty(
            Method::DELETE,
            "/api/v1/auth/oidc/identities/oidc",
            Auth::Cookie(&cookie)
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
}

/// A subject already attached to one account is refused rather than moved.
/// Moving it would sign two people in as one.
#[tokio::test]
async fn a_subject_cannot_be_attached_to_a_second_account() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = instance(&provider).await;
    provider.will_say(Script {
        claims: claims_for("sub-1", "alice@example.com"),
        ..Script::default()
    });
    let state = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;
    come_back(&app, &provider, &state).await;

    // A second account, with a password, tries to claim the same subject.
    let bob = register(&app, "bob").await;
    let state = leave(&app, "/api/v1/auth/oidc/link", Auth::Cookie(&bob)).await;
    let response = come_back(&app, &provider, &state).await;

    assert_eq!(rejection(&response).as_deref(), Some("already_linked"));
    assert_eq!(
        identities(&app, &bob).await.as_array().map(Vec::len),
        Some(0)
    );
}

/// Nothing here exists on an instance that has no accounts at all.
#[tokio::test]
async fn an_instance_without_accounts_has_none_of_this() {
    let app = TestApp::with_config(|config: &mut Config| {
        config.features.accounts = false;
    })
    .await;

    for uri in [
        "/api/v1/auth/oidc",
        "/api/v1/auth/oidc/start",
        "/api/v1/auth/oidc/callback?code=c&state=s",
    ] {
        assert_eq!(
            app.empty(Method::GET, uri, Auth::None).await.status(),
            StatusCode::NOT_FOUND,
            "for {uri}"
        );
    }
}

/// What an account signs in through.
async fn identities(app: &TestApp, cookie: &str) -> serde_json::Value {
    common::body_json(
        expect_status(
            app.empty(
                Method::GET,
                "/api/v1/auth/oidc/identities",
                Auth::Cookie(cookie),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await
}

/// An ordinary account with a password, and the cookie it registered with.
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

/// The provider telling us somebody has signed out, over the endpoint it posts
/// to. The signature is real here, unlike the ID token, because this is the one
/// token nothing else vouches for.
mod backchannel {
    use axum::http::{Method, Request, StatusCode, header};

    use super::{
        Auth, CLIENT_ID, FakeProvider, Script, TestApp, claims_for, come_back, instance, leave,
        location, session_cookie, with_claim,
    };

    /// Signs somebody in through the provider and returns their cookie.
    async fn signed_in(app: &TestApp, provider: &FakeProvider, subject: &str) -> String {
        provider.will_say(Script {
            claims: claims_for(subject, &format!("{subject}@example.com")),
            ..Script::default()
        });
        let leaving = leave(app, "/api/v1/auth/oidc/start", Auth::None).await;
        let response = come_back(app, provider, &leaving).await;
        assert_eq!(location(&response), "/");
        session_cookie(&response)
    }

    /// The same, with the provider naming its own session identifier.
    async fn signed_in_with_sid(
        app: &TestApp,
        provider: &FakeProvider,
        subject: &str,
        sid: &str,
    ) -> String {
        let mut claims = claims_for(subject, &format!("{subject}@example.com"));
        with_claim(&mut claims, "sid", serde_json::json!(sid));
        provider.will_say(Script {
            claims,
            ..Script::default()
        });
        let leaving = leave(app, "/api/v1/auth/oidc/start", Auth::None).await;
        session_cookie(&come_back(app, provider, &leaving).await)
    }

    /// Posts a logout token the way a provider would.
    async fn push(app: &TestApp, token: &str) -> axum::http::Response<axum::body::Body> {
        let body = form_urlencoded::Serializer::new(String::new())
            .append_pair("logout_token", token)
            .finish();
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/oidc/backchannel-logout")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded"),
            axum::body::Body::from(body),
        )
        .await
    }

    /// Whether a session is still accepted.
    async fn still_valid(app: &TestApp, cookie: &str) -> bool {
        app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(cookie))
            .await
            .status()
            == StatusCode::OK
    }

    /// The whole point: the provider says somebody is out, and they are.
    #[tokio::test]
    async fn a_subject_logout_ends_that_persons_provider_sessions() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;
        let alice = signed_in(&app, &provider, "alice").await;
        let bob = signed_in(&app, &provider, "bob").await;

        let token = FakeProvider::logout_token(
            &provider.logout_claims(CLIENT_ID, &serde_json::json!({ "sub": "alice" })),
        );
        assert_eq!(push(&app, &token).await.status(), StatusCode::OK);

        assert!(
            !still_valid(&app, &alice).await,
            "alice should be signed out"
        );
        assert!(still_valid(&app, &bob).await, "bob was not named");
    }

    /// A password session is not the provider's to end.
    #[tokio::test]
    async fn a_logout_leaves_password_sessions_alone() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;
        // One account, two ways in: a password, and the provider.
        let with_password = super::register(&app, "alice").await;
        provider.will_say(Script {
            claims: claims_for("sub-1", "someone@example.com"),
            ..Script::default()
        });
        let leaving = leave(&app, "/api/v1/auth/oidc/link", Auth::Cookie(&with_password)).await;
        come_back(&app, &provider, &leaving).await;

        let token = FakeProvider::logout_token(
            &provider.logout_claims(CLIENT_ID, &serde_json::json!({ "sub": "sub-1" })),
        );
        assert_eq!(push(&app, &token).await.status(), StatusCode::OK);

        assert!(
            still_valid(&app, &with_password).await,
            "the provider has no say over a credential it did not issue"
        );
    }

    /// A `sid` names one session, not everything that person has open.
    #[tokio::test]
    async fn a_session_logout_ends_only_the_one_it_names() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;
        let laptop = signed_in_with_sid(&app, &provider, "alice", "laptop").await;
        let phone = signed_in_with_sid(&app, &provider, "alice", "phone").await;

        let token = FakeProvider::logout_token(
            &provider.logout_claims(CLIENT_ID, &serde_json::json!({ "sid": "laptop" })),
        );
        assert_eq!(push(&app, &token).await.status(), StatusCode::OK);

        assert!(!still_valid(&app, &laptop).await);
        assert!(still_valid(&app, &phone).await, "only the laptop was named");
    }

    /// The signature is the only thing that says who sent this, so a token
    /// signed by a key the provider never published is worth nothing.
    #[tokio::test]
    async fn a_token_signed_by_an_unknown_key_is_refused() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;
        let alice = signed_in(&app, &provider, "alice").await;

        let token = FakeProvider::logout_token_with_unknown_key(
            &provider.logout_claims(CLIENT_ID, &serde_json::json!({ "sub": "alice" })),
        );
        assert_eq!(push(&app, &token).await.status(), StatusCode::BAD_REQUEST);
        assert!(still_valid(&app, &alice).await);
    }

    /// The forgery a key lookup cannot catch: the key id names a key the
    /// provider really published, and the signature was made with another one.
    /// Only verifying it says no.
    #[tokio::test]
    async fn a_token_signed_by_an_impostor_is_refused() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;
        let alice = signed_in(&app, &provider, "alice").await;

        let token = FakeProvider::forged_logout_token(
            &provider.logout_claims(CLIENT_ID, &serde_json::json!({ "sub": "alice" })),
        );
        assert_eq!(push(&app, &token).await.status(), StatusCode::BAD_REQUEST);
        assert!(still_valid(&app, &alice).await);
    }

    /// The same replay bound as the subject case, on the path that names one
    /// session. They are two queries and only one of them was covered.
    #[tokio::test]
    async fn a_kept_token_cannot_end_a_named_session_made_after_it() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;
        let mut claims = provider.logout_claims(CLIENT_ID, &serde_json::json!({ "sid": "laptop" }));
        with_claim(
            &mut claims,
            "iat",
            serde_json::json!(time::OffsetDateTime::now_utc().unix_timestamp() - 60),
        );
        let token = FakeProvider::logout_token(&claims);

        let laptop = signed_in_with_sid(&app, &provider, "alice", "laptop").await;
        assert_eq!(push(&app, &token).await.status(), StatusCode::OK);

        assert!(
            still_valid(&app, &laptop).await,
            "a token older than the session must not end it"
        );
    }

    /// An unsigned token is the classic way in, and is refused before any key
    /// is looked up.
    #[tokio::test]
    async fn an_unsigned_token_is_refused() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;
        let alice = signed_in(&app, &provider, "alice").await;

        let claims = provider.logout_claims(CLIENT_ID, &serde_json::json!({ "sub": "alice" }));
        let unsigned = format!(
            "{}.{}.",
            base64_url(br#"{"alg":"none","typ":"JWT"}"#),
            base64_url(claims.to_string().as_bytes())
        );
        assert_eq!(
            push(&app, &unsigned).await.status(),
            StatusCode::BAD_REQUEST
        );
        assert!(still_valid(&app, &alice).await);
    }

    fn base64_url(bytes: &[u8]) -> String {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// An ID token carries a `nonce`, and the specification forbids one here so
    /// that an ID token cannot be replayed as a logout.
    #[tokio::test]
    async fn an_id_token_replayed_as_a_logout_is_refused() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;
        let alice = signed_in(&app, &provider, "alice").await;

        let mut claims = provider.logout_claims(CLIENT_ID, &serde_json::json!({ "sub": "alice" }));
        with_claim(&mut claims, "nonce", serde_json::json!("a-nonce"));
        assert_eq!(
            push(&app, &FakeProvider::logout_token(&claims))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert!(still_valid(&app, &alice).await);
    }

    /// Some other token the provider signed for us is not a logout.
    #[tokio::test]
    async fn a_token_without_the_logout_event_is_refused() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;
        let alice = signed_in(&app, &provider, "alice").await;

        let mut claims = provider.logout_claims(CLIENT_ID, &serde_json::json!({ "sub": "alice" }));
        with_claim(
            &mut claims,
            "events",
            serde_json::json!({ "something-else": {} }),
        );
        assert_eq!(
            push(&app, &FakeProvider::logout_token(&claims))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert!(still_valid(&app, &alice).await);
    }

    /// Signed by a key we trust, but claiming to come from somewhere else.
    /// That is where a mix-up starts, and the issuer check is what stops it.
    #[tokio::test]
    async fn a_token_from_another_issuer_is_refused() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;
        let alice = signed_in(&app, &provider, "alice").await;

        let token = FakeProvider::logout_token(&provider.logout_claims(
            CLIENT_ID,
            &serde_json::json!({ "sub": "alice", "iss": "https://elsewhere.example" }),
        ));
        assert_eq!(push(&app, &token).await.status(), StatusCode::BAD_REQUEST);
        assert!(still_valid(&app, &alice).await);
    }

    /// Issued for somebody else's client at the same provider.
    #[tokio::test]
    async fn a_token_for_another_client_is_refused() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;
        let alice = signed_in(&app, &provider, "alice").await;

        let token = FakeProvider::logout_token(
            &provider.logout_claims("somebody-else", &serde_json::json!({ "sub": "alice" })),
        );
        assert_eq!(push(&app, &token).await.status(), StatusCode::BAD_REQUEST);
        assert!(still_valid(&app, &alice).await);
    }

    /// A body with no token in it. Answered in the same shape as everything
    /// else here: `Form<T>` would reject it with its own 422 and its own words,
    /// which a provider cannot read and would retry for ever.
    #[tokio::test]
    async fn a_body_with_no_token_is_refused_in_the_same_shape() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;

        for body in ["", "something_else=1", "logout_token="] {
            let response = app
                .send(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/api/v1/auth/oidc/backchannel-logout")
                        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded"),
                    axum::body::Body::from(body),
                )
                .await;
            assert_eq!(
                response.status(),
                StatusCode::BAD_REQUEST,
                "for a body of {body:?}"
            );
            let reported = super::common::body_json(response).await;
            assert_eq!(
                reported.get("error"),
                Some(&serde_json::json!("invalid_request"))
            );
        }
    }

    /// A token that names nobody has nothing to act on.
    #[tokio::test]
    async fn a_token_naming_nobody_is_refused() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;

        let token =
            FakeProvider::logout_token(&provider.logout_claims(CLIENT_ID, &serde_json::json!({})));
        assert_eq!(push(&app, &token).await.status(), StatusCode::BAD_REQUEST);
    }

    /// A kept copy cannot be used later to end a session made since. Without
    /// this, replaying an old token would be a way to keep signing somebody out.
    #[tokio::test]
    async fn a_kept_token_cannot_end_a_session_made_after_it() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;
        let mut claims = provider.logout_claims(CLIENT_ID, &serde_json::json!({ "sub": "alice" }));
        // Issued a minute ago, and still inside the age bound.
        with_claim(
            &mut claims,
            "iat",
            serde_json::json!(time::OffsetDateTime::now_utc().unix_timestamp() - 60),
        );
        let token = FakeProvider::logout_token(&claims);

        // The session starts *after* the token was issued.
        let alice = signed_in(&app, &provider, "alice").await;
        assert_eq!(push(&app, &token).await.status(), StatusCode::OK);

        assert!(
            still_valid(&app, &alice).await,
            "a token older than the session must not end it"
        );
    }

    /// One somebody kept for hours is refused outright.
    #[tokio::test]
    async fn a_stale_token_is_refused() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;
        let alice = signed_in(&app, &provider, "alice").await;

        let mut claims = provider.logout_claims(CLIENT_ID, &serde_json::json!({ "sub": "alice" }));
        with_claim(
            &mut claims,
            "iat",
            serde_json::json!(time::OffsetDateTime::now_utc().unix_timestamp() - 3600),
        );
        assert_eq!(
            push(&app, &FakeProvider::logout_token(&claims))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert!(still_valid(&app, &alice).await);
    }

    /// A subject nobody here has ever signed in as is reported as success. The
    /// provider asked for a state that already holds, and a refusal would have
    /// it retry something that cannot change.
    #[tokio::test]
    async fn a_logout_for_somebody_we_do_not_know_is_not_an_error() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;

        let token = FakeProvider::logout_token(
            &provider.logout_claims(CLIENT_ID, &serde_json::json!({ "sub": "a-stranger" })),
        );

        assert_eq!(push(&app, &token).await.status(), StatusCode::OK);
    }

    /// Refusals answer in the shape the specification says a provider reads.
    #[tokio::test]
    async fn a_refusal_is_in_the_shape_a_provider_expects() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;

        let mut claims = provider.logout_claims(CLIENT_ID, &serde_json::json!({ "sub": "alice" }));
        with_claim(&mut claims, "events", serde_json::json!({}));
        let response = push(&app, &FakeProvider::logout_token(&claims)).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("no-store")
        );
        let body = super::common::body_json(response).await;
        assert_eq!(
            body.get("error"),
            Some(&serde_json::json!("invalid_request"))
        );
        assert!(body.get("error_description").is_some());
    }
}
