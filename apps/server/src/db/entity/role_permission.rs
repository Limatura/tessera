//! The `role_permissions` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `role_permissions`.
///
/// The primary key is the pair, so a role cannot hold the same permission
/// twice — holding it twice is not holding it more.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "role_permissions")]
pub struct Model {
    /// Which role.
    #[sea_orm(primary_key, auto_increment = false)]
    pub role_id: String,
    /// A `tessera_core::Permission`, as its `resource:verb` spelling.
    #[sea_orm(primary_key, auto_increment = false)]
    pub permission: String,
}

/// No relations declared: see [`super::role`].
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
