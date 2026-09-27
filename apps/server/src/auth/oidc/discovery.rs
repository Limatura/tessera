//! What the provider says about itself.
//!
//! Every provider publishes its endpoints at
//! `{issuer}/.well-known/openid-configuration`. Reading it means an operator
//! configures one URL instead of four, and endpoint moves are picked up.
//!
//! # Caching and fetch failures
//!
//! The document rarely changes, and a sign-in should not wait on a second
//! server with nothing new to say. It is fetched on the first sign-in and kept
//! for a day.
//!
//! If a refresh fails and a previous copy exists, that copy is used: a brief
//! provider outage should not block sign-ins with endpoints that have not moved
//! in months. With no copy at all, the sign-in is refused with a reason.
//!
//! Nothing is fetched at start-up. Otherwise a slow or down provider would
//! decide whether the process starts, although nothing else it serves depends
//! on sign-in.

use std::{sync::Arc, time::Duration};

use serde::Deserialize;
use time::OffsetDateTime;

/// How long a fetched document is used before being fetched again.
const TTL: Duration = Duration::from_hours(24);

/// How long to wait on the provider. Short, since a user is waiting on a tab.
const TIMEOUT: Duration = Duration::from_secs(10);

/// The endpoints and capabilities needed from a provider.
///
/// A small subset. The rest of the document is either unused or something we
/// verify ourselves anyway.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Provider {
    /// The provider's own issuer value. Compared to the configured issuer by
    /// exact string equality, as the spec requires.
    pub issuer: String,
    /// Where the browser is sent.
    pub authorization_endpoint: String,
    /// Where the code is exchanged, server to server.
    pub token_endpoint: String,
    /// Where an access token fetches the claims missing from the ID token.
    #[serde(default)]
    pub userinfo_endpoint: Option<String>,
    /// Where to send a user to sign out of the provider itself.
    ///
    /// Google's document lacks it, so revocation cannot assume it exists. See
    /// `docs/oidc.md`.
    #[serde(default)]
    pub end_session_endpoint: Option<String>,
    /// Where the signing keys are published.
    ///
    /// Only back-channel logout reads them: an ID token arrives over a channel
    /// this server opened, and a logout token does not. See `claims.rs`.
    #[serde(default)]
    pub jwks_uri: Option<String>,
    /// Whether the provider notifies us when a user signs out there.
    #[serde(default)]
    pub backchannel_logout_supported: bool,
    /// The scopes it advertises.
    ///
    /// Not read here: the scopes to request are configured by the operator.
    /// Carried so the panel can offer what the provider advertises instead of
    /// making the operator type them.
    #[serde(default)]
    pub scopes_supported: Vec<String>,
    /// The claims it advertises.
    ///
    /// Carried for the panel, like the scopes: if a provider does not list
    /// `email_verified`, `when_verified` will never link anyone, and saying so
    /// when the document is read beats learning it from the first refusal.
    #[serde(default)]
    pub claims_supported: Vec<String>,
    /// The PKCE methods it accepts. We require `S256`.
    #[serde(default)]
    pub code_challenge_methods_supported: Vec<String>,
}

/// Why a provider's document could not be used.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// The issuer is not configured.
    #[error("no identity provider is configured")]
    NotConfigured,
    /// The document could not be fetched and nothing was cached.
    #[error("could not reach the identity provider: {0}")]
    Unreachable(String),
    /// The document was fetched but is not usable.
    #[error("the identity provider's configuration is not usable: {0}")]
    Unusable(String),
}

/// A cached provider document.
#[derive(Debug)]
struct Cached {
    provider: Arc<Provider>,
    fetched_at: OffsetDateTime,
}

/// Fetches and remembers what a provider says about itself.
#[derive(Debug, Default)]
pub struct Discovery {
    cached: parking_lot::Mutex<Option<Cached>>,
}

impl Discovery {
    /// The provider's document, fetched if it is missing or stale.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryError`] when there is no issuer configured, or when
    /// the document cannot be fetched and none was cached.
    pub async fn provider(
        &self,
        issuer: &str,
        now: OffsetDateTime,
    ) -> Result<Arc<Provider>, DiscoveryError> {
        let issuer = issuer.trim().trim_end_matches('/');
        if issuer.is_empty() {
            return Err(DiscoveryError::NotConfigured);
        }

        let fresh = {
            let cached = self.cached.lock();
            cached.as_ref().and_then(|held| {
                let age = now - held.fetched_at;
                (age.is_positive() && age < TTL.try_into().unwrap_or(time::Duration::ZERO))
                    .then(|| Arc::clone(&held.provider))
            })
        };
        if let Some(provider) = fresh {
            return Ok(provider);
        }

        match fetch(issuer).await {
            Ok(provider) => {
                let provider = Arc::new(provider);
                *self.cached.lock() = Some(Cached {
                    provider: Arc::clone(&provider),
                    fetched_at: now,
                });
                Ok(provider)
            }
            Err(error) => {
                // A stale copy beats no sign-ins at all. Endpoints rarely move,
                // and a brief provider outage should not lock everyone out.
                let stale = self
                    .cached
                    .lock()
                    .as_ref()
                    .map(|held| Arc::clone(&held.provider));
                match stale {
                    Some(provider) => {
                        tracing::warn!(%error, "using the last known provider configuration");
                        Ok(provider)
                    }
                    None => Err(error),
                }
            }
        }
    }

    /// Drops the cached document, so the next sign-in fetches again.
    ///
    /// Called when the issuer changes, since the document belongs to the old
    /// one.
    pub fn forget(&self) {
        *self.cached.lock() = None;
    }
}

