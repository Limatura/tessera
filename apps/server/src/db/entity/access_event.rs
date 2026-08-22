//! The `access_events` table, as SeaORM sees it.
//!
//! Nothing writes here unless `share.access_log.enabled` is on — see the
//! migration for why that is the default.

use sea_orm::entity::prelude::*;

/// A row of `access_events`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "access_events")]
pub struct Model {
    /// ULID in text form.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// The share that was fetched.
    pub share_id: String,
    /// When it was fetched.
    pub occurred_at: TimeDateTimeWithTimeZone,
    /// Where the request came from, or `None` when there is no honest answer —
    /// behind a proxy this instance does not trust, the address it can see
    /// belongs to the proxy rather than to the person.
    pub ip_address: Option<String>,
    /// What the client called itself. Recorded as presented, never echoed.
    pub user_agent: Option<String>,
}

/// No relations declared; the foreign key is enforced by the database.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
