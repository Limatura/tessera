//! Works through what the scanner has yet to check.
//!
//! Woken on upload, and every thirty seconds regardless, which is how files
//! uploaded while the scanner was down get scanned once it is back.
//!
//! # When the scanner is unreachable
//!
//! The current file is marked `deferred`, along with everything else still
//! `pending`, and all of it is served from then on instead of waiting on a
//! missing scanner. Each later pass retries the deferred ones, stopping at the
//! first that still gets no answer.

use std::time::Duration;

use bytes::Bytes;
use http_body_util::BodyDataStream;
use time::OffsetDateTime;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::{Change, Unreachable, Verdict};
use crate::{
    db::repository::{
        scan::{Job, Scan, Skip},
        share::Content,
    },
    state::AppState,
};

/// How long one file may take, from connecting to the verdict. Generous: a
/// scanner reading a few gigabytes is slow but not broken.
pub const SCAN_TIMEOUT: Duration = Duration::from_mins(10);

/// How long connecting may take before the scanner counts as unreachable.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// How often the queue is checked when nothing has woken it.
const INTERVAL: Duration = Duration::from_secs(30);

/// How many files are with the scanner at once, so one large file does not
/// hold up every small one behind it.
const PARALLEL: usize = 4;

/// How many queued items are read at a time.
const BATCH: u64 = 32;

/// Scans until `shutdown` is cancelled.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    // Assumed on at start, so a process starting with scanning off releases
    // whatever an earlier configuration left pending.
    let mut was_enabled = true;
    loop {
        let enabled = state.config().scan.is_enabled();
        if was_enabled && !enabled {
            // Nothing is left to deliver a verdict, so nothing waits for one.
            defer_the_rest(&state).await;
        }
        was_enabled = enabled;
        run_once(&state).await;
        tokio::select! {
            () = shutdown.cancelled() => return,
            () = state.scan_wake.notified() => {}
            () = tokio::time::sleep(INTERVAL) => {}
        }
    }
}

/// One pass: everything pending, then a batch of what was deferred.
///
/// Public so tests can drive the scanner one step at a time instead of
/// waiting on a timer.
pub async fn run_once(state: &AppState) {
    if !state.config().scan.is_enabled() {
        return;
    }
    // Pending first: those are the shares someone is waiting on.
    loop {
        let jobs = match state.scans.in_state(&Scan::Pending, BATCH).await {
            Ok(jobs) if !jobs.is_empty() => jobs,
            Ok(_) => break,
            Err(error) => {
                tracing::error!(%error, "could not read the scanner's queue");
                return;
            }
        };
        if !work_through(state, jobs).await {
            defer_the_rest(state).await;
            return;
        }
    }
    match state.scans.in_state(&Scan::Deferred, BATCH).await {
        Ok(jobs) => {
            work_through(state, jobs).await;
        }
        Err(error) => tracing::error!(%error, "could not read the scanner's queue"),
    }
    ask_if_due(state).await;
}

/// Sends the scanner an empty file when no scan has said for a while whether
/// it answers. See [`super::health`].
async fn ask_if_due(state: &AppState) {
    let config = state.config();
    let scan = &config.scan;
    if !state
        .scanner_health
        .due(&scan.url, OffsetDateTime::now_utc())
    {
        return;
    }
    let nothing = tokio_stream::empty::<Result<Bytes, std::convert::Infallible>>();
    let error = super::scan(scan, nothing, 0)
        .await
        .err()
        .map(|error| error.to_string());
    if let Some(error) = &error {
        tracing::warn!(error, "the malware scanner is not answering");
    }
    state.scanner_health.note(&scan.url, error);
}

