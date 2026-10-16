//! Storage backends an operator can add without restarting.
//!
//! The SQL lives in `sql/`. See the SQLite one for why these are rows rather
//! than settings, why the credentials are sealed, and why the name is the key.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260830_000001_storage_backends.sqlite.sql");
pub(super) const POSTGRES: &str =
    include_str!("sql/m20260830_000001_storage_backends.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260830_000001_storage_backends.mysql.sql");

/// The backend built from `storage.url` is not in this table, so rolling back
/// leaves an instance able to serve everything written before a second backend
/// was added, and unable to reach anything written to one.
const DOWN: &str = "DROP TABLE IF EXISTS storage_backends;\n";

#[derive(Debug, DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
// `MigrationTrait` elides the lifetime in its own signature. Writing
// `SchemaManager<'_>` here would make it early-bound and the impl would no
// longer match the trait, so the elision is required, not a style choice.
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

    /// Both carry credentials, so both are stored sealed, and the column
    /// holding sealed bytes cannot be length-limited by an index that is not
    /// there.
    #[test]
    fn the_credential_columns_are_text() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(sql.contains("url         TEXT"), "{dialect}");
            assert!(sql.contains("options     TEXT"), "{dialect}");
        }
    }

    /// `file_segments.backend` names one of these, and that reference has to
    /// stay readable after the row is gone. A key across would turn "an
    /// operator removed a backend" into a database error.
    #[test]
    fn nothing_points_at_this_table() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(!sql.contains("REFERENCES"), "{dialect}");
        }
    }
}
