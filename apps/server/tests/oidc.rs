//! Signing in through an identity provider, over HTTP, against a real one.
//!
//! The provider is `common::provider::FakeProvider`: a loopback server with a
//! discovery document and a token endpoint. Everything else is the shipped
//! code, so these cover what unit tests cannot: the order of the steps and
//! what each refusal does to the browser.
//!
//! Every outcome here is a `303`, refusals included (see `api/oidc.rs` for
//! why). The assertions read the `Location`.
//!
//! `clippy.toml`'s test exemption only covers code inside a `#[test]` body,
//! and the helpers below are outside one. Same reasoning as `common/mod.rs`: a
//! fixture that cannot build what it needs can only fail.
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
    Auth, TestApp, body_json, body_string, expect_status,
    provider::{FakeProvider, Script},
    session_cookie,
};
use tessera_server::config::{Config, LinkByEmail};
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

/// Reading a provider's document from the server. That is the point: the
/// panel's own fetch is blocked by providers without CORS, and the server has
/// no origin to be blocked for.
#[tokio::test]
async fn the_panel_can_read_a_provider_document_through_the_server() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = TestApp::ready().await;
    let owner = register(&app, "first").await;

    for given in [
        provider.issuer.clone(),
        format!("{}/.well-known/openid-configuration", provider.issuer),
        format!("{}/", provider.issuer),
    ] {
        let read = body_json(
            expect_status(
                app.json(
                    Method::POST,
                    "/api/v1/admin/settings/oidc/discovery",
                    Auth::Cookie(&owner),
                    &serde_json::json!({ "url": given }),
                )
                .await,
                StatusCode::OK,
            )
            .await,
        )
        .await;
        assert_eq!(read["issuer"], provider.issuer);
        assert_eq!(read["backchannel_logout_supported"], true);
        assert_eq!(
            read["scopes_supported"],
            serde_json::json!(["openid", "email", "profile"])
        );
        // Lets the panel warn, before any refusal, that this provider cannot
        // vouch for an address.
        assert_eq!(
            read["claims_supported"],
            serde_json::json!(["sub", "email", "groups"])
        );
    }
}

