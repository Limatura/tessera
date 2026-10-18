//! Changes announced inside the process, for the requests that stream them out
//! as server-sent events.
//!
//! A [`Topic`] is a broadcast channel with a bounded buffer, and nothing more.
//! A subscriber that falls behind is not buffered for: it misses what it could
//! not keep up with and is told so ([`RecvError::Lagged`]). So a slow or
//! stalled client never holds memory, and a subscriber is expected to read the
//! current state again after a lag rather than replay what it missed.

use tokio::sync::broadcast;
pub use tokio::sync::broadcast::error::RecvError;

/// How many announcements a subscriber may fall behind by before it misses
/// some. Generous for anything announced by hand; a subscriber that lags
/// still recovers by reading the state again.
const CAPACITY: usize = 256;

/// Announcements of `T`, delivered to whoever is subscribed at the time.
#[derive(Debug, Clone)]
pub struct Topic<T> {
    sender: broadcast::Sender<T>,
}

impl<T: Clone> Topic<T> {
    /// A topic nobody is subscribed to yet.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(CAPACITY)
    }

    /// A topic whose subscribers may fall `capacity` announcements behind,
    /// for one announced far more often than by hand.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            sender: broadcast::channel(capacity).0,
        }
    }

    /// Announces `value` to every current subscriber. Nobody listening is the
    /// usual case, not an error.
    pub fn publish(&self, value: T) {
        drop(self.sender.send(value));
    }

    /// Everything announced from now on.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<T> {
        self.sender.subscribe()
    }
}

impl<T: Clone> Default for Topic<T> {
    fn default() -> Self {
        Self::new()
    }
}
