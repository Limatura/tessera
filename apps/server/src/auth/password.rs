//! Password hashing.

use std::sync::LazyLock;

use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{
        PasswordHash, PasswordHasher as _, PasswordVerifier as _, SaltString, rand_core,
    },
};
use tessera_core::Password;

use crate::config::HashingConfig;

/// Failure to hash or verify.
#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    /// The hasher itself failed, or a stored hash is not a PHC string.
    #[error("password hashing failed")]
    Hasher,

    /// The blocking pool was shut down mid-operation.
    #[error("password hashing was cancelled")]
    Cancelled,
}

/// How many hashes may be in flight at once.
///
/// # Why there is a limit at all
///
/// Each hash holds `auth.hashing.memory_kib` — 19 MiB by default — for its
/// duration, and `spawn_blocking`
/// will happily run hundreds in parallel. Without a bound, an unauthenticated
/// stranger sending sign-in attempts as fast as the network allows makes the
/// process allocate until it is killed. Measured before this existed: a few
/// hundred requests over a couple of seconds took resident memory from 34 MB to
/// 792 MB, and nothing in the request path stopped it.
///
/// Requests past the limit wait rather than fail. Hashing is CPU-bound, so more
/// of it in parallel than there are cores buys nothing anyway; the queue is what
/// turns "allocate without limit" into "take longer", and the request timeout is
/// what ends it.
///
/// Clamped at both ends: at least two so a single slow hash cannot stall every
/// sign-in, at most eight so a large machine does not reserve 300 MiB for a
/// burst it will never see.
static IN_FLIGHT: LazyLock<tokio::sync::Semaphore> = LazyLock::new(|| {
    let permits = std::thread::available_parallelism()
        .map_or(2, std::num::NonZero::get)
        .clamp(2, 8);
    tokio::sync::Semaphore::new(permits)
});

/// Peak memory the hasher can hold, for the start-up log.
///
/// The configured cost times how many hashes may run at once — which is the
/// number an operator sizing a small machine actually needs.
#[must_use]
pub fn peak_memory_bytes(cost: HashingConfig) -> u64 {
    let permits = u64::try_from(IN_FLIGHT.available_permits()).unwrap_or(0);
    u64::from(cost.memory_kib) * 1024 * permits
}

fn hasher(cost: HashingConfig) -> Result<Argon2<'static>, PasswordError> {
    let params = Params::new(cost.memory_kib, cost.iterations, cost.lanes, None)
        .map_err(|_| PasswordError::Hasher)?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

/// Hashes a password, returning a PHC string.
///
/// The result carries its own parameters, so raising the cost later does not
/// invalidate existing passwords — they verify against what they were made with
/// and can be re-hashed on next login.
///
/// # Errors
///
/// Returns [`PasswordError`] if hashing fails or the runtime is shutting down.
pub async fn hash(password: Password, cost: HashingConfig) -> Result<String, PasswordError> {
    let _permit = IN_FLIGHT
        .acquire()
        .await
        .map_err(|_| PasswordError::Cancelled)?;
    // Argon2 is deliberately slow and deliberately memory-hungry: it is CPU-bound
    // work measured in tens of milliseconds. Running it on an async worker would
    // block that thread for the duration, stalling every other request the
    // executor had scheduled there. Nothing in the lint set catches this; it has
    // to be remembered, so it is written down.
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut rand_core::OsRng);
        hasher(cost)?
            .hash_password(password.expose().as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|_| PasswordError::Hasher)
    })
    .await
    .map_err(|_| PasswordError::Cancelled)?
}

/// Whether `password` matches `stored`.
///
/// A malformed stored hash is reported as "does not match" rather than as an
/// error, so a corrupted row cannot be told apart from a wrong password by
/// anyone probing the login endpoint.
///
/// # Errors
///
/// Returns [`PasswordError::Cancelled`] if the runtime is shutting down.
pub async fn verify(password: Password, stored: String) -> Result<bool, PasswordError> {
    // Held for the whole verification — see `IN_FLIGHT`. This is the path an
    // attacker reaches, because a wrong password still costs a full hash.
    let _permit = IN_FLIGHT
        .acquire()
        .await
        .map_err(|_| PasswordError::Cancelled)?;
    tokio::task::spawn_blocking(move || {
        let Ok(parsed) = PasswordHash::new(&stored) else {
            return false;
        };
        // `verify_password` is constant-time with respect to the hash contents.
        Argon2::default()
            .verify_password(password.expose().as_bytes(), &parsed)
            .is_ok()
    })
    .await
    .map_err(|_| PasswordError::Cancelled)
}

#[cfg(test)]
mod tests {
    use tessera_core::PasswordPolicy;

    use super::*;

    fn password(raw: &str) -> Password {
        Password::parse(raw, PasswordPolicy::default()).expect("valid for the default policy")
    }

