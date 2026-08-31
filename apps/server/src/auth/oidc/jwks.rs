//! The provider's signing keys.
//!
//! Fetched only for back-channel logout. Everything else this module tree does
//! reads a token that arrived over a channel this server opened, and `claims.rs`
//! explains why that one does not need a signature. A logout token is pushed to
//! us by somebody claiming to be the provider, so it does.
//!
//! # Refetching, and why it is rate limited
//!
//! Providers rotate keys, and a token signed with a key this has never seen is
//! the ordinary way that shows up. So an unknown `kid` refetches once.
//!
//! The endpoint that reads these is unauthenticated by necessity: the provider
//! is a server, not a browser, and has no credential here. Anyone can therefore
//! post a token naming a `kid` that will never exist, and without a floor every
//! one of those would be a request to the provider. A floor on how often this
//! fetches is what keeps it from being an amplifier pointed at the provider.

use std::{sync::Arc, time::Duration};

use jsonwebtoken::{DecodingKey, jwk::JwkSet};
use time::OffsetDateTime;

/// The least time between two fetches, however many unknown key ids arrive.
const REFETCH_FLOOR: Duration = Duration::from_mins(5);

/// How long to wait on the provider.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Why a key could not be had.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    /// The provider's document names no key set.
    #[error("the identity provider publishes no signing keys")]
    NoKeySet,
    /// The key set could not be fetched.
    #[error("could not reach the identity provider's key set: {0}")]
    Unreachable(String),
    /// It was fetched but is not a key set.
    #[error("the identity provider's key set could not be read: {0}")]
    Unreadable(String),
    /// No key in the set has that id.
    #[error("the identity provider has no signing key called `{0}`")]
    Unknown(String),
    /// The key is there but cannot be used to verify anything.
    #[error("the identity provider's signing key is not usable: {0}")]
    Unusable(String),
}

/// A fetched key set, and when it arrived.
#[derive(Debug, Clone)]
struct Cached {
    keys: Arc<JwkSet>,
    fetched_at: OffsetDateTime,
}

/// The provider's signing keys, as this process remembers them.
///
/// There is no expiry. A key that is in the set is the same key however old the
/// copy is, so nothing is gained by fetching on a timer; a rotation arrives as
/// a key id that is not there, and that is what triggers a fetch.
#[derive(Debug, Default)]
pub struct Keys {
    cached: parking_lot::Mutex<Option<Cached>>,
}

impl Keys {
    /// The key a token was signed with.
    ///
    /// # Errors
    ///
    /// Returns [`KeyError`] when there is no key set to read, it cannot be
    /// fetched, or nothing in it has that id.
    pub async fn key_for(
        &self,
        jwks_uri: Option<&str>,
        kid: &str,
        now: OffsetDateTime,
    ) -> Result<DecodingKey, KeyError> {
        let uri = jwks_uri
            .map(str::trim)
            .filter(|uri| !uri.is_empty())
            .ok_or(KeyError::NoKeySet)?;

        let held = self.cached.lock().clone();
        if let Some(cached) = held.as_ref()
            && let Some(key) = find(&cached.keys, kid)
        {
            return key;
        }

        // Either nothing is cached, or what is cached does not have this key.
        // Both are answered by fetching, and both are refused while a recent
        // fetch already failed to produce it.
        let too_soon = held.is_some_and(|cached| {
            let age = now - cached.fetched_at;
            age.is_positive() && age < floor()
        });
        if too_soon {
            return Err(KeyError::Unknown(kid.to_owned()));
        }

        let keys = Arc::new(fetch(uri).await?);
        *self.cached.lock() = Some(Cached {
            keys: Arc::clone(&keys),
            fetched_at: now,
        });
        find(&keys, kid).unwrap_or_else(|| Err(KeyError::Unknown(kid.to_owned())))
    }

    /// Throws away what is cached, so the next token fetches again.
    ///
    /// Called when the issuer changes: the keys belong to the old one.
    pub fn forget(&self) {
        *self.cached.lock() = None;
    }
}

fn floor() -> time::Duration {
    time::Duration::try_from(REFETCH_FLOOR).unwrap_or(time::Duration::ZERO)
}

/// Turns the named key into something that can verify a signature.
fn find(keys: &JwkSet, kid: &str) -> Option<Result<DecodingKey, KeyError>> {
    let jwk = keys.find(kid)?;
    Some(DecodingKey::from_jwk(jwk).map_err(|error| KeyError::Unusable(error.to_string())))
}

async fn fetch(uri: &str) -> Result<JwkSet, KeyError> {
    let response = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|error| KeyError::Unreachable(error.to_string()))?
        .get(uri)
        .send()
        .await
        .map_err(|error| KeyError::Unreachable(error.to_string()))?;

    if !response.status().is_success() {
        return Err(KeyError::Unreachable(format!(
            "{uri} answered {}",
            response.status()
        )));
    }
    response
        .json()
        .await
        .map_err(|error| KeyError::Unreadable(error.to_string()))
}
