//! Gives an API token a set of permissions of its own.
//!
//! The SQL lives in `sql/`, one file per dialect. This module only picks which
//! one to run — see the module docs on [`super`] for why the DDL is hand-written
//! rather than built with `sea-query`.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260818_000002_token_permissions.sqlite.sql");
pub(super) const POSTGRES: &str =
    include_str!("sql/m20260818_000002_token_permissions.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260818_000002_token_permissions.mysql.sql");

/// Dropping the table puts every token back to acting as its owner, which is
/// what it did before this ran.
const DOWN: &str = "DROP TABLE IF EXISTS api_token_permissions;\n";

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
    use tessera_core::Permission;

    use super::{MYSQL, POSTGRES, SQLITE};

    /// An existing token that is not given a permission loses it, silently, on
    /// upgrade — and the symptom lands on whoever's script is using it.
    #[test]
    fn existing_tokens_keep_every_permission() {
        for (dialect, sql) in [("sqlite", SQLITE), ("postgres", POSTGRES), ("mysql", MYSQL)] {
            for permission in Permission::ALL {
                assert!(
                    sql.contains(&format!("'{}'", permission.as_str())),
                    "{dialect} does not carry {permission} over"
                );
            }
        }
    }
}
