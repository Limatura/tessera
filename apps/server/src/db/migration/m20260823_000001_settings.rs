//! Settings changed from the admin panel.
//!
//! The SQL lives in `sql/`, one file per dialect — see the SQLite one for why
//! this is a table rather than a file, and why only hot-changeable settings
//! belong in it.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260823_000001_settings.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260823_000001_settings.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260823_000001_settings.mysql.sql");

/// Dropping the table returns the instance to its file and environment, which
/// is where it was before. Nothing else depends on these rows.
const DOWN: &str = "DROP TABLE IF EXISTS settings;\n";

#[derive(Debug, DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
// `MigrationTrait` elides the lifetime in its own signature. Writing
// `SchemaManager<'_>` here would make it early-bound, and the impl would no
// longer match the trait — so the elision is required, not a style choice.
#[expect(
    elided_lifetimes_in_paths,
    reason = "must match MigrationTrait's declared signature exactly"
)]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        super::execute_for_backend(manager, SQLITE, POSTGRES, MYSQL).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        super::execute_portable(manager, DOWN).await
    }
}

#[cfg(test)]
mod tests {
    use super::{MYSQL, POSTGRES, SQLITE};

    /// `key` is reserved in MySQL. Quoting it works until somebody writes one
    /// query without the backticks, so the column is not called that anywhere.
    #[test]
    fn no_dialect_names_the_column_key() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(
                sql.contains("name "),
                "{dialect} should key the table on `name`"
            );
            for line in sql.lines() {
                let statement = line.trim_start();
                assert!(
                    !statement.starts_with("key "),
                    "{dialect} declares a column called `key`, which MySQL reserves"
                );
            }
        }
    }

    #[test]
    fn every_dialect_creates_the_table() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(
                sql.contains("CREATE TABLE IF NOT EXISTS settings"),
                "{dialect} does not create it"
            );
        }
    }
}
