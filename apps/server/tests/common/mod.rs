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
    storage::{Backends, DEFAULT_BACKEND, Pool},
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
        let (store, multipart) =
            storage::build_with_multipart(&config.storage).expect("local store");

        let assets =
            tessera_server::assets::Source::resolve(&config.server.assets_dir).expect("assets_dir");
        // The one backend the fixture has, under the name every file records.
        let pool = Pool {
            open: Backends::from([(
                DEFAULT_BACKEND.to_owned(),
                (Arc::clone(&store), multipart.clone()),
            )]),
            configured: Vec::new(),
        };
        let state = AppState::new(Arc::new(config), db, store, multipart, pool, assets);
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
    account.records_a_long_user_agent().await;
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
    /// Signs in with a `User-Agent` far longer than the column allows.
    ///
    /// Here rather than in the SQLite-only suite because SQLite has no column
    /// width to exceed. MySQL does — `VARCHAR(512)` — so a bound that drifted
    /// from it would fail there and nowhere else, on a header any caller sets.
    ///
    /// Deliberately ASCII: a header value that is not visible ASCII never
    /// reaches the column at all (see `session_origin`), so a multi-byte one
    /// would store `NULL` and quietly prove nothing.
    async fn records_a_long_user_agent(&self) {
        let response = self
            .app
            .send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::USER_AGENT, "u".repeat(2000)),
                Body::from(
                    serde_json::json!({
                        "identifier": self.username,
                        "password": Self::REPLACEMENT,
                    })
                    .to_string(),
                ),
            )
            .await;
        let cookie = session_cookie(&expect_status(response, StatusCode::OK).await);

        let value = cookie.split_once('=').expect("a name=value pair").1;
        let session = self
            .app
            .state
            .sessions
            .find_by_secret(&tessera_server::auth::secret::Secret::from_client(
                value.to_owned(),
            ))
            .await
            .expect("query")
            .expect("the session");
        assert_eq!(
            session
                .created_user_agent
                .map(|agent| agent.chars().count()),
            Some(512),
            "the stored agent must be cut to what the narrowest column holds"
        );
    }

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

/// Writes shares and reads them back, through the repository.
///
/// Lives here so the cross-backend suite runs the same scenario: the tables are
/// hand-written per dialect, and only writing rows and reading them back proves
/// the entity and the schema agree about every column — Postgres `TIMESTAMPTZ`
/// against SQLite's TEXT, MySQL's `MEDIUMBLOB`, the nullable owner foreign key,
/// and the atomic use-count update.
///
/// `suffix` keeps the retrieval codes unique, because the networked backends run
/// against a database that survives the run.
pub(crate) async fn share_repository_round_trip(app: &TestApp, suffix: &str) {
    let shares = Shares::new(app, suffix);
    shares.inline_text_survives_the_round_trip().await;
    shares.an_object_backed_share_has_no_inline_row().await;
    shares.the_gate_reads_the_columns_it_should();
    shares.the_use_count_is_spent_in_the_database().await;
    shares.a_view_is_not_a_retrieval().await;
    shares.deleting_takes_the_content_with_it().await;
    shares.dead_shares_are_listed_for_cleanup().await;
}

use tessera_core::{RetrievalCode, ShareId};
use tessera_server::db::repository::share::{
    Availability, NewContent, NewFile, Share, ShareEvent, ShareKind, ShareRepository,
    StorageLocation,
};
use time::{Duration, OffsetDateTime};

/// Fixture for [`share_repository_round_trip`].
struct Shares<'a> {
    repository: &'a ShareRepository,
    suffix: String,
    now: OffsetDateTime,
}

impl<'a> Shares<'a> {
    fn new(app: &'a TestApp, suffix: &str) -> Self {
        Self {
            repository: &app.state.shares,
            suffix: suffix.to_owned(),
            // Truncated to microseconds. Rust's clock is nanosecond-resolution
            // and no backend stores that: Postgres `TIMESTAMPTZ` and MySQL
            // `TIMESTAMP(6)` are both microseconds. Writing nanoseconds and then
            // asserting the row reads back unchanged would fail on the precision
            // rather than on anything this test is about.
            now: truncate_to_microseconds(OffsetDateTime::now_utc()),
        }
    }

