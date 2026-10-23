//! The `webhooks` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `webhooks`: a URL told when something happens to a share.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "webhooks")]
pub struct Model {
    /// ULID in text form.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// The account it belongs to, or `None` for the instance's own.
    pub user_id: Option<String>,
    /// Where events are sent, sealed.
    pub url: String,
    /// The events it is sent, space-separated.
    pub events: String,
    /// `json` or `discord`.
    pub format: String,
    /// What the body is signed with, sealed.
    pub secret: String,
    /// When it was added.
    pub created_at: TimeDateTimeWithTimeZone,
}

/// No relations declared; the foreign key is enforced by the database.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
