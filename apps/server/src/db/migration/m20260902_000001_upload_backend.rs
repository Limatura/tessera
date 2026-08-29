//! Which backend a resumable upload is writing to.
//!
//! The SQL lives in `sql/`. See the SQLite one for why it is nullable and why
//! it is never changed once written.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260902_000001_upload_backend.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260902_000001_upload_backend.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260902_000001_upload_backend.mysql.sql");

/// Rolling back sends every in-flight session back to the backend from
/// `storage.url`, where they all were before this, so a session opened since
/// and placed elsewhere loses track of its parts. Finish or abandon the open
/// ones first.
const DOWN: &str = "ALTER TABLE uploads DROP COLUMN backend;\n";

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

    /// A session opened before this migration is already writing somewhere, and
    /// this column will not say where: NULL has to read as the backend from
    /// `storage.url`, where it went.
    #[test]
    fn the_column_is_nullable() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            let line = sql
                .lines()
                .find(|line| line.contains("ADD COLUMN backend"))
                .unwrap_or_default();
            assert!(
                !line.contains("NOT NULL") && !line.contains("DEFAULT"),
                "{dialect}: {line}"
            );
        }
    }

    /// No index and no key names it, so one rollback works everywhere, unlike
    /// the quota migration, which needed a MySQL-only one.
    #[test]
    fn nothing_needs_a_dialect_specific_rollback() {
        for sql in [SQLITE, POSTGRES, MYSQL] {
            assert!(!sql.contains("REFERENCES"));
            assert!(!sql.contains("CREATE INDEX"));
        }
    }
}
