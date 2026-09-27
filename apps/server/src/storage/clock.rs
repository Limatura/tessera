//! How far a store's clock is from this one.
//!
//! # Why it matters
//!
//! A download redirected to a store carries a URL valid for fifteen seconds.
//! This server stamps it with its own time; the store judges the fifteen
//! seconds by its own. If the store's clock runs a minute ahead, the URL
//! expired forty-five seconds before it was made, and the user gets an error
//! page from another server with nothing here to explain it.
//!
//! A long lifetime would hide that, and make every URL copied out of a browser
//! valid for longer. So the difference is measured, and the lifetime extended
//! by exactly how far ahead the store is.
//!
//! # How it is measured
//!
//! A signed `HEAD` for a nonexistent object. The store answers `403` or `404`,
//! always with a `Date`. The halved round trip is taken as when that `Date`
//! was written. `Date` has one-second resolution, so this is accurate to a
//! second, which is all fifteen seconds need.
//!
//! # When it is not trusted
//!
//! A store more than ten seconds behind gets no redirects: the URL would carry
//! a time in its future, and stores differ on what they do with that. The
//! same goes for a store that cannot be measured; its downloads are served from
//! here until it can.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use object_store::{path::Path, signer::Signer};
use parking_lot::RwLock;

/// How long a download URL is usable, by the store's clock.
pub const DOWNLOAD_TTL: Duration = Duration::from_secs(15);

/// A store further behind than this gets no redirects.
const MAX_BEHIND_SECONDS: i64 = 10;

/// Nor one further ahead than this. Stretching a URL by an hour to cover a
/// clock that wrong would be hiding a broken store.
const MAX_AHEAD_SECONDS: i64 = 3600;

/// How long a measurement is trusted. Clocks drift in seconds per day, not per
/// minute.
const FRESH: Duration = Duration::from_mins(10);

/// How long a failed measurement is remembered. Otherwise an unresponsive store
/// would be probed by every download, each waiting out the timeout first.
const RETRY_AFTER: Duration = Duration::from_mins(1);

/// How long to wait on the store. A download is waiting on this.
const TIMEOUT: Duration = Duration::from_secs(5);

/// The object the probe asks about. It never exists; only the `Date` is read.
const PROBE_KEY: &str = ".tessera/clock";

/// The last measurement of each backend's clock, by backend name.
#[derive(Debug, Default)]
pub struct Clocks {
    readings: RwLock<HashMap<String, Reading>>,
}

#[derive(Debug, Clone, Copy)]
struct Reading {
    /// Seconds the store is ahead of this server, negative when behind, or
    /// `None` when it could not be measured.
    skew: Option<i64>,
    at: Instant,
}

impl Clocks {
    /// How many seconds `backend`'s clock is ahead of this server's, negative
    /// when it is behind, or `None` when it cannot be told.
    ///
    /// Measured at most every ten minutes per backend, and a failure is
    /// remembered for a minute.
    pub async fn skew(&self, backend: &str, signer: &Arc<dyn Signer>) -> Option<i64> {
        if let Some(reading) = self.readings.read().get(backend).copied() {
            let trusted_for = if reading.skew.is_some() {
                FRESH
            } else {
                RETRY_AFTER
            };
            if reading.at.elapsed() < trusted_for {
                return reading.skew;
            }
        }

        let skew = measure(signer).await;
        match skew {
            Some(seconds) if download_ttl(seconds).is_none() => tracing::warn!(
                backend,
                seconds,
                "this store's clock is too far from ours to send anybody to it; serving its downloads from here"
            ),
            None => tracing::warn!(
                backend,
                "could not read this store's clock; serving its downloads from here for now"
            ),
            Some(_) => {}
        }
        self.readings.write().insert(
            backend.to_owned(),
            Reading {
                skew,
                at: Instant::now(),
            },
        );
        skew
    }
}

/// Asks the store what time it is.
async fn measure(signer: &Arc<dyn Signer>) -> Option<i64> {
    let url = signer
        .signed_url(
            reqwest::Method::HEAD,
            &Path::from(PROBE_KEY),
            Duration::from_mins(1),
        )
        .await
        .ok()?;
    let client = reqwest::Client::builder().timeout(TIMEOUT).build().ok()?;

    let sent = SystemTime::now();
    let response = client.head(url).send().await.ok()?;
    let received = SystemTime::now();

    let date = response
        .headers()
        .get(reqwest::header::DATE)?
        .to_str()
        .ok()?;
    let theirs = httpdate::parse_http_date(date).ok()?;
    let round_trip = received.duration_since(sent).unwrap_or_default();
    let ours = sent.checked_add(round_trip.checked_div(2)?)?;
    Some(seconds_between(theirs, ours))
}

/// Whole seconds `theirs` is after `ours`, negative when before.
fn seconds_between(theirs: SystemTime, ours: SystemTime) -> i64 {
    match theirs.duration_since(ours) {
        Ok(ahead) => i64::try_from(ahead.as_secs()).unwrap_or(i64::MAX),
        Err(behind) => {
            i64::try_from(behind.duration().as_secs()).map_or(i64::MIN, |seconds| -seconds)
        }
    }
}

/// How long to sign a download URL for, given how far ahead the store is, or
/// `None` when the store should get no redirects.
///
/// A store `n` seconds ahead sees the URL as `n` seconds old on arrival, so it
/// gets `n` more. A store behind gets no less, since the URL will not expire
/// early there, but past a few seconds it is not trusted at all.
#[must_use]
pub fn download_ttl(skew_seconds: i64) -> Option<Duration> {
    if !(-MAX_BEHIND_SECONDS..=MAX_AHEAD_SECONDS).contains(&skew_seconds) {
        return None;
    }
    let ahead = u64::try_from(skew_seconds.max(0)).unwrap_or(0);
    Some(DOWNLOAD_TTL.saturating_add(Duration::from_secs(ahead)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store in step gets exactly the fifteen seconds.
    #[test]
    fn a_store_in_step_gets_the_plain_lifetime() {
        assert_eq!(download_ttl(0), Some(DOWNLOAD_TTL));
    }

    /// A store ahead gets the difference added, or the URL arrives partly used.
    /// Up to an hour; beyond that the clock is broken, not ahead.
    #[test]
    fn a_store_ahead_gets_its_lead_added() {
        assert_eq!(download_ttl(40), Some(Duration::from_secs(55)));
        assert_eq!(
            download_ttl(MAX_AHEAD_SECONDS),
            Some(DOWNLOAD_TTL + Duration::from_hours(1))
        );
        assert_eq!(download_ttl(MAX_AHEAD_SECONDS + 1), None);
    }

    /// Slightly behind is harmless; further is not trusted, since the URL would
    /// carry a time in the store's future.
    #[test]
    fn a_store_behind_is_trusted_only_a_little() {
        assert_eq!(download_ttl(-MAX_BEHIND_SECONDS), Some(DOWNLOAD_TTL));
        assert_eq!(download_ttl(-MAX_BEHIND_SECONDS - 1), None);
    }

    #[test]
    fn the_difference_is_signed() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert_eq!(seconds_between(now + Duration::from_secs(7), now), 7);
        assert_eq!(seconds_between(now - Duration::from_secs(7), now), -7);
        assert_eq!(seconds_between(now, now), 0);
    }
}
