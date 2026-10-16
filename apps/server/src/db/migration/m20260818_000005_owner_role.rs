//! Splits `owner` out of `admin`.
//!
//! The SQL lives in `sql/`, one file per dialect. This module only picks which
//! one to run. See the module docs on [`super`] for why the DDL is
//! hand-written instead of built with `sea-query`.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260818_000005_owner_role.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260818_000005_owner_role.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260818_000005_owner_role.mysql.sql");

/// Puts the three permissions back on `admin` before removing `owner`, so a
/// rollback leaves whoever was administering the instance still able to.
const DOWN: &str = "INSERT INTO role_permissions (role_id, permission) \
                    VALUES ('00ADMN00000000000000000000', 'users:delete'), \
                           ('00ADMN00000000000000000000', 'shares:delete:any'), \
                           ('00ADMN00000000000000000000', 'roles:write');\n\
                    DELETE FROM roles WHERE id = '00WNER00000000000000000000';\n";

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

    /// The three that separate a helper from the person whose instance it is.
    const WITHHELD: [Permission; 3] = [
        Permission::UsersDelete,
        Permission::SharesDeleteAny,
        Permission::RolesWrite,
    ];

    #[test]
    fn every_dialect_seeds_the_owner_the_program_names() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            assert!(
                sql.contains(RoleId::OWNER),
                "{dialect} does not seed {}",
                RoleId::OWNER
            );
        }
    }

    /// Narrowing `admin` before moving its members into `owner` would take the
    /// instance away from whoever is running it, possibly including the only
    /// person who could put it back.
    #[test]
    fn the_members_move_before_the_permissions_are_taken() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            let moved = sql.find("INTO user_roles").expect("the membership copy");
            let narrowed = sql
                .find("DELETE FROM role_permissions")
                .expect("the narrowing");
            assert!(moved < narrowed, "{dialect} narrows admin too early");
        }
    }

    /// Whichever way round, the three have to be the three.
    #[test]
    fn exactly_the_three_are_withheld() {
        for permission in WITHHELD {
            assert!(
                SQLITE.contains(&format!("'{}'", permission.as_str())),
                "{permission} is not named"
            );
        }
    }
}
