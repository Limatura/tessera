//! The `settings` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `settings`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "settings")]
pub struct Model {
    /// The dotted key as it appears in the configuration, `share.code_length`.
    #[sea_orm(primary_key, auto_increment = false)]
    pub name: String,
    /// The value as JSON, so a number stays a number.
    pub value: String,
    /// When it last changed.
    pub updated_at: TimeDateTimeWithTimeZone,
}

/// No relations; this table points at nothing.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
