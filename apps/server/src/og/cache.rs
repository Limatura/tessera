//! Rendered cards, kept while they are worth keeping.
//!
//! # Purpose
//!
//! `/s/{code}/og.png` is public and unthrottled, and the number of share links
//! only grows. Without a place for finished cards, every visit to every link
//! is a fresh render, and bursts are all the same image: a link posted in a
//! channel is fetched by every unfurler that saw it.
//!
//! An entity tag already handles a crawler that *comes back*. This handles
//! several arriving at once, and the same crawler after its own copy expired.
//!
//! # Why the key is the entity tag
//!
//! The tag is already a digest of everything the drawing reads: the text, the
//! retrieval code and the operator's layout. Two requests with the same tag
//! would draw identical images, so one entry serves both, and anything that
//! changes the image changes the key.
//!
//! So nothing needs invalidating. A counting-down card keeps its tag while its
//! text holds, and gets a new tag and entry when the image would change. The
//! old entry then becomes the least recently used.
//!
//! # Why the budget is in bytes
//!
//! An entry count says nothing about memory when entries are PNGs, and an
//! instance with a hundred thousand shares must not hold a hundred thousand
//! images in memory. The budget caps the cache's memory; how many cards fit
//! depends on their size.

use std::collections::{BTreeMap, HashMap};

use bytes::Bytes;
use parking_lot::Mutex;

/// How much memory rendered cards may occupy.
///
/// A card is 1200×630 and encodes to 3 to 30 KB, so this holds a few thousand:
/// comfortably more than one burst touches, and small next to an upload
/// buffer.
///
/// Not a setting. An operator has no better way to pick this number, and every
/// setting is one more thing to understand before running the software.
pub const BUDGET_BYTES: usize = 64 * 1024 * 1024;

/// A card too large to be worth keeping.
///
/// Far above any real card; it stops a single pathological entry from evicting
/// everything else on the way in.
const LARGEST_ENTRY_BYTES: usize = BUDGET_BYTES / 16;

/// Drawn cards, keyed by entity tag.
#[derive(Debug)]
pub struct Cards {
    inner: Mutex<Inner>,
    budget: usize,
}

#[derive(Debug, Default)]
struct Inner {
    /// The cards, and when each was last used.
    entries: HashMap<String, (Bytes, u64)>,
    /// The same keys ordered by last use, so eviction can find the oldest
    /// without scanning every entry.
    recency: BTreeMap<u64, String>,
    /// Ticks once per read and once per write. A counter, not a clock: two
    /// cards used in the same microsecond still order, and a clock stepping
    /// backwards is not a concern.
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

        // Move it to the front. Update both, or `recency` keeps a stale key
        // pointing at an entry that has moved.
        inner.tick += 1;
        let now = inner.tick;
        inner.recency.remove(&was);
        inner.recency.insert(now, etag.to_owned());
        if let Some(entry) = inner.entries.get_mut(etag) {
            entry.1 = now;
        }
        Some(png)
    }

    /// Keeps this card, evicting the least recently used until it fits.
    pub fn insert(&self, etag: &str, png: Bytes) {
        if png.len() > LARGEST_ENTRY_BYTES || png.len() > self.budget {
            return;
        }

        let mut inner = self.inner.lock();
        inner.tick += 1;
        let now = inner.tick;

        // Rendering twice at once is normal: two requests for an uncached card
        // both miss and both render. The second insert replaces the first, and
        // the old bytes must leave the accounting with it.
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

    /// Why the budget is in bytes: what is held must stay under it however many
    /// entries that is.
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
    /// being requested is the one worth keeping.
    #[test]
    fn what_is_still_being_asked_for_survives() {
        let cards = Cards::with_budget(100);
        cards.insert("\"kept\"", card(40));
        cards.insert("\"cold\"", card(40));

        // Reading it moves it ahead of the one written after it.
        assert!(cards.get("\"kept\"").is_some());

        cards.insert("\"new\"", card(40));

        assert!(
            cards.get("\"kept\"").is_some(),
            "the wanted card was evicted"
        );
        assert!(cards.get("\"cold\"").is_none(), "the cold card survived");
    }

    /// Writing the same tag twice is normal: two requests for an uncached card
    /// both miss and both render. The accounting must survive it, or the cache
    /// gradually overcounts and evicts everything.
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
