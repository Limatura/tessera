//! One row per person for the choices that are theirs rather than the
//! instance's.
//!
//! The SQL lives in `sql/`, and the SQLite one carries the reasoning: why the
//! identifier is the primary key, why the choices are one JSON column, and why
//! the table is not named after the only column it starts with.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260908_000001_user_preferences.sqlite.sql");
pub(super) const POSTGRES: &str =
    include_str!("sql/m20260908_000001_user_preferences.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260908_000001_user_preferences.mysql.sql");

const DOWN_SQLITE: &str = include_str!("sql/down/m20260908_000001_user_preferences.sqlite.sql");
const DOWN_POSTGRES: &str = include_str!("sql/down/m20260908_000001_user_preferences.postgres.sql");
const DOWN_MYSQL: &str = include_str!("sql/down/m20260908_000001_user_preferences.mysql.sql");

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
