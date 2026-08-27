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

/// The quota columns, against a real dialect.
///
/// `quota_bytes` is a nullable `BIGINT` whose NULL and whose zero are opposite
/// instructions — ask the next layer, or no ceiling at all — so a dialect that
/// coerced one into the other would quietly widen or narrow everybody's
/// allowance with nothing to say it had.
#[tokio::test]
#[ignore = "needs a live database; see the module docs"]
async fn quotas_round_trip() {
    use tessera_core::PermissionSet;
    use tessera_server::db::repository::role::{RoleFields, RoleRepository};

    let app = TestApp::with_database(&database_url(), ServiceStatus::Ready).await;
    let now = time::OffsetDateTime::now_utc();
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let roles = RoleRepository::new(app.state.db.clone());
    let permissions = PermissionSet::default();

    let role = roles
        .create(
            RoleFields {
                name: &format!("quota{suffix}"),
                description: None,
                quota_bytes: Some(4096),
                max_file_bytes: None,
                permissions: &permissions,
            },
            now,
        )
        .await
        .expect("create the role");
    assert_eq!(role.quota_bytes, Some(4096));

    // Zero survives as zero rather than arriving back as "not set".
    roles
        .set_quota(role.id, Some(0), now)
        .await
        .expect("store zero");
    assert_eq!(
        roles
            .find_by_id(role.id)
            .await
            .expect("query")
            .expect("the role")
            .quota_bytes,
        Some(0),
        "zero came back as something else"
    );

    // And NULL survives as NULL.
    roles.set_quota(role.id, None, now).await.expect("clear it");
    assert_eq!(
        roles
            .find_by_id(role.id)
            .await
            .expect("query")
            .expect("the role")
            .quota_bytes,
        None,
        "a cleared quota came back as a figure"
    );

    // An edit through the whole-record path keeps it too, which is the path the
    // panel takes.
    roles
        .update(
            role.id,
            RoleFields {
                name: &format!("quota{suffix}"),
                description: None,
                quota_bytes: Some(8192),
                max_file_bytes: None,
                permissions: &permissions,
            },
            now,
        )
        .await
        .expect("edit the role");
    assert_eq!(
        roles
            .find_by_id(role.id)
            .await
            .expect("query")
            .expect("the role")
            .quota_bytes,
        Some(8192)
    );
}

/// The scoped totals, against a real dialect.
///
/// Two ways this query only fails away from SQLite. It restricts through
/// `share_id IN (SELECT id FROM shares WHERE ...)` rather than a join, and it
/// reads a blob's length — which used to be `CHAR_LENGTH`, a function Postgres
/// does not have for `bytea`. And Postgres widens `SUM(bigint)` to `NUMERIC`,
/// so the result would not decode as an `i64` even once the sum ran. Both were
/// found by running this suite rather than by reasoning about it, and both made
/// every upload a 500 on a Postgres instance with a ceiling set.
#[tokio::test]
#[ignore = "needs a live database; see the module docs"]
async fn the_scoped_totals_run_on_this_dialect() {
    use tessera_core::PermissionSet;
    use tessera_server::db::repository::{
        quota::Scope,
        role::{RoleFields, RoleRepository},
    };

    let app = TestApp::with_database(&database_url(), ServiceStatus::Ready).await;
    let now = time::OffsetDateTime::now_utc();
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let role = RoleRepository::new(app.state.db.clone())
        .create(
            RoleFields {
                name: &format!("scope{suffix}"),
                description: None,
                quota_bytes: Some(4096),
                max_file_bytes: None,
                permissions: &PermissionSet::default(),
            },
            now,
        )
        .await
        .expect("create the role");
    // Nothing is charged to a role nobody is in, so zero is the honest answer
    // — what is being tested is that the subquery runs at all.
    assert_eq!(
        app.state
            .shares
            .stored_bytes(Scope::Role(role.id))
            .await
            .expect("the scoped total"),
        0
    );
    assert_eq!(
        app.state
            .uploads
            .reserved_bytes(Scope::Role(role.id))
            .await
            .expect("the reserved total"),
        0
    );

    // And the instance total with a text share in it, which is the half that
    // reads the blob's length.
    let text = "quota accounting\n";
    let before = app
        .state
        .shares
        .stored_bytes(Scope::Instance)
        .await
        .expect("the instance total");
    expect_status(
        app.send(
            axum::http::Request::builder()
                .method(axum::http::Method::POST)
                .uri("/api/v1/shares?language=text"),
            axum::body::Body::from(text),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let after = app
        .state
        .shares
        .stored_bytes(Scope::Instance)
        .await
        .expect("the instance total");
    assert_eq!(
        after - before,
        u64::try_from(text.len()).expect("a length"),
        "the text share's bytes were not counted"
    );
}

/// Every migration rolls back, on this dialect, all the way to an empty
/// database — and then forward again.
///
/// Nothing ran `down` before this test existed, on any dialect, ever. The `up`
/// path is exercised by every other test here; the rollbacks were written,
/// reviewed and never executed. Four of them did not work on MySQL: two used
/// `DROP INDEX IF EXISTS`, which is not syntax it has; one rebuilt a table with
/// `TEXT` in its primary key, which `InnoDB` cannot index without a prefix
/// length; and one dropped the last index a foreign key depended on. All four
/// were found by running this, not by reading them.
///
/// Rolling forward again afterwards is half the point: a `down` that leaves the
/// schema subtly different from where it started is a rollback that works once.
#[tokio::test]
#[ignore = "needs a live database; see the module docs"]
async fn a_full_rollback_runs_on_this_dialect() {
    use sea_orm_migration::MigratorTrait as _;

    let conn = db::connect(&database_config()).await.expect("pool");
    db::migrate(&conn).await.expect("up");

    let applied = db::migration::Migrator::get_applied_migrations(&conn)
        .await
        .expect("what is applied");
    assert!(
        applied.len() > 1,
        "nothing to roll back, so this test proves nothing"
    );

    db::migration::Migrator::down(&conn, None)
        .await
        .expect("every rollback");
    let left = db::migration::Migrator::get_applied_migrations(&conn)
        .await
        .expect("what is applied");
    assert!(
        left.is_empty(),
        "{} migrations would not roll back",
        left.len()
    );

    // Straight back up on the schema the rollback left behind. A `down` that
    // restores a column as the wrong type, or forgets an index, passes the
    // rollback and fails here.
    db::migrate(&conn).await.expect("up again");
    db::close(conn).await;
}
