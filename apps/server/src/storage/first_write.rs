//! Holds back concurrent first writes into a location that does not exist yet.
//!
//! Object stores have no directories, so for most backends this does nothing
//! anybody would notice. WebDAV does have them, and it does not create them on
//! `PUT`: the write answers `409`, and `object_store` reacts by walking up with
//! MKCOL and retrying — *once*. Several uploads arriving together into a fresh
//! backend each hit the `409`, each start creating `shares/YYYY/MM`, and the one
//! whose single retry lands while a sibling's MKCOL is still in flight has
//! nothing left to try and fails.
//!
//! Measured against `rclone serve webdav`: run in parallel a four-test suite was
//! reliably flaky, and one CI run saw three of four uploads fail then pass on
//! retry. Serialising just the first write into each location makes it
//! deterministic, and costs nothing after that — the second upload into the same
//! month takes no lock at all.
//!
//! Deliberately not restricted to the schemes that need it. The check is one
//! hash lookup on a set that never holds more than a handful of entries, and a
//! flag saying "this store has directories" would be a third capability bit to
//! keep honest — see [`Stores`](crate::storage::Stores), which already carries
//! two that are easy to confuse.

use std::collections::HashSet;

/// The locations already written to, and the gate that keeps the first write
/// into a new one alone.
///
/// Forgetting is safe and reloading the backends throws this away: the cost of a
/// forgotten location is one serialised write, not a wrong answer.
#[derive(Debug, Default)]
pub struct FirstWrites {
    /// `backend\0prefix` for each location known to exist.
    known: parking_lot::Mutex<HashSet<String>>,
    /// Held for the duration of a first write.
    ///
    /// One gate rather than one per location: contention needs two uploads into
    /// two *different* new locations at the same moment, which happens at most
    /// once a month per backend, and a map of mutexes would have to be reaped.
    gate: tokio::sync::Mutex<()>,
}

impl FirstWrites {
    /// Runs `write`, alone if nothing has been written to this location yet.
    ///
    /// The location is remembered only when the write succeeds, so a failure
    /// does not convince the next upload that the directory is there.
    ///
    /// # Errors
    ///
    /// Whatever `write` returns.
    pub async fn once<T, E, F>(&self, backend: &str, prefix: &str, write: F) -> Result<T, E>
    where
        F: Future<Output = Result<T, E>>,
    {
        let location = format!("{backend}\0{prefix}");
        if self.contains(&location) {
            return write.await;
        }

        let gate = self.gate.lock().await;
        // Checked again under the gate: several uploads can pass the test above
        // together, and without this every one of them would take the gate in
        // turn and write on its own.
        if self.contains(&location) {
            drop(gate);
            return write.await;
        }

        let written = write.await;
        if written.is_ok() {
            self.remember(location);
        }
        drop(gate);
        written
    }

    /// Whether this location has already been written to successfully.
    ///
    /// Public so a test can tell "the write went through the gate" from "the
    /// write happened to work".
    #[must_use]
    pub fn knows(&self, backend: &str, prefix: &str) -> bool {
        self.contains(&format!("{backend}\0{prefix}"))
    }

    fn contains(&self, location: &str) -> bool {
        self.known.lock().contains(location)
    }

    fn remember(&self, location: String) {
        self.known.lock().insert(location);
    }
}

/// Everything in `key` up to its last `/`, which is the location the object
/// lands in.
///
/// `shares/2026/08/01ABC` is written into `shares/2026/08`. A key with no `/`
/// has no location to create.
#[must_use]
pub fn location_of(key: &str) -> &str {
    key.rsplit_once('/').map_or("", |(prefix, _)| prefix)
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    #[rstest::rstest]
    #[case::sharded("shares/2026/08/01ABC", "shares/2026/08")]
    #[case::one_level("shares/01ABC", "shares")]
    #[case::bare("01ABC", "")]
    fn a_key_names_the_location_it_lands_in(#[case] key: &str, #[case] expected: &str) {
        assert_eq!(location_of(key), expected);
    }

    /// The property the whole type exists for: exactly one write runs while the
    /// location does not exist yet.
    ///
    /// Not "no two writes ever overlap" — that would be a global write lock and
    /// the wrong thing entirely. Once the location is there the rest run
    /// together, which is what `a_known_location_is_not_gated` covers. What must
    /// not happen twice is the write that creates it, because on WebDAV that
    /// write is a MKCOL walk and two of them racing is the whole bug.
    #[tokio::test]
    async fn only_one_write_runs_while_the_location_is_unknown() {
        let first = Arc::new(FirstWrites::default());
        let creating = Arc::new(AtomicUsize::new(0));

        let mut writing = Vec::new();
        for _ in 0..8 {
            let (first, creating) = (Arc::clone(&first), Arc::clone(&creating));
            let gated = Arc::clone(&first);
            writing.push(tokio::spawn(async move {
                gated
                    .once("default", "shares/2026/08", async {
                        if !first.knows("default", "shares/2026/08") {
                            creating.fetch_add(1, Ordering::SeqCst);
                        }
                        // Long enough that a missing gate lets a sibling in.
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                        Ok::<(), ()>(())
                    })
                    .await
            }));
        }
        for one in writing {
            one.await.expect("the task").expect("the write");
        }

        assert_eq!(
            creating.load(Ordering::SeqCst),
            1,
            "more than one write ran before the location existed"
        );
    }

    /// A different location is a different question, and a known one costs
    /// nothing.
    #[tokio::test]
    async fn a_known_location_is_not_gated() {
        let first = FirstWrites::default();
        first
            .once("default", "shares/2026/08", async { Ok::<(), ()>(()) })
            .await
            .expect("the first write");

        let held = first.gate.lock().await;
        // Would deadlock if a known location still took the gate.
        first
            .once("default", "shares/2026/08", async { Ok::<(), ()>(()) })
            .await
            .expect("the second write");
        drop(held);
    }

    /// A failed write proves nothing about the location.
    #[tokio::test]
    async fn a_failed_write_is_not_remembered() {
        let first = FirstWrites::default();
        let _ = first
            .once("default", "shares/2026/08", async { Err::<(), ()>(()) })
            .await;
        assert!(
            !first.knows("default", "shares/2026/08"),
            "a location was remembered from a write that failed"
        );
    }
}
