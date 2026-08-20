//! The `user_roles` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `user_roles`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "user_roles")]
pub struct Model {
    /// Who.
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: String,
    /// Which role.
    #[sea_orm(primary_key, auto_increment = false)]
    pub role_id: String,
    /// When they were put in it.
    pub granted_at: TimeDateTimeWithTimeZone,
}

/// No relations declared: see [`super::role`].
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
