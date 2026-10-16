//! Redirecting browsers straight to the store: the switch that allows it, and
//! the count of bytes sent there.
//!
//! The SQL lives in `sql/`, and the SQLite file carries the reasoning.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260914_000001_direct_access.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260914_000001_direct_access.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260914_000001_direct_access.mysql.sql");

/// No index references either column, so SQLite can drop both directly.
const DOWN: &str = "ALTER TABLE shares DROP COLUMN bytes_redirected;\n\
                    ALTER TABLE storage_backends DROP COLUMN direct_access;\n";

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
