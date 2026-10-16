//! Resumable uploads: a file that arrives across several requests.
//!
//! The SQL lives in `sql/`, one file per dialect. See the SQLite one for why
//! an unfinished upload is not a `file_shares` row, and why its object key is
//! decided once.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260820_000001_uploads.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260820_000001_uploads.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260820_000001_uploads.mysql.sql");

/// Parts first: it points at `uploads`, and dropping the target of a foreign
/// key before the thing that points at it fails on MySQL.
const DOWN: &str = "DROP TABLE IF EXISTS upload_parts;\n\
                    DROP TABLE IF EXISTS uploads;\n";

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

    /// `InnoDB` silently ignores `REFERENCES` written on a column and only
    /// honours a table-level `FOREIGN KEY`. A migration using the column form
    /// would create the table, pass every test, and leave MySQL with no cascade
    /// at all, so an upload session would outlive the share it was appending
    /// to.
    #[test]
    fn mysql_declares_its_foreign_keys_where_innodb_reads_them() {
        assert_eq!(
            MYSQL.matches("FOREIGN KEY").count(),
            3,
            "a relationship lost its cascade on MySQL"
        );
        for line in MYSQL.lines() {
            let statement = line.trim_start();
            if statement.starts_with("--") || statement.starts_with("CONSTRAINT") {
                continue;
            }
            assert!(
                !statement.contains("REFERENCES"),
                "`InnoDB` ignores this: {statement}"
            );
        }
    }

    /// The sweep is the only query here that is not by primary key, so it is
    /// the only one that needs an index. Without it, a table of abandoned
    /// uploads makes every sweep a full scan.
    #[test]
    fn every_dialect_indexes_what_the_sweep_reads() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(
                sql.contains("idx_uploads_expires"),
                "{dialect} has no index for the sweep"
            );
        }
    }

    /// Both tables in every dialect, or one backend quietly has no resumable
    /// uploads.
    #[test]
    fn every_dialect_creates_both_tables() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            for table in ["uploads", "upload_parts"] {
                assert!(
                    sql.contains(&format!("CREATE TABLE {table}")),
                    "{dialect} does not create {table}"
                );
            }
        }
    }
}
