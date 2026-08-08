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

use sea_orm_migration::{prelude::*, sea_orm::DbBackend};

/// Applies every migration in this module, in order.
#[derive(Debug)]
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20260808_000001_initial::Migration)]
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

    use super::*;

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

    /// Every dialect file must contain the same set of statements, or one of them
    /// has been updated and the others forgotten. This is the cheap half of the
    /// guard; CI applies all three for real and compares the resulting schemas.
    #[test]
    fn every_dialect_file_has_the_same_statement_count() {
        let files = [
            ("sqlite", m20260808_000001_initial::SQLITE),
            ("postgres", m20260808_000001_initial::POSTGRES),
            ("mysql", m20260808_000001_initial::MYSQL),
        ];
        let counts: Vec<_> = files
            .iter()
            .map(|(name, sql)| (*name, statements(sql).count()))
            .collect();

        let first = counts[0].1;
        for (name, count) in &counts {
            assert_eq!(
                *count, first,
                "{name} has {count} statements but sqlite has {first} — a dialect was missed"
            );
        }
        assert!(first > 0, "the initial migration must do something");
    }
}
