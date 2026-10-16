//! Whether each storage backend is answering, as last seen.
//!
//! Mostly learned in passing. Every store in the pool is wrapped in an
//! observer that notes how each call to it went, so a backend in use is known
//! about from its own traffic, at no extra cost. Only one left idle is asked
//! directly, by [`watch`]: object stores bill per request, and a busy backend
//! needs no probe to say whether it works.
//!
//! Kept in memory only. After a restart nothing is known until [`watch`]'s
//! first pass, a moment after start-up.

use std::{collections::HashMap, ops::Range, pin::Pin, sync::Arc, time::Duration};

use bytes::Bytes;
use object_store::{
    CopyOptions, GetOptions, GetResult, ListResult, MultipartUpload, ObjectMeta, ObjectStore,
    ObjectStoreExt as _, PutMultipartOptions, PutOptions, PutPayload, PutResult, RenameOptions,
    Result, path::Path,
};
use parking_lot::Mutex;
use time::OffsetDateTime;
use tokio_stream::{Stream, StreamExt as _};
use tokio_util::sync::CancellationToken;

use crate::state::AppState;

/// How long a backend may go unused before [`watch`] asks it directly.
pub const IDLE: Duration = Duration::from_mins(15);

/// How often [`watch`] looks for idle backends.
const PASS: Duration = Duration::from_mins(5);

/// How long a probe may take before the backend counts as not answering.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// The prefix every object this instance writes is under, and so the one a
/// probe lists: permission to list it is already needed by the
/// reconciliation.
const PROBED_PREFIX: &str = "shares";

/// How much of a failure's message is kept. A store's error can carry a whole
/// response body.
const MAX_ERROR_CHARS: usize = 300;

/// How the last call to a backend went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    /// When.
    pub at: OffsetDateTime,
    /// Why it failed, or `None` when the backend answered.
    pub error: Option<String>,
}

/// One backend's latest observation, shared by its observer and whoever asks.
type Slot = Arc<Mutex<Option<Observation>>>;

/// Every backend's latest observation, by name.
///
/// Outlives the pool: a reload rebuilds every store, and what was learned
/// about a backend that did not change still holds.
#[derive(Debug, Clone, Default)]
pub struct Health {
    slots: Arc<Mutex<HashMap<String, Slot>>>,
}

impl Health {
    /// What was last seen of `name`, or `None` before anything was.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Observation> {
        self.slots
            .lock()
            .get(name)
            .and_then(|slot| slot.lock().clone())
    }

    /// Notes that `name` failed for a reason no call to it reported, such as
    /// a store that would not open.
    pub fn record_failure(&self, name: &str, error: &impl std::fmt::Display) {
        note(&self.slot(name), Some(shortened(error)));
    }

    /// `store`, noting how every call to it goes under `name`.
    #[must_use]
    pub fn observe(&self, name: &str, store: Arc<dyn ObjectStore>) -> Arc<dyn ObjectStore> {
        Arc::new(Observed {
            inner: store,
            slot: self.slot(name),
        })
    }

    /// Forgets every backend not in `names`, once it is removed.
    pub fn retain<'a>(&self, names: impl IntoIterator<Item = &'a str>) {
        let keep: std::collections::HashSet<&str> = names.into_iter().collect();
        self.slots
            .lock()
            .retain(|name, _| keep.contains(name.as_str()));
    }

    fn slot(&self, name: &str) -> Slot {
        Arc::clone(self.slots.lock().entry(name.to_owned()).or_default())
    }
}

/// Asks `name`'s `store` whether it answers, and notes the outcome.
///
/// A listing of `shares/` for every backend, plus, when `write` is
/// set, writing and removing a small object there: a bucket can be readable
/// with credentials that may not write, and new uploads need the write. An
/// object left behind by a failed removal is unreferenced, so the
/// reconciliation takes it.
///
/// # Errors
///
/// Why the backend did not answer, as noted.
pub async fn check(
    health: &Health,
    name: &str,
    store: &Arc<dyn ObjectStore>,
    write: bool,
) -> Result<(), String> {
    let prefix = Path::from(PROBED_PREFIX);
    let work = async {
        store.list_with_delimiter(Some(&prefix)).await?;
        if write {
            let key = prefix.clone().join(format!(
                ".probe-{}",
                ulid::Ulid::generate().to_string().to_ascii_lowercase()
            ));
            store.put(&key, PutPayload::from_static(b"tessera")).await?;
            store.delete(&key).await?;
        }
        Ok::<(), object_store::Error>(())
    };
    let outcome = match tokio::time::timeout(PROBE_TIMEOUT, work).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) if !is_failure(&error) => Ok(()),
        Ok(Err(error)) => Err(shortened(&error)),
        Err(_) => Err(format!(
            "no answer within {} seconds",
            PROBE_TIMEOUT.as_secs()
        )),
    };
    // The store noted each call already; this covers the timeout, which
    // no call returned.
    if let Err(error) = &outcome {
        health.record_failure(name, error);
    }
    outcome
}

