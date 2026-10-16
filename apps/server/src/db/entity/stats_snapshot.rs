//! The `stats_snapshots` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `stats_snapshots`: one UTC day.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "stats_snapshots")]
pub struct Model {
    /// The day, `YYYY-MM-DD`.
    #[sea_orm(primary_key, auto_increment = false)]
    pub day: String,
    /// Bytes of uploaded content on the store.
    pub stored_bytes: i64,
    /// Shares, including ones that can no longer be opened.
    pub shares: i64,
    /// Shares a visitor could still open.
    pub live_shares: i64,
    /// Bytes sent out, over the shares still here.
    pub bytes_served: i64,
    /// The pass that wrote this row.
    pub recorded_at: TimeDateTimeWithTimeZone,
}

/// No relations.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
