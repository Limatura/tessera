//! A row a day of what the instance held, for the dashboard's trend.
//!
//! Written by the hourly sweep, read by `GET /admin/stats/history`. See the
//! migration for why the past has to be recorded rather than computed.

use sea_orm::{
    ColumnTrait as _, DatabaseConnection, EntityTrait as _, QueryFilter as _, QueryOrder as _, Set,
    sea_query::OnConflict,
};
use time::{Date, OffsetDateTime, format_description::FormatItem, macros::format_description};

use crate::db::entity::stats_snapshot::{self, Entity as SnapshotEntity};

/// How a day is spelled in the `day` column, which also sorts it.
const DAY: &[FormatItem<'static>] = format_description!("[year]-[month]-[day]");

/// What the instance held, as `GET /admin/stats` counts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    /// Bytes of uploaded content on the store.
    pub stored_bytes: u64,
    /// Shares, including ones that can no longer be opened.
    pub shares: u64,
    /// Shares a visitor could still open.
    pub live_shares: u64,
    /// Bytes sent out, over the shares still here.
    pub bytes_served: u64,
}

/// The snapshots, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct StatsRepository {
    connection: DatabaseConnection,
}

impl StatsRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// Writes `day`'s row, replacing one already written that day.
    ///
    /// Replaced, not kept, so the row ends the day holding the day's last pass:
    /// what the instance held when the day was over.
    ///
    /// # Errors
    ///
    /// Returns [`sea_orm::DbErr`] if the write fails.
    pub async fn record(
        &self,
        day: Date,
        snapshot: Snapshot,
        now: OffsetDateTime,
    ) -> Result<(), sea_orm::DbErr> {
        SnapshotEntity::insert(stats_snapshot::ActiveModel {
            day: Set(spell(day)),
            stored_bytes: Set(signed(snapshot.stored_bytes)),
            shares: Set(signed(snapshot.shares)),
            live_shares: Set(signed(snapshot.live_shares)),
            bytes_served: Set(signed(snapshot.bytes_served)),
            recorded_at: Set(now),
        })
        .on_conflict(
            OnConflict::column(stats_snapshot::Column::Day)
                .update_columns([
                    stats_snapshot::Column::StoredBytes,
                    stats_snapshot::Column::Shares,
                    stats_snapshot::Column::LiveShares,
                    stats_snapshot::Column::BytesServed,
                    stats_snapshot::Column::RecordedAt,
                ])
                .to_owned(),
        )
        .exec(&self.connection)
        .await?;
        Ok(())
    }

    /// The days from `from` to `to`, both included, oldest first.
    ///
    /// Only days a pass recorded: one the server was down for all day is
    /// absent rather than zero, which would draw a drop that never happened.
    ///
    /// # Errors
    ///
    /// Returns [`sea_orm::DbErr`] if the query fails.
    pub async fn between(
        &self,
        from: Date,
        to: Date,
    ) -> Result<Vec<(Date, Snapshot)>, sea_orm::DbErr> {
        Ok(SnapshotEntity::find()
            .filter(stats_snapshot::Column::Day.gte(spell(from)))
            .filter(stats_snapshot::Column::Day.lte(spell(to)))
            .order_by_asc(stats_snapshot::Column::Day)
            .all(&self.connection)
            .await?
            .into_iter()
            // A row whose day does not parse was not written here; skipping it
            // costs one point on a graph.
            .filter_map(|row| {
                let day = Date::parse(&row.day, DAY).ok()?;
                Some((
                    day,
                    Snapshot {
                        stored_bytes: unsigned(row.stored_bytes),
                        shares: unsigned(row.shares),
                        live_shares: unsigned(row.live_shares),
                        bytes_served: unsigned(row.bytes_served),
                    },
                ))
            })
            .collect())
    }

    /// Removes the days before `day`.
    ///
    /// # Errors
    ///
    /// Returns [`sea_orm::DbErr`] if the delete fails.
    pub async fn delete_before(&self, day: Date) -> Result<u64, sea_orm::DbErr> {
        Ok(SnapshotEntity::delete_many()
            .filter(stats_snapshot::Column::Day.lt(spell(day)))
            .exec(&self.connection)
            .await?
            .rows_affected)
    }
}

/// `YYYY-MM-DD`. A date always formats, so the fallback is never taken.
fn spell(day: Date) -> String {
    day.format(DAY).unwrap_or_default()
}

fn signed(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn unsigned(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}