    /// A distinct code per case. The alphabet excludes `I`, `L`, `O` and `U`, so
    /// digits are the safe way to vary it.
    fn code(&self, n: u8) -> RetrievalCode {
        let raw = format!("{}{n:02}", self.suffix);
        RetrievalCode::parse(&raw)
            .unwrap_or_else(|error| panic!("`{raw}` should be a valid code: {error}"))
    }

    /// A share with nothing optional set, so each case can name only what it
    /// cares about.
    fn blank(&self, n: u8) -> Share {
        Share {
            role_id: None,
            id: ShareId::generate(),
            code: self.code(n),
            user_id: None,
            kind: ShareKind::Text,
            password_hash: None,
            use_limit: None,
            use_count: 0,
            expires_at: None,
            disabled_at: None,
            title: None,
            og_config: None,
            management_token_hash: None,
            view_count: 0,
            copy_count: 0,
            bytes_served: 0,
            last_accessed_at: None,
            created_at: self.now,
            updated_at: self.now,
        }
    }

    /// Text for a case that only needs the share to exist.
    fn some_text() -> NewContent {
        NewContent::Text {
            language: None,
            body: b"gone shortly".to_vec(),
        }
    }

    async fn stored(&self, id: ShareId) -> Share {
        self.repository
            .find_by_id(id)
            .await
            .expect("query")
            .expect("the share should still be there")
    }

