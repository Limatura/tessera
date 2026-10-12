//! Where a sign-in at a provider sends the browser once it succeeds.
//!
//! The SQL lives in `sql/`, and the SQLite file carries the reasoning.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20261006_000001_auth_flow_return.sqlite.sql");
pub(super) const POSTGRES: &str =
    include_str!("sql/m20261006_000001_auth_flow_return.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20261006_000001_auth_flow_return.mysql.sql");

/// Only sign-ins in progress lose their return path.
const DOWN: &str = "ALTER TABLE auth_flows DROP COLUMN return_to;\n";

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
