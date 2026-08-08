//! Session and API token secrets.
//!
//! # Why these are not hashed with Argon2
//!
//! Argon2 exists to make guessing *low-entropy* secrets expensive — people pick
//! passwords from a small space, so the defence is to make each guess cost real
//! time and memory.
//!
//! These secrets are 256 bits from the operating system's random source. There
//! is nothing to guess: an attacker with the full hash and unlimited hardware is
//! not going to find the preimage. Spending 19 MiB and tens of milliseconds
//! deriving a key on *every authenticated request* would buy exactly nothing and
//! would make the service unusable.
//!
//! So they are stored under SHA-256. The reason to hash at all is a stolen
//! database: the rows must not be usable as credentials.

use std::fmt;

use argon2::password_hash::rand_core::{OsRng, RngCore as _};
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;

/// Bytes of entropy in a secret. 256 bits, matching the digest that stores it.
const SECRET_BYTES: usize = 32;

/// Characters of a secret shown to the user after creation, so an API token can
/// be told apart from its siblings in a list without revealing it.
pub const DISPLAY_PREFIX_LEN: usize = 8;

/// Prefix on every API token.
///
/// Fixed and distinctive on purpose: secret scanners (GitHub's, gitleaks,
/// trufflehog) match on patterns like this, so a token pasted into a public
/// repository has a chance of being caught. A bare random string has none.
pub const API_TOKEN_PREFIX: &str = "tsk_";

/// A freshly minted secret, in the clear.
///
/// Exists only between generation and the response that carries it. Like
/// [`Password`](tessera_core::Password) it cannot be printed or serialised by
/// accident — its `Debug` shows nothing.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// Generates a session secret.
    #[must_use]
    pub fn generate_session() -> Self {
        Self(random_hex())
    }

    /// Generates an API token, carrying [`API_TOKEN_PREFIX`].
    #[must_use]
    pub fn generate_api_token() -> Self {
        Self(format!("{API_TOKEN_PREFIX}{}", random_hex()))
    }

    /// Reads a secret supplied by a client.
    ///
    /// No validation: whatever arrived is looked up by its digest, and a value
    /// that was never issued simply matches no row. Rejecting on shape would
    /// only tell an attacker which guesses were the right *kind* of wrong.
    #[must_use]
    pub fn from_client(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    /// The digest stored in the database.
    #[must_use]
    pub fn digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.0.as_bytes());
        hex(&hasher.finalize())
    }

    /// The leading characters, for showing which token a row refers to.
    ///
    /// Includes the `tsk_` prefix where present, so a listing reads the way the
    /// value did when it was issued.
    #[must_use]
    pub fn display_prefix(&self) -> String {
        self.0.chars().take(DISPLAY_PREFIX_LEN).collect()
    }

    /// The secret itself, for putting in a cookie or a response body once.
    ///
    /// Named so that a call site reads as a deliberate act and shows up in a
    /// search for anywhere secrets are handled.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// Prints a fixed placeholder. A secret must not reach a log because something
/// containing it derived `Debug`.
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// Constant-time, so comparing two secrets in process cannot leak a prefix
/// through timing. Lookups go through [`Secret::digest`] and the database's own
/// index, so this is rarely the path taken — but the operator exists, and it
/// should not be the naive one.
impl PartialEq for Secret {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_bytes().ct_eq(other.0.as_bytes()).into()
    }
}

impl Eq for Secret {}

fn random_hex() -> String {
    let mut bytes = [0_u8; SECRET_BYTES];
    // `OsRng` reads the operating system's CSPRNG directly — the same source the
    // password salt uses, reached through a crate already in the tree rather than
    // by adding another.
    OsRng.fill_bytes(&mut bytes);
    hex(&bytes)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Writing into a String is infallible; the result is discarded rather
        // than unwrapped so this stays panic-free.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn session_secrets_are_unique() {
        let secrets: BTreeSet<_> = (0..1_000)
            .map(|_| Secret::generate_session().expose().to_owned())
            .collect();
        assert_eq!(secrets.len(), 1_000);
    }

    #[test]
    fn secrets_carry_the_expected_entropy() {
        // 32 bytes as hex.
        assert_eq!(Secret::generate_session().expose().len(), SECRET_BYTES * 2);
    }

    /// The prefix is what lets a secret scanner recognise a leaked token.
    #[test]
    fn api_tokens_are_recognisable() {
        let token = Secret::generate_api_token();
        assert!(token.expose().starts_with(API_TOKEN_PREFIX));
        assert!(token.display_prefix().starts_with(API_TOKEN_PREFIX));
        assert_eq!(token.display_prefix().len(), DISPLAY_PREFIX_LEN);
    }

    #[test]
    fn the_digest_is_stable_and_not_the_secret() {
        let secret = Secret::generate_api_token();
        assert_eq!(secret.digest(), secret.digest());
        assert_ne!(secret.digest(), secret.expose());
        // SHA-256 as hex.
        assert_eq!(secret.digest().len(), 64);
    }

    #[test]
    fn different_secrets_digest_differently() {
        assert_ne!(
            Secret::generate_session().digest(),
            Secret::generate_session().digest()
        );
    }

    /// A client-supplied value must digest to the same thing the issued one did,
    /// or nobody could ever authenticate.
    #[test]
    fn a_secret_read_back_from_a_client_matches_the_issued_one() {
        let issued = Secret::generate_api_token();
        let presented = Secret::from_client(issued.expose());
        assert_eq!(presented.digest(), issued.digest());
        assert_eq!(presented, issued);
    }

    /// Stands in for the real thing: anything holding a secret that derives
    /// `Debug`. That is how secrets reach logs.
    #[derive(Debug)]
    struct Session {
        #[expect(dead_code, reason = "only its Debug output is under test")]
        secret: Secret,
    }

    #[test]
    fn debug_output_never_contains_the_secret() {
        let secret = Secret::generate_api_token();
        let rendered = format!("{secret:?}");
        assert!(!rendered.contains(secret.expose()), "leaked: {rendered}");

        let exposed = secret.expose().to_owned();
        let rendered = format!("{:?}", Session { secret });
        assert!(!rendered.contains(&exposed), "leaked: {rendered}");
    }

    /// Garbage from a client must not be rejected on shape — it should simply
    /// match no row, so probing cannot distinguish "wrong format" from "unknown".
    #[test]
    fn client_supplied_garbage_digests_without_complaint() {
        for raw in ["", "not a token", "tsk_", "\u{1f600}"] {
            let digest = Secret::from_client(raw).digest();
            assert_eq!(digest.len(), 64);
        }
    }
}
