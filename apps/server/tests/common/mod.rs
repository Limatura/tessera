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
    http::{Request, Response, StatusCode},
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

    /// Same, against an arbitrary database URL — used by the Postgres suite,
    /// which has to prove the *other* backend serves identical responses.
    pub(crate) async fn with_database(url: &str, status: ServiceStatus) -> Self {
        let blob_root = tempfile::tempdir().expect("temp dir");

        let config = Config {
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
        let request = Request::builder()
            .uri(uri)
            .body(Body::empty())
            .expect("request");
        self.router
            .clone()
            .oneshot(request)
            .await
            .expect("response")
    }
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
