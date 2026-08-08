//! Password hashing.

use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{
        PasswordHash, PasswordHasher as _, PasswordVerifier as _, SaltString, rand_core,
    },
};
use tessera_core::Password;

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

/// Argon2 parameters.
///
/// OWASP's current recommendation for Argon2id: 19 MiB of memory, two
/// iterations, one lane. Memory is what makes GPU cracking expensive, which is
/// why it is the parameter that matters most.
///
/// **This costs 19 MiB per hash in flight.** On a Raspberry Pi serving a burst
/// of logins that is real memory, and it is the reason [`hash`] and [`verify`]
/// go through `spawn_blocking` — the blocking pool bounds how many can run at
/// once, where an unbounded async task would not.
const MEMORY_KIB: u32 = 19 * 1024;
const ITERATIONS: u32 = 2;
const LANES: u32 = 1;

fn hasher() -> Result<Argon2<'static>, PasswordError> {
    let params =
        Params::new(MEMORY_KIB, ITERATIONS, LANES, None).map_err(|_| PasswordError::Hasher)?;
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
pub async fn hash(password: Password) -> Result<String, PasswordError> {
    // Argon2 is deliberately slow and deliberately memory-hungry: it is CPU-bound
    // work measured in tens of milliseconds. Running it on an async worker would
    // block that thread for the duration, stalling every other request the
    // executor had scheduled there. Nothing in the lint set catches this; it has
    // to be remembered, so it is written down.
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut rand_core::OsRng);
        hasher()?
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

    #[tokio::test]
    async fn a_password_verifies_against_its_own_hash() {
        let stored = hash(password("correct horse battery staple"))
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
        let stored = hash(password("correct horse battery staple"))
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
        let first = hash(password("a shared passphrase")).await.unwrap();
        let second = hash(password("a shared passphrase")).await.unwrap();
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
        let stored = hash(password("a passphrase here")).await.unwrap();
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
        let stored = hash(password("hunter2 hunter2 hunter2")).await.unwrap();
        assert!(!stored.contains("hunter2"));
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
