//! Exercises a networked database backend against a real server.
//!
//! Until this file existed, the `postgres` and `mysql` features only ever
//! proved that they *compiled*: the migrations, the pool options and the health
//! path had never run against a live server. That gap opened the moment the
//! project chose three dialects, since `sqlx::query!` verified one at compile
//! time and the other two were always going to need a real database.
//!
//! Every test here is `#[ignore]`d, because it needs a server that a plain
//! `cargo nextest run` has no reason to provide. Ignoring, not silently
//! skipping is deliberate: a test that returns early when an environment
//! variable is missing reports "passed" and teaches you to trust coverage that
//! never ran.
//!
//! The target comes from `TESSERA_TEST_DATABASE_URL`, so the same suite runs
//! against every backend instead of being copied per dialect:
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
// The helpers below sit outside a `#[test]` body, and that is as far as
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
    auth::oidc::flow::Purpose,
    config::DatabaseConfig,
    db::{self, Backend, repository::oidc::Flow},
    state::ServiceStatus,
};
use time::OffsetDateTime;

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

/// Backend the configured URL points at. Asserting on this, not hard-coding
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
    // safe: sea-orm-migration does not lock before creating its bookkeeping
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
/// the hand-written per-dialect SQL agree about every *column* (Postgres
/// `TIMESTAMPTZ` against SQLite's TEXT, MySQL's `BOOLEAN` being `TINYINT(1)`,
/// nullability, the unique indexes and the foreign keys) is only settled by
/// writing rows and reading them back. That is what this does, through the HTTP
/// surface, so a disagreement shows up as the wrong status code and not a panic
/// that maps back to nothing.
#[tokio::test]
#[ignore = "needs a live database; see the module docs"]
async fn accounts_sessions_and_tokens_round_trip() {
    let app = TestApp::with_database(&database_url(), ServiceStatus::Ready).await;
    // Unlike the in-memory SQLite the other suites use, this database survives
    // the run, so the account names have to be unique or the second invocation
    // fails on a conflict that says nothing about the schema. Whole seconds,
    // because a username is capped at 32 characters and the backend name has
    // already spent some of them.
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
/// instructions (ask the next layer, or no ceiling at all) so a dialect that
/// coerced one into the other would quietly widen or narrow everybody's
/// allowance with nothing to say it had.
#[tokio::test]
#[ignore = "needs a live database; see the module docs"]
async fn quotas_round_trip() {
    use tessera_core::PermissionSet;
    use tessera_server::db::repository::role::{RoleFields, RoleRepository};

    let app = TestApp::with_database(&database_url(), ServiceStatus::Ready).await;
    let now = OffsetDateTime::now_utc();
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

    // Zero survives as zero, not as "not set".
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

    // An edit through the whole-record path keeps it too, and that is the path
    // the
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
/// `share_id IN (SELECT id FROM shares WHERE ...)` and not a join, and it
/// reads a blob's length, which used to be `CHAR_LENGTH`, a function Postgres
/// does not have for `bytea`. And Postgres widens `SUM(bigint)` to `NUMERIC`,
/// so the result would not decode as an `i64` even once the sum ran. Both were
/// found by running this suite, not by reasoning about it, and both made
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
    let now = OffsetDateTime::now_utc();
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
    // Nothing is charged to an empty role, so zero is the honest answer.
    // What is being tested is that the subquery runs at all.
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

    // And the instance total with a text share in it, the half that
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

/// A full rollback on SQLite, not `#[ignore]`d.
///
/// The dialect test below needs a server, so it only runs where CI starts one:
/// Postgres and MySQL. SQLite needs nothing and is the shipped default, yet it
/// was the dialect nothing checked.
///
/// It was broken when this was written: `m20260824_000001_single_role`'s
/// rollback dropped `users.role_id` while `idx_users_role` still referenced
/// it, which SQLite refuses. Postgres drops the index with the column and
/// never noticed.
#[tokio::test]
async fn a_rollback_runs_on_sqlite() {
    use sea_orm_migration::MigratorTrait as _;

    let conn = db::connect(&DatabaseConfig {
        url: "sqlite::memory:".to_owned(),
        // One connection: `sqlite::memory:` gives each connection its own
        // database, so a larger pool would roll back an empty copy.
        max_connections: 1,
        ..Default::default()
    })
    .await
    .expect("pool");
    db::migrate(&conn).await.expect("up");

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

    // And back up on what the rollback left, which is where a `down` that
    // restores the wrong type or forgets an index fails.
    db::migrate(&conn).await.expect("up again");
    db::close(conn).await;
}

/// Every migration rolls back on this dialect, all the way to an empty
/// database, and then forward again.
///
/// Before this test, nothing had ever run `down`, on any dialect. The `up`
/// path is exercised by every other test here; the rollbacks were written and
/// reviewed but never executed. Four failed on MySQL: two used
/// `DROP INDEX IF EXISTS`, which it lacks; one rebuilt a table with `TEXT` in
/// its primary key, which `InnoDB` cannot index without a prefix length; and
/// one dropped the last index a foreign key depended on. All four were found
/// by running this, not by reading them.
///
/// Rolling forward again is half the point: a `down` that leaves the schema
/// subtly different is a rollback that works once.
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

/// One user's preferences, written and read back on this dialect.
///
/// The same gap `auth_flows` fell into: nothing else in the suite writes to
/// this table, so a migration that runs proves nothing about storing a row.
/// MySQL is the risky one, where `TEXT` cannot be a primary key without a
/// prefix length and cannot have a `DEFAULT` at all.
///
/// The upsert is the other half. Saving twice is normal (one device, then
/// another), and it is one statement whose conflict clause each dialect
/// spells differently.
#[tokio::test]
#[ignore = "needs a live database; see the module docs"]
async fn preferences_round_trip_on_this_dialect() {
    use tessera_core::{
        Accent, AccountState, Appearance, EmailAddress, Mode, Theme, UserId, Username,
    };
    use tessera_server::db::repository::user::User;

    let app = TestApp::with_database(&database_url(), ServiceStatus::Ready).await;
    let now = OffsetDateTime::now_utc();

    // The row has a foreign key to `users`, so it needs an owner. A fresh
    // account, not a shared one: this database outlives the run, so a fixed
    // name would collide with the previous run and the failure would say
    // nothing about the schema.
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let user = UserId::generate();
    app.state
        .users
        .insert(
            &User {
                id: user,
                username: Username::parse(&format!("prefs{suffix}")).expect("a plain name"),
                email: EmailAddress::parse(&format!("prefs{suffix}@example.test"))
                    .expect("a plain address"),
                role_id: None,
                quota_bytes: None,
                max_file_bytes: None,
                must_change_password: false,
                state: AccountState::Active,
                deleted_at: None,
                deleted_by: None,
                email_verified_at: None,
                created_at: now,
                updated_at: now,
            },
            None,
        )
        .await
        .expect("an account for the row to belong to");

    // Nothing stored reads as nothing chosen: not an error, and not the
    // instance default in disguise.
    assert_eq!(
        app.state
            .preferences
            .appearance_for(user)
            .await
            .expect("a query"),
        Appearance::default()
    );

    let chosen = Appearance {
        mode: Some(Mode::Dark),
        theme: Some(Theme::Sepia),
        accent: Some(Accent::Rose),
    };
    app.state
        .preferences
        .set_appearance(user, &chosen, now)
        .await
        .expect("the preferences should be storable on this dialect");
    assert_eq!(
        app.state
            .preferences
            .appearance_for(user)
            .await
            .expect("a query"),
        chosen
    );

    // Saving again is an upsert, not a second row: one user, one row, on every
    // dialect.
    let narrowed = Appearance {
        accent: Some(Accent::Teal),
        ..Appearance::default()
    };
    app.state
        .preferences
        .set_appearance(user, &narrowed, now)
        .await
        .expect("the second save should replace the first");
    assert_eq!(
        app.state
            .preferences
            .appearance_for(user)
            .await
            .expect("a query"),
        narrowed,
        "the whole object is replaced, so the other two go back to following \
         the instance"
    );
}

/// A sign-in in progress at a provider, written and read back on this dialect.
///
/// This test was missing. `auth_flows` was added with every column typed
/// `TEXT` on every dialect, copied from the SQLite file, and the suite passed:
/// MySQL rejected the index and said so, but Postgres accepted a table nothing
/// here ever wrote to. A time in a `text` column is a string, and
/// `expires_at <= now` would have compared lexicographically.
///
/// So the round trip matters more than the migration running. A column of the
/// wrong type fails here instead of in a sweep that quietly stops collecting.
#[tokio::test]
#[ignore = "needs a live database; see the module docs"]
async fn a_provider_sign_in_round_trips_on_this_dialect() {
    let app = TestApp::with_database(&database_url(), ServiceStatus::Ready).await;
    let now = OffsetDateTime::now_utc();
    let flow = Flow {
        state: format!("state-{}", now.unix_timestamp_nanos()),
        provider: "github".to_owned(),
        // Absent: the case a `NOT NULL` column would have refused.
        nonce: None,
        verifier: "a-verifier".to_owned(),
        purpose: Purpose::Login,
        user_id: None,
        invite_code: Some("an-invite".to_owned()),
        expires_at: now + time::Duration::minutes(15),
    };

    app.state
        .identities
        .begin(&flow, now)
        .await
        .expect("the flow should be storable on this dialect");

    let taken = app
        .state
        .identities
        .consume(&flow.state)
        .await
        .expect("a query")
        .expect("the row this test just wrote");
    assert_eq!(taken.provider, "github");
    assert!(taken.nonce.is_none());
    assert_eq!(taken.invite_code.as_deref(), Some("an-invite"));

    // What a `text` column would get wrong: comparing times, not strings. The
    // expired row goes and the live one stays.
    let expired = Flow {
        state: format!("expired-{}", now.unix_timestamp_nanos()),
        expires_at: now - time::Duration::minutes(1),
        ..flow.clone()
    };
    let live = Flow {
        state: format!("live-{}", now.unix_timestamp_nanos()),
        ..flow
    };
    app.state
        .identities
        .begin(&expired, now)
        .await
        .expect("stored");
    app.state
        .identities
        .begin(&live, now)
        .await
        .expect("stored");

    app.state
        .identities
        .sweep_expired(now)
        .await
        .expect("a sweep");

    assert!(
        app.state
            .identities
            .consume(&expired.state)
            .await
            .expect("a query")
            .is_none(),
        "the expired flow should have been swept"
    );
    assert!(
        app.state
            .identities
            .consume(&live.state)
            .await
            .expect("a query")
            .is_some(),
        "the live flow should have been left alone"
    );
}
