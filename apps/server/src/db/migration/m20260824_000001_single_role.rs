//! One role per account.
//!
//! The SQL lives in `sql/`, one file per dialect — see the SQLite one for why
//! the join table goes and which role is kept when an account had several.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260824_000001_single_role.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260824_000001_single_role.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260824_000001_single_role.mysql.sql");

/// Rebuilds the join table with one row per account.
///
/// Lossless for anything this migration produced, and lossy only for an
/// instance that genuinely had somebody in two roles before it ran — that
/// second role is gone, and nothing here remembers it.
const DOWN: &str = "CREATE TABLE IF NOT EXISTS user_roles (\n\
                    \x20   user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,\n\
                    \x20   role_id    TEXT NOT NULL REFERENCES roles (id) ON DELETE CASCADE,\n\
                    \x20   granted_at TEXT NOT NULL,\n\
                    \x20   PRIMARY KEY (user_id, role_id)\n\
                    );\n\
                    INSERT INTO user_roles (user_id, role_id, granted_at)\n\
                    SELECT id, role_id, '1970-01-01T00:00:00Z' FROM users WHERE role_id IS NOT NULL;\n\
                    ALTER TABLE users DROP COLUMN role_id;\n";

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

    /// The API took a list, so an account with two roles is possible and the
    /// choice has to be deterministic — including between dialects, and between
    /// two runs of the same one.
    #[test]
    fn every_dialect_breaks_ties_the_same_way() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(
                sql.contains("ORDER BY COUNT(rp.permission) DESC, ur.role_id ASC"),
                "{dialect} does not pick deterministically"
            );
        }
    }

    /// Removing a role must take the assignment, not the account — the same
    /// thing the join table's `ON DELETE CASCADE` on `role_id` did.
    #[test]
    fn removing_a_role_leaves_the_account() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(
                sql.contains("ON DELETE SET NULL"),
                "{dialect} would take the account with the role"
            );
        }
    }

    #[test]
    fn every_dialect_drops_the_join_table() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(sql.contains("DROP TABLE user_roles"), "{dialect}");
        }
    }
}
