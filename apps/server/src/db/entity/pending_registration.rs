//! The `pending_registrations` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `pending_registrations`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "pending_registrations")]
pub struct Model {
    /// ULID in text form.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// The name asked for. Not reserved; see the migration for why.
    pub username: String,
    /// The address being confirmed. One live registration per address.
    #[sea_orm(unique)]
    pub email: String,
    /// Already hashed. The password is carried through the wait instead of
    /// asked for again, and this table must be no more use to a thief than
    /// `users` is.
    pub password_hash: String,
    /// SHA-256 of the token. Never the token itself: the link in the email is
    /// the only copy that ever exists.
    #[sea_orm(unique)]
    pub secret_hash: String,
    /// When the link was sent.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When it stops working. The row is swept after this.
    pub expires_at: TimeDateTimeWithTimeZone,
    /// Which invite this registration came in on, on an invite-only instance.
    ///
    /// The place is charged when the account exists, not when the form was
    /// submitted; see the migration.
    pub invite_id: Option<String>,
}

/// No relations declared. This row points at no account, because no account
/// exists yet.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
