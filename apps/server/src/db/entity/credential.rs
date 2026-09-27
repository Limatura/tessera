//! The `credentials` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `credentials`: one account's password, when it has one.
///
/// No row means no password, as for an account that has only ever signed in
/// through an identity provider. That is why this is a table and not a
/// nullable column on `users`; see the migration.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "credentials")]
pub struct Model {
    /// Owning account. One password per account, so this is the key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: String,
    /// PHC-format Argon2 string.
    pub password_hash: String,
    /// When a password was first set.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When it last changed.
    pub updated_at: TimeDateTimeWithTimeZone,
}

/// No relations declared: the foreign key is enforced by the database, and
/// nothing here needs SeaORM to join across it.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
