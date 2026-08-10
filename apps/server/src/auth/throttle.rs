//! Login throttling.
//!
//! # Why in memory
//!
//! Failed-attempt counters are worthless data: losing them on restart costs an
//! attacker one restart's worth of guesses, while storing them means a database
//! write on every wrong password — turning the login endpoint into a way to make
//! an unauthenticated stranger generate disk I/O.
//!
//! The trade is that a multi-process deployment throttles per process. This
//! software ships as one process by design, so that is not a case it needs to
//! serve.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use parking_lot::Mutex;

/// Failures before a key is refused.
const MAX_ATTEMPTS: u32 = 10;

/// How long the window lasts, and how long a refusal holds.
const WINDOW: Duration = Duration::from_mins(15);

/// How many keys to track before old ones are swept.
///
/// A bound, not a target: without one, an attacker cycling usernames turns the
/// login endpoint into unbounded memory growth.
const MAX_TRACKED_KEYS: usize = 10_000;

/// What a throttle decision looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// The attempt may proceed.
    Allow,
    /// The attempt is refused; try again after this long.
    Deny {
        /// Time remaining on the block.
        retry_after: Duration,
    },
}

#[derive(Debug, Clone, Copy)]
struct Attempts {
    count: u32,
    first_seen: Instant,
}

/// Counts failed attempts per key.
///
/// The key is chosen by the caller and should combine the account being targeted
/// with where the request came from. Keying on the account alone lets anyone lock
/// a known user out by guessing at them; keying on the address alone lets a
/// botnet spread its guesses and never trip the limit.
#[derive(Debug)]
pub struct Throttle {
    // `parking_lot::Mutex` and not an async lock: the critical section is a
    // hash-map lookup, so an await point here would cost more than it saves. It
    // is never held across an await — see `check` and `record_failure`, both of
    // which are synchronous by construction.
    attempts: Mutex<HashMap<String, Attempts>>,
    max_attempts: u32,
    window: Duration,
}

impl Default for Throttle {
    fn default() -> Self {
        Self::new(MAX_ATTEMPTS, WINDOW)
    }
}

