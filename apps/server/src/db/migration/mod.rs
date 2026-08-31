//! Migration runner.
//!
//! # Why the SQL is hand-written and not built with `sea-query`
//!
//! SeaORM's schema builder produces one definition that renders correct DDL for
//! every dialect, the obvious thing to reach for. It is not what this
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
//! migrations are rare; the queries, which change often, are still written once
//! through SeaORM. The risk is a migration updated for one dialect and not the
//! others, which CI catches by applying all three and comparing the schemas.

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
mod m20260902_000001_upload_backend;
mod m20260903_000001_upload_segments;
mod m20260904_000001_parts_per_segment;
mod m20260905_000001_oidc;
mod m20260906_000001_backchannel_logout;

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
            Box::new(m20260902_000001_upload_backend::Migration),
            Box::new(m20260903_000001_upload_segments::Migration),
            Box::new(m20260904_000001_parts_per_segment::Migration),
            Box::new(m20260905_000001_oidc::Migration),
            Box::new(m20260906_000001_backchannel_logout::Migration),
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
        // answer: running another dialect's DDL would half-create a schema.
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
/// Statements are sent one at a time, not as one blob, because MySQL's
/// protocol rejects multi-statement queries unless the client opts in, so "just
/// execute the whole file" works on two backends out of three.
///
/// The split is naive: it breaks on a `;` that ends a line and understands
/// neither string literals nor `$$`-quoted bodies. Migration SQL
/// here is DDL, where that is not a real constraint. If a future migration
/// needs a semicolon inside a literal, give it its own file instead of making
/// this cleverer. A parser in a migration runner is a bug factory.
///
/// Line endings are normalised first. A checkout with `core.autocrlf` on would
/// otherwise leave every `;` followed by `\r\n`, no split would match, and the
/// whole file would go out as one statement, which SQLite and Postgres tolerate
/// and MySQL rejects: a bug reaching exactly one backend on exactly one kind of
/// developer machine.
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
    /// has to reach it, or the only account that can grant permissions cannot
    /// grant the new one to anybody, itself included.
    ///
    /// Across the whole chain, not one file, because a permission added
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
    // single statement, accepted by SQLite and Postgres and rejected by MySQL.
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
        // The comment travels with its statement instead of being stripped, so
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

    /// Every migration's three dialect files, read from disk and not listed.
    ///
    /// It used to be a hand-written array with a comment saying that forgetting
    /// a line would show up as a missing entry. Nothing checked, so it did not:
    /// the list had seven of thirty-eight migrations and the other thirty-one
    /// had never been compared. A list that has to be remembered is not a
    /// guard, it is a second thing to forget.
    fn dialect_files() -> Vec<(String, String, String, String)> {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migration/sql");
        let mut found = Vec::new();
        for entry in std::fs::read_dir(&directory).expect("the migration sql directory") {
            let path = entry.expect("a directory entry").path();
            let Some(name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
                continue;
            };
            let Some(migration) = name.strip_suffix(".sqlite.sql") else {
                continue;
            };
            let read = |dialect: &str| {
                std::fs::read_to_string(directory.join(format!("{migration}.{dialect}.sql")))
                    .unwrap_or_else(|_| panic!("{migration} has no {dialect} file"))
            };
            found.push((
                migration.to_owned(),
                read("sqlite"),
                read("postgres"),
                read("mysql"),
            ));
        }
        assert!(
            found.len() > 30,
            "found {} migrations, which is too few to be the whole set",
            found.len()
        );
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found
    }

    /// `scripts/schema-docs.py` builds the database it documents by replaying
    /// every `sql/*.sqlite.sql` in name order. A rollback left in that
    /// directory sorts *after* the migration it undoes (`.` before `_`), so the
    /// generated schema comes out rolled back and the `checks` job is happy
    /// because the files match what the script produced. Nothing else catches
    /// it.
    ///
    /// Rollbacks therefore live in `sql/down/`, and this is what says so.
    #[test]
    fn no_rollback_sits_where_the_schema_docs_replay_from() {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migration/sql");
        let mut misplaced = Vec::new();
        for entry in std::fs::read_dir(&directory).expect("the migration sql directory") {
            let path = entry.expect("a directory entry").path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or_default()
                .to_owned();
            if name.contains("down") {
                misplaced.push(name);
            }
        }
        assert!(
            misplaced.is_empty(),
            "rollbacks belong in sql/down/, or the generated schema documents the \
             rolled-back database: {misplaced:?}"
        );
    }

    /// Every dialect file is non-empty, or empty by design.
    ///
    /// This used to compare statement *counts* across the three files, over a
    /// hand-written list of seven migrations. Widening it to all thirty-eight
    /// showed the rule was never true: MySQL needs a separate `ALTER TABLE ADD
    /// CONSTRAINT` where SQLite inlines `REFERENCES`, SQLite rebuilds a table
    /// where MySQL drops a column, and one migration is MySQL's alone. The
    /// seven on the list were the seven where the counts coincided.
    ///
    /// What is left is the part that is actually invariant. The real guard
    /// against "sqlite was updated and the others were forgotten" is the
    /// `backends` job, which applies all three against real servers, and the
    /// table comparison below.
    #[test]
    fn every_migration_has_all_three_dialects() {
        for (migration, sqlite, postgres, mysql) in dialect_files() {
            // `m20260819_000002_mysql_datetime` fixes a column type only MySQL
            // ever had, so the other two are empty by design.
            let intentionally_empty = migration.contains("mysql_datetime");
            for (dialect, sql) in [("sqlite", &sqlite), ("postgres", &postgres)] {
                assert!(
                    intentionally_empty || statements(sql).count() > 0,
                    "{migration}: {dialect} does nothing"
                );
            }
            assert!(
                statements(&mysql).count() > 0,
                "{migration}: mysql does nothing"
            );
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

        for (migration, sqlite, postgres, mysql) in dialect_files() {
            // A rebuild is exempt. SQLite cannot alter a primary key, so it
            // renames the table aside, creates the new shape under the real
            // name and copies, where Postgres and MySQL say `ALTER TABLE` and
            // create nothing. One `CREATE TABLE` against none is a difference
            // in technique, not in schema, and reading the text cannot tell
            // them apart. The `backends` job checks these by applying all three
            // against real servers and rolling them back.
            if [&sqlite, &postgres, &mysql]
                .iter()
                .any(|sql| sql.to_lowercase().contains("rename to"))
            {
                continue;
            }

            let expected = tables(&sqlite);
            for (dialect, sql) in [("postgres", &postgres), ("mysql", &mysql)] {
                assert_eq!(
                    tables(sql),
                    expected,
                    "{migration}: {dialect} creates different tables than sqlite"
                );
            }
        }
    }
}
