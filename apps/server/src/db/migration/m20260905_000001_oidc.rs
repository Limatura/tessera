//! Signing in with an identity provider.
//!
//! The SQL lives in `sql/`, and the SQLite one carries the reasoning: why
//! identities are rows rather than columns, why two of `users`'s columns become
//! nullable, and why the in-flight sign-in state is a table rather than a
//! cookie.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260905_000001_oidc.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260905_000001_oidc.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260905_000001_oidc.mysql.sql");

const DOWN_SQLITE: &str = include_str!("sql/down/m20260905_000001_oidc.sqlite.sql");
const DOWN_POSTGRES: &str = include_str!("sql/down/m20260905_000001_oidc.postgres.sql");
const DOWN_MYSQL: &str = include_str!("sql/down/m20260905_000001_oidc.mysql.sql");

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
        super::execute_for_backend(manager, DOWN_SQLITE, DOWN_POSTGRES, DOWN_MYSQL).await
    }
}
