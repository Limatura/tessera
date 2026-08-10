//! Exercises a networked database backend against a real server.
//!
//! Until this file existed, the `postgres` and `mysql` features only ever proved
//! that they *compiled* — the migrations, the pool options and the health path
//! had never run against a live server. That is the gap this closes, and it is
//! the gap that opened the moment the project chose three dialects: `sqlx::query!`
//! verified one at compile time, so the other two were always going to need a
//! real database somewhere.
//!
//! Every test here is `#[ignore]`d, because it needs a server that a plain
//! `cargo nextest run` has no reason to provide. Ignoring rather than silently
//! skipping is deliberate: a test that returns early when an environment
//! variable is missing reports "passed" and teaches you to trust coverage that
//! never ran.
//!
//! The target comes from `TESSERA_TEST_DATABASE_URL`, so the same suite runs
//! against every backend rather than being copied per dialect:
//!
//! ```text
//! docker run -d --name tessera-pg -p 5432:5432 \
//!   -e POSTGRES_USER=tessera -e POSTGRES_PASSWORD=tessera \
//!   -e POSTGRES_DB=tessera_test postgres:17-alpine
//!
//! TESSERA_TEST_DATABASE_URL=postgres://tessera:tessera@localhost:5432/tessera_test \
//!   cargo nextest run --run-ignored all -E 'binary(backends)'
//! ```
#![cfg(any(feature = "postgres", feature = "mysql"))]
// The helpers below sit outside a `#[test]` body, which is as far as
// clippy.toml's test exemptions reach. A fixture that cannot resolve its target
// has nothing to report but a failure.
#![expect(clippy::expect_used, reason = "test fixture; see module docs")]

mod common;

use axum::http::StatusCode;
use common::{
    TestApp, account_lifecycle, body_json, expect_status, share_repository_round_trip,
    share_retrieval_lifecycle,
};
use sea_orm::{ConnectionTrait as _, Statement};
use tessera_server::{
    config::DatabaseConfig,
    db::{self, Backend},
    state::ServiceStatus,
};

/// Overridable so the suite can point at an existing instance instead of the
/// throwaway container the docs describe.
const DEFAULT_URL: &str = "postgres://tessera:tessera@localhost:5432/tessera_test";

fn database_url() -> String {
    std::env::var("TESSERA_TEST_DATABASE_URL").unwrap_or_else(|_| DEFAULT_URL.to_owned())
}

fn database_config() -> DatabaseConfig {
    DatabaseConfig {
        url: database_url(),
        ..Default::default()
    }
}

/// Backend the configured URL points at. Asserting on this rather than hard-coding
/// Postgres is what lets one suite cover every dialect.
fn expected_backend() -> Backend {
    Backend::from_url(&database_url()).expect("TESSERA_TEST_DATABASE_URL must name a known backend")
}

#[tokio::test]
#[ignore = "needs a live database; see the module docs"]
async fn connects_migrates_and_pings() {
    let conn = db::connect(&database_config()).await.expect("pool");
    assert_eq!(db::backend_of(&conn), Some(expected_backend()));

    db::migrate(&conn).await.expect("migrations");
    db::ping(&conn).await.expect("ping");
    db::close(conn).await;
}

#[tokio::test]
#[ignore = "needs a live database; see the module docs"]
async fn migrations_are_idempotent() {
    // Sequential re-application only. Concurrent first-time migration is *not*
    // safe — sea-orm-migration does not lock before creating its bookkeeping
    // table, so parallel runners collide on a virgin database. That is why this
    // binary is pinned to one thread in `.config/nextest.toml`.
    let conn = db::connect(&database_config()).await.expect("pool");
    db::migrate(&conn).await.expect("first run");
    db::migrate(&conn).await.expect("second run");
    db::close(conn).await;
}

#[tokio::test]
#[ignore = "needs a live database; see the module docs"]
async fn the_initial_migration_creates_its_table() {
    // The per-dialect SQL files are hand-written, so "sqlite was updated and the
    // others were forgotten" is the failure this guards. The unit test compares
    // statement counts; this one proves the statements actually ran.
    let conn = db::connect(&database_config()).await.expect("pool");
    db::migrate(&conn).await.expect("migrations");

    let statement = Statement::from_string(
        conn.get_database_backend(),
        "SELECT COUNT(*) AS n FROM schema_metadata",
    );
    let row = conn
        .query_one_raw(statement)
        .await
        .expect("the table must exist and be queryable")
        .expect("one row");
    let count: i64 = row.try_get("", "n").expect("column");
    assert_eq!(
        count, 0,
        "the initial migration should leave the table empty"
    );

    db::close(conn).await;
}

#[tokio::test]
#[ignore = "needs a live database; see the module docs"]
async fn health_reports_this_backend() {
    // The point of the abstraction is that everything above `db` is unaware of
    // which backend is underneath. This asserts that end to end.
    let app = TestApp::with_database(&database_url(), ServiceStatus::Ready).await;
    let response = expect_status(app.get("/health").await, StatusCode::OK).await;
    let body = body_json(response).await;

    assert_eq!(body["status"], "ready");
    assert_eq!(body["database"], expected_backend().as_str());
    assert_eq!(body["version"], tessera_server::VERSION);
}

/// The reason this suite exists at all, applied to the tables that carry data.
///
/// Connecting and migrating only prove the DDL parses. Whether the entities and
/// the hand-written per-dialect SQL agree about every *column* — Postgres
/// `TIMESTAMPTZ` against SQLite's TEXT, MySQL's `BOOLEAN` being `TINYINT(1)`,
/// nullability, the unique indexes and the foreign keys — is only settled by
/// writing rows and reading them back. That is what this does, through the HTTP
/// surface, so a disagreement shows up as the wrong status code rather than as a
/// panic nobody maps back to a schema.
#[tokio::test]
#[ignore = "needs a live database; see the module docs"]
async fn accounts_sessions_and_tokens_round_trip() {
    let app = TestApp::with_database(&database_url(), ServiceStatus::Ready).await;
    // Unlike the in-memory SQLite the other suites use, this database survives
    // the run — so the account names have to be unique or the second invocation
    // fails on a conflict that says nothing about the schema.
    // Whole seconds, because a username is capped at 32 characters and the
    // backend name has already spent some of them.
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    account_lifecycle(&app, &format!("{}{suffix}", expected_backend().as_str())).await;
}

/// Same reasoning as the account lifecycle, for the shares tables: `MEDIUMBLOB`
/// on MySQL, `TIMESTAMPTZ` on Postgres, the nullable owner foreign key, and the
/// atomic use-count update are all things only a live server can settle.
#[tokio::test]
#[ignore = "needs a live database; see the module docs"]
async fn shares_round_trip_through_the_repository() {
    let app = TestApp::with_database(&database_url(), ServiceStatus::Ready).await;
    // Codes must be unique across runs, and the alphabet excludes I, L, O and U.
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    share_repository_round_trip(&app, &format!("{suffix}")).await;
}

/// Tickets are a table with a foreign key and a timestamp, so the same
/// dialect questions apply to them as to everything else.
#[tokio::test]
#[ignore = "needs a live database; see the module docs"]
async fn a_share_can_be_created_authorised_and_retrieved() {
    let app = TestApp::with_database(&database_url(), ServiceStatus::Ready).await;
    share_retrieval_lifecycle(&app).await;
}
