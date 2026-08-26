//! Where an upload in flight is putting its bytes, as a list rather than three
//! columns.
//!
//! The SQL lives in `sql/` — see the SQLite one for what the three columns could
//! not say, why the state is named rather than inferred, and why `planned` is
//! the reservation rather than bookkeeping.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260903_000001_upload_segments.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260903_000001_upload_segments.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260903_000001_upload_segments.mysql.sql");

/// Rollbacks live in `sql/down/`, not beside the migrations. Everything in
/// `sql/` is replayed, in order, by `scripts/schema-docs.py` to build the schema
/// it documents — a rollback sitting among them is applied straight after the
/// migration it undoes, and the generated documentation describes a database
/// nobody runs. `no_rollback_sits_where_the_schema_docs_replay_from` is what
/// keeps that from happening again.
const DOWN_SQLITE: &str = include_str!("sql/down/m20260903_000001_upload_segments.sqlite.sql");
/// Postgres takes the SQLite spelling unchanged: `ADD COLUMN`, a correlated
/// subquery and a `DROP TABLE` are the same statements in both.
const DOWN_POSTGRES: &str = include_str!("sql/down/m20260903_000001_upload_segments.postgres.sql");
/// MySQL cannot read the table it is updating from a correlated subquery, so the
/// restore is a join. Written out rather than shared for the same reason the
/// quota rollback was: a `down` that has never run on the dialect it claims to
/// support is not a rollback, it is a note.
const DOWN_MYSQL: &str = include_str!("sql/down/m20260903_000001_upload_segments.mysql.sql");

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
        super::execute_for_backend(manager, DOWN_SQLITE, DOWN_POSTGRES, DOWN_MYSQL).await
    }
}

#[cfg(test)]
mod tests {
    use super::{DOWN_MYSQL, DOWN_POSTGRES, DOWN_SQLITE, MYSQL, POSTGRES, SQLITE};

    const DIALECTS: [(&str, &str); 3] =
        [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)];

    /// A session in flight when this runs has to come out with a segment naming
    /// where its bytes were already going. Without the backfill every resumable
    /// upload on the instance would forget its destination mid-transfer.
    #[test]
    fn every_dialect_backfills_the_sessions_in_flight() {
        for (dialect, sql) in DIALECTS {
            assert!(sql.contains("INSERT INTO upload_segments"), "{dialect}");
            assert!(sql.contains("FROM uploads"), "{dialect}");
            // Sessions opened before backends were recorded meant the one from
            // `storage.url`, which is the name the configuration seeds.
            assert!(sql.contains("COALESCE(u.backend, 'default')"), "{dialect}");
        }
    }

    /// Every session in flight backfills as `open`, not as unstarted. A local
    /// filesystem never has a multipart id, so keying the state off that column
    /// would file every local session as unopened and the sweep would leave
    /// their bytes behind.
    #[test]
    fn every_session_in_flight_backfills_as_open() {
        for (dialect, sql) in DIALECTS {
            assert!(sql.contains("u.multipart_id, 'open',"), "{dialect}");
            assert!(!sql.contains("'planned'"), "{dialect} guesses at a state");
        }
    }

    /// The columns the list replaces go, in the same migration that fills it.
    /// Leaving them would mean two records of the same fact and one of them
    /// going stale the first time a session has two segments.
    #[test]
    fn the_columns_the_list_replaces_are_dropped() {
        for (dialect, sql) in DIALECTS {
            for column in ["multipart_id", "object_key", "backend"] {
                assert!(
                    sql.contains(&format!("ALTER TABLE uploads DROP COLUMN {column};")),
                    "{dialect} keeps {column}"
                );
            }
        }
    }

    /// Every rollback restores all three, and takes the table with it. Checked
    /// per dialect because the three are not the same statement: MySQL cannot
    /// read the table it is updating from a subquery.
    #[test]
    fn every_rollback_restores_the_columns_and_drops_the_table() {
        for (dialect, sql) in [
            ("sqlite", DOWN_SQLITE),
            ("postgres", DOWN_POSTGRES),
            ("mysql", DOWN_MYSQL),
        ] {
            for column in ["multipart_id", "object_key", "backend"] {
                assert!(
                    sql.contains(&format!("ADD COLUMN {column}")),
                    "{dialect} does not restore {column}"
                );
            }
            assert!(sql.contains("FROM upload_segments") || sql.contains("JOIN upload_segments"));
            assert!(
                sql.contains("DROP TABLE IF EXISTS upload_segments;"),
                "{dialect}"
            );
        }
    }

    /// `InnoDB` silently ignores a `REFERENCES` written on a column, and cannot
    /// index `TEXT` without a prefix length.
    #[test]
    fn mysql_declares_the_key_where_innodb_reads_it() {
        assert_eq!(MYSQL.matches("FOREIGN KEY").count(), 1);
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
        assert!(MYSQL.contains("upload_id     VARCHAR(191)"));
        assert!(MYSQL.contains("state         VARCHAR(32)"));
        // Not in any key, and a store prefix an operator chooses could be long.
        assert!(MYSQL.contains("object_key    TEXT"));
    }

    /// `CREATE INDEX IF NOT EXISTS` is not MySQL syntax — the same trap the
    /// quota rollback hit with `DROP INDEX IF EXISTS`, in the other direction.
    #[test]
    fn mysql_creates_its_index_without_the_guard() {
        assert!(MYSQL.contains("CREATE INDEX idx_upload_segments_state"));
        assert!(!MYSQL.contains("CREATE INDEX IF NOT EXISTS"));
    }
}
