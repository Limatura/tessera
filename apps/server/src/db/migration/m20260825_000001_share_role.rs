//! Which role uploaded a share.
//!
//! The SQL lives in `sql/`, one file per dialect — see the SQLite one for why
//! it is a snapshot, and why anonymous uploads carry `guest` rather than NULL.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260825_000001_share_role.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260825_000001_share_role.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260825_000001_share_role.mysql.sql");

/// Dropping the column loses which role each share was uploaded under, and
/// nothing can recompute it — the uploader's role today is not necessarily the
/// one they held then, which is the whole reason the column exists.
const DOWN: &str = "DROP INDEX IF EXISTS idx_shares_role;\n\
                    ALTER TABLE shares DROP COLUMN role_id;\n";

/// The same, in the two ways MySQL differs — the same pair the quota rollback
/// hit. `DROP INDEX IF EXISTS` is not syntax it has, and a column a foreign key
/// still names cannot be dropped. So the key goes first and the index is left to
/// go with the column, which MySQL does on its own.
const DOWN_MYSQL: &str = "ALTER TABLE shares DROP FOREIGN KEY fk_shares_role;\n\
                          ALTER TABLE shares DROP COLUMN role_id;\n";

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
    use super::{MYSQL, POSTGRES, SQLITE};

    /// `InnoDB` silently ignores a `REFERENCES` written on a column, so the
    /// column form would leave MySQL without the cascade — and removing a role
    /// would then fail, or take somebody's shares with it.
    #[test]
    fn mysql_declares_the_key_where_innodb_reads_it() {
        assert_eq!(MYSQL.matches("FOREIGN KEY").count(), 1);
        for line in MYSQL.lines() {
            let statement = line.trim_start();
            if statement.starts_with("--") || statement.starts_with("ALTER TABLE shares ADD CONS") {
                continue;
            }
            assert!(
                !statement.contains("REFERENCES"),
                "`InnoDB` ignores this: {statement}"
            );
        }
    }

    /// Summing one role's usage reads by role, and a table with a row per
    /// upload cannot answer that with a scan.
    #[test]
    fn every_dialect_indexes_the_column() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(sql.contains("idx_shares_role"), "{dialect}");
        }
    }

    /// Losing the role is recoverable; refusing to delete a role, or deleting
    /// somebody's share along with it, is not.
    #[test]
    fn removing_a_role_leaves_the_shares() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(sql.contains("ON DELETE SET NULL"), "{dialect}");
        }
    }
}
