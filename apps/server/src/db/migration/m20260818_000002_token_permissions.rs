//! Gives an API token a set of permissions of its own.
//!
//! The SQL lives in `sql/`, one file per dialect. This module only picks which
//! one to run. See the module docs on [`super`] for why the DDL is
//! hand-written instead of built with `sea-query`.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260818_000002_token_permissions.sqlite.sql");
pub(super) const POSTGRES: &str =
    include_str!("sql/m20260818_000002_token_permissions.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260818_000002_token_permissions.mysql.sql");

/// Dropping the table puts every token back to acting as its owner. That is
/// what it did before this ran.
const DOWN: &str = "DROP TABLE IF EXISTS api_token_permissions;\n";

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
    use tessera_core::Permission;

    use super::{MYSQL, POSTGRES, SQLITE};

    /// What a token could do before this migration ran.
    ///
    /// Not [`Permission::ALL`]. A token minted before this version acted as its
    /// owner *as the program was then*, and permissions added
    /// later were not among them. Carrying a later one over would widen every
    /// existing token on upgrade, the opposite of what this is for.
    const CARRIED_OVER: [Permission; 13] = [
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

    /// An existing token that is not given a permission loses it silently on
    /// upgrade, and the symptom lands on whoever's script is using it.
    #[test]
    fn existing_tokens_keep_what_they_could_already_do() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            for permission in CARRIED_OVER {
                assert!(
                    sql.contains(&format!("'{}'", permission.as_str())),
                    "{dialect} does not carry {permission} over"
                );
            }
        }
    }
}
