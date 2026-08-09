//! Shared fixtures for the integration tests.
//!
//! `clippy.toml`'s `allow-expect-in-tests` / `allow-panic-in-tests` only reach code
//! inside a `#[test]` function body. Helpers like these live outside one, so the
//! test-code exemption has to be restated here. Both are the correct behaviour for
//! a fixture: a fixture that cannot build its dependencies has nothing to report
//! but a failure.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test fixture; see module docs"
)]
// `allow` rather than `expect`: each test binary that includes this module uses
// a different subset of the fixtures, so `dead_code` fires in some and not
// others — an `#[expect]` would go unfulfilled in the binaries that use
// everything, which is a warning of its own.
#![allow(dead_code, reason = "each test binary uses a different subset")]

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Method, Request, Response, StatusCode, header, request::Builder},
};
use http_body_util::BodyExt as _;
use tempfile::TempDir;
use tessera_server::{
    api,
    config::{Config, DatabaseConfig, StorageConfig},
    db,
    state::{AppState, ServiceStatus},
    storage,
};
use tower::ServiceExt as _;

/// A fully assembled app plus the temporary directory its blob store lives in.
///
/// The directory is held here rather than dropped at construction time: dropping
/// a [`TempDir`] deletes it, which would pull the storage root out from under the
/// running app.
pub(crate) struct TestApp {
    pub(crate) router: Router,
    pub(crate) state: AppState,
    _blob_root: TempDir,
}

/// Database every test uses unless it asks for something else. In-memory, so
/// each test gets a private database and nothing has to be cleaned up.
pub(crate) const IN_MEMORY_SQLITE: &str = "sqlite::memory:";

impl TestApp {
    /// Builds an app backed by an in-memory SQLite database and a temporary blob
    /// directory, already migrated and marked ready.
    pub(crate) async fn ready() -> Self {
        Self::build(ServiceStatus::Ready).await
    }

    /// Same, but left in an arbitrary lifecycle state.
    pub(crate) async fn build(status: ServiceStatus) -> Self {
        Self::with_database(IN_MEMORY_SQLITE, status).await
    }

    /// Same, but with the configuration adjusted first — for tests that need a
    /// feature switched on, since the fixture otherwise mirrors the shipped
    /// defaults.
    pub(crate) async fn with_config(adjust: impl FnOnce(&mut Config)) -> Self {
        Self::build_with(IN_MEMORY_SQLITE, ServiceStatus::Ready, adjust).await
    }

    /// Same, against an arbitrary database URL — used by the backend suite,
    /// which has to prove the *other* backends serve identical responses.
    pub(crate) async fn with_database(url: &str, status: ServiceStatus) -> Self {
        Self::build_with(url, status, |_| {}).await
    }

    async fn build_with(
        url: &str,
        status: ServiceStatus,
        adjust: impl FnOnce(&mut Config),
    ) -> Self {
        let blob_root = tempfile::tempdir().expect("temp dir");

        let mut config = Config {
            database: DatabaseConfig {
                url: url.to_owned(),
                ..Default::default()
            },
            storage: StorageConfig {
                url: blob_root.path().join("blobs").display().to_string(),
                ..Default::default()
            },
            ..Config::default()
        };
        adjust(&mut config);

        let db = db::connect(&config.database).await.expect("database pool");
        db::migrate(&db).await.expect("migrations");
        let store = storage::build(&config.storage).expect("local store");

        let state = AppState::new(Arc::new(config), db, store);
        state.set_status(status);

        Self {
            router: api::build(state.clone()),
            state,
            _blob_root: blob_root,
        }
    }

    /// Issues a `GET` and returns the response.
    pub(crate) async fn get(&self, uri: &str) -> Response<Body> {
        self.send(Request::builder().uri(uri), Body::empty()).await
    }

    /// Issues a request built by the caller, so a test can attach a cookie or an
    /// `Authorization` header without a helper per combination.
    pub(crate) async fn send(&self, builder: Builder, body: Body) -> Response<Body> {
        self.router
            .clone()
            .oneshot(builder.body(body).expect("request"))
            .await
            .expect("response")
    }

    /// Issues a JSON request with the given method, optionally authenticated.
    pub(crate) async fn json(
        &self,
        method: Method,
        uri: &str,
        auth: Auth<'_>,
        body: &serde_json::Value,
    ) -> Response<Body> {
        let builder = auth.apply(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json"),
        );
        self.send(builder, Body::from(body.to_string())).await
    }

    /// Issues a request with no body.
    pub(crate) async fn empty(&self, method: Method, uri: &str, auth: Auth<'_>) -> Response<Body> {
        let builder = auth.apply(Request::builder().method(method).uri(uri));
        self.send(builder, Body::empty()).await
    }
}

/// How a test request proves who it is.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Auth<'a> {
    /// Anonymous.
    None,
    /// A session cookie, as a browser would send it.
    Cookie(&'a str),
    /// An API token, as a script would send it.
    Bearer(&'a str),
}

impl Auth<'_> {
    fn apply(self, builder: Builder) -> Builder {
        match self {
            Self::None => builder,
            Self::Cookie(value) => builder.header(header::COOKIE, value),
            Self::Bearer(token) => builder.header(header::AUTHORIZATION, format!("Bearer {token}")),
        }
    }
}

