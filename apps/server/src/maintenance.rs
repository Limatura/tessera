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

use crate::db::repository::session::SessionRepository;

/// How often to sweep.
///
/// Hourly rather than on a timer tuned to the session TTL: this is a cheap
/// `DELETE` over an indexed column, and the cost of running it more often than
/// strictly needed is far smaller than the cost of a knob nobody understands.
const INTERVAL: Duration = Duration::from_hours(1);

/// Deletes expired sessions until `shutdown` is cancelled.
///
/// Runs once at start-up rather than waiting out the first interval, so a
/// process that is restarted more often than the interval still sweeps.
pub async fn sweep_expired_sessions(sessions: SessionRepository, shutdown: CancellationToken) {
    loop {
        match sessions.delete_expired(OffsetDateTime::now_utc()).await {
            // Logged only when something was actually removed. A quiet instance
            // should not write an identical line every hour forever.
            Ok(removed) if removed > 0 => {
                tracing::info!(removed, "swept expired sessions");
            }
            Ok(_) => {}
            // A failed sweep is not fatal: the rows are harmless, and the next
            // pass will try again. Worth a warning because a *persistent*
            // failure means the table is growing.
            Err(error) => tracing::warn!(?error, "could not sweep expired sessions"),
        }

        tokio::select! {
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(INTERVAL) => {}
        }
    }
}
