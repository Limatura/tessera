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

use object_store::{ObjectStore, ObjectStoreExt as _, PutPayload, path::Path as ObjectPath};
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;

use crate::{
    db::repository::share::{Share, ShareKind, ShareRepository, ShareRepositoryError},
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
                    time::Duration::try_from(state.config.auth.login_history())
                        .unwrap_or_else(|_| time::Duration::days(90)),
                ),
            )
            .await,
    );
    report(
        "expired download authorisations",
        state.tickets.delete_expired(now).await,
    );
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
        "content moved to where this version keeps it",
        relocate_misplaced_content(state, now).await,
    );
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
            state.config.share.retention(),
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
/// Anything that stopped one share's content being moved.
///
/// Its own type because moving touches two systems: a row and an object. The
/// existing sweeps only ever talk to one, so folding an object-store failure
/// into a repository error would say "database" about a thing that was the
/// store.
#[derive(Debug, thiserror::Error)]
enum RelocationError {
    /// The row could not be read or written.
    #[error(transparent)]
    Row(#[from] ShareRepositoryError),
    /// The object could not be read or written.
    #[error(transparent)]
    Object(#[from] object_store::Error),
}

/// Moves content that an older version put somewhere else.
///
/// # Why here and not in a migration
///
/// The rule changed from "small things live in the database" to "text lives in
/// the database and files live in the store". Rows written under the old rule
/// still work — nothing reads them wrongly — but a file held inline never
/// appears in the store an operator is backing up, and text in the store is a
/// round trip for something that could have been a column.
///
/// Putting them right means reading and writing an object, which SQL cannot do
/// and a migration should not attempt: it would turn an upgrade into an
/// operation that can fail half way with a database mid-flight.
///
/// So it happens here, a few at a time, on a pass that already runs. A failure
/// costs one row one hour. Nobody has to run anything, and an instance that is
/// switched off for a month simply catches up when it comes back.
///
/// # Why the count is small
///
/// Each one is an object-store round trip and a write. Moving thirty an hour
/// clears a thousand shares in a day and a half, which is fast enough for
/// something nobody is waiting on, and slow enough that the pass stays a pass.
const RELOCATION_BATCH: u64 = 30;

async fn relocate_misplaced_content(
    state: &AppState,
    now: OffsetDateTime,
) -> Result<u64, RelocationError> {
    let misplaced = state.shares.find_misplaced(RELOCATION_BATCH).await?;
    let mut moved = 0;

    for share in misplaced {
        // One at a time and one failure at a time: a store that is refusing
        // everything should not turn a housekeeping pass into a hundred
        // consecutive errors in the log.
        let result = match share.kind {
            ShareKind::File => relocate_into_store(state, &share, now).await,
            ShareKind::Text => relocate_into_database(state, &share).await,
        };
        match result {
            Ok(()) => moved += 1,
            Err(error) => {
                tracing::warn!(?error, share = %share.id, "could not move a share's content");
                break;
            }
        }
    }
    Ok(moved)
}

/// A file that was small enough to be kept inline under the old rule.
async fn relocate_into_store(
    state: &AppState,
    share: &Share,
    now: OffsetDateTime,
) -> Result<(), RelocationError> {
    let Some(content) = state.shares.inline_content(share.id).await? else {
        // Nothing to move. The row says inline and there is no row to be inline
        // in, which the reaper handles rather than this.
        return Ok(());
    };
    let key = crate::api::shares::object_key(share.id, now);
    state
        .storage
        .put(&ObjectPath::from(key.as_str()), PutPayload::from(content))
        .await?;
    state.shares.moved_to_object(share.id, &key).await?;
    Ok(())
}

/// Text that was large enough to be streamed out under the old rule.
async fn relocate_into_database(state: &AppState, share: &Share) -> Result<(), RelocationError> {
    let Some(key) = share.storage.object_key() else {
        return Ok(());
    };
    let content = state
        .storage
        .get(&ObjectPath::from(key))
        .await?
        .bytes()
        .await?;
    state
        .shares
        .moved_to_database(share, content.to_vec())
        .await?;
    Ok(())
}

async fn reap_dead_content(
    shares: &ShareRepository,
    storage: &Arc<dyn ObjectStore>,
    now: OffsetDateTime,
) -> Result<u64, ShareRepositoryError> {
    let mut reaped: u64 = 0;
    for share in shares.find_reapable(now, SHARES_PER_PASS).await? {
        if let Some(key) = share.storage.object_key()
            && !remove_object(storage, key).await
        {
            // Leave the row saying `object`: it is the only record of what to
            // delete, and the next pass will find it again. Marking it `gone`
            // here would strand those bytes for good.
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
        // Usually already `gone` by now — `reap_dead_content` runs first — but a
        // disabled share reaches retention with its object intact, and so does
        // anything the reaper could not delete earlier.
        if let Some(key) = share.storage.object_key()
            && !remove_object(storage, key).await
        {
            // The row stays. It is the only record of which object to remove, so
            // dropping it now would strand those bytes for good; the next pass
            // tries again.
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
