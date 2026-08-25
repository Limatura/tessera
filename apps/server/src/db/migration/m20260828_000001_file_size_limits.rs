//! Per-role and per-account limits on the size of a single file.
//!
//! The SQL lives in `sql/`, one file per dialect — see the SQLite one for why
//! the columns are nullable and what NULL means.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260828_000001_file_size_limits.sqlite.sql");
pub(super) const POSTGRES: &str =
    include_str!("sql/m20260828_000001_file_size_limits.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260828_000001_file_size_limits.mysql.sql");

/// Everything falls back to `server.max_body_bytes`, which is where it was
/// before. What is lost is the figures somebody set, and nothing can recompute
/// those.
///
/// No index and no foreign key on either column, so one spelling works on every
/// dialect — unlike the quota migration, which had to drop a key on MySQL first.
const DOWN: &str = "ALTER TABLE users DROP COLUMN max_file_bytes;\n\
                    ALTER TABLE roles DROP COLUMN max_file_bytes;\n";

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
    use super::{DOWN, MYSQL, POSTGRES, SQLITE};

    /// Nullable on purpose: NULL asks the next layer, zero means no limit.
    /// `NOT NULL DEFAULT 0` would give every role and account an explicit
    /// "unlimited" and make the layering unreachable.
    #[test]
    fn the_columns_are_nullable() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            for line in sql.lines().filter(|line| line.contains("ADD COLUMN")) {
                assert!(
                    !line.contains("NOT NULL") && !line.contains("DEFAULT"),
                    "{dialect} would stop NULL meaning \"ask the next layer\": {line}"
                );
            }
        }
    }

    /// The quota migration needed a MySQL-only rollback because a foreign key
    /// named the column. These carry neither key nor index, which is the whole
    /// reason one spelling is enough — so if that ever changes, this fails.
    #[test]
    fn nothing_here_needs_a_dialect_specific_rollback() {
        for sql in [SQLITE, POSTGRES, MYSQL] {
            assert!(!sql.contains("REFERENCES"));
            assert!(!sql.contains("CREATE INDEX"));
        }
        assert!(!DOWN.contains("DROP INDEX") && !DOWN.contains("DROP FOREIGN KEY"));
    }
}
