//! Removes the content columns `shares` no longer owns.
//!
//! The SQL lives in `sql/`, one file per dialect. This module only picks which
//! one to run — see the module docs on [`super`] for why the DDL is hand-written
//! rather than built with `sea-query`.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str =
    include_str!("sql/m20260818_000008_drop_legacy_share_columns.sqlite.sql");
pub(super) const POSTGRES: &str =
    include_str!("sql/m20260818_000008_drop_legacy_share_columns.postgres.sql");
pub(super) const MYSQL: &str =
    include_str!("sql/m20260818_000008_drop_legacy_share_columns.mysql.sql");

/// There is no going back from this one.
///
/// The columns can be recreated empty, but what was in them cannot: the values
/// live on `file_shares` and `text_shares` now, and putting them back would mean
/// copying content details into a shape that cannot hold a share with several
/// files. A downgrade past this point is a restore from backup.
const DOWN: &str = "SELECT 1;\n";

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
