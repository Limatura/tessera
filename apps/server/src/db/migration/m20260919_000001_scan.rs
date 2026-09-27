//! What a malware scanner reported about each file and note.
//!
//! The SQL lives in `sql/`, and the SQLite file carries the reasoning.

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260919_000001_scan.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260919_000001_scan.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260919_000001_scan.mysql.sql");

/// The index goes first: SQLite will not drop a column an index references.
const DOWN: &str = "DROP INDEX file_shares_scan_state;\n\
                    ALTER TABLE text_shares DROP COLUMN scan_detail;\n\
                    ALTER TABLE text_shares DROP COLUMN scan_state;\n\
                    ALTER TABLE file_shares DROP COLUMN scan_detail;\n\
                    ALTER TABLE file_shares DROP COLUMN scan_state;\n";

/// The same, with the `ON <table>` MySQL's `DROP INDEX` requires.
const DOWN_MYSQL: &str = "DROP INDEX file_shares_scan_state ON file_shares;\n\
                          ALTER TABLE text_shares DROP COLUMN scan_detail;\n\
                          ALTER TABLE text_shares DROP COLUMN scan_state;\n\
                          ALTER TABLE file_shares DROP COLUMN scan_detail;\n\
                          ALTER TABLE file_shares DROP COLUMN scan_state;\n";

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