/// What it must not do: leave the tried-out document where a sign-in would
/// pick it up. The configured provider is a different one and stays so.
#[tokio::test]
async fn reading_a_document_does_not_decide_what_sign_in_talks_to() {
    let configured = FakeProvider::start(CLIENT_ID).await;
    let other = FakeProvider::start(CLIENT_ID).await;
    let app = instance(&configured).await;
    let owner = register(&app, "first").await;

    expect_status(
        app.json(
            Method::POST,
            "/api/v1/admin/settings/oidc/discovery",
            Auth::Cookie(&owner),
            &serde_json::json!({ "url": other.issuer }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    let used = app
        .state
        .oidc
        .provider(&configured.issuer, OffsetDateTime::now_utc())
        .await
        .expect("the configured provider");
    assert_eq!(used.issuer, configured.issuer);
}

#[tokio::test]
async fn an_address_the_document_cannot_be_read_from_is_refused() {
    let app = TestApp::ready().await;
    let owner = register(&app, "first").await;
    let member = register(&app, "second").await;

    let refuse = async |auth: Auth<'_>, url: &str, expected: StatusCode| {
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/admin/settings/oidc/discovery",
                auth,
                &serde_json::json!({ "url": url }),
            )
            .await,
            expected,
        )
        .await
    };

    // Not https and not a private address: the rule the issuer setting enforces.
    let plain = refuse(
        Auth::Cookie(&owner),
        "http://id.example.com",
        StatusCode::BAD_REQUEST,
    )
    .await;
    assert!(body_string(plain).await.contains("https"));
    refuse(Auth::Cookie(&owner), "   ", StatusCode::BAD_REQUEST).await;
    // Nothing listening: the provider's own failure, reported as is.
    refuse(
        Auth::Cookie(&owner),
        "http://127.0.0.1:1",
        StatusCode::BAD_REQUEST,
    )
    .await;
    // And not a fetch just anyone can make this server perform.
    refuse(
        Auth::Cookie(&member),
        "http://127.0.0.1:1",
        StatusCode::FORBIDDEN,
    )
    .await;
    refuse(Auth::None, "http://127.0.0.1:1", StatusCode::UNAUTHORIZED).await;
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

/// Answers the callback as the provider's redirect would.
///
/// The nonce goes in first: a provider puts the one it was given into the
/// token, and refusing a token without it is what is being tested.
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

/// The claims a provider would release for a user it accepts.
fn claims_for(subject: &str, email: &str) -> serde_json::Value {
    serde_json::json!({
        "sub": subject,
        "exp": OffsetDateTime::now_utc().unix_timestamp() + 300,
        "email": email,
        "email_verified": true,
        "preferred_username": "alice",
    })
}

/// The main path: a user not seen here before arrives from the provider and
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

    // The session is real, and belongs to an account built from the claims.
    let body = body_json(
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

/// Coming back a second time signs in as the same account instead of creating
/// another, because the subject is the key.
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

    // The provider now uses a different name. The subject is unchanged, so it
    // is still the same user.
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
    body_json(
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

/// A provider refusal is reported as such, and the flow row is removed with it
/// instead of staying answerable.
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

    // Used up, not left behind: the same state is now unknown.
    provider.will_say(Script {
        claims: claims_for("sub-1", "alice@example.com"),
        ..Script::default()
    });
    assert_eq!(
        rejection(&come_back(&app, &provider, &state).await).as_deref(),
        Some("stale")
    );
}

/// The provider refusing in the redirect, not at the token endpoint.
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

/// A token from another sign-in must not work; that is what the nonce is for.
/// The provider here answers with a made-up one.
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

/// The `email` scope is optional in the spec, and an account here needs an
/// address. The refusal names the case instead of just saying "refused".
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

/// A provider that leaves the address out of the token releases it from
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

/// An instance whose provider may attach sign-ins to existing accounts as
/// `link` says, with a password account already holding `alice@example.com`.
async fn with_alice(provider: &FakeProvider, link: LinkByEmail) -> TestApp {
    let issuer = provider.issuer.clone();
    let app = TestApp::with_config(move |config: &mut Config| {
        PUBLIC_URL.clone_into(&mut config.server.public_url);
        config.auth.oidc.issuer = issuer;
        CLIENT_ID.clone_into(&mut config.auth.oidc.client_id);
        "a-secret".clone_into(&mut config.auth.oidc.client_secret);
        config.auth.oidc.link_by_email = link;
    })
    .await;
    register(&app, "alice").await;
    app
}

/// Signs in through the provider as a user at `alice@example.com`, with
/// `verified` as the provider's `email_verified`, or no claim at all.
async fn arrives_as_alice(
    app: &TestApp,
    provider: &FakeProvider,
    verified: Option<bool>,
) -> Response<Body> {
    let mut claims = claims_for("sub-alice", "alice@example.com");
    match verified {
        Some(verified) => with_claim(&mut claims, "email_verified", serde_json::json!(verified)),
        None => {
            if let Some(fields) = claims.as_object_mut() {
                fields.remove("email_verified");
            }
        }
    }
    provider.will_say(Script {
        claims,
        ..Script::default()
    });
    let state = leave(app, "/api/v1/auth/oidc/start", Auth::None).await;
    come_back(app, provider, &state).await
}

/// Whether the sign-in landed on the account registered with a password.
async fn landed_on_alice(app: &TestApp, response: &Response<Body>) -> bool {
    if location(response) != "/" {
        return false;
    }
    let body = body_json(
        app.empty(
            Method::GET,
            "/api/v1/auth/session",
            Auth::Cookie(&session_cookie(response)),
        )
        .await,
    )
    .await;
    common::field(&body, "username") == serde_json::json!("alice")
}

/// Synology's SSO Server has no `email_verified` claim, Entra ID omits it, and
/// Authentik sends `false` by default. With those the old switch did nothing
/// either way; these are the three values that replaced it.
#[tokio::test]
async fn how_far_a_provider_is_trusted_decides_whether_it_links() {
    for (link, verified, links) in [
        (LinkByEmail::Never, Some(true), false),
        (LinkByEmail::WhenVerified, Some(true), true),
        (LinkByEmail::WhenVerified, None, false),
        (LinkByEmail::WhenVerified, Some(false), false),
        (LinkByEmail::Always, None, true),
        (LinkByEmail::Always, Some(false), true),
    ] {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = with_alice(&provider, link).await;
        let response = arrives_as_alice(&app, &provider, verified).await;
        if links {
            assert!(
                landed_on_alice(&app, &response).await,
                "{link:?} with {verified:?} should have linked"
            );
        } else {
            assert_eq!(
                rejection(&response).as_deref(),
                Some("address_taken"),
                "{link:?} with {verified:?} should have been refused"
            );
        }
    }
}

/// A provider that puts the address in the token but the verified flag only in
/// userinfo. Reading the flag only when the token lacked an address left these
/// unverified for good.
#[tokio::test]
async fn a_verdict_from_userinfo_counts_for_an_address_from_the_token() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = with_alice(&provider, LinkByEmail::WhenVerified).await;
    let mut claims = claims_for("sub-alice", "alice@example.com");
    if let Some(fields) = claims.as_object_mut() {
        fields.remove("email_verified");
    }
    provider.will_say(Script {
        claims,
        userinfo: Some(serde_json::json!({
            "email": "alice@example.com",
            "email_verified": true,
        })),
        ..Script::default()
    });
    let state = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;
    let response = come_back(&app, &provider, &state).await;
    assert!(landed_on_alice(&app, &response).await);
}

/// But only a verdict on the same address. Userinfo vouching for a different
/// address says nothing about the one in the token.
#[tokio::test]
async fn a_verdict_about_another_address_does_not_count() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = with_alice(&provider, LinkByEmail::WhenVerified).await;
    let mut claims = claims_for("sub-alice", "alice@example.com");
    if let Some(fields) = claims.as_object_mut() {
        fields.remove("email_verified");
    }
    provider.will_say(Script {
        claims,
        userinfo: Some(serde_json::json!({
            "email": "someone-else@example.com",
            "email_verified": true,
        })),
        ..Script::default()
    });
    let state = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;
    let response = come_back(&app, &provider, &state).await;
    assert_eq!(rejection(&response).as_deref(), Some("address_taken"));
}

