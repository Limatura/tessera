//! The `webhook_deliveries` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `webhook_deliveries`: one event on its way to one webhook.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "webhook_deliveries")]
pub struct Model {
    /// ULID in text form, sent as the `webhook-id` header.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// The webhook it goes to.
    pub webhook_id: String,
    /// Which event, such as `share.downloaded`.
    pub event: String,
    /// What is sent, exactly.
    pub body: String,
    /// How many times it has been sent.
    pub attempts: i32,
    /// When to send it next, or `None` once delivered or given up on.
    pub next_attempt_at: Option<TimeDateTimeWithTimeZone>,
    /// When the receiver accepted it.
    pub delivered_at: Option<TimeDateTimeWithTimeZone>,
    /// The status the receiver last answered with.
    pub last_status: Option<i32>,
    /// Why the last attempt got no answer, or what it answered.
    pub last_error: Option<String>,
    /// When the event happened.
    pub created_at: TimeDateTimeWithTimeZone,
}

/// No relations declared; the foreign key is enforced by the database.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