impl Throttle {
    /// Builds a throttle with explicit limits.
    #[must_use]
    pub fn new(max_attempts: u32, window: Duration) -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            max_attempts,
            window,
        }
    }

    /// Whether an attempt for `key` may proceed.
    #[must_use]
    pub fn check(&self, key: &str) -> Decision {
        self.check_at(key, Instant::now())
    }

    /// Records a failed attempt.
    pub fn record_failure(&self, key: &str) {
        self.record_failure_at(key, Instant::now());
    }

    /// Forgets a key's failures, called after a successful login so that a user
    /// who eventually remembers their password is not still being counted.
    pub fn reset(&self, key: &str) {
        self.attempts.lock().remove(key);
    }

    /// [`Throttle::check`] against a supplied instant, so the behaviour can be
    /// tested without sleeping.
    #[must_use]
    pub fn check_at(&self, key: &str, now: Instant) -> Decision {
        let attempts = self.attempts.lock();
        let Some(entry) = attempts.get(key) else {
            return Decision::Allow;
        };

        let elapsed = now.saturating_duration_since(entry.first_seen);
        if elapsed >= self.window || entry.count < self.max_attempts {
            Decision::Allow
        } else {
            Decision::Deny {
                retry_after: self.window.saturating_sub(elapsed),
            }
        }
    }

    /// [`Throttle::record_failure`] against a supplied instant.
    pub fn record_failure_at(&self, key: &str, now: Instant) {
        let mut attempts = self.attempts.lock();

        if attempts.len() >= MAX_TRACKED_KEYS {
            attempts
                .retain(|_, entry| now.saturating_duration_since(entry.first_seen) < self.window);
        }

        let entry = attempts.entry(key.to_owned()).or_insert(Attempts {
            count: 0,
            first_seen: now,
        });

        // A window that has run out starts again rather than accumulating,
        // otherwise a key would be blocked forever after enough scattered
        // mistakes.
        if now.saturating_duration_since(entry.first_seen) >= self.window {
            entry.count = 0;
            entry.first_seen = now;
        }
        entry.count = entry.count.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn throttle() -> Throttle {
        Throttle::new(3, Duration::from_mins(1))
    }

    #[test]
    fn an_unknown_key_is_allowed() {
        assert_eq!(throttle().check("alice"), Decision::Allow);
    }

    #[test]
    fn failures_below_the_limit_still_allow() {
        let throttle = throttle();
        throttle.record_failure("alice");
        throttle.record_failure("alice");
        assert_eq!(throttle.check("alice"), Decision::Allow);
    }

    #[test]
    fn the_limit_denies_and_says_when_to_retry() {
        let throttle = throttle();
        for _ in 0..3 {
            throttle.record_failure("alice");
        }
        match throttle.check("alice") {
            Decision::Deny { retry_after } => assert!(retry_after <= Duration::from_mins(1)),
            Decision::Allow => panic!("should have been denied"),
        }
    }

    /// One account being guessed at must not affect another.
    #[test]
    fn keys_are_counted_independently() {
        let throttle = throttle();
        for _ in 0..3 {
            throttle.record_failure("alice");
        }
        assert_eq!(throttle.check("bob"), Decision::Allow);
    }

    #[test]
    fn a_successful_login_clears_the_count() {
        let throttle = throttle();
        for _ in 0..3 {
            throttle.record_failure("alice");
        }
        throttle.reset("alice");
        assert_eq!(throttle.check("alice"), Decision::Allow);
    }

    /// Nobody should be locked out permanently because of mistakes spread over
    /// months.
    #[test]
    fn the_window_expires() {
        let throttle = throttle();
        let start = Instant::now();
        for _ in 0..3 {
            throttle.record_failure_at("alice", start);
        }
        assert!(matches!(
            throttle.check_at("alice", start),
            Decision::Deny { .. }
        ));

        let later = start
            .checked_add(Duration::from_secs(61))
            .expect("representable");
        assert_eq!(throttle.check_at("alice", later), Decision::Allow);
    }

    #[test]
    fn a_failure_after_the_window_starts_a_fresh_count() {
        let throttle = throttle();
        let start = Instant::now();
        for _ in 0..3 {
            throttle.record_failure_at("alice", start);
        }

        let later = start
            .checked_add(Duration::from_secs(61))
            .expect("representable");
        throttle.record_failure_at("alice", later);
        assert_eq!(throttle.check_at("alice", later), Decision::Allow);
    }

    /// The hole the source counter exists to close: keying only on the target
    /// means the key changes when the target does, so naming a different
    /// account each time is never refused. Measured against a running server
    /// before this existed — two hundred attempts, none blocked.
    #[test]
    fn rotating_the_target_does_not_escape_the_limit() {
        let throttles = Throttles::default();
        let source = "203.0.113.9";

        // Every attempt names something new, so the per-target counter never
        // sees the same key twice.
        for attempt in 0..SOURCE_MAX_ATTEMPTS {
            let key = format!("victim{attempt}|{source}");
            assert_eq!(
                throttles.check(&throttles.login, &key, source),
                Decision::Allow,
                "attempt {attempt} should still be allowed"
            );
            throttles.record_failure(&throttles.login, &key, source);
        }

        let fresh = format!("victim-new|{source}");
        assert!(
            matches!(
                throttles.check(&throttles.login, &fresh, source),
                Decision::Deny { .. }
            ),
            "a new target from a source that has already failed {SOURCE_MAX_ATTEMPTS} times \
             must not get a clean slate"
        );
    }

    /// One address failing must not refuse a different one.
    #[test]
    fn sources_are_counted_independently() {
        let throttles = Throttles::default();
        for attempt in 0..SOURCE_MAX_ATTEMPTS {
            let key = format!("victim{attempt}|noisy");
            throttles.record_failure(&throttles.login, &key, "noisy");
        }
        assert_eq!(
            throttles.check(&throttles.login, "someone|quiet", "quiet"),
            Decision::Allow
        );
    }

    /// Without a bound, cycling usernames is a memory-growth attack on an
    /// unauthenticated endpoint.
    #[test]
    fn tracking_is_bounded() {
        let throttle = Throttle::new(3, Duration::from_millis(1));
        let start = Instant::now();
        let later = start
            .checked_add(Duration::from_secs(1))
            .expect("representable");

        for n in 0..=MAX_TRACKED_KEYS {
            throttle.record_failure_at(&format!("key-{n}"), start);
        }
        // The sweep runs on the next insert once the bound is reached.
        throttle.record_failure_at("trigger", later);

        assert!(
            throttle.attempts.lock().len() < MAX_TRACKED_KEYS,
            "expired entries should have been swept"
        );
    }
}

