//! The `share_tickets` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `share_tickets`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "share_tickets")]
pub struct Model {
    /// ULID in text form.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// The share this authorises, and only that share.
    pub share_id: String,
    /// SHA-256 of the ticket. Never the ticket itself.
    #[sea_orm(unique)]
    pub secret_hash: String,
    /// When the retrieval was authorised.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When the authorisation stops being honoured.
    pub expires_at: TimeDateTimeWithTimeZone,
}

/// No relations declared; the foreign key is enforced by the database.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
