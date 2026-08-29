//! The bytes that are arriving right now, which no table knows about yet.
//!
//! # The hole this fills
//!
//! Both ceilings an upload is checked against, the quota for its scope and the
//! room left on its backend, are read from the database once before the body is
//! read and then compared against a running count. Two uploads arriving
//! together read the *same* figure and both believe the whole remainder is
//! theirs.
//!
//! Measured before this existed: an instance capped at 50 KiB, two concurrent
//! 40 KiB uploads, both answered `201`. Eighty kilobytes stored against a fifty
//! kilobyte ceiling, from two requests. A free-tier bucket does not fill up
//! from one enormous file; it fills up because several people sent ordinary
//! ones at the same moment.
//!
//! Resumable uploads were already covered: a `tus` session writes an `uploads`
//! row carrying `declared_bytes`, and `reserved_bytes` counts it. A plain
//! `POST /shares` writes no such row, since a chunked body has nothing to
//! declare, so nothing is reserved on its behalf.
//!
//! # Why a counter in the process
//!
//! The same reasoning as `first_write`: tessera is one process, and a number
//! held in it is exact for every upload that process is serving. A second
//! instance sharing a bucket would not see these bytes. That is the honest
//! limit of this and why the database figures remain the base. This is added to
//! them, never a substitute.
//!
//! Reserving the declared size up front would be the alternative, and it cannot
//! work here: a chunked body declares nothing, so there would be nothing to
//! reserve until the bytes had already arrived.
//!
//! # What it does not do
//!
//! It does not pick a winner. Counting as the bytes land is symmetric, so two
//! uploads crossing the line at the same moment both stop: thirty kilobytes
//! each into fifty kilobytes of room, and neither finishes. The room is then
//! free for whoever retries first.
//!
//! Deliberate, not unnoticed. Choosing a winner means claiming room before it
//! is used, and the size to claim is exactly what a chunked body will not say.
//! Refusing both is merely wasteful; letting both through leaves a bucket over
//! its quota, which on a free tier retrying does not fix.

use std::{collections::HashMap, sync::Arc};

/// Bytes in flight, by whatever key the caller counts them under.
///
/// Keys are opaque strings so the same ledger serves both questions: a quota
/// scope (`user:01ABC`, `role:01DEF`, `instance`) and a backend name.
#[derive(Debug, Default)]
pub struct InFlight {
    counts: parking_lot::Mutex<HashMap<String, u64>>,
}

impl InFlight {
    /// Starts counting for one upload.
    ///
    /// The guard adds to every key it was opened with and takes all of it back
    /// when dropped, on the error path as much as the happy one. That is what
    /// makes a refused upload stop holding room it never used.
    #[must_use]
    pub fn begin(self: &Arc<Self>, keys: Vec<String>) -> Arrival {
        Arrival {
            ledger: Arc::clone(self),
            keys,
            counted: 0,
        }
    }

    /// What everything *else* is currently holding under `key`.
    ///
    /// Excluding `mine` matters: the caller is already comparing its
    /// own running total against the ceiling, and counting those bytes twice
    /// would refuse an upload for competing with itself.
    #[must_use]
    pub fn elsewhere(&self, key: &str, mine: u64) -> u64 {
        self.counts
            .lock()
            .get(key)
            .copied()
            .unwrap_or(0)
            .saturating_sub(mine)
    }

    fn add(&self, keys: &[String], bytes: u64) {
        let mut counts = self.counts.lock();
        for key in keys {
            let entry = counts.entry(key.clone()).or_insert(0);
            *entry = entry.saturating_add(bytes);
        }
    }

    fn take_back(&self, keys: &[String], bytes: u64) {
        let mut counts = self.counts.lock();
        for key in keys {
            if let Some(entry) = counts.get_mut(key) {
                *entry = entry.saturating_sub(bytes);
                if *entry == 0 {
                    counts.remove(key);
                }
            }
        }
    }
}

/// One upload's share of the ledger.
///
/// Dropping it gives every byte back, so a body that failed halfway stops
/// counting against the next request the moment the handler returns.
#[derive(Debug)]
pub struct Arrival {
    ledger: Arc<InFlight>,
    keys: Vec<String>,
    counted: u64,
}

impl Arrival {
    /// Records that `total` bytes have now arrived for this upload.
    ///
    /// Takes the running total, not the delta, because that is what the caller
    /// already has, and because a missed call then costs accuracy rather
    /// than leaving the ledger permanently wrong.
    pub fn reached(&mut self, total: u64) {
        let more = total.saturating_sub(self.counted);
        if more > 0 {
            self.ledger.add(&self.keys, more);
            self.counted = total;
        }
    }

    /// Adds a key to count under, from the moment a backend is chosen.
    ///
    /// A backend is picked partway through the body, so its name is not known
    /// when the guard opens. Everything counted so far is added to it, because
    /// those bytes are going there too.
    pub fn also_count_under(&mut self, key: String) {
        if self.keys.contains(&key) {
            return;
        }
        self.ledger.add(std::slice::from_ref(&key), self.counted);
        self.keys.push(key);
    }
}

impl Drop for Arrival {
    fn drop(&mut self) {
        self.ledger.take_back(&self.keys, self.counted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_one_upload_holds_is_visible_to_another() {
        let ledger = Arc::new(InFlight::default());
        let mut first = ledger.begin(vec!["instance".to_owned()]);
        first.reached(4096);

        assert_eq!(ledger.elsewhere("instance", 0), 4096);
    }

    /// The upload asking must not be told to compete with itself.
    #[test]
    fn an_upload_does_not_count_against_itself() {
        let ledger = Arc::new(InFlight::default());
        let mut only = ledger.begin(vec!["instance".to_owned()]);
        only.reached(4096);

        assert_eq!(ledger.elsewhere("instance", 4096), 0);
    }

    /// A failed upload stops holding room. Without this every refusal would
    /// shrink the instance a little more until it restarted.
    #[test]
    fn dropping_gives_every_byte_back() {
        let ledger = Arc::new(InFlight::default());
        {
            let mut abandoned = ledger.begin(vec!["instance".to_owned()]);
            abandoned.reached(8192);
            assert_eq!(ledger.elsewhere("instance", 0), 8192);
        }
        assert_eq!(ledger.elsewhere("instance", 0), 0);
    }

    /// The backend is chosen partway through, and the bytes already read are
    /// going to it too.
    #[test]
    fn a_key_added_late_gets_what_came_before_it() {
        let ledger = Arc::new(InFlight::default());
        let mut arrival = ledger.begin(vec!["instance".to_owned()]);
        arrival.reached(1000);
        arrival.also_count_under("cold".to_owned());

        assert_eq!(ledger.elsewhere("cold", 0), 1000);
        arrival.reached(1500);
        assert_eq!(ledger.elsewhere("cold", 0), 1500);
    }

    /// Several at once add up, the arithmetic the ceiling check needs.
    #[test]
    fn several_uploads_add_up() {
        let ledger = Arc::new(InFlight::default());
        let mut a = ledger.begin(vec!["instance".to_owned()]);
        let mut b = ledger.begin(vec!["instance".to_owned()]);
        a.reached(1000);
        b.reached(2000);

        assert_eq!(ledger.elsewhere("instance", 1000), 2000);
        assert_eq!(ledger.elsewhere("instance", 2000), 1000);
    }
}
