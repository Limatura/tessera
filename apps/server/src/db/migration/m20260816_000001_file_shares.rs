//! Gives a share somewhere to keep more than one file.
//!
//! The SQL lives in `sql/`, one file per dialect. This module only picks which
//! one to run — see the module docs on [`super`] for why the DDL is hand-written
//! rather than built with `sea-query`.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260816_000001_file_shares.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260816_000001_file_shares.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260816_000001_file_shares.mysql.sql");

/// The old columns are still on `shares` and still written, so going back loses
/// nothing. That is deliberate for this landing: the columns are removed only
/// once the code that reads them is gone.
const DOWN: &str = "DROP TABLE IF EXISTS file_shares;\n";

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
