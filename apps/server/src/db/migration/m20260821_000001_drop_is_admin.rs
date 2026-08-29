//! Removes `users.is_admin`, which roles replaced.
//!
//! The SQL lives in `sql/`, one file per dialect. See the SQLite one for what
//! the column was and why nothing has to change above it.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260821_000001_drop_is_admin.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260821_000001_drop_is_admin.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260821_000001_drop_is_admin.mysql.sql");

/// There is no going back from this one.
///
/// The column can be recreated empty, but which accounts held it cannot be
/// recovered, and deriving it from the roles table would invent history, since
/// `admin` granted through the role endpoints was never mirrored here. A
/// downgrade past this point is a restore from backup.
const DOWN: &str = "SELECT 1;\n";

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

    /// A column dropped on one backend and left on another is a schema that
    /// disagrees with itself, and the readers are gone either way, so the
    /// backend that kept it would carry a column nothing can explain.
    #[test]
    fn every_dialect_drops_the_column() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(
                sql.contains("DROP COLUMN is_admin"),
                "{dialect} still has `users.is_admin`"
            );
        }
    }
}
