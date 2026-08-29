//! Grants the two settings permissions to the roles that run the instance.
//!
//! The SQL lives in `sql/`, one file per dialect. See the SQLite one for why
//! `owner` gets write and `admin` only gets read.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str =
    include_str!("sql/m20260823_000002_settings_permissions.sqlite.sql");
pub(super) const POSTGRES: &str =
    include_str!("sql/m20260823_000002_settings_permissions.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260823_000002_settings_permissions.mysql.sql");

/// Takes them back off the seeded roles. An operator who granted them to a role
/// of their own keeps them, because this only names the two it added them to.
const DOWN: &str = "DELETE FROM role_permissions\n\
                     WHERE permission IN ('settings:read', 'settings:write')\n\
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

    /// Re-running a migration has to be a no-op. Each dialect spells "leave the
    /// row alone" differently, and getting it wrong turns an interrupted
    /// migration into a duplicate-key failure that blocks every later one.
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

    /// `admin` reads and does not write. Granting write here would make the
    /// "help run the instance" role able to reconfigure it.
    #[test]
    fn admin_is_granted_reading_only() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            for line in sql.lines().filter(|line| line.contains("00ADMN")) {
                assert!(
                    line.contains("settings:read"),
                    "{dialect} grants admin something other than reading: {line}"
                );
            }
        }
    }
}
