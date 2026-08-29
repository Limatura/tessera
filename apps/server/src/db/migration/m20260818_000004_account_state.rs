//! Gives an account a lifecycle: active, disabled, or deleted.
//!
//! The SQL lives in `sql/`, one file per dialect. This module only picks which
//! one to run. See the module docs on [`super`] for why the DDL is
//! hand-written instead of built with `sea-query`.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260818_000004_account_state.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260818_000004_account_state.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260818_000004_account_state.mysql.sql");

/// SQLite cannot drop a column that an index names, so the index goes first.
///
/// Rolling back leaves every account looking active, as the previous version
/// believed of all of them, disabled ones included. Worth saying out
/// loud: a downgrade re-admits suspended accounts.
const DOWN: &str = "DROP INDEX IF EXISTS idx_users_state;\n\
                    ALTER TABLE users DROP COLUMN deleted_by;\n\
                    ALTER TABLE users DROP COLUMN deleted_at;\n\
                    ALTER TABLE users DROP COLUMN state;\n";

/// The same, in the two ways MySQL differs. `DROP INDEX IF EXISTS` is not
/// syntax it has, and `deleted_by` is named by a foreign key that has to go
/// first, after which the index goes with its column on its own.
const DOWN_MYSQL: &str = "ALTER TABLE users DROP FOREIGN KEY fk_users_deleted_by;\n\
                          ALTER TABLE users DROP COLUMN deleted_by;\n\
                          ALTER TABLE users DROP COLUMN deleted_at;\n\
                          ALTER TABLE users DROP COLUMN state;\n";

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
