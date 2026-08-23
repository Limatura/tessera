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
/// How many abandoned uploads one pass aborts.
const UPLOADS_PER_PASS: u64 = 100;

const SHARES_PER_PASS: u64 = 500;

/// Deletes what has run out until `shutdown` is cancelled.
///
/// Runs once at start-up rather than waiting out the first interval, so a
/// process that is restarted more often than the interval still sweeps.
pub async fn sweep_expired(state: AppState, shutdown: CancellationToken) {
    loop {
        sweep_once(&state, OffsetDateTime::now_utc()).await;

        tokio::select! {
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(INTERVAL) => {}
        }
    }
}

/// One pass of everything the sweep does, at a time the caller chooses.
///
/// Separate from the loop so a test can run a pass without waiting an hour or
/// standing up a background task, and so `now` can be a time other than the
/// present — which is the only way to assert what happens to something that
/// expired last week.
pub async fn sweep_once(state: &AppState, now: OffsetDateTime) {
    // Sessions are the login history now, so an expired one keeps its row. It
    // goes once it is older than the history is kept for, which is the only
    // thing giving the table an upper bound.
    report(
        "sessions past the history window",
        state
            .sessions
            .delete_ended_before(
                now.saturating_sub(
                    time::Duration::try_from(state.config().auth.login_history())
                        .unwrap_or_else(|_| time::Duration::days(90)),
                ),
            )
            .await,
    );
    report(
        "expired download authorisations",
        state.tickets.delete_expired(now).await,
    );

    // Unconditionally, and not only while the log is on. Switching it off has
    // to mean the rows already written go away on schedule too — gating this
    // on `enabled` would make "stop recording people" the one action that
    // keeps their records for ever. The query costs nothing on an instance
    // that has never written a row.
    report(
        "access events past their retention",
        state
            .events
            .delete_before(
                now.saturating_sub(
                    time::Duration::try_from(state.config().share.access_log.retention())
                        .unwrap_or_else(|_| time::Duration::days(30)),
                ),
            )
            .await,
    );

    // An abandoned upload is holding real bytes in the store, and they count
    // against the quota. Aborting the multipart is what actually frees them —
    // deleting the row alone would leave parts that nothing will ever look for
    // and that the store will keep billing for. That is `vastsa/FileCodeBox#362`
    // in our shape.
    sweep_abandoned_uploads(state, now).await;
    // Spent links are left until they expire rather than removed on use: while
    // the row is there, following the same link twice can say "already used"
    // instead of "no such link", and those deserve different answers.
    report(
        "expired password reset links",
        state.resets.delete_expired(now).await,
    );
    // These hold a password hash, so letting them linger past their usefulness
    // keeps a credential around for an account that was never created.
    report(
        "expired invite codes",
        state.invites.delete_expired(now).await,
    );
    report(
        "unconfirmed registrations",
        state.pending.delete_expired(now).await,
    );
    // Content first, rows second, and not only for tidiness: the row is what
    // records which object to delete, so releasing the bytes has to happen while
    // there is still something pointing at them.
    report(
        "dead shares' content",
        reap_dead_content(&state.shares, &state.storage, now).await,
    );
    report(
        "shares past their retention",
        sweep_dead_shares(
            &state.shares,
            &state.storage,
            now,
            state.config().share.retention(),
        )
        .await,
    );
}

/// Removes the objects of shares that are finished with, then their rows.
///
/// # Why the object goes first
///
/// A row without its object is a share that accepts a request and then fails to
/// deliver. An object without its row is an orphan — invisible, but findable by
/// listing the store, and harmless until then. If only one of the two can
/// happen, that is the one to choose.
/// Removes the content of shares that have died, leaving their rows.
///
/// The two halves of a dead share have different reasons to exist. The row is
/// what lets a link say "this expired" rather than "no such code", which is
/// worth a week. The content is worth nothing the moment the share dies —
/// [`crate::api::gate::open`] refuses a dead share before anything reads it, so
/// from that instant the object is unreachable — and it is the half that costs
/// money.
///
/// Doing this separately is what stops a burn-after-reading share from
/// occupying its full size for a week after the one person who was meant to
/// have it already did.
/// Removes every one, and says whether all of them went.
///
/// One failure stops the share being marked, because a share marked clean with
/// an object still in the store is an object nothing will ever look for again.
async fn remove_every_object(storage: &Arc<dyn ObjectStore>, keys: &[String]) -> bool {
    let mut all_gone = true;
    for key in keys {
        if !remove_object(storage, key).await {
            all_gone = false;
        }
    }
    all_gone
}

