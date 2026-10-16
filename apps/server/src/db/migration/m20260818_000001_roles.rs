//! Replaces the `is_admin` boolean with roles and permissions.
//!
//! The SQL lives in `sql/`, one file per dialect. This module only picks which
//! one to run. See the module docs on [`super`] for why the DDL is
//! hand-written instead of built with `sea-query`.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260818_000001_roles.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260818_000001_roles.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260818_000001_roles.mysql.sql");

/// `users.is_admin` is untouched by `up`, so going back loses nothing: the
/// boolean still says what it always said.
const DOWN: &str = "DROP TABLE IF EXISTS user_roles;\n\
                    DROP TABLE IF EXISTS role_permissions;\n\
                    DROP TABLE IF EXISTS roles;\n";

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
    use tessera_core::{Permission, RoleId};

    use super::{MYSQL, POSTGRES, SQLITE};

    /// The program names the two seeded roles by literal, and so does the SQL.
    /// Nothing else ties the two spellings together, and a mismatch shows up as
    /// "every new account is in no role", which points nowhere near either
    /// file.
    #[test]
    fn every_dialect_seeds_the_identifiers_the_program_names() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            for id in [RoleId::ADMIN, RoleId::MEMBER] {
                assert!(sql.contains(id), "{dialect} does not seed {id}");
            }
        }
    }

    /// The permissions that existed when this migration was written.
    ///
    /// Named, not taken from [`Permission::ALL`]. A migration records what it
    /// did, and a later version adding a permission does not change
    /// that. Granting the new ones is the later migration's job; see
    /// `m20260818_000003`. The invariant that matters after every migration has
    /// run is checked against a real database, in `tests/admin.rs`.
    const SEEDED: [Permission; 13] = [
        Permission::SharesCreate,
        Permission::SharesRead,
        Permission::SharesDelete,
        Permission::TokensManage,
        Permission::UsersRead,
        Permission::UsersWrite,
        Permission::UsersDelete,
        Permission::InvitesRead,
        Permission::InvitesCreate,
        Permission::InvitesDelete,
        Permission::RolesRead,
        Permission::RolesWrite,
        Permission::StatsRead,
    ];

    /// A permission the administrator role is not granted cannot be held on a
    /// fresh instance, and the endpoint protecting it is unreachable.
    #[test]
    fn the_administrator_role_is_seeded_with_the_permissions_of_its_day() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            for permission in SEEDED {
                assert!(
                    sql.contains(&format!("'{}'", permission.as_str())),
                    "{dialect} does not grant {permission} to anybody"
                );
            }
        }
    }
}
