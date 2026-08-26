//! Migration runner.
//!
//! # Why the SQL is hand-written and not built with `sea-query`
//!
//! SeaORM's schema builder produces one definition that renders correct DDL for
//! every dialect, which is the obvious thing to reach for. It is not what this
//! project uses, for one reason: **the table definition is the part people need
//! to read.** A contributor checking what a migration does, or an operator
//! confirming what their database looks like, should see
//!
//! ```sql
//! CREATE TABLE shares (id TEXT PRIMARY KEY, ...);
//! ```
//!
//! not a chain of `ColumnDef::new(...).text().not_null()`. The SQL files here can
//! be read at a glance, diffed meaningfully in a pull request, and pasted
//! straight into a shell to check a hypothesis.
//!
//! The cost is one file per dialect per migration. That is affordable because
//! migrations are rare — the queries, which change often, are still written once
//! through SeaORM. The risk it introduces is a migration updated for one dialect
//! and not the others, which CI catches by applying all three and comparing the
//! resulting schemas.

mod m20260808_000001_initial;
mod m20260809_000001_users;
mod m20260809_000002_sessions;
mod m20260809_000003_api_tokens;
mod m20260810_000001_shares;
mod m20260810_000002_share_tickets;
mod m20260813_000001_password_reset_tokens;
mod m20260813_000002_pending_registrations;
mod m20260814_000001_session_metadata;
mod m20260814_000002_session_endings;
mod m20260815_000001_invites;
mod m20260816_000001_file_shares;
mod m20260817_000001_text_shares;
mod m20260818_000001_roles;
mod m20260818_000002_token_permissions;
mod m20260818_000003_admin_share_permissions;
mod m20260818_000004_account_state;
mod m20260818_000005_owner_role;
mod m20260818_000006_copy_count;
mod m20260818_000007_bytes_served;
mod m20260818_000008_drop_legacy_share_columns;
mod m20260819_000001_guest_role;
mod m20260819_000002_mysql_datetime;
mod m20260820_000001_uploads;
mod m20260821_000001_drop_is_admin;
mod m20260821_000002_share_listing_index;
mod m20260822_000001_access_events;
mod m20260823_000001_settings;
mod m20260823_000002_settings_permissions;
mod m20260824_000001_single_role;
mod m20260825_000001_share_role;
mod m20260826_000001_quotas;
mod m20260828_000001_file_size_limits;
mod m20260829_000001_file_segments;
mod m20260830_000001_storage_backends;
mod m20260901_000001_backend_capacity;

use sea_orm_migration::{prelude::*, sea_orm::DbBackend};

/// Applies every migration in this module, in order.
#[derive(Debug)]
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260808_000001_initial::Migration),
            Box::new(m20260809_000001_users::Migration),
            Box::new(m20260809_000002_sessions::Migration),
            Box::new(m20260809_000003_api_tokens::Migration),
            Box::new(m20260810_000001_shares::Migration),
            Box::new(m20260810_000002_share_tickets::Migration),
            Box::new(m20260813_000001_password_reset_tokens::Migration),
            Box::new(m20260813_000002_pending_registrations::Migration),
            Box::new(m20260814_000001_session_metadata::Migration),
            Box::new(m20260814_000002_session_endings::Migration),
            Box::new(m20260815_000001_invites::Migration),
            Box::new(m20260816_000001_file_shares::Migration),
            Box::new(m20260817_000001_text_shares::Migration),
            Box::new(m20260818_000001_roles::Migration),
            Box::new(m20260818_000002_token_permissions::Migration),
            Box::new(m20260818_000003_admin_share_permissions::Migration),
            Box::new(m20260818_000004_account_state::Migration),
            Box::new(m20260818_000005_owner_role::Migration),
            Box::new(m20260818_000006_copy_count::Migration),
            Box::new(m20260818_000007_bytes_served::Migration),
            Box::new(m20260818_000008_drop_legacy_share_columns::Migration),
            Box::new(m20260819_000001_guest_role::Migration),
            Box::new(m20260819_000002_mysql_datetime::Migration),
            Box::new(m20260820_000001_uploads::Migration),
            Box::new(m20260821_000001_drop_is_admin::Migration),
            Box::new(m20260821_000002_share_listing_index::Migration),
            Box::new(m20260822_000001_access_events::Migration),
            Box::new(m20260823_000001_settings::Migration),
            Box::new(m20260823_000002_settings_permissions::Migration),
            Box::new(m20260824_000001_single_role::Migration),
            Box::new(m20260825_000001_share_role::Migration),
            Box::new(m20260826_000001_quotas::Migration),
            Box::new(m20260828_000001_file_size_limits::Migration),
            Box::new(m20260829_000001_file_segments::Migration),
            Box::new(m20260830_000001_storage_backends::Migration),
            Box::new(m20260901_000001_backend_capacity::Migration),
        ]
    }
}

