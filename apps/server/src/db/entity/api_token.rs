//! The `api_tokens` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `api_tokens`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "api_tokens")]
pub struct Model {
    /// ULID in text form.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Owning account.
    pub user_id: String,
    /// What the owner called it.
    pub name: String,
    /// SHA-256 of the token. Never the token itself.
    #[sea_orm(unique)]
    pub secret_hash: String,
    /// Leading characters, so a listing can identify a token without revealing
    /// it.
    pub display_prefix: String,
    /// When the token was issued.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When it was last accepted, or `None` if never used.
    pub last_used_at: Option<TimeDateTimeWithTimeZone>,
    /// When it stops being accepted, or `None` to live until revoked.
    pub expires_at: Option<TimeDateTimeWithTimeZone>,
    /// When it was revoked. Recorded, not deleted, so an audit still shows the
    /// token existed.
    pub revoked_at: Option<TimeDateTimeWithTimeZone>,
}

/// No relations declared; the foreign key is enforced by the database.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
