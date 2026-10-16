//! The `recovery_codes` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `recovery_codes`: one single-use code.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "recovery_codes")]
pub struct Model {
    /// ULID in text form.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Whose code this is.
    pub user_id: String,
    /// SHA-256 of the code, never the code itself.
    pub code_hash: String,
    /// When it was issued.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When it was used, if it was.
    pub used_at: Option<TimeDateTimeWithTimeZone>,
}

/// No relations declared; the foreign key is enforced by the database.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
