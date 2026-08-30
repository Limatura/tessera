//! Periodic housekeeping.
//!
//! Everything here is also done opportunistically elsewhere: an expired session
//! presented to the server is deleted on sight, and a dead share stops being
//! served the moment it dies. This exists for what is never come back for: a
//! closed browser, a replaced laptop, or a share whose recipient never
//! followed the link. Those otherwise accumulate invisibly.
//!
//! Dead shares matter most, because they are the only ones holding bytes. A row
//! is a couple of hundred; the payload can be gigabytes, and no database action
//! reaches the blob store. See `docs/maintenance.md`.

use std::{collections::HashSet, sync::Arc, time::Duration};

use object_store::{ObjectStore, ObjectStoreExt as _, path::Path as ObjectPath};
use time::OffsetDateTime;
// `futures-util` is not a dependency of the server; see the note in
// `Cargo.toml`. This is the same `next` and is already in the tree.
use tokio_stream::StreamExt as _;
use tokio_util::sync::CancellationToken;

use crate::{
    db::repository::share::{ShareRepository, ShareRepositoryError},
    state::AppState,
};

/// How often to sweep.
///
/// Hourly, not tuned to any particular lifetime. These are cheap
/// `DELETE`s over indexed columns, and running one more often than needed costs
/// less than an extra setting to explain.
const INTERVAL: Duration = Duration::from_hours(1);

/// How many abandoned uploads one pass aborts.
const UPLOADS_PER_PASS: u64 = 100;

/// Dead shares removed in one pass.
///
/// Bounded because each one is a blob-store round trip, and a pass clearing a
/// year's backlog at once would hold the pool and compete with the requests the
/// process exists to serve. A backlog drains over several passes instead.
const SHARES_PER_PASS: u64 = 500;

/// Objects removed by one reconciliation pass.
///
/// Bounded like the others, and for a stronger reason: this deletes objects no
/// row names, so a bug here destroys data nothing can point at afterwards. A
/// cap makes a bug take many passes to do its damage, and the log says what
/// went on each.
const ORPHANS_PER_PASS: usize = 200;

/// The only prefix this instance writes under, and so the only one it will
/// delete from.
///
/// A bucket is often shared with something else. Listing from the root and
/// deleting what our database does not recognise would turn this into "delete
/// everything that is not ours".
const OWNED_PREFIX: &str = "shares";

/// Deletes what has run out until `shutdown` is cancelled.
///
/// Runs once at start-up instead of waiting out the first interval, so a
/// process that is restarted more often than the interval still sweeps.
pub async fn sweep_expired(state: AppState, shutdown: CancellationToken) {
    let mut since_reconcile = Duration::ZERO;
    loop {
        let now = OffsetDateTime::now_utc();
        sweep_once(&state, now).await;

        // On its own cadence, and read fresh each pass so changing the setting
        // takes effect without a restart. Counted, not timed, so a
        // process restarted more often than the interval does not skip it for
        // ever, and does not run it every time either.
        if let Some(interval) = state.config().storage.reconcile_interval() {
            if since_reconcile >= interval {
                since_reconcile = Duration::ZERO;
                reconcile_once(&state, now).await;
            } else {
                since_reconcile = since_reconcile.saturating_add(INTERVAL);
            }
        }

        tokio::select! {
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(INTERVAL) => {}
        }
    }
}

/// Removes objects in the store that no row names.
///
/// # Why this is needed at all
///
/// Every other sweep here deletes things the database knows about. Nothing
/// walks the *store*, so an object that lost its row is invisible: not in any
/// listing, not in any total, and never mentioned by a query again. It still
/// occupies real space, and on a ten-gigabyte free tier one leaked upload is a
/// percent of everything the operator has.
///
/// Two ways a row goes missing. The bytes are written before the row, so a
/// crash in between leaves the object; and a permanently failing delete
/// (rotated credentials, a read-only bucket) leaves one behind after the row is
/// gone. Neither is exotic and neither is otherwise recoverable.
///
/// # Why it deletes instead of reporting
///
/// An unactioned report is no report, and the action is always the same. Two
/// bounds make deleting safe: only under the `shares/` prefix this instance
/// writes to, and only objects older than `storage.reconcile_grace_hours`.
pub async fn reconcile_once(state: &AppState, now: OffsetDateTime) {
    // Both tables, because an unfinished upload names an object with no share
    // yet. Read before the listing, not after. That is what makes the race
    // safe: a row written while this runs is not in the set, but its
    // object is younger than the grace period and is skipped on age instead.
    let Some(live) = live_keys(state).await else {
        return;
    };
    let removed = remove_orphans(state, live, cutoff(state, now)).await;
    if removed.count > 0 {
        tracing::info!(
            removed = removed.count,
            freed_bytes = removed.bytes,
            "removed orphaned objects"
        );
    }
}

