//! Links that prove an account's address, for addresses registration did not.
//!
//! The SQL lives in `sql/`, and the SQLite file carries the reasoning.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260926_000001_email_verifications.sqlite.sql");
pub(super) const POSTGRES: &str =
    include_str!("sql/m20260926_000001_email_verifications.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260926_000001_email_verifications.mysql.sql");

/// Nothing depends on the rows: an unspent link lost to a rollback is asked for
/// again.
const DOWN: &str = "DROP TABLE IF EXISTS email_verifications;\n";

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
        super::execute_for_backend(manager, DOWN, DOWN, DOWN).await
    }
}
