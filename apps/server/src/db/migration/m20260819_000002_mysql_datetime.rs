//! Makes every stored time the same thing on MySQL: a UTC wall clock.
//!
//! The SQL lives in `sql/`, one file per dialect — see the MySQL one for why.
//! SQLite and Postgres have nothing to do, and their files say so rather than
//! being absent, so that "no file" always means "somebody forgot".

use sea_orm_migration::prelude::*;

pub(super) const SQLITE: &str = include_str!("sql/m20260819_000002_mysql_datetime.sqlite.sql");
pub(super) const POSTGRES: &str = include_str!("sql/m20260819_000002_mysql_datetime.postgres.sql");
pub(super) const MYSQL: &str = include_str!("sql/m20260819_000002_mysql_datetime.mysql.sql");

/// Deliberately empty.
///
/// Going back means putting a 2038 ceiling on columns that may by then hold a
/// value above it, and MySQL refuses those rows rather than rounding them —
/// so a `down` that looked reversible would fail part way through, leaving
/// half the tables converted. Nothing is lost by staying: `DATETIME` holds
/// every instant `TIMESTAMP` could, read by the same code.
const DOWN: &str = "";

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

#[cfg(test)]
mod tests {
    use super::MYSQL;

    /// A column left behind keeps the 2038 ceiling and keeps reading a
    /// different number from the ones beside it, which is the whole thing this
    /// migration exists to end. The list is what `information_schema` reported
    /// after every earlier migration had run.
    const CONVERTED: [(&str, usize); 10] = [
        ("api_tokens", 4),
        ("file_shares", 1),
        ("invites", 2),
        ("password_reset_tokens", 3),
        ("pending_registrations", 2),
        ("schema_metadata", 1),
        ("sessions", 3),
        ("share_tickets", 2),
        ("shares", 5),
        ("users", 3),
    ];

    #[test]
    fn every_table_that_held_a_timestamp_is_converted() {
        for (table, _) in CONVERTED {
            assert!(
                MYSQL.contains(&format!("ALTER TABLE {table}\n")),
                "{table} is not converted"
            );
        }
        assert_eq!(
            MYSQL.matches("ALTER TABLE ").count(),
            CONVERTED.len(),
            "a table was added or removed without updating this list"
        );
        assert_eq!(
            MYSQL.matches("MODIFY COLUMN ").count(),
            CONVERTED.iter().map(|(_, columns)| columns).sum::<usize>()
        );
    }

    /// `MODIFY COLUMN` restates the whole definition, so a default that is not
    /// repeated is a default that is dropped. Only one column has one.
    #[test]
    fn the_one_default_survives() {
        assert!(MYSQL.contains("DEFAULT CURRENT_TIMESTAMP(6)"));
        assert_eq!(MYSQL.matches("DEFAULT ").count(), 1);
    }

    /// Nothing here may leave a `TIMESTAMP` behind.
    #[test]
    fn no_column_is_converted_back_to_the_type_being_removed() {
        for line in MYSQL.lines().filter(|line| line.contains("MODIFY COLUMN")) {
            assert!(line.contains("DATETIME(6)"), "{line}");
        }
    }
}
