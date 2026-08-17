//! The `api_token_permissions` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `api_token_permissions`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "api_token_permissions")]
pub struct Model {
    /// Which token.
    #[sea_orm(primary_key, auto_increment = false)]
    pub token_id: String,
    /// A `tessera_core::Permission`, as its `resource:verb` spelling.
    #[sea_orm(primary_key, auto_increment = false)]
    pub permission: String,
}

/// No relations declared: the foreign key is enforced by the database.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
