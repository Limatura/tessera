//! Drawn cards, kept for as long as they are worth keeping.
//!
//! # What this is for
//!
//! `/s/{code}/og.png` is public and unthrottled, and the number of share links
//! on an instance only ever goes up. Without somewhere to put a finished card,
//! every visit to every link is a fresh render, and the ones that arrive in a
//! burst are all the same picture: a link posted in a channel is fetched by
//! every unfurler that saw it.
//!
//! An entity tag already answers a crawler that *comes back*. This answers the
//! several that arrive at once, and the same crawler after its own copy has
//! expired.
//!
//! # Why the key is the entity tag
//!
//! Because that tag is already a digest of everything the drawing reads: the
//! words, the retrieval code, and the operator's layout. Two requests with the
//! same tag would draw byte-identical pictures, so one entry serves both, and
//! anything that would change the picture changes the key by construction.
//!
//! It also means there is nothing to invalidate. A card that is counting down
//! keeps its tag for as long as its sentence holds, and gets a new one, and a
//! new entry, exactly when the picture would have changed. The old entry is
//! then simply the least recently used thing here.
//!
//! # Why the budget is in bytes
//!
//! A count of entries says nothing about memory when an entry is a PNG, and an
//! instance holding a hundred thousand shares must not turn that into a
//! hundred thousand pictures in memory. The budget is what the cache may
//! occupy; how many cards that is depends on how big they are.

use std::collections::{BTreeMap, HashMap};

use bytes::Bytes;
use parking_lot::Mutex;

/// How much memory drawn cards may occupy.
///
/// A card is 1200 by 630 and encodes to somewhere between three and thirty
/// kilobytes, so this is a few thousand of them: comfortably more than the
/// links a single burst touches, and small enough to be unremarkable next to
/// what an upload buffer costs.
///
/// Not a setting. It is a number an operator has no way to choose better than
/// this one, and every setting is one more thing to understand before running
/// the software.
pub const BUDGET_BYTES: usize = 64 * 1024 * 1024;

/// A card too large to be worth keeping.
///
/// Nothing near a real card, and the point is that a single pathological entry
/// cannot evict everything else on its way in.
const LARGEST_ENTRY_BYTES: usize = BUDGET_BYTES / 16;

/// Drawn cards, keyed by entity tag.
#[derive(Debug)]
pub struct Cards {
    inner: Mutex<Inner>,
    budget: usize,
}

#[derive(Debug, Default)]
struct Inner {
    /// The cards, and when each was last wanted.
    entries: HashMap<String, (Bytes, u64)>,
    /// The same keys ordered by when they were last wanted, so eviction does
    /// not have to look at every entry to find the oldest.
    recency: BTreeMap<u64, String>,
    /// Ticks once per read and once per write. A counter and not a clock: two
    /// cards wanted in the same microsecond still order, and there is no
    /// question of what happens when the clock steps backwards.
    tick: u64,
    /// What `entries` currently occupies.
    held: usize,
}

impl Default for Cards {
    fn default() -> Self {
        Self::with_budget(BUDGET_BYTES)
    }
}

impl Cards {
    /// A cache holding at most `budget` bytes of drawn cards.
    #[must_use]
    pub fn with_budget(budget: usize) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            budget,
        }
    }

    /// The card for this tag, if one is held.
    #[must_use]
    pub fn get(&self, etag: &str) -> Option<Bytes> {
        let mut inner = self.inner.lock();
        let (png, was) = inner
            .entries
            .get(etag)
            .map(|(png, was)| (png.clone(), *was))?;

        // Move it to the front. Both halves, or `recency` grows a stale key
        // that names an entry which is no longer where it says.
        inner.tick += 1;
        let now = inner.tick;
        inner.recency.remove(&was);
        inner.recency.insert(now, etag.to_owned());
        if let Some(entry) = inner.entries.get_mut(etag) {
            entry.1 = now;
        }
        Some(png)
    }

    /// Keeps this card, evicting the least recently wanted until it fits.
    pub fn insert(&self, etag: &str, png: Bytes) {
        if png.len() > LARGEST_ENTRY_BYTES || png.len() > self.budget {
            return;
        }

        let mut inner = self.inner.lock();
        inner.tick += 1;
        let now = inner.tick;

        // Drawn twice at once is ordinary: two requests for a card nobody holds
        // yet both miss and both render. The second insert replaces the first,
        // and its bytes have to leave the accounting with it.
        if let Some((old, was)) = inner.entries.remove(etag) {
            inner.held -= old.len();
            inner.recency.remove(&was);
        }

        inner.held += png.len();
        inner.entries.insert(etag.to_owned(), (png, now));
        inner.recency.insert(now, etag.to_owned());

        while inner.held > self.budget {
            let Some((oldest, key)) = inner.recency.iter().next().map(|(k, v)| (*k, v.clone()))
            else {
                break;
            };
            inner.recency.remove(&oldest);
            if let Some((evicted, _)) = inner.entries.remove(&key) {
                inner.held -= evicted.len();
            }
        }
    }

    /// What the cache currently occupies, for tests and for a log line.
    #[must_use]
    pub fn held_bytes(&self) -> usize {
        self.inner.lock().held
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(size: usize) -> Bytes {
        Bytes::from(vec![7_u8; size])
    }

    #[test]
    fn a_card_comes_back() {
        let cards = Cards::with_budget(1024);
        cards.insert("\"a\"", card(10));
        assert_eq!(cards.get("\"a\"").map(|png| png.len()), Some(10));
        assert!(cards.get("\"b\"").is_none());
    }

    /// The reason the budget is in bytes: what is held has to stay under it
    /// however many entries that turns out to be.
    #[test]
    fn the_budget_is_never_exceeded() {
        let cards = Cards::with_budget(100);
        for i in 0..50 {
            cards.insert(&format!("\"{i}\""), card(30));
            assert!(
                cards.held_bytes() <= 100,
                "held {} after {i}",
                cards.held_bytes()
            );
        }
    }

    /// Least *recently used*, not least recently written. A card that keeps
    /// being asked for is the one worth keeping.
    #[test]
    fn what_is_still_being_asked_for_survives() {
        let cards = Cards::with_budget(100);
        cards.insert("\"kept\"", card(40));
        cards.insert("\"cold\"", card(40));

        // Asking for it moves it ahead of the one written after it.
        assert!(cards.get("\"kept\"").is_some());

        cards.insert("\"new\"", card(40));

        assert!(
            cards.get("\"kept\"").is_some(),
            "the wanted card was evicted"
        );
        assert!(cards.get("\"cold\"").is_none(), "the cold card survived");
    }

    /// Writing the same tag twice is ordinary: two requests for a card nobody
    /// holds yet both miss and both draw. The accounting has to survive it, or
    /// the cache slowly believes it holds more than it does and evicts
    /// everything.
    #[test]
    fn writing_the_same_tag_twice_does_not_leak_the_accounting() {
        let cards = Cards::with_budget(1000);
        for _ in 0..20 {
            cards.insert("\"same\"", card(50));
        }
        assert_eq!(cards.held_bytes(), 50);
        assert_eq!(cards.get("\"same\"").map(|png| png.len()), Some(50));
    }

    /// One enormous entry must not be able to empty the cache on its way in.
    #[test]
    fn something_far_too_large_is_not_kept() {
        let cards = Cards::with_budget(BUDGET_BYTES);
        cards.insert("\"small\"", card(10));
        cards.insert("\"huge\"", card(LARGEST_ENTRY_BYTES + 1));

        assert!(cards.get("\"huge\"").is_none());
        assert!(cards.get("\"small\"").is_some(), "the huge card evicted it");
    }
}