/// Scans `jobs`, a few at a time. `false` once the scanner stops answering.
async fn work_through(state: &AppState, jobs: Vec<Job>) -> bool {
    let mut running = JoinSet::new();
    let mut jobs = jobs.into_iter();
    let mut reachable = true;
    loop {
        while reachable && running.len() < PARALLEL {
            let Some(job) = jobs.next() else { break };
            let state = state.clone();
            running.spawn(async move {
                let outcome = check(&state, &job).await;
                (job, outcome)
            });
        }
        let Some(finished) = running.join_next().await else {
            return reachable;
        };
        let Ok((job, outcome)) = finished else {
            continue;
        };
        let url = &state.config().scan.url;
        match &outcome {
            Ok(_) => state.scanner_health.note(url, None),
            Err(Unreachable::Store(_)) => {}
            Err(error) => state.scanner_health.note(url, Some(error.to_string())),
        }
        match outcome {
            Ok(verdict) => settle(state, &job, verdict).await,
            // The file could not be read back; the scanner is fine. Served like
            // anything else that could not be scanned, and retried later.
            Err(Unreachable::Store(reason)) => {
                tracing::warn!(share = job.share_id(), %reason, "could not read a file to scan it");
                record(state, &job, &Scan::Deferred).await;
            }
            Err(error) => {
                if reachable {
                    tracing::warn!(%error, "the malware scanner is unavailable; files are served unscanned until it is back");
                }
                reachable = false;
                record(state, &job, &Scan::Deferred).await;
            }
        }
    }
}

/// Defers everything still pending, once the scanner has stopped answering.
async fn defer_the_rest(state: &AppState) {
    match state.scans.defer_all_pending().await {
        Ok(0) => {}
        Ok(_) => state.scan_changes.publish(Change::All),
        Err(error) => {
            tracing::error!(%error, "could not release the shares waiting on the scanner");
        }
    }
}

async fn settle(state: &AppState, job: &Job, verdict: Verdict) {
    let scan = match verdict {
        Verdict::Clean => Scan::Clean,
        Verdict::TooLarge => Scan::Skipped(Skip::TooLarge),
        Verdict::Infected(threat) => {
            tracing::warn!(share = job.share_id(), %threat, "the scanner found a threat; the share is switched off");
            if let Err(error) = state
                .scans
                .disable_share(job.share_id(), OffsetDateTime::now_utc())
                .await
            {
                tracing::error!(%error, share = job.share_id(), "could not switch off an infected share");
            }
            Scan::Infected(threat)
        }
    };
    record(state, job, &scan).await;
}

async fn record(state: &AppState, job: &Job, scan: &Scan) {
    match state.scans.record(job, scan).await {
        Ok(()) => state
            .scan_changes
            .publish(Change::Share(job.share_id().to_owned())),
        Err(error) => {
            tracing::error!(%error, share = job.share_id(), "could not record a scan");
        }
    }
}

/// Reads one job's content back and sends it to the scanner.
async fn check(state: &AppState, job: &Job) -> Result<Verdict, Unreachable> {
    let config = state.config().scan.clone();
    let share_id = tessera_core::ShareId::parse(job.share_id())
        .map_err(|error| Unreachable::Store(error.to_string()))?;
    let unreadable = |error: &dyn std::fmt::Display| Unreachable::Store(error.to_string());
    let share = state
        .shares
        .find_by_id(share_id)
        .await
        .map_err(|error| unreadable(&error))?
        .ok_or_else(|| Unreachable::Store("the share is gone".to_owned()))?;
    match job {
        Job::Text { .. } => {
            let text = state
                .shares
                .text_of(share.id)
                .await
                .map_err(|error| unreadable(&error))?
                .ok_or_else(|| Unreachable::Store("the note is gone".to_owned()))?;
            let length = u64::try_from(text.content.len()).unwrap_or(u64::MAX);
            let body =
                tokio_stream::once(Ok::<_, std::convert::Infallible>(Bytes::from(text.content)));
            super::scan(&config, body, length).await
        }
        Job::File { position, .. } => {
            let content = state
                .shares
                .content_of(&share)
                .await
                .map_err(|error| unreadable(&error))?;
            let Content::Files(files) = content else {
                return Err(Unreachable::Store("the file is gone".to_owned()));
            };
            let file = files
                .get(*position)
                .ok_or_else(|| Unreachable::Store("the file is gone".to_owned()))?;
            let body = crate::api::public::stream_one(state, file, None)
                .await
                .map_err(|error| unreadable(&error))?;
            let stream: BodyDataStream<axum::body::Body> = BodyDataStream::new(body);
            super::scan(&config, stream, file.size_bytes).await
        }
    }
}
