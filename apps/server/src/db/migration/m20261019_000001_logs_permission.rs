//! Grants `logs:read` to the roles that run the instance.
//!
//! The SQL lives in `sql/`, one file per dialect.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20261019_000001_logs_permission.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20261019_000001_logs_permission.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20261019_000001_logs_permission.mysql.sql");

/// Takes it back off the seeded roles. A role of the operator's own that was
/// given it keeps it.
const DOWN: &str = "DELETE FROM role_permissions\n\
                     WHERE permission = 'logs:read'\n\
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

    /// Re-running a migration has to be a no-op.
    #[test]
    fn every_dialect_tolerates_the_row_already_being_there() {
        for (dialect, sql, expected) in [
            ("sqlite", SQLITE, "INSERT OR IGNORE"),
            ("postgres", POSTGRES, "ON CONFLICT DO NOTHING"),
            ("mysql", MYSQL, "INSERT IGNORE"),
        ] {
            assert!(sql.contains(expected), "{dialect} is missing `{expected}`");
        }
    }
}
