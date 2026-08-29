//! Which segment a part belongs to.
//!
//! The SQL lives in `sql/`. See the SQLite one for the collision this prevents
//! and why the table is rebuilt there but altered in place elsewhere.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260904_000001_parts_per_segment.sqlite.sql");
pub(super) const POSTGRES: &str =
    include_str!("sql/m20260904_000001_parts_per_segment.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260904_000001_parts_per_segment.mysql.sql");

const DOWN_SQLITE: &str = include_str!("sql/down/m20260904_000001_parts_per_segment.sqlite.sql");
const DOWN_POSTGRES: &str =
    include_str!("sql/down/m20260904_000001_parts_per_segment.postgres.sql");
const DOWN_MYSQL: &str = include_str!("sql/down/m20260904_000001_parts_per_segment.mysql.sql");

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
        super::execute_for_backend(manager, DOWN_SQLITE, DOWN_POSTGRES, DOWN_MYSQL).await
    }
}

#[cfg(test)]
mod tests {
    use super::{DOWN_MYSQL, MYSQL, POSTGRES, SQLITE};

    /// What this migration is for: the segment is in the key.
    #[test]
    fn every_dialect_puts_the_segment_in_the_key() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(
                sql.contains("(upload_id, seq, part_number)"),
                "{dialect} does not key on the segment"
            );
        }
    }

    /// Existing rows are the first segment, not a new one. A default of anything
    /// else would file every part already written under a segment that does not
    /// exist.
    #[test]
    fn existing_parts_become_the_first_segment() {
        assert!(SQLITE.contains("SELECT upload_id, 0, part_number, content_id"));
        for sql in [POSTGRES, MYSQL] {
            assert!(sql.contains("seq BIGINT NOT NULL DEFAULT 0"));
        }
    }

    /// `InnoDB` will not drop a primary key that is the only index a foreign key
    /// has, so the swap cannot be two statements there.
    #[test]
    fn mysql_swaps_the_key_in_one_statement() {
        for sql in [MYSQL, DOWN_MYSQL] {
            assert!(
                sql.contains("DROP PRIMARY KEY, ADD PRIMARY KEY"),
                "the key swap has to be atomic on MySQL"
            );
        }
    }
}
