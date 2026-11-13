//! The audit log: a row per thing done, and `audit:read` for who may read it.
//!
//! The SQL lives in `sql/`, one file per dialect. See the SQLite one for why
//! this is not `access_events` and why it has no foreign keys.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20261111_000001_audit_events.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20261111_000001_audit_events.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20261111_000001_audit_events.mysql.sql");

/// Drops the table and takes the permission back off the seeded roles. A role
/// of the operator's own that was given it keeps the row, which then grants
/// nothing this version checks.
const DOWN: &str = "DROP TABLE IF EXISTS audit_events;\n\
                    DELETE FROM role_permissions\n\
                     WHERE permission = 'audit:read'\n\
                       AND role_id IN ('00WNER00000000000000000000', \
                    '00ADMN00000000000000000000');\n";

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

    /// The listing filters by time, by actor, by target and by action. Each of
    /// those misses the primary key, and this table only grows.
    #[test]
    fn every_dialect_indexes_every_reader() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            for index in [
                "idx_audit_events_occurred",
                "idx_audit_events_actor",
                "idx_audit_events_target",
                "idx_audit_events_action",
            ] {
                assert!(sql.contains(index), "{dialect} is missing {index}");
            }
        }
    }

    /// An entry must outlive the account or share it names: that is what
    /// "who deleted that" needs.
    #[test]
    fn nothing_cascades_into_the_log() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(!sql.contains("REFERENCES"), "{dialect} has a foreign key");
        }
    }

    /// Re-running a migration has to be a no-op.
    #[test]
    fn every_dialect_tolerates_the_grant_already_being_there() {
        for (dialect, sql, expected) in [
            ("sqlite", SQLITE, "INSERT OR IGNORE"),
            ("postgres", POSTGRES, "ON CONFLICT DO NOTHING"),
            ("mysql", MYSQL, "INSERT IGNORE"),
        ] {
            assert!(sql.contains(expected), "{dialect} is missing `{expected}`");
        }
    }
}
