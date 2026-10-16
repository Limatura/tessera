//! The `password_reset_tokens` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `password_reset_tokens`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "password_reset_tokens")]
pub struct Model {
    /// ULID in text form.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Whose password this would set.
    pub user_id: String,
    /// SHA-256 of the token. Never the token itself: the link in the email is
    /// the only copy that ever exists.
    #[sea_orm(unique)]
    pub secret_hash: String,
    /// When the link was sent.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When it stops working.
    pub expires_at: TimeDateTimeWithTimeZone,
    /// When it was spent, if it was.
    ///
    /// Kept, not deleted, so "already used" and "never existed" stay
    /// distinguishable: the first means look for a newer mail, the second means
    /// the link is being guessed at.
    pub used_at: Option<TimeDateTimeWithTimeZone>,
}

/// No relations declared; the foreign key is enforced by the database.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