/// Reads the session cookie a response sets, as `name=value` ready to be sent
/// back — which is what a browser would do with it.
pub(crate) fn session_cookie(response: &Response<Body>) -> String {
    let raw = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("a Set-Cookie header")
        .to_str()
        .expect("an ASCII cookie");
    raw.split(';')
        .next()
        .expect("a name=value pair")
        .trim()
        .to_owned()
}

/// Drains a response body into a `String`.
pub(crate) async fn body_string(response: Response<Body>) -> String {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Drains a response body and parses it as JSON.
pub(crate) async fn body_json(response: Response<Body>) -> serde_json::Value {
    serde_json::from_str(&body_string(response).await).expect("json body")
}

/// Asserts on a status code with the body included in the failure message —
/// otherwise a failing assertion says only "expected 200, got 500".
pub(crate) async fn expect_status(
    response: Response<Body>,
    expected: StatusCode,
) -> Response<Body> {
    let status = response.status();
    if status == expected {
        return response;
    }
    let body = body_string(response).await;
    panic!("expected {expected}, got {status}; body: {body}");
}

/// Walks an account from registration through to a revoked token and a logout.
///
/// Lives here rather than in one test binary because the cross-backend suite has
/// to run exactly this scenario: it is the only thing that proves the entities
/// and the hand-written per-dialect SQL agree about every column — timestamps,
/// booleans, nullability, the unique indexes and the foreign keys. Unit tests
/// against `sqlite::memory:` cannot prove any of that for Postgres or MySQL.
///
/// `suffix` keeps the account names unique, because the networked backends run
/// against a database that survives the run.
pub(crate) async fn account_lifecycle(app: &TestApp, suffix: &str) {
    let account = Scenario::register(app, suffix).await;
    account.rejects_a_duplicate_registration().await;
    let second = account.sign_in_again_by_email().await;
    account.exercises_an_api_token().await;
    account.changes_its_password(&second).await;
}

/// One account, plus the session it registered with.
struct Scenario<'a> {
    app: &'a TestApp,
    username: String,
    email: String,
    password: String,
    cookie: String,
    id: serde_json::Value,
}

impl<'a> Scenario<'a> {
    const PASSWORD: &'static str = "a perfectly adequate passphrase";
    const REPLACEMENT: &'static str = "an entirely different passphrase";

    async fn register(app: &'a TestApp, suffix: &str) -> Self {
        let username = format!("scenario{suffix}");
        let email = format!("{username}@example.com");

        let response = expect_status(
            app.json(
                Method::POST,
                "/api/v1/auth/register",
                Auth::None,
                &serde_json::json!({
                    "username": username,
                    "email": email,
                    "password": Self::PASSWORD,
                }),
            )
            .await,
            StatusCode::CREATED,
        )
        .await;
        let cookie = session_cookie(&response);
        let account = body_json(response).await;

        assert_eq!(account.get("username"), Some(&json_string(&username)));
        assert!(
            account.get("password").is_none() && account.get("password_hash").is_none(),
            "an account response must never carry a credential: {account}"
        );

        let scenario = Self {
            app,
            username,
            email,
            password: Self::PASSWORD.to_owned(),
            cookie,
            id: field(&account, "id"),
        };

        // The cookie identifies the account it was issued for.
        let body = scenario.session_as(Auth::Cookie(&scenario.cookie)).await;
        assert_eq!(field(&body, "id"), scenario.id);
        scenario
    }

    /// Reads `/auth/session` with the given credential, asserting it is accepted.
    async fn session_as(&self, auth: Auth<'_>) -> serde_json::Value {
        body_json(
            expect_status(
                self.app
                    .empty(Method::GET, "/api/v1/auth/session", auth)
                    .await,
                StatusCode::OK,
            )
            .await,
        )
        .await
    }

    async fn login(&self, identifier: &str, password: &str, expected: StatusCode) -> String {
        let response = expect_status(
            self.app
                .json(
                    Method::POST,
                    "/api/v1/auth/login",
                    Auth::None,
                    &serde_json::json!({ "identifier": identifier, "password": password }),
                )
                .await,
            expected,
        )
        .await;
        if expected == StatusCode::OK {
            session_cookie(&response)
        } else {
            String::new()
        }
    }

    async fn rejects_a_duplicate_registration(&self) {
        expect_status(
            self.app
                .json(
                    Method::POST,
                    "/api/v1/auth/register",
                    Auth::None,
                    &serde_json::json!({
                        "username": self.username,
                        "email": format!("other-{}", self.email),
                        "password": self.password,
                    }),
                )
                .await,
            StatusCode::CONFLICT,
        )
        .await;
    }

    /// Signing in by address rather than name, and getting a session of its own.
    async fn sign_in_again_by_email(&self) -> String {
        let second = self
            .login(&self.email.clone(), &self.password, StatusCode::OK)
            .await;
        assert_ne!(
            self.cookie, second,
            "each sign-in must issue its own session"
        );
        second
    }