/// With a group required, a user outside it is refused even though the
/// provider vouched for them.
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

    // And a user inside it is not.
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

/// An instance closed to new accounts is closed to them through the provider
/// too. Being vouched for is not being invited.
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

/// An instance run from its own identity provider: no sign-up form, and users
/// the provider admits get an account on first sign-in without an invitation.
/// `auth.registration` alone could not express that; the per-provider setting
/// can, in both directions.
#[tokio::test]
async fn a_provider_can_make_accounts_whatever_registration_says() {
    use tessera_server::config::{ProviderRegistration, RegistrationPolicy};

    for (registration, provider_says, makes_one) in [
        (
            RegistrationPolicy::Closed,
            ProviderRegistration::Always,
            true,
        ),
        (
            RegistrationPolicy::Invite,
            ProviderRegistration::Always,
            true,
        ),
        (RegistrationPolicy::Open, ProviderRegistration::Never, false),
        (
            RegistrationPolicy::Closed,
            ProviderRegistration::Inherit,
            false,
        ),
        (
            RegistrationPolicy::Invite,
            ProviderRegistration::Inherit,
            false,
        ),
        (
            RegistrationPolicy::Open,
            ProviderRegistration::Inherit,
            true,
        ),
    ] {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let issuer = provider.issuer.clone();
        let app = TestApp::with_config(move |config: &mut Config| {
            PUBLIC_URL.clone_into(&mut config.server.public_url);
            config.auth.oidc.issuer = issuer;
            CLIENT_ID.clone_into(&mut config.auth.oidc.client_id);
            "a-secret".clone_into(&mut config.auth.oidc.client_secret);
            config.auth.registration = registration;
            config.auth.oidc.allow_registration = provider_says;
        })
        .await;
        provider.will_say(Script {
            claims: claims_for("sub-1", "alice@example.com"),
            ..Script::default()
        });
        let state = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;
        let response = come_back(&app, &provider, &state).await;
        if makes_one {
            assert_eq!(
                location(&response),
                "/",
                "{registration:?} with {provider_says:?} should have made an account"
            );
        } else {
            assert_eq!(
                rejection(&response).as_deref(),
                Some("not_invited"),
                "{registration:?} with {provider_says:?} should have been refused"
            );
        }
    }
}