    /// Every optional column populated, so one the entity and the schema
    /// disagree about cannot slip through as NULL.
    fn populated_text(&self, _content: &[u8]) -> Share {
        Share {
            kind: ShareKind::Text,
            title: Some("a snippet".to_owned()),
            password_hash: Some("$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA".to_owned()),
            use_limit: Some(3),
            expires_at: Some(self.now + Duration::hours(1)),
            og_config: Some(r#"{"layout":"default"}"#.to_owned()),
            management_token_hash: Some("a".repeat(64)),
            ..self.blank(1)
        }
    }

    async fn inline_text_survives_the_round_trip(&self) {
        let content = "const foo = \"bar\";\n".repeat(64).into_bytes();
        let share = self.populated_text(&content);
        self.repository
            .insert(
                &share,
                &NewContent::Text {
                    language: Some("typescript".to_owned()),
                    body: content.clone(),
                },
            )
            .await
            .expect("insert");

        let stored = self
            .repository
            .find_by_code(&share.code)
            .await
            .expect("query")
            .expect("the share just written");
        assert_eq!(stored, share, "a stored share must read back unchanged");
        let text = self
            .repository
            .text_of(share.id)
            .await
            .expect("query")
            .expect("text lives in its own table now");
        assert_eq!(
            text.content, content,
            "the text must survive the round trip"
        );
        assert_eq!(text.language.as_deref(), Some("typescript"));
    }

    async fn an_object_backed_share_has_no_inline_row(&self) {
        let share = Share {
            kind: ShareKind::File,
            ..self.blank(2)
        };
        self.repository
            .insert(
                &share,
                &NewContent::File(NewFile {
                    filename: Some("專案.zip".to_owned()),
                    content_type: Some("application/zip".to_owned()),
                    size_bytes: 1_048_576,
                    storage: StorageLocation::Object {
                        backend: DEFAULT_BACKEND.to_owned(),
                        key: format!("2026/08/{}", self.suffix),
                    },
                }),
            )
            .await
            .expect("insert");

        assert_eq!(self.stored(share.id).await, share);
        let keys = self
            .repository
            .object_keys_of(share.id)
            .await
            .expect("query");
        assert_eq!(keys.len(), 1, "an object-backed share names its object");
    }

    fn the_gate_reads_the_columns_it_should(&self) {
        let live = Share {
            password_hash: Some("irrelevant".to_owned()),
            ..self.blank(3)
        };
        assert_eq!(live.availability_at(self.now), Availability::Available);
        assert!(live.needs_password());

        for (label, share, expected) in [
            (
                "expired",
                Share {
                    expires_at: Some(self.now - Duration::seconds(1)),
                    ..self.blank(4)
                },
                Availability::Expired,
            ),
            (
                "exhausted",
                Share {
                    use_limit: Some(2),
                    use_count: 2,
                    ..self.blank(5)
                },
                Availability::Exhausted,
            ),
            (
                "disabled",
                Share {
                    disabled_at: Some(self.now),
                    ..self.blank(6)
                },
                Availability::Disabled,
            ),
        ] {
            assert_eq!(share.availability_at(self.now), expected, "{label}");
        }
    }

    /// A share with three uses must yield exactly three, however many callers
    /// ask — and the refusal has to come from the `WHERE` clause, not from a
    /// check the handler could forget.
    async fn the_use_count_is_spent_in_the_database(&self) {
        let share = Share {
            use_limit: Some(3),
            ..self.blank(7)
        };
        self.repository
            .insert(&share, &Self::some_text())
            .await
            .expect("insert");

        for spent in 1..=3 {
            assert!(
                self.repository
                    .consume(share.id, self.now)
                    .await
                    .expect("consume"),
                "use {spent} of 3 should have been granted"
            );
        }
        assert!(
            !self
                .repository
                .consume(share.id, self.now)
                .await
                .expect("consume"),
            "the fourth use must be refused"
        );

        let stored = self.stored(share.id).await;
        assert_eq!(stored.use_count, 3);
        assert_eq!(stored.availability_at(self.now), Availability::Exhausted);
        assert!(stored.last_accessed_at.is_some());
    }

    async fn a_view_is_not_a_retrieval(&self) {
        let share = self.blank(8);
        self.repository
            .insert(&share, &Self::some_text())
            .await
            .expect("insert");

        // An unlimited share is never refused.
        for _ in 0..5 {
            assert!(
                self.repository
                    .consume(share.id, self.now)
                    .await
                    .expect("consume")
            );
        }
        self.repository
            .record_event(share.id, ShareEvent::View)
            .await
            .expect("record view");
        self.repository
            .record_event(share.id, ShareEvent::Copy)
            .await
            .expect("record copy");

        let stored = self.stored(share.id).await;
        assert_eq!(stored.view_count, 1);
        assert_eq!(stored.copy_count, 1);
        assert_eq!(stored.use_count, 5, "neither one spends a use");
    }

    async fn deleting_takes_the_content_with_it(&self) {
        let share = self.blank(9);
        self.repository
            .insert(&share, &Self::some_text())
            .await
            .expect("insert");

        self.repository.delete(share.id).await.expect("delete");
        assert!(
            self.repository
                .find_by_id(share.id)
                .await
                .expect("query")
                .is_none()
        );
        assert!(
            self.repository
                .text_of(share.id)
                .await
                .expect("query")
                .is_none(),
            "the content goes with the row"
        );
    }

    /// Dead shares have to be findable *before* their rows go, so whatever is in
    /// the blob store can be removed first — no database action reaches it.
    ///
    /// Two ways to be dead, and both have to be found. Cleaning only the expired
    /// ones would leave every burn-after-reading share accumulating forever:
    /// they are dead the moment they are read and have no expiry to notice.
    async fn dead_shares_are_listed_for_cleanup(&self) {
        let expired = Share {
            expires_at: Some(self.now - Duration::days(1)),
            ..self.blank(10)
        };
        let exhausted = Share {
            use_limit: Some(1),
            ..self.blank(11)
        };
        let live = self.blank(12);
        for share in [&expired, &exhausted, &live] {
            self.repository
                .insert(share, &Self::some_text())
                .await
                .expect("insert");
        }
        // Spending the last retrieval is what makes it dead, and what stamps the
        // time its age is measured from.
        assert!(
            self.repository
                .consume(exhausted.id, self.now - Duration::days(1))
                .await
                .expect("consume")
        );

        let dead = self
            .repository
            .find_dead(self.now, 100)
            .await
            .expect("query");
        let found = |id| dead.iter().any(|share: &Share| share.id == id);

        assert!(found(expired.id), "an expired share should be swept");
        assert!(
            found(exhausted.id),
            "a used-up share should be swept too — it holds bytes and nothing \
             will ever ask for them again"
        );
        assert!(!found(live.id), "a live share must never be swept");

        // Retention is what the cutoff expresses: measured from far enough back,
        // nothing is old enough to remove yet.
        let none_yet = self
            .repository
            .find_dead(self.now - Duration::days(7), 100)
            .await
            .expect("query");
        assert!(
            !none_yet.iter().any(|share| share.id == expired.id),
            "a share that died yesterday is not past a week's retention"
        );
    }
}

/// Drops sub-microsecond digits, which no backend keeps.
fn truncate_to_microseconds(value: OffsetDateTime) -> OffsetDateTime {
    let microseconds = value.nanosecond() / 1_000 * 1_000;
    value.replace_nanosecond(microseconds).unwrap_or(value)
}

/// Walks a share from creation to retrieval, the way a person and a script both
/// would.
///
/// Lives here so the cross-backend suite runs the same scenario: tickets are a
/// table with a foreign key and a timestamp, and only writing rows and reading
/// them back proves the three dialects agree.
pub(crate) async fn share_retrieval_lifecycle(app: &TestApp) {
    let Retrieval { code, ticket } = Retrieval::create(app).await;

    // `create` already spent one of the two retrievals, at `unlock`. The
    // metadata endpoint spends none — a link preview, a prefetch or a mail
    // scanner following the URL must not destroy a one-time share.
    let before = field(
        &Retrieval::describe(app, &code, Some("1234")).await,
        "uses_remaining",
    );
    assert_eq!(before, serde_json::json!(1));

    // One authorisation, then as many transfer requests as it takes.
    let full = Retrieval::download(app, &code, &ticket, None).await;
    assert_eq!(full.0, StatusCode::OK);
    assert_eq!(full.1.len(), 8192);

    for range in ["bytes=0-99", "bytes=100-199", "bytes=8000-"] {
        let (status, body) = Retrieval::download(app, &code, &ticket, Some(range)).await;
        assert_eq!(status, StatusCode::PARTIAL_CONTENT, "{range}");
        assert!(!body.is_empty(), "{range}");
    }

    let after = field(
        &Retrieval::describe(app, &code, Some("1234")).await,
        "uses_remaining",
    );
    assert_eq!(
        after, before,
        "four transfer requests under one ticket must cost nothing beyond the \
         authorisation that issued it"
    );
}

/// Fixture for [`share_retrieval_lifecycle`].
struct Retrieval {
    code: String,
    ticket: String,
}

impl Retrieval {
    async fn create(app: &TestApp) -> Self {
        let content: Vec<u8> = (0..8192_u32)
            .map(|i| u8::try_from(i % 256).unwrap_or(0))
            .collect();
        let created = body_json(
            expect_status(
                app.send(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/api/v1/shares?filename=probe.bin&max_uses=2")
                        .header("x-share-password", "1234"),
                    Body::from(content),
                )
                .await,
                StatusCode::CREATED,
            )
            .await,
        )
        .await;
        let code = string_field(&created, "code");

        let ticket = body_json(
            expect_status(
                app.send(
                    Request::builder()
                        .method(Method::POST)
                        .uri(format!("/api/v1/shares/{code}/unlock"))
                        .header("x-share-password", "1234"),
                    Body::empty(),
                )
                .await,
                StatusCode::OK,
            )
            .await,
        )
        .await;

        Self {
            code,
            ticket: string_field(&ticket, "ticket"),
        }
    }

    async fn describe(app: &TestApp, code: &str, password: Option<&str>) -> serde_json::Value {
        let mut builder = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/v1/shares/{code}"));
        if let Some(password) = password {
            builder = builder.header("x-share-password", password);
        }
        body_json(expect_status(app.send(builder, Body::empty()).await, StatusCode::OK).await).await
    }

    async fn download(
        app: &TestApp,
        code: &str,
        ticket: &str,
        range: Option<&str>,
    ) -> (StatusCode, Vec<u8>) {
        let mut builder = Request::builder()
            .method(Method::GET)
            .uri(format!("/s/{code}/download?t={ticket}"));
        if let Some(range) = range {
            builder = builder.header(header::RANGE, range);
        }
        let response = app.send(builder, Body::empty()).await;
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes()
            .to_vec();
        (status, bytes)
    }
}
