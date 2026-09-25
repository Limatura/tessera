//! Checking new passwords against the passwords known to have leaked.
//!
//! The service here answers the way the real range API does, and records what
//! it was asked, so the test can say what left the process: five characters of
//! a hash, and nothing else.

#![expect(
    clippy::expect_used,
    reason = "test fixture; a panic is how it reports"
)]

mod common;

use std::sync::Arc;

use axum::{
    extract::Path,
    http::{Method, StatusCode},
};
use common::{Auth, TestApp, body_json, body_string, expect_status, session_cookie};
use parking_lot::Mutex;
use tokio::net::TcpListener;

/// `password`, the most breached password there is. Eight characters, so the
/// length policy accepts it and the breach list is what refuses it.
const BREACHED: &str = "password";

/// Its SHA-1 is `5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8`.
const BREACHED_PREFIX: &str = "5BAA6";
const BREACHED_SUFFIX: &str = "1E4C9B93F3F0682250B6CF8331B7EE68FD8";

/// Not in the list.
const FRESH: &str = "a wholly unremarkable passphrase";

/// Every path the service was asked for.
type Asked = Arc<Mutex<Vec<String>>>;

/// A stand-in for the range API. `answer` decides what it does.
///
/// No mail is configured on the apps that use it: with a relay, registering
/// answers `202` and waits for a confirmation, and these tests are about the
/// password, not the post.
async fn range_api(answer: Option<StatusCode>) -> (String, Asked) {
    let asked: Asked = Arc::default();
    let recorder = Arc::clone(&asked);
    let app = axum::Router::new().route(
        "/range/{prefix}",
        axum::routing::get(move |Path(prefix): Path<String>| {
            let recorder = Arc::clone(&recorder);
            async move {
                use axum::response::IntoResponse as _;
                recorder.lock().push(prefix.clone());
                if let Some(status) = answer {
                    return status.into_response();
                }
                // A real answer: the match, and padding with a count of zero.
                if prefix.eq_ignore_ascii_case(BREACHED_PREFIX) {
                    format!("{BREACHED_SUFFIX}:9659365\r\n0018A45C4D1DEF81644B54AB7F969B88D65:0")
                        .into_response()
                } else {
                    "0018A45C4D1DEF81644B54AB7F969B88D65:0".into_response()
                }
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().expect("an address");
    tokio::spawn(async move { axum::serve(listener, app).await });
    (format!("http://{address}"), asked)
}

async fn checking(url: String) -> TestApp {
    TestApp::with_config(move |config| {
        config.auth.pwned.enabled = true;
        config.auth.pwned.url = url;
    })
    .await
}

async fn register(app: &TestApp, username: &str, password: &str) -> axum::response::Response {
    app.json(
        Method::POST,
        "/api/v1/auth/register",
        Auth::None,
        &serde_json::json!({
            "username": username,
            "email": format!("{username}@example.com"),
            "password": password,
        }),
    )
    .await
}

#[tokio::test]
async fn a_breached_password_is_refused_at_registration() {
    let (url, asked) = range_api(None).await;
    let app = checking(url).await;

    let refused = expect_status(
        register(&app, "hopeful", BREACHED).await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    assert!(body_string(refused).await.contains("data breach"));

    // And what the service was told: the prefix, and only the prefix.
    let asked = asked.lock().clone();
    assert_eq!(asked, vec![BREACHED_PREFIX.to_owned()]);

    expect_status(register(&app, "hopeful", FRESH).await, StatusCode::CREATED).await;
}

#[tokio::test]
async fn a_breached_password_is_refused_when_changing_it() {
    let (url, _) = range_api(None).await;
    let app = checking(url).await;
    let cookie = session_cookie(
        &expect_status(register(&app, "member", FRESH).await, StatusCode::CREATED).await,
    );

    let change = async |new: &'static str| {
        app.json(
            Method::PATCH,
            "/api/v1/auth/password",
            Auth::Cookie(&cookie),
            &serde_json::json!({ "current_password": FRESH, "new_password": new }),
        )
        .await
    };
    let refused = expect_status(change(BREACHED).await, StatusCode::BAD_REQUEST).await;
    assert!(body_string(refused).await.contains("data breach"));
    expect_status(
        change("another perfectly good passphrase").await,
        StatusCode::OK,
    )
    .await;
}

/// The reset routes exist only on an instance that can send mail, so this one
/// has a relay configured. Registering there waits for a confirmation email,
/// so the account is written straight to the table instead.
#[tokio::test]
async fn a_breached_password_is_refused_when_finishing_a_reset() {
    let (url, _) = range_api(None).await;
    let app = TestApp::with_config(move |config| {
        config.auth.pwned.enabled = true;
        config.auth.pwned.url = url;
        "smtp.invalid".clone_into(&mut config.mail.host);
        "tessera@example.com".clone_into(&mut config.mail.from);
        "https://tessera.example.com".clone_into(&mut config.mail.public_url);
    })
    .await;

    let now = time::OffsetDateTime::now_utc();
    let user = tessera_server::db::repository::user::User {
        id: tessera_core::UserId::generate(),
        username: tessera_core::Username::parse("forgetful").expect("a username"),
        email: tessera_core::EmailAddress::parse("forgetful@example.com").expect("an address"),
        role_id: None,
        quota_bytes: None,
        max_file_bytes: None,
        must_change_password: false,
        state: tessera_core::AccountState::Active,
        deleted_at: None,
        deleted_by: None,
        email_verified_at: Some(now),
        created_at: now,
        updated_at: now,
    };
    app.state
        .users
        .insert(&user, Some("$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA"))
        .await
        .expect("insert");

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

    let confirm = async |password: &'static str| {
        app.json(
            Method::POST,
            "/api/v1/auth/password-reset/confirm",
            Auth::None,
            &serde_json::json!({ "token": secret.expose(), "password": password }),
        )
        .await
    };
    expect_status(confirm(BREACHED).await, StatusCode::BAD_REQUEST).await;
    // The link survives a refusal, as it does for any other rejected password.
    expect_status(
        confirm("yet another passphrase entirely").await,
        StatusCode::NO_CONTENT,
    )
    .await;
}

/// The list being down must not stop anybody registering. It is advice, and
/// the length and composition rules are what actually gate a password.
#[tokio::test]
async fn a_service_that_is_down_lets_the_password_through() {
    let (url, _) = range_api(Some(StatusCode::INTERNAL_SERVER_ERROR)).await;
    let app = checking(url).await;
    expect_status(
        register(&app, "unlucky", BREACHED).await,
        StatusCode::CREATED,
    )
    .await;
}

#[tokio::test]
async fn nothing_is_asked_when_the_check_is_off() {
    let (url, asked) = range_api(None).await;
    let app = TestApp::with_config(move |config| {
        config.auth.pwned.url = url;
    })
    .await;
    let created = expect_status(
        register(&app, "unchecked", BREACHED).await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(body_json(created).await["username"], "unchecked");
    assert!(asked.lock().is_empty());
}

/// The real service, to check the format this parses is the format it sends.
/// `#[ignore]`d: it needs the internet.
#[tokio::test]
#[ignore = "talks to api.pwnedpasswords.com"]
async fn the_real_service_still_answers_the_way_this_reads() {
    let app = TestApp::with_config(|config| config.auth.pwned.enabled = true).await;
    expect_status(
        register(&app, "hopeful", BREACHED).await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    expect_status(register(&app, "hopeful", FRESH).await, StatusCode::CREATED).await;
}

/// Looking a prefix up while the form is typed into. What comes back is what
/// the browser compares against: real matches only, without the padding.
#[tokio::test]
async fn a_form_can_look_a_prefix_up_as_it_is_typed() {
    let (url, asked) = range_api(None).await;
    let app = checking(url).await;

    let capabilities =
        body_json(expect_status(app.get("/api/v1/capabilities").await, StatusCode::OK).await).await;
    assert_eq!(capabilities["account_password_breach_check"], true);

    // Lowercase is accepted and passed on as the service spells it.
    let found =
        body_json(expect_status(app.get("/api/v1/auth/pwned/5baa6").await, StatusCode::OK).await)
            .await;
    assert_eq!(found["suffixes"], serde_json::json!([BREACHED_SUFFIX]));
    assert_eq!(asked.lock().clone(), vec![BREACHED_PREFIX.to_owned()]);

    // Only padding under this one, which is not a breach.
    let none =
        body_json(expect_status(app.get("/api/v1/auth/pwned/00000").await, StatusCode::OK).await)
            .await;
    assert_eq!(none["suffixes"], serde_json::json!([]));
}

#[tokio::test]
async fn only_a_prefix_is_accepted() {
    let (url, asked) = range_api(None).await;
    let app = checking(url).await;
    for bad in ["5BAA", "5BAA61", "5BAAG", "password"] {
        expect_status(
            app.get(&format!("/api/v1/auth/pwned/{bad}")).await,
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
    assert!(
        asked.lock().is_empty(),
        "nothing malformed reaches the service"
    );
}

/// Off looks like an instance without the feature, and so does one without
/// accounts, where there is no password to check.
#[tokio::test]
async fn an_instance_that_does_not_check_has_no_lookup() {
    let (url, _) = range_api(None).await;
    let app = TestApp::with_config(move |config| config.auth.pwned.url = url).await;
    let capabilities =
        body_json(expect_status(app.get("/api/v1/capabilities").await, StatusCode::OK).await).await;
    assert_eq!(capabilities["account_password_breach_check"], false);
    expect_status(
        app.get("/api/v1/auth/pwned/5BAA6").await,
        StatusCode::NOT_FOUND,
    )
    .await;

    let (url, _) = range_api(None).await;
    let app = TestApp::with_config(move |config| {
        config.auth.pwned.enabled = true;
        config.auth.pwned.url = url;
        config.features.accounts = false;
    })
    .await;
    expect_status(
        app.get("/api/v1/auth/pwned/5BAA6").await,
        StatusCode::NOT_FOUND,
    )
    .await;
}

/// The lookup cannot fall back the way the check does, so it says it could
/// not answer. The form shows no warning, and the check runs when it is sent.
#[tokio::test]
async fn a_service_that_is_down_is_reported_as_unavailable() {
    let (url, _) = range_api(Some(StatusCode::INTERNAL_SERVER_ERROR)).await;
    let app = checking(url).await;
    expect_status(
        app.get("/api/v1/auth/pwned/5BAA6").await,
        StatusCode::SERVICE_UNAVAILABLE,
    )
    .await;
}

/// Every lookup counts, and on its own counter: a form typing through its
/// allowance must not use up the same address's sign-in attempts.
#[tokio::test]
async fn one_source_gets_so_many_lookups() {
    let (url, _) = range_api(None).await;
    let app = TestApp::with_config(move |config| {
        config.auth.pwned.enabled = true;
        config.auth.pwned.url = url;
        config.auth.throttle.pwned.max_attempts = 2;
    })
    .await;
    for _ in 0..2 {
        expect_status(app.get("/api/v1/auth/pwned/5BAA6").await, StatusCode::OK).await;
    }
    let refused = expect_status(
        app.get("/api/v1/auth/pwned/5BAA6").await,
        StatusCode::TOO_MANY_REQUESTS,
    )
    .await;
    assert!(refused.headers().contains_key("retry-after"));
    // The sign-in budget is untouched.
    expect_status(
        register(&app, "unaffected", FRESH).await,
        StatusCode::CREATED,
    )
    .await;
}
