//! Whether the scanner is answering, as last seen.
//!
//! Learned from the scans themselves: every verdict means it answered, and
//! every [`Unreachable`](super::Unreachable) other than a file that could not
//! be read means it did not. Only when nothing has been sent to it for a while
//! does the worker ask directly, with an empty file: the one request both
//! kinds of scanner already answer, so neither needs anything new.
//!
//! Tied to the scanner's URL: pointing `scan.url` somewhere else starts over,
//! instead of showing the old scanner's state as the new one's.
//!
//! Kept in memory only. After a restart nothing is known until the worker's
//! first pass, a moment after start-up.

use std::{sync::Arc, time::Duration};

use parking_lot::Mutex;
use time::OffsetDateTime;

/// How long the scanner may go without a scan before it is asked directly.
pub const IDLE: Duration = Duration::from_mins(15);

/// How long after a failure it is asked again, when no scan asks first.
pub const RETRY: Duration = Duration::from_mins(5);

/// How the last call to the scanner went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    /// When.
    pub at: OffsetDateTime,
    /// Why it failed, or `None` when it answered.
    pub error: Option<String>,
}

/// The latest [`Seen`], and the scanner URL it is about.
#[derive(Debug, Clone, Default)]
pub struct ScannerHealth {
    last: Arc<Mutex<Option<(String, Seen)>>>,
}

impl ScannerHealth {
    /// What was last seen of the scanner at `url`, or `None` before
    /// anything was.
    #[must_use]
    pub fn get(&self, url: &str) -> Option<Seen> {
        self.last
            .lock()
            .as_ref()
            .filter(|(seen_url, _)| seen_url == url)
            .map(|(_, seen)| seen.clone())
    }

    /// Notes how a call to the scanner at `url` just went.
    pub fn note(&self, url: &str, error: Option<String>) {
        *self.last.lock() = Some((
            url.to_owned(),
            Seen {
                at: OffsetDateTime::now_utc(),
                error,
            },
        ));
    }

    /// Whether the scanner at `url` is due to be asked directly at `now`:
    /// never seen, unused for [`IDLE`], or failing for [`RETRY`].
    #[must_use]
    pub fn due(&self, url: &str, now: OffsetDateTime) -> bool {
        self.get(url).is_none_or(|seen| {
            let since = now - seen.at;
            since >= if seen.error.is_some() { RETRY } else { IDLE }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_seen_is_due() {
        assert!(ScannerHealth::default().due("clamd://a", OffsetDateTime::now_utc()));
    }

    #[test]
    fn a_recent_answer_is_not_due_until_idle() {
        let health = ScannerHealth::default();
        health.note("clamd://a", None);
        let now = OffsetDateTime::now_utc();
        assert!(!health.due("clamd://a", now));
        assert!(!health.due("clamd://a", now + RETRY));
        assert!(health.due("clamd://a", now + IDLE));
    }

    #[test]
    fn a_failure_is_asked_again_sooner() {
        let health = ScannerHealth::default();
        health.note("clamd://a", Some("refused".to_owned()));
        assert!(health.due("clamd://a", OffsetDateTime::now_utc() + RETRY));
    }

    #[test]
    fn another_scanner_starts_over() {
        let health = ScannerHealth::default();
        health.note("clamd://a", Some("refused".to_owned()));
        assert_eq!(health.get("clamd://b"), None);
    }
}
