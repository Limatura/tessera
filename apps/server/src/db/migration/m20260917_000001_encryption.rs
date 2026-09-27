//! End-to-end encrypted shares: how each one was sealed, and the descriptions
//! that arrive sealed instead of in the clear.
//!
//! The SQL lives in `sql/`, and the SQLite file carries the reasoning.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260917_000001_encryption.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260917_000001_encryption.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260917_000001_encryption.mysql.sql");

/// No index references these columns, so SQLite can drop them directly.
const DOWN: &str = "ALTER TABLE uploads DROP COLUMN sealed_name;\n\
                    ALTER TABLE uploads DROP COLUMN key_salt;\n\
                    ALTER TABLE uploads DROP COLUMN encryption;\n\
                    ALTER TABLE file_shares DROP COLUMN sealed_name;\n\
                    ALTER TABLE shares DROP COLUMN sealed_details;\n\
                    ALTER TABLE shares DROP COLUMN key_salt;\n\
                    ALTER TABLE shares DROP COLUMN encryption;\n";

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
