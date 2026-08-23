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
    sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering},
    time::{Duration, Instant},
};

use parking_lot::Mutex;

use crate::config::{Limit, ThrottleConfig};

/// Failures before a key is refused.
pub const MAX_ATTEMPTS: u32 = 10;

/// How long the window lasts, and how long a refusal holds.
pub const WINDOW: Duration = Duration::from_mins(15);

/// How many keys to track before old ones are swept.
///
/// A bound, not a target: without one, an attacker cycling usernames turns the
/// login endpoint into unbounded memory growth. That is why it has its own
/// ceiling in [`ThrottleConfig::validate`] — raising it is spending memory an
/// unauthenticated stranger gets to fill.
pub const MAX_TRACKED_KEYS: usize = 10_000;

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
    // Atomics rather than plain fields, so the limits can change while the
    // counters keep counting. Rebuilding the throttle to apply a new limit
    // would empty the map — which would make changing any unrelated setting a
    // way to clear everybody's failed attempts, and hand an attacker a reset
    // button.
    max_attempts: AtomicU32,
    window_secs: AtomicU64,
    max_tracked_keys: AtomicUsize,
}

impl Default for Throttle {
    fn default() -> Self {
        Self::new(MAX_ATTEMPTS, WINDOW, MAX_TRACKED_KEYS)
    }
}