/// Probes every open backend nothing has been seen of for [`IDLE`], or whose
/// last call failed, until `shutdown` is cancelled. The first pass is at
/// start-up, when nothing has been seen of any.
///
/// A failing one is asked every pass so it is shown as back soon after it
/// is, even with nothing else calling it.
///
/// Only listing, never writing: this runs unattended, and a backend being
/// read-only is not a fault.
#[expect(
    clippy::iter_over_hash_type,
    reason = "every probe runs at once; the order they start in means nothing"
)]
pub async fn watch(state: AppState, shutdown: CancellationToken) {
    loop {
        let pool = state.backends.load_full();
        let now = OffsetDateTime::now_utc();
        let mut probes = tokio::task::JoinSet::new();
        for (name, stores) in &pool.open {
            let fine = pool
                .health
                .get(name)
                .is_some_and(|seen| seen.error.is_none() && now - seen.at < IDLE);
            if !fine {
                let health = pool.health.clone();
                let store = Arc::clone(&stores.store);
                let name = name.clone();
                probes.spawn(async move {
                    if let Err(error) = check(&health, &name, &store, false).await {
                        tracing::warn!(backend = name, error, "a storage backend is not answering");
                    }
                });
            }
        }
        drop(pool);
        probes.join_all().await;

        tokio::select! {
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(PASS) => {}
        }
    }
}

/// Whether `error` means the backend did not answer, as opposed to answering
/// "no such object" or "that already exists".
///
/// Unknown kinds count as failures: a new variant is more likely another way
/// to fail than another way to succeed.
const fn is_failure(error: &object_store::Error) -> bool {
    !matches!(
        error,
        object_store::Error::NotFound { .. }
            | object_store::Error::AlreadyExists { .. }
            | object_store::Error::Precondition { .. }
            | object_store::Error::NotModified { .. }
            | object_store::Error::InvalidPath { .. }
            | object_store::Error::NotSupported { .. }
            | object_store::Error::NotImplemented { .. }
            | object_store::Error::UnknownConfigurationKey { .. }
    )
}

fn shortened(error: &impl std::fmt::Display) -> String {
    let message = error.to_string();
    if message.chars().count() <= MAX_ERROR_CHARS {
        return message;
    }
    let mut cut: String = message.chars().take(MAX_ERROR_CHARS).collect();
    cut.push('…');
    cut
}

fn note(slot: &Slot, error: Option<String>) {
    *slot.lock() = Some(Observation {
        at: OffsetDateTime::now_utc(),
        error,
    });
}

fn note_result<T>(slot: &Slot, result: &Result<T>) {
    match result {
        Err(error) if is_failure(error) => note(slot, Some(shortened(error))),
        _ => note(slot, None),
    }
}

type BoxStream<T> = Pin<Box<dyn Stream<Item = Result<T>> + Send + 'static>>;

/// A store that notes how each call to it went.
///
/// Calls that return at once are noted when they return. Streams are noted
/// item by item. What happens inside a body being read, or a multipart upload
/// already open, is not: the call that opened it has been noted, and the next
/// one will be.
#[derive(Debug)]
struct Observed {
    inner: Arc<dyn ObjectStore>,
    slot: Slot,
}

impl Observed {
    fn noted<T: 'static>(&self, stream: BoxStream<T>) -> BoxStream<T> {
        let slot = Arc::clone(&self.slot);
        Box::pin(stream.map(move |item| {
            note_result(&slot, &item);
            item
        }))
    }
}

impl std::fmt::Display for Observed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

