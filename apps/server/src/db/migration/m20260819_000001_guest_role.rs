//! Seeds `guest`, a role that is allowed nothing.
//!
//! The SQL lives in `sql/`, one file per dialect. This module only picks which
//! one to run — see the module docs on [`super`] for why the DDL is hand-written
//! rather than built with `sea-query`.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260819_000001_guest_role.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260819_000001_guest_role.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260819_000001_guest_role.mysql.sql");

/// Removing the role takes its holders with it, through
/// `user_roles.role_id ON DELETE CASCADE`. Those accounts end up in no role,
/// which is where they would have been without this migration — the state
/// `guest` exists to give a name to, so going back loses the name and nothing
/// else.
///
/// An instance whose `auth.default_role` names `guest` will refuse to start
/// after this runs. That is the intended behaviour for a role that is not
/// there: the alternative is registering people into a role that does not
/// exist.
const DOWN: &str = "DELETE FROM roles WHERE id = '00GEST00000000000000000000';\n";

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
    use tessera_core::RoleId;

    use super::{MYSQL, POSTGRES, SQLITE};

    /// The program names the role by literal and so does the SQL. Nothing else
    /// ties the two spellings together, and a mismatch shows up as
    /// "`auth.default_role: guest` will not start", which points at the
    /// configuration rather than at either file.
    #[test]
    fn every_dialect_seeds_the_identifier_the_program_names() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(sql.contains(RoleId::GUEST), "{dialect} does not seed guest");
        }
    }

    /// Granting it anything would defeat the only thing it is for. Checked
    /// against the SQL rather than the database because it is a property of
    /// what this migration writes, not of what somebody later chose.
    ///
    /// Comments are stripped first: the prose above the statements says why
    /// there are no `role_permissions` rows, and searching the whole file for
    /// the table name finds that sentence.
    #[test]
    fn no_dialect_grants_it_a_permission() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            let statements: String = sql
                .lines()
                .filter(|line| !line.trim_start().starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                !statements.contains("role_permissions"),
                "{dialect} grants guest something"
            );
        }
    }
}
