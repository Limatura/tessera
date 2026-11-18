//! The `audit_events` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `audit_events`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "audit_events")]
pub struct Model {
    /// ULID in text form, so the primary key sorts in the order things happened.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// When it happened.
    pub occurred_at: TimeDateTimeWithTimeZone,
    /// What was done, such as `share.create`.
    pub action: String,
    /// `success` or `failure`.
    pub outcome: String,
    /// `anonymous`, `user`, `api_token` or `system`.
    pub actor_kind: String,
    /// The account that did it, when one did.
    pub actor_user_id: Option<String>,
    /// The token it was done with, when it was.
    pub api_token_id: Option<String>,
    /// What kind of thing it was done to.
    pub target_type: Option<String>,
    /// Which one.
    pub target_id: Option<String>,
    /// Where the request came from, under the same rule as `access_events`.
    pub ip_address: Option<String>,
    /// What the client called itself.
    pub user_agent: Option<String>,
    /// JSON with whatever this action records. Never a secret.
    pub metadata: Option<String>,
}

/// No relations: an entry outlives whatever it names.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