impl Throttle {
    /// Builds a throttle with explicit limits.
    #[must_use]
    pub fn new(max_attempts: u32, window: Duration, max_tracked_keys: usize) -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            max_attempts: AtomicU32::new(max_attempts),
            window_secs: AtomicU64::new(window.as_secs()),
            max_tracked_keys: AtomicUsize::new(max_tracked_keys),
        }
    }

    /// Changes the limits without touching who has failed how many times.
    ///
    /// `Relaxed` throughout: these are three independent numbers read on a path
    /// that already takes a mutex, and a request that sees the old limit for
    /// one more moment is not a defect worth ordering for.
    pub fn apply(&self, max_attempts: u32, window: Duration, max_tracked_keys: usize) {
        self.max_attempts.store(max_attempts, Ordering::Relaxed);
        self.window_secs.store(window.as_secs(), Ordering::Relaxed);
        self.max_tracked_keys
            .store(max_tracked_keys, Ordering::Relaxed);
    }

    fn max_attempts(&self) -> u32 {
        self.max_attempts.load(Ordering::Relaxed)
    }

    fn window(&self) -> Duration {
        Duration::from_secs(self.window_secs.load(Ordering::Relaxed))
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
        if elapsed >= self.window() || entry.count < self.max_attempts() {
            Decision::Allow
        } else {
            Decision::Deny {
                retry_after: self.window().saturating_sub(elapsed),
            }
        }
    }

    /// [`Throttle::record_failure`] against a supplied instant.
    pub fn record_failure_at(&self, key: &str, now: Instant) {
        let mut attempts = self.attempts.lock();

        if attempts.len() >= self.max_tracked_keys.load(Ordering::Relaxed) {
            attempts
                .retain(|_, entry| now.saturating_duration_since(entry.first_seen) < self.window());
        }

        let entry = attempts.entry(key.to_owned()).or_insert(Attempts {
            count: 0,
            first_seen: now,
        });

        // A window that has run out starts again rather than accumulating,
        // otherwise a key would be blocked forever after enough scattered
        // mistakes.
        if now.saturating_duration_since(entry.first_seen) >= self.window() {
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
        Throttle::new(3, Duration::from_mins(1), MAX_TRACKED_KEYS)
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

    /// The counters have to come from the file, or `auth.throttle` is six
    /// settings an operator can write and none the process reads.
    #[test]
    fn the_configured_limits_are_the_ones_enforced() {
        let mut config = ThrottleConfig::default();
        config.login.max_attempts = 2;
        config.login.window_secs = 60;
        let throttles = Throttles::from_config(&config);

        throttles.login.record_failure("alice");
        assert_eq!(throttles.login.check("alice"), Decision::Allow);
        throttles.login.record_failure("alice");
        assert!(
            matches!(throttles.login.check("alice"), Decision::Deny { .. }),
            "the third attempt should have been refused at max_attempts = 2"
        );
    }

    /// Same reason, for the setting that is a memory ceiling rather than a
    /// limit: a configured bound that the sweep ignores is not a bound.
    #[test]
    fn the_configured_key_bound_is_the_one_swept_at() {
        let config = ThrottleConfig {
            max_tracked_keys: 16,
            ..ThrottleConfig::default()
        };
        let throttle = Throttles::from_config(&config).login;
        let start = Instant::now();
        let later = start
            .checked_add(Duration::from_hours(2))
            .expect("representable");

        for n in 0..=16 {
            throttle.record_failure_at(&format!("key-{n}"), start);
        }
        throttle.record_failure_at("trigger", later);

        assert!(
            throttle.attempts.lock().len() < 16,
            "expired entries should have been swept at the configured bound"
        );
    }

    /// Without a bound, cycling usernames is a memory-growth attack on an
    /// unauthenticated endpoint.
    #[test]
    fn tracking_is_bounded() {
        let throttle = Throttle::new(3, Duration::from_millis(1), MAX_TRACKED_KEYS);
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
    /// Retrieval codes tried and not found, keyed on the source alone.
    ///
    /// Keyed on the source because the *code* is what varies — that is what
    /// walking the code space means, so counting per code would count nothing.
    ///
    /// Deliberately not routed through [`Throttles::check`]: a lookup is one
    /// indexed `SELECT`, nothing like the cost of a hash, and letting a few
    /// mistyped codes eat into the budget that protects the hasher would mean a
    /// visitor guessing at links could stop somebody else signing in.
    pub lookup: Throttle,
    /// Password reset requests, keyed on the address asked about.
    ///
    /// Keyed on the address rather than the source, because the thing being
    /// protected is somebody's inbox: without this, one request per second at
    /// the same address is a mail flood that the account holder experiences and
    /// cannot stop. Keying on the source instead would let an attacker rotate
    /// addresses and still bury one victim.
    ///
    /// Counted on *success*, not failure — see the handler. Every request looks
    /// successful from outside by design, so there is no failure to count, and
    /// what needs limiting is how often a message is actually sent.
    pub reset: Throttle,
    /// Registration attempts, keyed on the address asked about.
    ///
    /// Its own counter rather than sharing [`Throttles::reset`], and that is the
    /// whole reason it exists. Sharing would mean anyone could spend somebody
    /// else's reset budget just by typing their address into the registration
    /// form — three attempts and the account holder can no longer ask for a
    /// password reset. Losing access to a recovery route is a worse outcome than
    /// the six messages an hour that two counters allow instead of three.
    pub registration: Throttle,

    /// Failed share-password attempts, keyed on the code and the source.
    ///
    /// A share password may be as short as four characters, which is the right
    /// trade for something read out over the phone — and worthless without
    /// this: unthrottled, ten thousand combinations fall in under a minute.
    pub share: Throttle,
}

impl Default for Throttles {
    fn default() -> Self {
        Self::from_config(&ThrottleConfig::default())
    }
}

impl Throttles {
    /// Applies new limits to the counters already running.
    ///
    /// The counters are the point: a change to any setting must not be a way to
    /// forget who has been failing to sign in. See [`Throttle::apply`].
    pub fn apply(&self, config: &ThrottleConfig) {
        let keys = config.max_tracked_keys;
        for (throttle, limit) in [
            (&self.login, &config.login),
            (&self.source, &config.source),
            (&self.lookup, &config.lookup),
            (&self.reset, &config.reset),
            (&self.registration, &config.registration),
            (&self.share, &config.share),
        ] {
            throttle.apply(limit.max_attempts, limit.window(), keys);
        }
    }

    /// Builds the six counters an instance keeps from its configuration.
    ///
    /// The key bound is shared rather than per-counter: it is a memory ceiling,
    /// and an operator sizing a container wants one number to reason about
    /// rather than six that multiply.
    #[must_use]
    pub fn from_config(config: &ThrottleConfig) -> Self {
        let keys = config.max_tracked_keys;
        let build = |limit: &Limit| Throttle::new(limit.max_attempts, limit.window(), keys);
        Self {
            login: build(&config.login),
            source: build(&config.source),
            lookup: build(&config.lookup),
            reset: build(&config.reset),
            registration: build(&config.registration),
            share: build(&config.share),
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
pub const SOURCE_MAX_ATTEMPTS: u32 = 120;

/// How long a source is refused for. Deliberately much shorter than
/// [`WINDOW`] — see [`SOURCE_MAX_ATTEMPTS`].
pub const SOURCE_WINDOW: Duration = Duration::from_mins(1);

/// Mails to one address before further requests are refused, counted separately
/// for resets and registrations.
///
/// Three is enough for somebody who did not receive the first one and tried
/// again, and few enough that the inbox is not a place to hide an attack.
pub const RESET_MAX_REQUESTS: u32 = 3;

/// How long that limit lasts. Longer than the link itself, so asking again
/// after being refused means the earlier link has expired anyway.
pub const RESET_WINDOW: Duration = Duration::from_hours(1);

/// Codes tried and not found, from one source, before it is refused.
///
/// A person who mistypes a code they were given tries two or three times. Sixty
/// in a minute is not that.
///
/// The default eight-character code is 32^8 — about 1.1 x 10^12 — so walking it
/// was never going to work at any rate. This matters for the instance that sets
/// `share.code_length` to four: a million combinations fall in minutes without a
/// limit, and in about eleven days with one.
pub const LOOKUP_MAX_ATTEMPTS: u32 = 60;

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

#[cfg(test)]
mod hot_change_tests {
    use super::*;

    /// Changing a limit must not be a way to forget who has been failing. If
    /// applying a setting rebuilt the throttle, any unrelated change in the
    /// admin panel would clear every counter — a reset button for whoever is
    /// guessing at passwords.
    #[test]
    fn a_new_limit_keeps_the_counters() {
        let throttles = Throttles::default();
        for _ in 0..MAX_ATTEMPTS {
            throttles.login.record_failure("alice");
        }
        assert!(matches!(
            throttles.login.check("alice"),
            Decision::Deny { .. }
        ));

        let mut config = ThrottleConfig::default();
        config.login.max_attempts = MAX_ATTEMPTS + 5;
        throttles.apply(&config);

        assert_eq!(
            throttles.login.check("alice"),
            Decision::Allow,
            "the raised limit should apply to the count that was already there"
        );
        assert_eq!(
            throttles
                .login
                .attempts
                .lock()
                .get("alice")
                .map(|e| e.count),
            Some(MAX_ATTEMPTS),
            "and the count itself must have survived"
        );
    }

    /// The other direction: tightening takes effect at once, on people already
    /// being counted.
    #[test]
    fn a_lowered_limit_applies_immediately() {
        let throttles = Throttles::default();
        throttles.login.record_failure("alice");
        assert_eq!(throttles.login.check("alice"), Decision::Allow);

        let mut config = ThrottleConfig::default();
        config.login.max_attempts = 1;
        throttles.apply(&config);

        assert!(
            matches!(throttles.login.check("alice"), Decision::Deny { .. }),
            "one failure against a limit of one should now refuse"
        );
    }
}
