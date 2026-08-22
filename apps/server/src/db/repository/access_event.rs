//! Recording and sweeping access events.
//!
//! # What this is for
//!
//! An operator who runs an instance for other people sometimes needs to answer
//! "who has been fetching this". The counters cannot: they say how many times,
//! never by whom, which is the whole reason they need no switch.
//!
//! What is written here does identify people, so nothing is written unless
//! `share.access_log.enabled` says so, and what is written goes away after
//! `share.access_log.retention_days`. Both decisions belong to whoever runs the
//! instance; neither is made here.

use sea_orm::{
    ActiveValue::Set, ColumnTrait as _, DatabaseConnection, EntityTrait as _, QueryFilter as _,
    QueryOrder as _, QuerySelect as _,
};
use tessera_core::{AccessEventId, ShareId};
use time::OffsetDateTime;

use crate::db::entity::access_event::{self, Entity as AccessEventEntity};

/// One retrieval, as the rest of the server sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessEvent {
    /// When it happened.
    pub occurred_at: OffsetDateTime,
    /// Where it came from, when that could be established.
    pub ip_address: Option<String>,
    /// What the client called itself.
    pub user_agent: Option<String>,
}

/// The longest User-Agent kept.
///
/// The header is caller-controlled and has no length limit of its own, so
/// without this a single request decides how much of the operator's database
/// one row occupies. Real ones are comfortably under this; anything longer is
/// not a browser telling the truth about itself.
const MAX_USER_AGENT: usize = 512;

/// Access events, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct AccessEventRepository {
    connection: DatabaseConnection,
}

impl AccessEventRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// Writes one event.
    ///
    /// # Errors
    ///
    /// Returns [`sea_orm::DbErr`] if the insert fails.
    pub async fn record(
        &self,
        share_id: ShareId,
        occurred_at: OffsetDateTime,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<(), sea_orm::DbErr> {
        AccessEventEntity::insert(access_event::ActiveModel {
            id: Set(AccessEventId::generate().to_string()),
            share_id: Set(share_id.to_string()),
            occurred_at: Set(occurred_at),
            ip_address: Set(ip_address.map(ToOwned::to_owned)),
            user_agent: Set(user_agent.map(|agent| truncate(agent, MAX_USER_AGENT))),
        })
        .exec(&self.connection)
        .await?;
        Ok(())
    }

    /// A share's events, most recent first.
    ///
    /// # Errors
    ///
    /// Returns [`sea_orm::DbErr`] if the query fails.
    pub async fn for_share(
        &self,
        share_id: ShareId,
        limit: u64,
    ) -> Result<Vec<AccessEvent>, sea_orm::DbErr> {
        Ok(AccessEventEntity::find()
            .filter(access_event::Column::ShareId.eq(share_id.to_string()))
            .order_by_desc(access_event::Column::OccurredAt)
            .limit(limit)
            .all(&self.connection)
            .await?
            .into_iter()
            .map(|row| AccessEvent {
                occurred_at: row.occurred_at,
                ip_address: row.ip_address,
                user_agent: row.user_agent,
            })
            .collect())
    }

    /// Removes events older than `cutoff`.
    ///
    /// The retention period is the whole of the promise this feature makes
    /// about how long a visit is remembered, so this is what keeps it.
    ///
    /// # Errors
    ///
    /// Returns [`sea_orm::DbErr`] if the delete fails.
    pub async fn delete_before(&self, cutoff: OffsetDateTime) -> Result<u64, sea_orm::DbErr> {
        Ok(AccessEventEntity::delete_many()
            .filter(access_event::Column::OccurredAt.lt(cutoff))
            .exec(&self.connection)
            .await?
            .rows_affected)
    }
}

/// Cuts a string to `limit` bytes without splitting a character.
///
/// Slicing by byte index would panic on a multi-byte boundary, and a User-Agent
/// can carry anything a caller chose to send.
fn truncate(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let mut end = limit;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    // `get` rather than an index: the loop above already stepped back to a
    // boundary, and the fallible form says so to the reader instead of leaning
    // on a panic that cannot happen.
    value.get(..end).unwrap_or_default().to_owned()
}

#[cfg(test)]
mod tests {
    use super::{MAX_USER_AGENT, truncate};

    /// A caller-controlled header with no length limit decides how much of the
    /// operator's database one row takes, unless something here says otherwise.
    #[test]
    fn a_long_user_agent_is_cut_to_the_bound() {
        let long = "a".repeat(MAX_USER_AGENT * 3);
        assert_eq!(truncate(&long, MAX_USER_AGENT).len(), MAX_USER_AGENT);
    }

    /// Cutting by byte index alone panics when the boundary lands inside a
    /// character, and a User-Agent can carry any bytes the caller chose.
    #[test]
    fn cutting_never_splits_a_character() {
        // Three bytes each, so a limit of 4 lands mid-character.
        let text = "中文中文";
        let cut = truncate(text, 4);
        assert_eq!(cut, "中", "should have stopped at the boundary below 4");
    }

    #[test]
    fn something_short_is_left_alone() {
        assert_eq!(truncate("Mozilla/5.0", MAX_USER_AGENT), "Mozilla/5.0");
    }
}