    async fn exercises_an_api_token(&self) {
        let created = body_json(
            expect_status(
                self.app
                    .json(
                        Method::POST,
                        "/api/v1/tokens",
                        Auth::Cookie(&self.cookie),
                        &serde_json::json!({ "name": "scenario", "expires_in_days": 30 }),
                    )
                    .await,
                StatusCode::CREATED,
            )
            .await,
        )
        .await;

        let secret = string_field(&created, "secret");
        let id = string_field(&created, "id");
        assert!(secret.starts_with("tsk_"), "got: {secret}");

        // The token authenticates as the same account the cookie does.
        assert_eq!(
            field(&self.session_as(Auth::Bearer(&secret)).await, "id"),
            self.id
        );

        self.assert_listing_hides_secrets(&id).await;

        // A token may not mint another: one leaked token would otherwise be
        // permanent access, since revoking it leaves the ones it issued.
        expect_status(
            self.app
                .json(
                    Method::POST,
                    "/api/v1/tokens",
                    Auth::Bearer(&secret),
                    &serde_json::json!({ "name": "escalation" }),
                )
                .await,
            StatusCode::FORBIDDEN,
        )
        .await;

        // Revocation stops it working, and cannot be repeated.
        self.revoke(&id, StatusCode::NO_CONTENT).await;
        expect_status(
            self.app
                .empty(Method::GET, "/api/v1/auth/session", Auth::Bearer(&secret))
                .await,
            StatusCode::UNAUTHORIZED,
        )
        .await;
        self.revoke(&id, StatusCode::NOT_FOUND).await;
    }

    async fn assert_listing_hides_secrets(&self, id: &str) {
        let listed = body_json(
            expect_status(
                self.app
                    .empty(Method::GET, "/api/v1/tokens", Auth::Cookie(&self.cookie))
                    .await,
                StatusCode::OK,
            )
            .await,
        )
        .await;

        let entry = listed
            .as_array()
            .expect("an array")
            .iter()
            .find(|token| token.get("id") == Some(&json_string(id)))
            .expect("the token just created")
            .clone();
        assert!(
            entry.get("secret").is_none(),
            "a listing must not carry secrets: {entry}"
        );
        assert_eq!(entry.get("revoked_at"), Some(&serde_json::Value::Null));
    }

    async fn revoke(&self, id: &str, expected: StatusCode) {
        expect_status(
            self.app
                .empty(
                    Method::DELETE,
                    &format!("/api/v1/tokens/{id}"),
                    Auth::Cookie(&self.cookie),
                )
                .await,
            expected,
        )
        .await;
    }

    /// Changing the password must end every *other* session, keep this one, and
    /// leave only the new password working.
    async fn changes_its_password(&self, other_session: &str) {
        expect_status(
            self.app
                .json(
                    Method::PATCH,
                    "/api/v1/auth/password",
                    Auth::Cookie(&self.cookie),
                    &serde_json::json!({
                        "current_password": self.password,
                        "new_password": Self::REPLACEMENT,
                    }),
                )
                .await,
            StatusCode::OK,
        )
        .await;

        expect_status(
            self.app
                .empty(
                    Method::GET,
                    "/api/v1/auth/session",
                    Auth::Cookie(other_session),
                )
                .await,
            StatusCode::UNAUTHORIZED,
        )
        .await;

        self.login(&self.username, &self.password, StatusCode::UNAUTHORIZED)
            .await;
        let fresh = self
            .login(&self.username, Self::REPLACEMENT, StatusCode::OK)
            .await;

        expect_status(
            self.app
                .empty(Method::POST, "/api/v1/auth/logout", Auth::Cookie(&fresh))
                .await,
            StatusCode::NO_CONTENT,
        )
        .await;
        expect_status(
            self.app
                .empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&fresh))
                .await,
            StatusCode::UNAUTHORIZED,
        )
        .await;
    }
}

/// `value[key]`, without the panicking index operator — a missing field should
/// fail the assertion that uses it, not the line that reads it.
fn field(value: &serde_json::Value, key: &str) -> serde_json::Value {
    value.get(key).cloned().unwrap_or(serde_json::Value::Null)
}

fn string_field(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("`{key}` should be a string in {value}"))
        .to_owned()
}

fn json_string(value: &str) -> serde_json::Value {
    serde_json::Value::String(value.to_owned())
}

/// Sets the flag directly. The bootstrap only sets it for a password it
/// generated, and a test that wanted one would have to scrape a log line.
pub(crate) async fn force_password_change(app: &TestApp, username: &str) {
    let statement = sea_orm::Statement::from_string(
        sea_orm::ConnectionTrait::get_database_backend(&app.state.db),
        format!("UPDATE users SET must_change_password = 1 WHERE username = '{username}'"),
    );
    sea_orm::ConnectionTrait::execute_raw(&app.state.db, statement)
        .await
        .expect("set the flag");
}
