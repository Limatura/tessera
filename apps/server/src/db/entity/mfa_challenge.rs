//! The `mfa_challenges` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `mfa_challenges`: a sign-in past its password, waiting for a code.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "mfa_challenges")]
pub struct Model {
    /// SHA-256 of the ticket the client holds. The key, so a ticket names one
    /// row and, once that row is deleted, nothing.
    #[sea_orm(primary_key, auto_increment = false)]
    pub secret_hash: String,
    /// The account that passed the password.
    pub user_id: String,
    /// When the password was accepted.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When the ticket stops working.
    pub expires_at: TimeDateTimeWithTimeZone,
}

/// No relations declared; the foreign key is enforced by the database.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