/// The counters an instance keeps, and what each is keyed on.
///
/// # Why more than one
///
/// A single counter keyed on "who is being targeted" is the obvious design and
/// it has a hole: the key changes when the target does. Someone guessing at
/// `alice`, then `bob`, then `carol` gets a fresh key every time and is never
/// refused — which was measurable, not theoretical. Two hundred sign-in
/// attempts from one address, each naming a different account, were all
/// answered.
///
/// So there is a second counter keyed on the source alone. It is deliberately
/// looser: shared addresses are normal — an office, a school, a mobile carrier —
/// and only *failures* count, which a real person rarely produces sixty of.
#[derive(Debug)]
pub struct Throttles {
    /// Failed sign-ins, keyed on the account *and* the source. Stops one
    /// account being ground down.
    pub login: Throttle,
    /// Anything expensive, keyed on the source alone. Stops the key rotating.
    pub source: Throttle,
    /// Failed share-password attempts, keyed on the code and the source.
    ///
    /// A share password may be as short as four characters, which is the right
    /// trade for something read out over the phone — and worthless without
    /// this: unthrottled, ten thousand combinations fall in under a minute.
    pub share: Throttle,
}

impl Default for Throttles {
    fn default() -> Self {
        Self {
            login: Throttle::default(),
            source: Throttle::new(SOURCE_MAX_ATTEMPTS, SOURCE_WINDOW),
            share: Throttle::default(),
        }
    }
}

/// Failures from one address before it is refused, whatever it was aiming at.
///
/// Higher than the per-target limit, and over a much shorter window, because it
/// catches a different thing. The per-target counters are what stop a password
/// being guessed; the hasher's own concurrency limit is what stops memory being
/// exhausted. All this adds is a ceiling on how fast one source can *try*.
///
/// That means it can afford to be forgiving, and it needs to be: an address is
/// shared — an office, a school, a mobile carrier — so refusing one refuses
/// everybody behind it. A minute of that is a nuisance; fifteen would be a
/// support ticket.
const SOURCE_MAX_ATTEMPTS: u32 = 120;

/// How long a source is refused for. Deliberately much shorter than
/// [`WINDOW`] — see [`SOURCE_MAX_ATTEMPTS`].
const SOURCE_WINDOW: Duration = Duration::from_mins(1);

impl Throttles {
    /// Whether an attempt keyed on both `specific` and `source` may proceed.
    ///
    /// Both counters are consulted, and the stricter answer wins.
    #[must_use]
    pub fn check(&self, counter: &Throttle, specific: &str, source: &str) -> Decision {
        match (counter.check(specific), self.source.check(source)) {
            (Decision::Allow, Decision::Allow) => Decision::Allow,
            (Decision::Deny { retry_after }, Decision::Allow)
            | (Decision::Allow, Decision::Deny { retry_after }) => Decision::Deny { retry_after },
            (Decision::Deny { retry_after: a }, Decision::Deny { retry_after: b }) => {
                Decision::Deny {
                    retry_after: a.max(b),
                }
            }
        }
    }

    /// Records a failure against both counters.
    pub fn record_failure(&self, counter: &Throttle, specific: &str, source: &str) {
        counter.record_failure(specific);
        self.source.record_failure(source);
    }
}