    /// The bound is what stops an unauthenticated stranger making the process
    /// allocate until it dies. Asserted rather than assumed, because the failure
    /// is invisible until the machine is out of memory.
    #[test]
    fn the_hasher_reserves_a_bounded_amount_of_memory() {
        let cost = HashingConfig::default();
        let peak = peak_memory_bytes(cost);
        assert!(peak > 0, "a zero bound would mean nothing can hash at all");
        assert!(
            peak <= u64::from(cost.memory_kib) * 1024 * 8,
            "at most eight hashes in flight, or a large machine reserves more \
             than it will ever need: {peak} bytes"
        );
        assert!(
            peak >= u64::from(cost.memory_kib) * 1024 * 2,
            "at least two, so one slow hash cannot stall every sign-in"
        );
    }

    #[tokio::test]
    async fn a_password_verifies_against_its_own_hash() {
        let stored = hash(
            password("correct horse battery staple"),
            HashingConfig::default(),
        )
        .await
        .unwrap();
        assert!(
            verify(password("correct horse battery staple"), stored)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn a_different_password_does_not_verify() {
        let stored = hash(
            password("correct horse battery staple"),
            HashingConfig::default(),
        )
        .await
        .unwrap();
        assert!(
            !verify(password("incorrect horse battery"), stored)
                .await
                .unwrap()
        );
    }

    /// Two hashes of the same password must differ, or the salt is not doing its
    /// job and identical passwords become visible in a stolen database.
    #[tokio::test]
    async fn the_same_password_hashes_differently_each_time() {
        let first = hash(password("a shared passphrase"), HashingConfig::default())
            .await
            .unwrap();
        let second = hash(password("a shared passphrase"), HashingConfig::default())
            .await
            .unwrap();
        assert_ne!(first, second);

        // Both must still verify.
        assert!(
            verify(password("a shared passphrase"), first)
                .await
                .unwrap()
        );
        assert!(
            verify(password("a shared passphrase"), second)
                .await
                .unwrap()
        );
    }

    /// The hash carries its parameters, which is what lets the cost be raised
    /// later without locking anybody out.
    #[tokio::test]
    async fn the_stored_hash_is_a_phc_string_naming_its_parameters() {
        let stored = hash(password("a passphrase here"), HashingConfig::default())
            .await
            .unwrap();
        assert!(stored.starts_with("$argon2id$"), "got: {stored}");
        assert!(
            stored.contains("m=19456"),
            "memory parameter missing: {stored}"
        );
        assert!(
            stored.contains("t=2"),
            "iteration parameter missing: {stored}"
        );
    }

    #[tokio::test]
    async fn the_plaintext_never_appears_in_the_hash() {
        let stored = hash(
            password("hunter2 hunter2 hunter2"),
            HashingConfig::default(),
        )
        .await
        .unwrap();
        assert!(!stored.contains("hunter2"));
    }

    /// The reason `auth.hashing` is safe to change on a running instance: the
    /// parameters are written into the hash, so verification reads the cost the
    /// hash was made with, not the one configured now. Without this, raising
    /// the cost would lock every existing account out — which is the failure an
    /// operator would only discover from their users.
    #[tokio::test]
    async fn changing_the_cost_does_not_invalidate_an_existing_hash() {
        let cheap = HashingConfig {
            memory_kib: 8 * 1024,
            iterations: 1,
            lanes: 1,
        };
        let stored = hash(password("correct horse battery staple"), cheap)
            .await
            .expect("hash");

        assert!(
            verify(password("correct horse battery staple"), stored.clone())
                .await
                .expect("verify"),
            "a hash made with a different cost must still verify: {stored}"
        );
        assert!(
            !verify(password("some other passphrase"), stored)
                .await
                .expect("verify"),
            "and a wrong password must still be wrong"
        );
    }

    /// The configured cost is what a new hash is actually made with. Reading it
    /// back out of the PHC string is the only way to tell — a hash made with
    /// the wrong parameters is indistinguishable from a correct one until
    /// somebody counts how long it took.
    #[tokio::test]
    async fn the_configured_cost_is_the_one_written_into_the_hash() {
        let cost = HashingConfig {
            memory_kib: 9 * 1024,
            iterations: 3,
            lanes: 1,
        };
        let stored = hash(password("correct horse battery staple"), cost)
            .await
            .expect("hash");

        assert!(stored.contains("m=9216"), "memory not applied: {stored}");
        assert!(stored.contains("t=3"), "iterations not applied: {stored}");
    }

    /// A corrupted row must look exactly like a wrong password from outside, so
    /// that probing the login endpoint cannot map which accounts are damaged.
    #[tokio::test]
    async fn a_malformed_stored_hash_is_a_mismatch_not_an_error() {
        for stored in ["", "not a phc string", "$argon2id$broken"] {
            assert!(
                !verify(password("any passphrase at all"), stored.to_owned())
                    .await
                    .unwrap(),
                "`{stored}` should have been treated as a mismatch"
            );
        }
    }
}
