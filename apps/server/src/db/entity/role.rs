//! The `roles` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `roles`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "roles")]
pub struct Model {
    /// Row identifier.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// What an operator types and reads. Unique.
    pub name: String,
    /// What it is for, in the operator's own words.
    pub description: Option<String>,
    /// How much storage accounts in this role may hold, or `None` to ask the
    /// instance ceiling. `Some(0)` is no ceiling.
    pub quota_bytes: Option<i64>,
    /// Whether removing it would leave the instance unadministrable.
    pub is_system: bool,
    /// When it was created.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When its permissions last changed.
    pub updated_at: TimeDateTimeWithTimeZone,
}

/// No relations declared: the foreign keys are enforced by the database, and
/// the queries here are explicit rather than joined through SeaORM.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