/// `never` is about new accounts. A user who already signs in this way keeps
/// doing so.
#[tokio::test]
async fn never_still_lets_an_existing_identity_in() {
    use tessera_server::config::ProviderRegistration;

    let provider = FakeProvider::start(CLIENT_ID).await;
    let issuer = provider.issuer.clone();
    let app = TestApp::with_config(move |config: &mut Config| {
        PUBLIC_URL.clone_into(&mut config.server.public_url);
        config.auth.oidc.issuer = issuer;
        CLIENT_ID.clone_into(&mut config.auth.oidc.client_id);
        "a-secret".clone_into(&mut config.auth.oidc.client_secret);
    })
    .await;
    provider.will_say(Script {
        claims: claims_for("sub-1", "alice@example.com"),
        ..Script::default()
    });
    let first = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;
    assert_eq!(location(&come_back(&app, &provider, &first).await), "/");

    let mut config = Config::clone(&app.state.config());
    config.auth.oidc.allow_registration = ProviderRegistration::Never;
    app.state.replace_config(std::sync::Arc::new(config));

    provider.will_say(Script {
        claims: claims_for("sub-1", "alice@example.com"),
        ..Script::default()
    });
    let again = leave(&app, "/api/v1/auth/oidc/start", Auth::None).await;
    assert_eq!(location(&come_back(&app, &provider, &again).await), "/");
}

/// With no provider configured there is nowhere to send anyone, and the
/// sign-in page is told so instead of being left waiting.
#[tokio::test]
async fn an_instance_with_no_provider_says_so() {
    let app = TestApp::ready().await;

    // Every provider is listed, disabled ones included: a page cannot know
    // without asking, and an empty list would need interpreting.
    let listed = providers(&app).await;
    assert_eq!(listed.len(), 3, "{listed:?}");
    assert!(
        listed
            .iter()
            .all(|entry| entry["enabled"] == serde_json::json!(false)),
        "{listed:?}"
    );

    for provider in ["oidc", "github", "discord"] {
        let response = app
            .empty(
                Method::GET,
                &format!("/api/v1/auth/{provider}/start"),
                Auth::None,
            )
            .await;
        assert_eq!(
            rejection(&response).as_deref(),
            Some("unavailable"),
            "for {provider}"
        );
    }
}

/// Configured, and the sign-in page is told what to write on the button.
#[tokio::test]
async fn a_configured_instance_advertises_its_provider() {
    let provider = FakeProvider::start(CLIENT_ID).await;
    let app = instance(&provider).await;

    let listed = providers(&app).await;
    let oidc = listed
        .iter()
        .find(|entry| entry["provider"] == "oidc")
        .expect("oidc should be listed");

    assert_eq!(oidc["enabled"], serde_json::json!(true));
    assert!(
        oidc["display_name"]
            .as_str()
            .is_some_and(|it| !it.is_empty()),
        "a button with nothing written on it: {oidc}"
    );
    // The other two are listed as disabled, since nothing configured them, and
    // still have a name so a page never draws a blank button.
    for name in ["github", "discord"] {
        let entry = listed
            .iter()
            .find(|entry| entry["provider"] == name)
            .unwrap_or_else(|| panic!("{name} should be listed"));
        assert_eq!(entry["enabled"], serde_json::json!(false));
        assert!(
            entry["display_name"]
                .as_str()
                .is_some_and(|it| !it.is_empty())
        );
    }
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

/// A user who registered with a password links the provider and keeps their
/// existing account.
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
    // Already signed in, so no session is issued and the account page is the
    // destination.
    assert_eq!(location(&response), "/account");
    assert!(!response.headers().contains_key(header::SET_COOKIE));

    let attached = identities(&app, &cookie).await;
    assert_eq!(attached.as_array().map(Vec::len), Some(1));
    // A password still exists, so this is not the only way in.
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

/// An account with no password and one identity cannot unlink it: nothing
/// would be left to sign in with.
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
    // And it is still linked.
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

/// A subject linked to one account is refused, not moved. Moving it would
/// sign two people in as one.
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
        "/api/v1/auth/providers",
        "/api/v1/auth/oidc/start",
        "/api/v1/auth/oidc/callback?code=c&state=s",
        "/api/v1/auth/github/start",
        "/api/v1/auth/discord/start",
    ] {
        assert_eq!(
            app.empty(Method::GET, uri, Auth::None).await.status(),
            StatusCode::NOT_FOUND,
            "for {uri}"
        );
    }
}

