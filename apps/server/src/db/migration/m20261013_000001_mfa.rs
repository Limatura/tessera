//! A second sign-in step: authenticator apps, recovery codes, and the
//! half-finished sign-ins waiting on them.
//!
//! The SQL lives in `sql/`, and the SQLite file carries the reasoning.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20261013_000001_mfa.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20261013_000001_mfa.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20261013_000001_mfa.mysql.sql");

/// Rolling back turns the second step off for everyone who set it up. That is
/// what removing the feature means; the passwords still work.
const DOWN: &str = "DROP TABLE IF EXISTS mfa_challenges;\n\
                    DROP TABLE IF EXISTS recovery_codes;\n\
                    DROP TABLE IF EXISTS totp_authenticators;\n";

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
