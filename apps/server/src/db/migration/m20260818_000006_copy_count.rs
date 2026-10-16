//! Adds the copy counter beside the view one.
//!
//! The SQL lives in `sql/`, one file per dialect. This module only picks which
//! one to run. See the module docs on [`super`] for why the DDL is
//! hand-written instead of built with `sea-query`.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260818_000006_copy_count.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260818_000006_copy_count.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260818_000006_copy_count.mysql.sql");

/// Rolling back loses the tally, a number nothing relied on before this version
/// existed.
const DOWN: &str = "ALTER TABLE shares DROP COLUMN copy_count;\n";

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