/// Every provider this instance knows about.
async fn providers(app: &TestApp) -> Vec<serde_json::Value> {
    body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/auth/providers", Auth::None)
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await
    .as_array()
    .expect("a list")
    .clone()
}

/// What an account signs in through.
async fn identities(app: &TestApp, cookie: &str) -> serde_json::Value {
    body_json(
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

/// The provider reporting a sign-out, via the endpoint it posts to. The
/// signature is real here, unlike the ID token, because nothing else vouches
/// for this token.
mod backchannel {
    use axum::http::{Method, Request, StatusCode, header};

    use super::{
        Auth, CLIENT_ID, FakeProvider, Script, TestApp, claims_for, come_back, instance, leave,
        location, session_cookie, with_claim,
    };

    /// Signs a user in through the provider and returns their cookie.
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

    /// The main path: the provider says a user is signed out, and they are.
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

    /// A `sid` names one session, not every session that user has open.
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

    /// Only the signature identifies the sender, so a token signed by a key
    /// the provider never published is worthless.
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
    /// provider did publish, but the signature was made with another. Only
    /// verification rejects it.
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

    /// The same replay bound as the subject case, on the single-session path.
    /// They are two queries and only one was covered.
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

    /// An unsigned token is the classic attack, refused before any key lookup.
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

    /// An ID token carries a `nonce`, and the spec forbids one here so an ID
    /// token cannot be replayed as a logout.
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

    /// Signed by a trusted key but claiming another issuer. That is how a
    /// mix-up starts, and the issuer check stops it.
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

    /// Issued for another client at the same provider.
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

    /// A body without a token. Answered in the same format as everything else
    /// here: `Form<T>` would reject it with its own 422 and wording, which a
    /// provider cannot parse and would retry forever.
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
            let reported = super::body_json(response).await;
            assert_eq!(
                reported.get("error"),
                Some(&serde_json::json!("invalid_request"))
            );
        }
    }

    /// A token that names no one has nothing to act on.
    #[tokio::test]
    async fn a_token_naming_nobody_is_refused() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;

        let token =
            FakeProvider::logout_token(&provider.logout_claims(CLIENT_ID, &serde_json::json!({})));
        assert_eq!(push(&app, &token).await.status(), StatusCode::BAD_REQUEST);
    }

    /// A kept copy cannot later end a session created since. Otherwise
    /// replaying an old token would keep signing a user out.
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

    /// One kept for hours is refused outright.
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

    /// A subject that never signed in here is reported as success. The
    /// requested state already holds, and a refusal would make the provider
    /// retry forever.
    #[tokio::test]
    async fn a_logout_for_somebody_we_do_not_know_is_not_an_error() {
        let provider = FakeProvider::start(CLIENT_ID).await;
        let app = instance(&provider).await;

        let token = FakeProvider::logout_token(
            &provider.logout_claims(CLIENT_ID, &serde_json::json!({ "sub": "a-stranger" })),
        );

        assert_eq!(push(&app, &token).await.status(), StatusCode::OK);
    }

    /// Refusals use the format the spec says a provider reads.
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
        let body = super::body_json(response).await;
        assert_eq!(
            body.get("error"),
            Some(&serde_json::json!("invalid_request"))
        );
        assert!(body.get("error_description").is_some());
    }
}