async fn reap_dead_content(
    shares: &ShareRepository,
    storage: &Arc<dyn ObjectStore>,
    now: OffsetDateTime,
) -> Result<u64, ShareRepositoryError> {
    let mut reaped: u64 = 0;
    for share in shares.find_reapable(now, SHARES_PER_PASS).await? {
        // Every object, not the first: a share holds one file today and several
        // once #42 lands, and stopping after one would leave the rest paid for
        // and unreachable once the rows naming them are gone.
        let keys = shares.object_keys_of(share.id).await?;
        if !remove_every_object(storage, &keys).await {
            // The rows stay, still naming what to delete, so the next pass finds
            // them again. Marking them `gone` here would strand those bytes.
            continue;
        }
        // Inline content lives in a child row that goes with the share, so
        // there is nothing to delete for it here — but marking it still takes
        // it out of the quota, which is the point.
        shares.mark_content_removed(share.id).await?;
        reaped = reaped.saturating_add(1);
    }
    Ok(reaped)
}

/// Deletes an object, treating "it was not there" as success.
///
/// The distinction matters because the caller retries on failure. A store that
/// is down should be retried — it will come back. An object that is simply
/// absent will not become present, so retrying it means the row is reprocessed
/// every hour forever and, worse, keeps counting against the quota for bytes
/// nobody has. Somebody tidying a bucket by hand is enough to cause it.
async fn remove_object(storage: &Arc<dyn ObjectStore>, key: &str) -> bool {
    match storage.delete(&ObjectPath::from(key)).await {
        Ok(()) => true,
        Err(object_store::Error::NotFound { .. }) => {
            tracing::debug!(key, "the object was already gone");
            true
        }
        Err(error) => {
            tracing::warn!(?error, key, "could not remove a dead share's object");
            false
        }
    }
}

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
        // Usually already gone by now — `reap_dead_content` runs first — but a
        // disabled share reaches retention with its objects intact, and so does
        // anything the reaper could not delete earlier.
        let keys = shares.object_keys_of(share.id).await?;
        if !remove_every_object(storage, &keys).await {
            // The rows stay. They are the only record of what to remove, so
            // dropping them now would strand those bytes; the next pass tries
            // again.
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

/// Aborts uploads nobody came back to finish.
///
/// Bounded per pass so that a thousand abandoned sessions become a thousand
/// aborts spread over several passes, rather than one pass holding a connection
/// for minutes.
async fn sweep_abandoned_uploads(state: &AppState, now: OffsetDateTime) {
    let expired = match state.uploads.expired(now, UPLOADS_PER_PASS).await {
        Ok(expired) => expired,
        Err(error) => {
            tracing::warn!(?error, "could not read abandoned uploads");
            return;
        }
    };
    if expired.is_empty() {
        return;
    }

    let mut removed = 0_usize;
    for upload in &expired {
        if abandon(state, upload).await {
            removed += 1;
        }
    }
    tracing::info!(removed, "abandoned uploads");
}

/// Frees one abandoned upload, reporting whether its row went.
///
/// The row stays when the store would not let go. S3 does not clean up
/// abandoned multipart uploads by itself, so those parts remain — and remain
/// billed — until an abort succeeds. Deleting the row would throw away the
/// identifier the abort needs, so nothing could ever try again and the space
/// would be lost silently.
///
/// Keeping it also keeps the quota honest: the bytes really are still there, so
/// the reservation should still stand. A later pass retries; an operator sees
/// the warning in the meantime.
async fn abandon(state: &AppState, upload: &crate::db::repository::upload::Upload) -> bool {
    if let Some(sink) = crate::upload::reopen(state, upload)
        && !sink.discard(&upload.object_key).await
    {
        return false;
    }
    match state.uploads.delete(upload.id).await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(?error, %upload.id, "could not remove an abandoned upload");
            false
        }
    }
}
