//! Access events: a row per retrieval, off unless the operator turns it on.
//!
//! The SQL lives in `sql/`, one file per dialect. See the SQLite one for why
//! this is a table and not columns, and why it defaults to writing nothing.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260822_000001_access_events.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260822_000001_access_events.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260822_000001_access_events.mysql.sql");

/// Dropping the table takes the events with it, and that is the right
/// direction: what is here is a record of people's visits, so a downgrade that
/// left it behind would keep the one thing this feature exists to let an
/// operator decide about.
const DOWN: &str = "DROP TABLE IF EXISTS access_events;\n";

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

    /// A share's events must not outlive the share. `InnoDB` silently ignores a
    /// `REFERENCES` written on a column and honours only a table-level
    /// `FOREIGN KEY`, so the column form would leave MySQL with no cascade, and
    /// this is the table where that means keeping a record of a visit to
    /// something that no longer exists.
    #[test]
    fn mysql_declares_the_cascade_where_innodb_reads_it() {
        assert_eq!(MYSQL.matches("FOREIGN KEY").count(), 1);
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

    /// One row per download is the fastest-growing table here, and both of its
    /// readers miss the primary key: the sweep by age, the listing by share.
    #[test]
    fn every_dialect_indexes_both_readers() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            for index in ["idx_access_events_occurred", "idx_access_events_share"] {
                assert!(sql.contains(index), "{dialect} is missing {index}");
            }
        }
    }
}
