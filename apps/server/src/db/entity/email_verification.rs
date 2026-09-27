//! The `email_verifications` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `email_verifications`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "email_verifications")]
pub struct Model {
    /// ULID in text form.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Whose address this would prove.
    pub user_id: String,
    /// The address the link was sent to, and the only one it can prove.
    pub email: String,
    /// SHA-256 of the token, never the token itself: the link in the email is
    /// the only copy.
    #[sea_orm(unique)]
    pub secret_hash: String,
    /// When the link was sent.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When it stops working.
    pub expires_at: TimeDateTimeWithTimeZone,
    /// When it was used, if it was.
    pub used_at: Option<TimeDateTimeWithTimeZone>,
}

/// No relations declared; the foreign key is enforced by the database.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