/// How old an object has to be before this pass will touch it, in unix seconds.
///
/// Compared as unix seconds because the store speaks `chrono` and the rest of
/// this crate speaks `time`. Converting between them to answer "is this older
/// than that" would be more code for the same answer.
fn cutoff(state: &AppState, now: OffsetDateTime) -> i64 {
    let grace = time::Duration::try_from(state.config().storage.reconcile_grace())
        .unwrap_or_else(|_| time::Duration::days(1));
    now.saturating_sub(grace).unix_timestamp()
}

/// What one pass reclaimed.
struct Reclaimed {
    count: usize,
    bytes: u64,
}

/// Walks the owned prefix and removes what is both unreferenced and old.
async fn remove_orphans(state: &AppState, live: HashSet<String>, cutoff: i64) -> Reclaimed {
    let mut listing = state.storage.list(Some(&ObjectPath::from(OWNED_PREFIX)));
    let mut reclaimed = Reclaimed { count: 0, bytes: 0 };

    while let Some(entry) = listing.next().await {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!(%error, "could not list the store; stopping this pass");
                break;
            }
        };
        if entry.last_modified.timestamp() > cutoff || live.contains(entry.location.as_ref()) {
            continue;
        }
        if !remove_object(&state.storage, entry.location.as_ref()).await {
            continue;
        }
        reclaimed.count += 1;
        reclaimed.bytes = reclaimed.bytes.saturating_add(entry.size);
        if reclaimed.count >= ORPHANS_PER_PASS {
            tracing::info!(
                cap = ORPHANS_PER_PASS,
                "reconciliation hit its per-pass cap; the rest goes next pass"
            );
            break;
        }
    }
    reclaimed
}

/// Every object key still spoken for, from both tables that can name one.
///
/// An in-flight upload's partial file sits beside its object under a suffix, so
/// the key it will become has to protect the key it is now, or the
/// reconciliation deletes the bytes out from under a live upload.
async fn live_keys(state: &AppState) -> Option<HashSet<String>> {
    let shares = match state.shares.all_object_keys().await {
        Ok(keys) => keys,
        Err(error) => {
            tracing::warn!(%error, "could not read share object keys; skipping reconciliation");
            return None;
        }
    };
    let uploads = match state.uploads.all_object_keys().await {
        Ok(keys) => keys,
        Err(error) => {
            // Either query failing has to abandon the pass, not proceed with a
            // partial set. An object missing from `live` is an object
            // this deletes.
            tracing::warn!(%error, "could not read in-flight upload keys; skipping reconciliation");
            return None;
        }
    };

    let mut live: HashSet<String> = shares.into_iter().collect();
    for key in uploads {
        live.insert(format!("{key}{}", crate::upload::sink::REMAINDER_SUFFIX));
        live.insert(key);
    }
    Some(live)
}

