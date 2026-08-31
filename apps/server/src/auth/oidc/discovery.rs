//! What the provider says about itself.
//!
//! Every provider publishes its endpoints at
//! `{issuer}/.well-known/openid-configuration`. Reading it once means an
//! operator configures one URL rather than four, and means we notice when a
//! provider moves an endpoint.
//!
//! # Why it is cached, and what happens when the fetch fails
//!
//! An issuer's document changes rarely and a sign-in should not wait on a
//! second server that has nothing new to say. It is fetched on the first
//! sign-in and kept for a day.
//!
//! When a refresh fails and there is a previous copy, the previous copy is
//! used: a provider having a bad minute should not stop people signing in with
//! endpoints that have not moved in months. When there is no copy at all, the
//! sign-in is refused and says so.
//!
//! Nothing is fetched at start-up. A provider that is slow or down would
//! otherwise decide whether this process starts, and everything else it serves
//! has nothing to do with signing in.

use std::{sync::Arc, time::Duration};

use serde::Deserialize;
use time::OffsetDateTime;

/// How long a fetched document is used before being fetched again.
const TTL: Duration = Duration::from_hours(24);

/// How long to wait on the provider. Short: a person is watching a browser tab.
const TIMEOUT: Duration = Duration::from_secs(10);

/// The endpoints and capabilities this needs from a provider.
///
/// Deliberately a small subset. Everything else in the document is either not
/// used here or is a promise we verify for ourselves anyway.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Provider {
    /// The provider's own name for itself. Compared to the configured issuer by
    /// exact string equality, which the specification requires.
    pub issuer: String,
    /// Where the browser is sent.
    pub authorization_endpoint: String,
    /// Where the code is exchanged, server to server.
    pub token_endpoint: String,
    /// Where an access token buys the claims that were not in the token.
    #[serde(default)]
    pub userinfo_endpoint: Option<String>,
    /// Where to send somebody to sign out of the provider itself.
    ///
    /// Absent from Google's document, which is why revocation cannot assume it
    /// exists. See `docs/oidc.md`.
    #[serde(default)]
    pub end_session_endpoint: Option<String>,
    /// Where the signing keys are published.
    ///
    /// Only back-channel logout reads them: an ID token arrives over a channel
    /// this server opened, and a logout token does not. See `claims.rs`.
    #[serde(default)]
    pub jwks_uri: Option<String>,
    /// Whether the provider will tell us when it signs somebody out.
    #[serde(default)]
    pub backchannel_logout_supported: bool,
    /// Which PKCE methods it accepts. `S256` is required by us.
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
    /// The document was fetched but is not one we can use.
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
                // A previous copy beats no sign-ins at all. Endpoints do not
                // usually move, and a provider having a bad minute is not a
                // reason to lock everybody out.
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

    /// Throws away what is cached, so the next sign-in fetches again.
    ///
    /// Called when the issuer changes: the document belongs to the old one.
    pub fn forget(&self) {
        *self.cached.lock() = None;
    }
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

    // The document naming a different issuer than the one it was fetched from
    // is how a mix-up attack starts, and it is cheap to refuse here rather than
    // once per sign-in.
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
    // Advertising nothing is not a refusal: the field is optional, and a
    // provider that omits it may still accept `S256`. Advertising a list that
    // excludes it is a refusal.
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

    /// A fetch that fails falls back to what was fetched before, because
    /// endpoints do not move and a provider having a bad minute should not lock
    /// everybody out.
    #[tokio::test]
    async fn a_failed_refresh_uses_the_previous_copy() {
        let discovery = Discovery::default();
        let now = OffsetDateTime::now_utc();
        *discovery.cached.lock() = Some(Cached {
            provider: Arc::new(provider("https://id.example")),
            // Old enough that it will be refreshed, and the refresh will fail
            // because nothing is listening there.
            fetched_at: now - time::Duration::days(2),
        });

        let held = discovery
            .provider("https://id.example", now)
            .await
            .expect("the previous copy");
        assert_eq!(held.token_endpoint, "https://id.example/token");
    }

    /// And with nothing cached, a failed fetch is a refusal rather than a
    /// silent sign-in with no endpoints.
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
