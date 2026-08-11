//! Periodic housekeeping.
//!
//! Everything here is work that is *also* done opportunistically elsewhere — an
//! expired session presented to the server is deleted on sight, and a dead share
//! stops being served the moment it dies. This exists for what nobody ever comes
//! back for: a browser that was closed, a laptop that was replaced, a share
//! whose recipient never followed the link. Without it those only ever
//! accumulate, and on a long-lived personal instance that growth is unbounded
//! and invisible.
//!
//! Dead shares matter most of the three, because they are the only ones holding
//! bytes. A row is a couple of hundred of them; the payload can be gigabytes,
//! and no database action reaches the blob store to remove it.

use std::{sync::Arc, time::Duration};

use object_store::{ObjectStore, ObjectStoreExt as _, path::Path as ObjectPath};
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;

use crate::{
    db::repository::share::{ShareRepository, ShareRepositoryError},
    state::AppState,
};

/// How often to sweep.
///
/// Hourly rather than on a timer tuned to any particular lifetime: these are
/// cheap `DELETE`s over indexed columns, and the cost of running one more often
/// than strictly needed is far smaller than the cost of a knob nobody
/// understands.
const INTERVAL: Duration = Duration::from_hours(1);

/// Dead shares removed in one pass.
///
/// Bounded because each one is a blob-store round trip, and a pass that tried to
/// clear a year's backlog at once would hold the pool and compete with the
/// requests the process exists to serve. A backlog drains over several passes
/// instead, which is the right shape for work nobody is waiting on.
const SHARES_PER_PASS: u64 = 500;

/// Deletes what has run out until `shutdown` is cancelled.
///
/// Runs once at start-up rather than waiting out the first interval, so a
/// process that is restarted more often than the interval still sweeps.
pub async fn sweep_expired(state: AppState, shutdown: CancellationToken) {
    loop {
        let now = OffsetDateTime::now_utc();
        report("expired sessions", state.sessions.delete_expired(now).await);
        report(
            "expired download authorisations",
            state.tickets.delete_expired(now).await,
        );
        report(
            "shares past their retention",
            sweep_dead_shares(
                &state.shares,
                &state.storage,
                now,
                state.config.share.retention(),
            )
            .await,
        );

        tokio::select! {
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(INTERVAL) => {}
        }
    }
}

/// Removes the objects of shares that are finished with, then their rows.
///
/// # Why the object goes first
///
/// A row without its object is a share that accepts a request and then fails to
/// deliver. An object without its row is an orphan — invisible, but findable by
/// listing the store, and harmless until then. If only one of the two can
/// happen, that is the one to choose.
async fn sweep_dead_shares(
    shares: &ShareRepository,
    storage: &Arc<dyn ObjectStore>,
    now: OffsetDateTime,
    retention: Duration,
) -> Result<u64, ShareRepositoryError> {
    let cutoff = now.saturating_sub(
        time::Duration::try_from(retention).unwrap_or_else(|_| time::Duration::days(7)),
    );

    let mut removed: u64 = 0;
    for share in shares.find_dead(cutoff, SHARES_PER_PASS).await? {
        if let Some(key) = share.storage.object_key()
            && let Err(error) = storage.delete(&ObjectPath::from(key)).await
        {
            // The row stays. It is the only record of which object to remove, so
            // dropping it now would strand those bytes for good; the next pass
            // tries again.
            tracing::warn!(?error, key, "could not remove a dead share's object");
            continue;
        }
        shares.delete(share.id).await?;
        removed = removed.saturating_add(1);
    }
    Ok(removed)
}

/// Says what a pass removed, and only when it removed something.
///
/// A quiet instance should not write an identical line every hour forever. A
/// failed pass is not fatal — the rows are harmless and the next one tries
/// again — but a *persistent* failure means something is growing, so it warns.
fn report<E: std::fmt::Debug>(what: &str, outcome: Result<u64, E>) {
    match outcome {
        Ok(0) => {}
        Ok(removed) => tracing::info!(removed, what, "swept"),
        Err(error) => tracing::warn!(?error, what, "could not sweep"),
    }
}
