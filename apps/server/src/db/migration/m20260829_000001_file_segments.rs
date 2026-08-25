//! Where a file's bytes are, as a list rather than a column.
//!
//! The SQL lives in `sql/` — see the SQLite one for why this is a list when
//! every row has exactly one entry, and why the backend is a name rather than a
//! foreign key.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260829_000001_file_segments.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260829_000001_file_segments.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260829_000001_file_segments.mysql.sql");

/// `file_shares.storage` and `object_key` are still there and still written, so
/// rolling back loses nothing: the segments were derived from them.
///
/// One spelling for every dialect. Dropping a table takes its own foreign keys
/// and indexes with it everywhere, which is why this needs none of the
/// MySQL-only handling the quota rollback did.
const DOWN: &str = "DROP TABLE IF EXISTS file_segments;\n";

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

    /// The backfill is what makes this migration safe to run on a live
    /// instance: every file that has bytes must come out with a segment naming
    /// them, or the share stops being deliverable.
    #[test]
    fn every_dialect_backfills_from_the_existing_column() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(sql.contains("INSERT INTO file_segments"), "{dialect}");
            assert!(sql.contains("FROM file_shares"), "{dialect}");
            // A share whose bytes were swept has no key, so it gets no segment.
            // Copying it anyway would produce a segment pointing at nothing.
            assert!(sql.contains("object_key IS NOT NULL"), "{dialect}");
        }
    }

    /// One segment per position, enforced rather than assumed — a second row at
    /// the same `seq` would make "read the file in order" ambiguous.
    #[test]
    fn the_position_is_the_key() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(sql.contains("PRIMARY KEY (file_id, seq)"), "{dialect}");
        }
    }

    /// `InnoDB` silently ignores a `REFERENCES` written on a column, and `TEXT`
    /// cannot take part in a key without a prefix length.
    #[test]
    fn mysql_declares_the_key_where_innodb_reads_it() {
        assert_eq!(MYSQL.matches("FOREIGN KEY").count(), 1);
        // The declaration spans two lines, so the check is that every
        // `REFERENCES` belongs to a table-level `CONSTRAINT` rather than
        // sitting on a column.
        let statements: Vec<&str> = MYSQL
            .lines()
            .map(str::trim_start)
            .filter(|line| !line.starts_with("--"))
            .collect();
        for (index, statement) in statements.iter().enumerate() {
            if !statement.contains("REFERENCES") {
                continue;
            }
            let follows_constraint = index
                .checked_sub(1)
                .is_some_and(|previous| statements[previous].contains("CONSTRAINT"));
            assert!(
                follows_constraint,
                "`InnoDB` ignores a column-level key: {statement}"
            );
        }
        // Only what the key needs is length-limited; a key can be long.
        assert!(MYSQL.contains("file_id       VARCHAR(191)"));
        assert!(MYSQL.contains("object_key    TEXT"));
    }
}