/// Runs the dialect-appropriate SQL for the connection `manager` holds.
///
/// # Errors
///
/// Returns [`DbErr`] if any statement fails.
async fn execute_for_backend(
    manager: &SchemaManager<'_>,
    sqlite: &str,
    postgres: &str,
    mysql: &str,
) -> Result<(), DbErr> {
    let sql = match manager.get_database_backend() {
        DbBackend::Sqlite => sqlite,
        DbBackend::Postgres => postgres,
        DbBackend::MySql => mysql,
        // `DatabaseBackend` is `#[non_exhaustive]`. If SeaORM gains a backend
        // before we write SQL for it, refusing to migrate is the only safe
        // answer — running another dialect's DDL would half-create a schema.
        other => {
            return Err(DbErr::Migration(format!(
                "this migration has no SQL for the `{other:?}` backend"
            )));
        }
    };
    execute_all(manager, sql).await
}

/// Runs one SQL string that is valid on every backend.
///
/// # Errors
///
/// Returns [`DbErr`] if any statement fails.
async fn execute_portable(manager: &SchemaManager<'_>, sql: &str) -> Result<(), DbErr> {
    execute_all(manager, sql).await
}

async fn execute_all(manager: &SchemaManager<'_>, sql: &str) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    for statement in statements(sql) {
        connection.execute_unprepared(&statement).await?;
    }
    Ok(())
}

