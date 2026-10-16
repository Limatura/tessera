//! The `identities` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `identities`: one way an account signs in through a provider.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "identities")]
pub struct Model {
    /// The provider that vouched for this.
    #[sea_orm(primary_key, auto_increment = false)]
    pub provider: String,
    /// The provider's own identifier for the user: the `sub` claim. Stable
    /// within one issuer and never reused, which makes it safe to key on where
    /// an address would not be.
    #[sea_orm(primary_key, auto_increment = false)]
    pub subject: String,
    /// The account this identity signs in as.
    pub user_id: String,
    /// When the identity was linked.
    pub created_at: TimeDateTimeWithTimeZone,
}

/// No relations declared: the foreign key is enforced by the database, and
/// nothing here needs SeaORM to join across it.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
