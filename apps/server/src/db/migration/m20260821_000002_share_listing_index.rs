//! Indexes for the queries that draw a list of shares.
//!
//! The SQL lives in `sql/`, one file per dialect — see the SQLite one for why
//! there are two indexes and why the drop comes last.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260821_000002_share_listing_index.sqlite.sql");
pub(super) const POSTGRES: &str =
    include_str!("sql/m20260821_000002_share_listing_index.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260821_000002_share_listing_index.mysql.sql");

/// Puts back what was there before. An index holds no data, so this one really
/// does undo itself.
const DOWN: &str = "DROP INDEX IF EXISTS idx_shares_user_created;\n\
                    DROP INDEX IF EXISTS idx_shares_created;\n\
                    CREATE INDEX IF NOT EXISTS idx_shares_user ON shares (user_id);\n";

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

    /// The whole point. An index added on two backends out of three is the
    /// failure mode hand-written per-dialect SQL exists to be checked for.
    #[test]
    fn every_dialect_gains_both_indexes() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            for index in ["idx_shares_user_created", "idx_shares_created"] {
                assert!(sql.contains(index), "{dialect} does not create {index}");
            }
        }
    }

    /// `shares.user_id` is a foreign key and `InnoDB` refuses to drop the only
    /// index covering one. The composite has to exist before the old index
    /// goes, or the migration fails on MySQL alone.
    #[test]
    fn the_replacement_is_created_before_the_old_index_is_dropped() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            let created = sql
                .find("CREATE INDEX idx_shares_user_created")
                .or_else(|| sql.find("EXISTS idx_shares_user_created"))
                .unwrap_or_else(|| panic!("{dialect} does not create the composite"));
            let dropped = sql
                .rfind("DROP INDEX")
                .unwrap_or_else(|| panic!("{dialect} does not drop the old index"));
            assert!(
                created < dropped,
                "{dialect} drops `idx_shares_user` before its replacement exists"
            );
        }
    }
}