#[async_trait::async_trait]
impl ObjectStore for Observed {
    async fn put_opts(
        &self,
        location: &Path,
        payload: PutPayload,
        opts: PutOptions,
    ) -> Result<PutResult> {
        let result = self.inner.put_opts(location, payload, opts).await;
        note_result(&self.slot, &result);
        result
    }

    async fn put_multipart_opts(
        &self,
        location: &Path,
        opts: PutMultipartOptions,
    ) -> Result<Box<dyn MultipartUpload>> {
        let result = self.inner.put_multipart_opts(location, opts).await;
        note_result(&self.slot, &result);
        result
    }

    async fn get_opts(&self, location: &Path, options: GetOptions) -> Result<GetResult> {
        let result = self.inner.get_opts(location, options).await;
        note_result(&self.slot, &result);
        result
    }

    async fn get_ranges(&self, location: &Path, ranges: &[Range<u64>]) -> Result<Vec<Bytes>> {
        let result = self.inner.get_ranges(location, ranges).await;
        note_result(&self.slot, &result);
        result
    }

    fn delete_stream(&self, locations: BoxStream<Path>) -> BoxStream<Path> {
        self.noted(self.inner.delete_stream(locations))
    }

    fn list(&self, prefix: Option<&Path>) -> BoxStream<ObjectMeta> {
        self.noted(self.inner.list(prefix))
    }

    fn list_with_offset(&self, prefix: Option<&Path>, offset: &Path) -> BoxStream<ObjectMeta> {
        self.noted(self.inner.list_with_offset(prefix, offset))
    }

    async fn list_with_delimiter(&self, prefix: Option<&Path>) -> Result<ListResult> {
        let result = self.inner.list_with_delimiter(prefix).await;
        note_result(&self.slot, &result);
        result
    }

    async fn copy_opts(&self, from: &Path, to: &Path, options: CopyOptions) -> Result<()> {
        let result = self.inner.copy_opts(from, to, options).await;
        note_result(&self.slot, &result);
        result
    }

    async fn rename_opts(&self, from: &Path, to: &Path, options: RenameOptions) -> Result<()> {
        let result = self.inner.rename_opts(from, to, options).await;
        note_result(&self.slot, &result);
        result
    }
}

#[cfg(test)]
mod tests {
    use object_store::memory::InMemory;

    use super::*;

    fn observed() -> (Health, Arc<dyn ObjectStore>) {
        let health = Health::default();
        let store = health.observe("cold", Arc::new(InMemory::new()));
        (health, store)
    }

    #[tokio::test]
    async fn nothing_is_known_before_a_call() {
        let (health, _store) = observed();
        assert_eq!(health.get("cold"), None);
    }

    #[tokio::test]
    async fn a_call_that_answers_is_noted() {
        let (health, store) = observed();
        store
            .put(&Path::from("a"), PutPayload::from_static(b"x"))
            .await
            .expect("put");
        assert_eq!(health.get("cold").expect("seen").error, None);
    }

    #[tokio::test]
    async fn a_missing_object_is_still_an_answer() {
        let (health, store) = observed();
        store
            .get(&Path::from("missing"))
            .await
            .expect_err("missing");
        assert_eq!(health.get("cold").expect("seen").error, None);
    }

    #[tokio::test]
    async fn a_failure_is_noted_until_a_call_answers() {
        let (health, store) = observed();
        health.record_failure("cold", &"would not open");
        assert_eq!(
            health.get("cold").expect("seen").error.as_deref(),
            Some("would not open")
        );

        assert_eq!(check(&health, "cold", &store, true).await, Ok(()));
        assert_eq!(health.get("cold").expect("seen").error, None);
    }

    #[tokio::test]
    async fn a_writing_probe_leaves_nothing_behind() {
        let (health, store) = observed();
        check(&health, "cold", &store, true).await.expect("probe");
        let left: Vec<_> = store.list(None).collect().await;
        assert!(left.is_empty(), "{left:?}");
    }

    #[test]
    fn a_long_message_is_cut() {
        let cut = shortened(&"x".repeat(MAX_ERROR_CHARS + 50));
        assert_eq!(cut.chars().count(), MAX_ERROR_CHARS + 1);
    }

    #[test]
    fn a_removed_backend_is_forgotten() {
        let health = Health::default();
        health.record_failure("cold", &"down");
        health.record_failure("hot", &"down");
        health.retain(["hot"]);
        assert_eq!(health.get("cold"), None);
        assert!(health.get("hot").is_some());
    }
}
