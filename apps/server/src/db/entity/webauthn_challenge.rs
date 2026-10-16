//! The `webauthn_challenges` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `webauthn_challenges`: a challenge a browser has not answered yet.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "webauthn_challenges")]
pub struct Model {
    /// SHA-256 of the challenge, as the browser echoes it back.
    #[sea_orm(primary_key, auto_increment = false)]
    pub secret_hash: String,
    /// `register` or `sign_in`.
    pub purpose: String,
    /// Whose account a registration adds to. `None` for a sign-in, which does
    /// not know who is signing in until the answer arrives.
    pub user_id: Option<String>,
    /// When it was issued.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When it stops being accepted.
    pub expires_at: TimeDateTimeWithTimeZone,
}

/// No relations declared; the foreign key is enforced by the database.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
