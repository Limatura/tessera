//! Webhooks, the events waiting to reach them, and when a share's end was
//! announced.
//!
//! The SQL lives in `sql/`, and the SQLite file carries the reasoning.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20261023_000002_webhooks.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20261023_000002_webhooks.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20261023_000002_webhooks.mysql.sql");

/// The index goes before the column: SQLite will not drop an indexed column.
const DOWN: &str = "DROP TABLE IF EXISTS webhook_deliveries;
DROP TABLE IF EXISTS webhooks;
DROP INDEX IF EXISTS idx_shares_end_announced;
ALTER TABLE shares DROP COLUMN end_announced_at;
";

/// MySQL names the table an index is dropped from, and has no `IF EXISTS` there.
const DOWN_MYSQL: &str = "DROP TABLE IF EXISTS webhook_deliveries;
DROP TABLE IF EXISTS webhooks;
DROP INDEX idx_shares_end_announced ON shares;
ALTER TABLE shares DROP COLUMN end_announced_at;
";

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
        super::execute_for_backend(manager, DOWN, DOWN, DOWN_MYSQL).await
    }
}