/// One pass of everything the sweep does, at a time the caller chooses.
///
/// Separate from the loop so a test can run a pass without waiting an hour or
/// standing up a background task, and so `now` can be a time other than the
/// present, the only way to assert what happens to something that
/// expired last week.
pub async fn sweep_once(state: &AppState, now: OffsetDateTime) {
    // Sessions are the login history now, so an expired one keeps its row. It
    // goes once it is older than the history is kept for, the only
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

    // Unconditionally, not only while the log is on. Switching it off has to
    // mean the rows already written go away on schedule too; gating this on
    // `enabled` would make "stop recording people" the one action that keeps
    // their records for ever. The query costs nothing on an instance that has
    // never written a row.
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

    // An abandoned upload holds real bytes that count against the quota, and
    // aborting the multipart is what frees them. Deleting the row alone leaves
    // parts nothing will look for and the store keeps billing for. That is
    // `vastsa/FileCodeBox#362` in our shape.
    sweep_abandoned_uploads(state, now).await;
    // Spent links are left until they expire, not removed on use. While
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
    // A browser closed on the provider's login page leaves a row nothing will
    // ever come back for. It holds no credential, only the values the callback
    // would have been checked against, but it is unbounded growth otherwise.
    report(
        "abandoned provider sign-ins",
        state.identities.sweep_expired(now).await,
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

/// Removes every one of a share's objects, and says whether all of them went.
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

/// Removes the content of shares that have died, leaving their rows.
///
/// The two halves of a dead share exist for different reasons. The row lets a
/// link say "this expired" instead of "no such code", worth a week.
/// The content is worth nothing from the moment the share dies, since
/// [`crate::api::gate::open`] refuses a dead share before anything reads it,
/// and it is the half that costs money.
///
/// Separating them is what stops a burn-after-reading share occupying its full
/// size for a week after the one person meant to have it already did.
async fn reap_dead_content(
    shares: &ShareRepository,
    storage: &Arc<dyn ObjectStore>,
    now: OffsetDateTime,
) -> Result<u64, ShareRepositoryError> {
    let mut reaped: u64 = 0;
    for share in shares.find_reapable(now, SHARES_PER_PASS).await? {
        // Every object, not the first: a share can hold several, and stopping
        // after one leaves the rest paid for and unreachable once the rows
        // naming them are gone.
        let keys = shares.object_keys_of(share.id).await?;
        if !remove_every_object(storage, &keys).await {
            // The rows stay, still naming what to delete, so the next pass finds
            // them again. Marking them `gone` here would strand those bytes.
            continue;
        }
        // Inline content lives in a child row that goes with the share, so
        // there is nothing to delete here, but marking it still takes it out of
        // the quota.
        shares.mark_content_removed(share.id).await?;
        reaped = reaped.saturating_add(1);
    }
    Ok(reaped)
}

/// Deletes an object, treating "it was not there" as success.
///
/// The distinction matters because the caller retries on failure. A store that
/// is down will come back. An absent object will not become present, so
/// retrying reprocesses the row every hour for ever and charges the quota for
/// bytes that do not exist. Tidying a bucket by hand is enough to cause it.
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

/// Removes the objects of shares that are finished with, then their rows.
///
/// # Why the object goes first
///
/// A row without its object is a share that accepts a request and then fails to
/// deliver. An object without its row is an orphan: invisible, but findable by
/// listing the store, and harmless until then.
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
        // Usually already gone, since `reap_dead_content` runs first, but a
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
/// A quiet instance should not write an identical line every hour for ever. A
/// failed pass is not fatal, since the rows are harmless and the next one tries
/// again, but a *persistent* failure means something is growing, so it warns.
fn report<E: std::fmt::Debug>(what: &str, outcome: Result<u64, E>) {
    match outcome {
        Ok(0) => {}
        Ok(removed) => tracing::info!(removed, what, "swept"),
        Err(error) => tracing::warn!(?error, what, "could not sweep"),
    }
}

/// Aborts uploads that were never finished.
///
/// Bounded per pass, so a thousand abandoned sessions become a thousand aborts
/// spread over several passes, instead of one pass holding a connection for
/// minutes.
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
/// abandoned multipart uploads by itself, so those parts remain billed until an
/// abort succeeds, and deleting the row would throw away the identifier the
/// abort needs.
///
/// Keeping it also keeps the quota honest: the bytes really are still there, so
/// the reservation should stand. A later pass retries, and an operator sees the
/// warning in the meantime.
async fn abandon(state: &AppState, upload: &crate::db::repository::upload::Upload) -> bool {
    // Every segment, not the first. A session spread over two backends has two
    // multipart uploads, and S3 bills for the parts of an abandoned one until
    // it is aborted, so stopping at the first leaves the second costing money
    // with nothing left that knows its identifier.
    //
    // One failure keeps the whole row: it is the only place the remaining
    // identifiers live, and a later pass has to be able to try again.
    for segment in &upload.segments {
        if let Some(sink) = crate::upload::reopen(state, segment)
            && !sink.discard(&segment.object_key).await
        {
            return false;
        }
    }
    match state.uploads.delete(upload.id).await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(?error, %upload.id, "could not remove an abandoned upload");
            false
        }
    }
}
