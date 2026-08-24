//! Per-role and per-account storage quotas.
//!
//! The SQL lives in `sql/`, one file per dialect — see the SQLite one for the
//! three layers, why NULL is not zero, and why `uploads` gains a role.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260826_000001_quotas.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260826_000001_quotas.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260826_000001_quotas.mysql.sql");

/// Every instance falls back to `storage.max_total_bytes`, which is where it
/// was before. What is lost is the per-role and per-account figures somebody
/// set, and nothing can recompute those.
///
/// SQLite refuses to drop an indexed column, so the index goes first. Postgres
/// does not need that but takes it.
const DOWN: &str = "DROP INDEX IF EXISTS idx_uploads_role;\n\
                    ALTER TABLE uploads DROP COLUMN role_id;\n\
                    ALTER TABLE users DROP COLUMN quota_bytes;\n\
                    ALTER TABLE roles DROP COLUMN quota_bytes;\n";

/// The same, in the two ways MySQL differs.
///
/// `DROP INDEX IF EXISTS` is not MySQL syntax at all — `DROP INDEX` there takes
/// no `IF EXISTS` and needs an `ON <table>` — and a column a foreign key still
/// names cannot be dropped — MySQL says "needed in a foreign key constraint".
/// So the key goes first, and the index is left to go with the column, which
/// MySQL does on its own.
///
/// Checked against a real MySQL rather than reasoned about, because the shared
/// version fails on both statements and no suite runs `down`.
const DOWN_MYSQL: &str = "ALTER TABLE uploads DROP FOREIGN KEY fk_uploads_role;\n\
                          ALTER TABLE uploads DROP COLUMN role_id;\n\
                          ALTER TABLE users DROP COLUMN quota_bytes;\n\
                          ALTER TABLE roles DROP COLUMN quota_bytes;\n";

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
        super::execute_for_backend(manager, DOWN, DOWN, DOWN_MYSQL).await
    }
}

#[cfg(test)]
mod tests {
    use super::{DOWN, DOWN_MYSQL, MYSQL, POSTGRES, SQLITE};

    /// Nullable on purpose: NULL asks the next layer, zero means no ceiling.
    /// A `NOT NULL DEFAULT 0` would silently give every account and every role
    /// an explicit "unlimited" and make the layering unreachable.
    #[test]
    fn the_quota_columns_are_nullable() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            for line in sql.lines().filter(|line| line.contains("quota_bytes")) {
                assert!(
                    !line.contains("NOT NULL") && !line.contains("DEFAULT"),
                    "{dialect} would stop NULL meaning \"ask the next layer\": {line}"
                );
            }
        }
    }

    /// `InnoDB` silently ignores a `REFERENCES` written on a column, so the
    /// column form would leave MySQL without the cascade above.
    #[test]
    fn mysql_declares_the_key_where_innodb_reads_it() {
        assert_eq!(MYSQL.matches("FOREIGN KEY").count(), 1);
        for line in MYSQL.lines() {
            let statement = line.trim_start();
            if statement.starts_with("--") || statement.contains("ADD CONSTRAINT") {
                continue;
            }
            assert!(
                !statement.contains("REFERENCES"),
                "`InnoDB` ignores this: {statement}"
            );
        }
    }

    /// Reserved bytes are summed per role, which a table with a row per
    /// in-flight upload cannot answer with a scan.
    #[test]
    fn every_dialect_indexes_the_upload_role() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(sql.contains("idx_uploads_role"), "{dialect}");
        }
    }

    /// `down` has two MySQL-only problems, and neither shows up anywhere else:
    /// `DROP INDEX IF EXISTS` is not syntax MySQL has, and a column a foreign
    /// key still names cannot be dropped.
    #[test]
    fn the_mysql_rollback_does_not_use_the_portable_one() {
        assert!(!DOWN_MYSQL.contains("DROP INDEX"));
        assert!(DOWN_MYSQL.contains("DROP FOREIGN KEY fk_uploads_role"));
        assert!(
            DOWN_MYSQL.find("DROP FOREIGN KEY") < DOWN_MYSQL.find("DROP COLUMN role_id"),
            "the key has to go before the column it names"
        );
        // And the portable one still drops the index first, because SQLite
        // refuses to drop a column that one names.
        assert!(DOWN.find("DROP INDEX") < DOWN.find("DROP COLUMN role_id"));
    }

    /// A role that cannot be deleted without abandoning somebody's upload is a
    /// role nobody can delete. Losing which role an in-flight session was
    /// charged to costs one session's accounting; refusing the delete, or
    /// taking the session with it, costs more.
    #[test]
    fn removing_a_role_leaves_the_open_sessions() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(sql.contains("ON DELETE SET NULL"), "{dialect}");
        }
    }
}