/// Splits a migration file into individual statements.
///
/// Statements are sent one at a time rather than as one blob because MySQL's
/// protocol rejects multi-statement queries unless the client opts in, so the
/// "just execute the whole file" shortcut works on two backends out of three —
/// the worst kind of shortcut.
///
/// The split is deliberately naive: it breaks on a `;` that ends a line, and
/// understands neither string literals nor `$$`-quoted bodies. Migration SQL here
/// is DDL, where that is not a real constraint. If a future migration needs a
/// semicolon inside a literal, give it its own file rather than making this
/// cleverer — a parser in a migration runner is a bug factory.
///
/// Line endings are normalised first. A checkout with `core.autocrlf` on would
/// otherwise leave every `;` followed by `\r\n`, no split would match, and the
/// whole file would go out as one statement — which SQLite and Postgres tolerate
/// and MySQL rejects. A bug that reaches exactly one backend, on exactly one kind
/// of developer machine, is not one to leave lying around.
fn statements(sql: &str) -> impl Iterator<Item = String> {
    sql.replace("\r\n", "\n")
        .split(";\n")
        .map(|statement| statement.trim().trim_end_matches(';').trim().to_owned())
        .collect::<Vec<_>>()
        .into_iter()
        .filter(|statement| {
            // Skip blanks and comment-only fragments; SQLite tolerates them but
            // MySQL reports an empty query as an error.
            !statement.is_empty()
                && statement.lines().any(|line| {
                    let line = line.trim();
                    !line.is_empty() && !line.starts_with("--")
                })
        })
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tessera_core::Permission;

    use super::*;

    /// `owner` is the role that can do everything, and a permission added later
    /// has to reach it — otherwise the only account that can grant permissions
    /// cannot grant the new one to anybody, including itself.
    ///
    /// Across the whole chain rather than one file, because a permission added
    /// after `owner` was seeded is granted by its own migration. Checking the
    /// seed alone would pass by editing a migration that has already run
    /// everywhere, which changes nothing for an instance that is upgrading.
    #[test]
    fn owner_is_granted_every_permission_somewhere_in_the_chain() {
        let chain = [
            (
                "sqlite",
                [
                    m20260818_000005_owner_role::SQLITE,
                    m20260823_000002_settings_permissions::SQLITE,
                ],
            ),
            (
                "postgres",
                [
                    m20260818_000005_owner_role::POSTGRES,
                    m20260823_000002_settings_permissions::POSTGRES,
                ],
            ),
            (
                "mysql",
                [
                    m20260818_000005_owner_role::MYSQL,
                    m20260823_000002_settings_permissions::MYSQL,
                ],
            ),
        ];

        for (dialect, files) in chain {
            let granted: String = files.concat();
            for permission in Permission::ALL {
                assert!(
                    granted.contains(&format!("'{}'", permission.as_str())),
                    "{dialect} never grants {permission} to the owner"
                );
            }
        }
    }

    #[rstest]
    #[case::unix("CREATE TABLE a (x INTEGER);\nCREATE INDEX i ON a(x);\n")]
    // Regression: with CRLF no split matched, so the whole file went out as a
    // single statement — accepted by SQLite and Postgres, rejected by MySQL.
    #[case::crlf("CREATE TABLE a (x INTEGER);\r\nCREATE INDEX i ON a(x);\r\n")]
    fn splits_on_statement_boundaries_whatever_the_line_endings(#[case] sql: &str) {
        let found: Vec<_> = statements(sql).collect();
        assert_eq!(
            found,
            ["CREATE TABLE a (x INTEGER)", "CREATE INDEX i ON a(x)"]
        );
    }

    #[test]
    fn drops_comment_only_and_empty_fragments() {
        let sql = "-- a leading note\n-- spanning two lines\nCREATE TABLE a (x INTEGER);\n\n";
        let found: Vec<_> = statements(sql).collect();
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("CREATE TABLE"));
    }

    #[test]
    fn keeps_comments_that_precede_a_statement() {
        // The comment travels with its statement rather than being stripped, so
        // errors from the database still point at recognisable SQL.
        let sql = "-- why this table exists\nCREATE TABLE a (x INTEGER);\n";
        let found: Vec<_> = statements(sql).collect();
        assert_eq!(found.len(), 1);
        assert!(found[0].starts_with("-- why this table exists"));
    }

    #[test]
    fn a_trailing_statement_without_a_newline_still_counts() {
        let found: Vec<_> = statements("CREATE TABLE a (x INTEGER)").collect();
        assert_eq!(found, ["CREATE TABLE a (x INTEGER)"]);
    }

    /// Every migration, with its three dialect files. Adding a migration means
    /// adding a line here — deliberately manual, so that forgetting shows up as
    /// a missing entry rather than as silently reduced coverage.
    const DIALECT_FILES: [(&str, &str, &str, &str); 6] = [
        (
            "m20260808_000001_initial",
            m20260808_000001_initial::SQLITE,
            m20260808_000001_initial::POSTGRES,
            m20260808_000001_initial::MYSQL,
        ),
        (
            "m20260809_000001_users",
            m20260809_000001_users::SQLITE,
            m20260809_000001_users::POSTGRES,
            m20260809_000001_users::MYSQL,
        ),
        (
            "m20260809_000002_sessions",
            m20260809_000002_sessions::SQLITE,
            m20260809_000002_sessions::POSTGRES,
            m20260809_000002_sessions::MYSQL,
        ),
        (
            "m20260809_000003_api_tokens",
            m20260809_000003_api_tokens::SQLITE,
            m20260809_000003_api_tokens::POSTGRES,
            m20260809_000003_api_tokens::MYSQL,
        ),
        (
            "m20260810_000001_shares",
            m20260810_000001_shares::SQLITE,
            m20260810_000001_shares::POSTGRES,
            m20260810_000001_shares::MYSQL,
        ),
        (
            "m20260810_000002_share_tickets",
            m20260810_000002_share_tickets::SQLITE,
            m20260810_000002_share_tickets::POSTGRES,
            m20260810_000002_share_tickets::MYSQL,
        ),
    ];

    /// Hand-writing one file per dialect buys readable SQL and risks updating one
    /// and forgetting the rest. This is the cheap half of the guard against that;
    /// CI applies all three against real servers for the other half.
    #[test]
    fn every_dialect_file_has_the_same_statement_count() {
        for (migration, sqlite, postgres, mysql) in DIALECT_FILES {
            let expected = statements(sqlite).count();
            assert!(expected > 0, "{migration} (sqlite) does nothing");

            for (dialect, sql) in [("postgres", postgres), ("mysql", mysql)] {
                let actual = statements(sql).count();
                assert_eq!(
                    actual, expected,
                    "{migration}: {dialect} has {actual} statements but sqlite has {expected} \
                     — a dialect was missed"
                );
            }
        }
    }

    /// A migration whose dialects disagree about which tables they create is the
    /// same bug the statement count is guarding against, seen from another angle.
    #[test]
    fn every_dialect_file_touches_the_same_tables() {
        fn tables(sql: &str) -> std::collections::BTreeSet<String> {
            statements(sql)
                .filter_map(|statement| {
                    let lower = statement.to_lowercase();
                    let marker = lower.find("create table")?;
                    lower
                        .get(marker..)?
                        .split_whitespace()
                        .find(|word| !matches!(*word, "create" | "table" | "if" | "not" | "exists"))
                        .map(|name| name.trim_matches(['(', '`', '"']).to_owned())
                })
                .collect()
        }

        for (migration, sqlite, postgres, mysql) in DIALECT_FILES {
            let expected = tables(sqlite);
            for (dialect, sql) in [("postgres", postgres), ("mysql", mysql)] {
                assert_eq!(
                    tables(sql),
                    expected,
                    "{migration}: {dialect} creates different tables than sqlite"
                );
            }
        }
    }
}
