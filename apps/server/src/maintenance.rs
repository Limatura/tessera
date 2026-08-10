//! Periodic housekeeping.
//!
//! Everything here is work that is *also* done opportunistically elsewhere — an
//! expired session presented to the server is deleted on sight. This exists for
//! the rows nobody ever comes back for: a browser that was closed, a laptop that
//! was replaced. Without it the sessions table only ever grows, and on a
//! long-lived personal instance that growth is unbounded and invisible.

use std::time::Duration;

use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;

use crate::db::repository::{session::SessionRepository, share::ShareTicketRepository};

/// How often to sweep.
///
/// Hourly rather than on a timer tuned to the session TTL: this is a cheap
/// `DELETE` over an indexed column, and the cost of running it more often than
/// strictly needed is far smaller than the cost of a knob nobody understands.
const INTERVAL: Duration = Duration::from_hours(1);

/// Deletes what has run out until `shutdown` is cancelled.
///
/// Runs once at start-up rather than waiting out the first interval, so a
/// process that is restarted more often than the interval still sweeps.
pub async fn sweep_expired(
    sessions: SessionRepository,
    tickets: ShareTicketRepository,
    shutdown: CancellationToken,
) {
    loop {
        let now = OffsetDateTime::now_utc();
        report("expired sessions", sessions.delete_expired(now).await);
        report(
            "expired download authorisations",
            tickets.delete_expired(now).await,
        );

        tokio::select! {
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(INTERVAL) => {}
        }
    }
}

/// Says what a pass removed, and only when it removed something.
///
/// A quiet instance should not write an identical line every hour forever. A
/// failed pass is not fatal — the rows are harmless and the next one tries
/// again — but a *persistent* failure means a table is growing, so it warns.
fn report<E: std::fmt::Debug>(what: &str, outcome: Result<u64, E>) {
    match outcome {
        Ok(0) => {}
        Ok(removed) => tracing::info!(removed, what, "swept"),
        Err(error) => tracing::warn!(?error, what, "could not sweep"),
    }
}
