//! How much a backend may hold, and whether it still takes new content.
//!
//! The SQL lives in `sql/`. See the SQLite one for why the figure is typed in
//! and not queried, and why read-only is its own state.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260901_000001_backend_capacity.sqlite.sql");
pub(super) const POSTGRES: &str =
    include_str!("sql/m20260901_000001_backend_capacity.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260901_000001_backend_capacity.mysql.sql");

/// Rolling back means every backend is unlimited and writable again, as they
/// were before. The figures that were typed in are lost and nothing
/// can recompute them; see the migration for why they cannot be queried.
///
/// One spelling everywhere: no index and no key names either column. The quota
/// rollback needed dialect-specific handling for exactly that.
const DOWN: &str = "ALTER TABLE storage_backends DROP COLUMN read_only;\n\
                    ALTER TABLE storage_backends DROP COLUMN capacity_bytes;\n";

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

    /// An existing backend has to keep taking uploads across this migration.
    /// A `NOT NULL DEFAULT TRUE` on `read_only` would stop every instance that
    /// applied it.
    #[test]
    fn existing_backends_stay_writable() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(sql.contains("read_only"), "{dialect}");
            assert!(
                sql.contains("DEFAULT FALSE"),
                "{dialect} would retire every existing backend"
            );
        }
    }

    /// Unlike the quota columns, there is no next layer to ask, so NULL is not
    /// a third state and the column carries no default.
    #[test]
    fn capacity_is_nullable_and_undefaulted() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            let line = sql
                .lines()
                .find(|line| line.contains("capacity_bytes"))
                .unwrap_or_default();
            assert!(
                !line.contains("NOT NULL") && !line.contains("DEFAULT"),
                "{dialect}: {line}"
            );
        }
    }
}
