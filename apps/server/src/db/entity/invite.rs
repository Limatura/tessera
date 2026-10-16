//! The `invites` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `invites`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "invites")]
pub struct Model {
    /// ULID in text form.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// SHA-256 of the code. Never the code itself: it exists in readable form
    /// once, on the terminal that made it.
    #[sea_orm(unique)]
    pub code_hash: String,
    /// What the operator called it, so a list of them means something.
    pub note: Option<String>,
    /// How many accounts this code may create.
    pub max_uses: i64,
    /// How many it has created. Charged when an account exists, not when a form
    /// is submitted.
    pub used: i64,
    /// When it was made.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When it stops working, or `None` for one that never does.
    pub expires_at: Option<TimeDateTimeWithTimeZone>,
}

/// No relations declared. `pending_registrations.invite_id` carries no foreign
/// key, so there is nothing for SeaORM to join across.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
