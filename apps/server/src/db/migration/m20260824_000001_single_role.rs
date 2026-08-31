//! One role per account.
//!
//! The SQL lives in `sql/`, one file per dialect. See the SQLite one for why
//! the join table goes and which role is kept when an account had several.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260824_000001_single_role.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260824_000001_single_role.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260824_000001_single_role.mysql.sql");

/// Rebuilds the join table with one row per account.
///
/// Lossless for anything this migration produced, and lossy only for an
/// instance that genuinely had an account in two roles before it ran. That
/// second role is gone and nothing here remembers it.
///
/// # Why the index goes first
///
/// SQLite refuses `ALTER TABLE ... DROP COLUMN` while an index names the
/// column, and says `error in index idx_users_role after drop column`. Postgres
/// drops the index with the column and does not care that this ran first.
///
/// It was missing because the test that would have caught it only ever ran
/// against Postgres and MySQL, and SQLite is the default this ships with. See
/// `a_rollback_runs_on_sqlite`.
const DOWN: &str = "DROP INDEX IF EXISTS idx_users_role;\n\
                    CREATE TABLE IF NOT EXISTS user_roles (\n\
                    \x20   user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,\n\
                    \x20   role_id    TEXT NOT NULL REFERENCES roles (id) ON DELETE CASCADE,\n\
                    \x20   granted_at TEXT NOT NULL,\n\
                    \x20   PRIMARY KEY (user_id, role_id)\n\
                    );\n\
                    INSERT INTO user_roles (user_id, role_id, granted_at)\n\
                    SELECT id, role_id, '1970-01-01T00:00:00Z' FROM users WHERE role_id IS NOT NULL;\n\
                    ALTER TABLE users DROP COLUMN role_id;\n";

/// The same, restoring the table the way MySQL had it.
///
/// Three differences, all things MySQL refuses instead of ignoring. `TEXT`
/// cannot be part of a key without a prefix length, so the identifiers come
/// back as `CHAR(26)`, the width `m20260818_000001_roles` gave them. `InnoDB`
/// silently drops a `REFERENCES` written on a column, so the keys are declared
/// at table level. And `users.role_id` is named by a foreign key, which has to
/// go before the column can.
///
/// Checked against a real MySQL, not reasoned about. See
/// `a_full_rollback_runs_on_this_dialect`.
const DOWN_MYSQL: &str = "CREATE TABLE IF NOT EXISTS user_roles (\n\
                          \x20   user_id    CHAR(26)    NOT NULL,\n\
                          \x20   role_id    CHAR(26)    NOT NULL,\n\
                          \x20   granted_at DATETIME(6) NOT NULL,\n\
                          \x20   PRIMARY KEY (user_id, role_id),\n\
                          \x20   CONSTRAINT fk_user_roles_user FOREIGN KEY (user_id)\n\
                          \x20       REFERENCES users (id) ON DELETE CASCADE,\n\
                          \x20   CONSTRAINT fk_user_roles_role FOREIGN KEY (role_id)\n\
                          \x20       REFERENCES roles (id) ON DELETE CASCADE\n\
                          );\n\
                          INSERT INTO user_roles (user_id, role_id, granted_at)\n\
                          SELECT id, role_id, '1970-01-01 00:00:00' FROM users WHERE role_id IS NOT NULL;\n\
                          ALTER TABLE users DROP FOREIGN KEY fk_users_role;\n\
                          ALTER TABLE users DROP COLUMN role_id;\n";

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
        super::execute_for_backend(manager, DOWN, DOWN, DOWN_MYSQL).await
    }
}

#[cfg(test)]
mod tests {
    use super::{MYSQL, POSTGRES, SQLITE};

    /// The API took a list, so an account with two roles is possible and the
    /// choice has to be deterministic, between dialects and between two runs of
    /// the same one.
    #[test]
    fn every_dialect_breaks_ties_the_same_way() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(
                sql.contains("ORDER BY COUNT(rp.permission) DESC, ur.role_id ASC"),
                "{dialect} does not pick deterministically"
            );
        }
    }

    /// Removing a role must take the assignment, not the account, as the join
    /// table's `ON DELETE CASCADE` on `role_id` did.
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