/// Fetches a provider's document once, without touching the cache.
///
/// For the panel, while an operator is trying a URL: a document from an unsaved
/// URL must not become the one sign-ins use.
///
/// # Errors
///
/// Returns [`DiscoveryError`] when the document cannot be fetched or is not
/// usable.
pub async fn probe(issuer: &str) -> Result<Provider, DiscoveryError> {
    fetch(issuer.trim().trim_end_matches('/')).await
}

async fn fetch(issuer: &str) -> Result<Provider, DiscoveryError> {
    // Appended, not joined: a path on the issuer is part of its identity, and
    // `Url::join` would replace it.
    let url = format!("{issuer}/.well-known/openid-configuration");

    let response = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|error| DiscoveryError::Unreachable(error.to_string()))?
        .get(&url)
        .send()
        .await
        .map_err(|error| DiscoveryError::Unreachable(error.to_string()))?;

    if !response.status().is_success() {
        return Err(DiscoveryError::Unreachable(format!(
            "{url} answered {}",
            response.status()
        )));
    }

    let provider: Provider = response
        .json()
        .await
        .map_err(|error| DiscoveryError::Unusable(error.to_string()))?;

    // A document naming a different issuer than the one it came from is how a
    // mix-up attack starts, and refusing it here is cheaper than per sign-in.
    if provider.issuer.trim_end_matches('/') != issuer {
        return Err(DiscoveryError::Unusable(format!(
            "it calls itself `{}`, but was fetched from `{issuer}`",
            provider.issuer
        )));
    }
    if !super::is_private_or_tls(&provider.authorization_endpoint)
        || !super::is_private_or_tls(&provider.token_endpoint)
    {
        return Err(DiscoveryError::Unusable(
            "its endpoints are not https".to_owned(),
        ));
    }
    // An empty list is not a refusal: the field is optional, and a provider
    // omitting it may still accept `S256`. A list without it is a refusal.
    if !provider.code_challenge_methods_supported.is_empty()
        && !provider
            .code_challenge_methods_supported
            .iter()
            .any(|method| method == "S256")
    {
        return Err(DiscoveryError::Unusable(
            "it does not accept the S256 code challenge, which this requires".to_owned(),
        ));
    }
    Ok(provider)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(issuer: &str) -> Provider {
        Provider {
            issuer: issuer.to_owned(),
            authorization_endpoint: format!("{issuer}/authorize"),
            token_endpoint: format!("{issuer}/token"),
            userinfo_endpoint: None,
            end_session_endpoint: None,
            jwks_uri: None,
            backchannel_logout_supported: false,
            scopes_supported: vec!["openid".to_owned(), "email".to_owned()],
            claims_supported: Vec::new(),
            code_challenge_methods_supported: vec!["S256".to_owned()],
        }
    }

    #[tokio::test]
    async fn an_empty_issuer_is_not_configured() {
        let discovery = Discovery::default();

        assert!(matches!(
            discovery.provider("  ", OffsetDateTime::now_utc()).await,
            Err(DiscoveryError::NotConfigured)
        ));
    }

    /// A failed fetch falls back to the previous copy, since endpoints rarely
    /// move and a brief provider outage should not lock everyone out.
    #[tokio::test]
    async fn a_failed_refresh_uses_the_previous_copy() {
        let discovery = Discovery::default();
        let now = OffsetDateTime::now_utc();
        *discovery.cached.lock() = Some(Cached {
            provider: Arc::new(provider("https://id.example")),
            // Old enough to be refreshed, and the refresh fails because nothing
            // is listening there.
            fetched_at: now - time::Duration::days(2),
        });

        let held = discovery
            .provider("https://id.example", now)
            .await
            .expect("the previous copy");
        assert_eq!(held.token_endpoint, "https://id.example/token");
    }

    /// With nothing cached, a failed fetch is a refusal, not a sign-in with no
    /// endpoints.
    #[tokio::test]
    async fn a_failed_first_fetch_is_refused() {
        let discovery = Discovery::default();

        assert!(matches!(
            discovery
                .provider("https://id.invalid", OffsetDateTime::now_utc())
                .await,
            Err(DiscoveryError::Unreachable(_))
        ));
    }

    #[tokio::test]
    async fn a_fresh_copy_is_not_fetched_again() {
        let discovery = Discovery::default();
        let now = OffsetDateTime::now_utc();
        *discovery.cached.lock() = Some(Cached {
            provider: Arc::new(provider("https://id.example")),
            fetched_at: now,
        });

        // Would fail if it went to the network: nothing is listening.
        let held = discovery
            .provider("https://id.example", now)
            .await
            .expect("the cached copy");
        assert_eq!(held.issuer, "https://id.example");
    }

    #[tokio::test]
    async fn forgetting_makes_the_next_call_fetch() {
        let discovery = Discovery::default();
        let now = OffsetDateTime::now_utc();
        *discovery.cached.lock() = Some(Cached {
            provider: Arc::new(provider("https://id.example")),
            fetched_at: now,
        });
        discovery.forget();

        assert!(discovery.provider("https://id.example", now).await.is_err());
    }

    /// A trailing slash on the configured issuer is not a different provider.
    #[tokio::test]
    async fn a_trailing_slash_matches_the_cached_copy() {
        let discovery = Discovery::default();
        let now = OffsetDateTime::now_utc();
        *discovery.cached.lock() = Some(Cached {
            provider: Arc::new(provider("https://id.example")),
            fetched_at: now,
        });

        assert!(discovery.provider("https://id.example/", now).await.is_ok());
    }
}
