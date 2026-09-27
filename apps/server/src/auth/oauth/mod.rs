//! Signing in with a provider that speaks plain OAuth 2.0.
//!
//! # Why these are hand-written and OIDC is not
//!
//! An `OpenID` Connect provider is configured by name. It publishes a document
//! listing its endpoints and returns an ID token whose shape the spec fixes, so
//! `auth::oidc` works with any of them without knowing which.
//!
//! GitHub and Discord publish no such document and issue no such token. Their
//! endpoints are constants, and the only way to learn who signed in is to call
//! a provider-specific API. So each is an adapter, not a configuration row, and
//! a third provider means writing a third adapter.
//!
//! # What is shared
//!
//! Everything after "who is this" is shared with OIDC: the flow row, the
//! one-shot `state`, PKCE, the registration policy, username selection, linking
//! to an existing account. See `api::oidc`.
//!
//! The difference is the nonce. An ID token carries one to tie it to the
//! sign-in that asked for it. There is no such token here, so the callback is
//! tied to its sign-in by `state` alone, which the flow table makes single-use
//! by deleting the row.

pub mod discord;
pub mod github;

use serde::Deserialize;

use crate::config::{Config, OauthConfig};

/// Which provider a flow belongs to.
///
/// The string form is what `identities.provider` and `sessions.origin` store,
/// so a session can be traced to the identity that started it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    /// Any `OpenID` Connect provider, configured by its issuer.
    Oidc,
    /// GitHub.
    GitHub,
    /// Discord.
    Discord,
}

impl Provider {
    /// Every provider this version knows, in sign-in page order.
    pub const ALL: [Self; 3] = [Self::Oidc, Self::GitHub, Self::Discord];

    /// How it is stored, and how it appears in a URL.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Oidc => "oidc",
            Self::GitHub => "github",
            Self::Discord => "discord",
        }
    }

    /// Reads one back from a path segment or a stored row.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "oidc" => Some(Self::Oidc),
            "github" => Some(Self::GitHub),
            "discord" => Some(Self::Discord),
            _ => None,
        }
    }

    /// The `auth.github` or `auth.discord` block, for the two that have one.
    ///
    /// `None` for OIDC, which is configured by [`OidcConfig`] and has an issuer
    /// instead of an adapter.
    ///
    /// [`OidcConfig`]: crate::config::OidcConfig
    #[must_use]
    pub const fn oauth(self, config: &Config) -> Option<&OauthConfig> {
        match self {
            Self::Oidc => None,
            Self::GitHub => Some(&config.auth.github),
            Self::Discord => Some(&config.auth.discord),
        }
    }

    /// Whether this instance has enough set to offer it.
    #[must_use]
    pub fn is_configured(self, config: &Config) -> bool {
        self.oauth(config).map_or_else(
            || config.auth.oidc.is_configured(),
            OauthConfig::is_configured,
        )
    }

    /// What to write on the button.
    #[must_use]
    pub fn display_name(self, config: &Config) -> String {
        let configured = self
            .oauth(config)
            .map_or(config.auth.oidc.display_name.as_str(), |oauth| {
                oauth.display_name.as_str()
            })
            .trim();
        if configured.is_empty() {
            match self {
                // No sensible guess at what an operator calls their provider.
                Self::Oidc => "Single sign-on".to_owned(),
                Self::GitHub => "GitHub".to_owned(),
                Self::Discord => "Discord".to_owned(),
            }
        } else {
            configured.to_owned()
        }
    }

    /// Whether a first sign-in through this provider creates an account.
    #[must_use]
    pub fn registration(self, config: &Config) -> crate::config::ProviderRegistration {
        self.oauth(config)
            .map_or(config.auth.oidc.allow_registration, |oauth| {
                oauth.allow_registration
            })
    }

    /// Whether an address from this provider may attach to the existing account
    /// with that address, given whether the provider says it verified it.
    #[must_use]
    pub fn may_link(self, config: &Config, verified: bool) -> bool {
        self.oauth(config).map_or_else(
            || config.auth.oidc.link_by_email.allows(verified),
            |oauth| oauth.link_by_verified_email && verified,
        )
    }
}

/// Who signed in, in the one shape everything downstream understands.
///
/// The same fields `auth::oidc::claims` produces, so the code that turns this
/// into an account does not know which kind of provider it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The provider's own identifier, never a name.
    ///
    /// Discord's documentation says a username is "not unique across the
    /// platform", and a GitHub login can be changed and a released one claimed
    /// by another user. Keying on either is an account takeover in waiting, so
    /// this is the numeric id or the snowflake.
    pub subject: String,
    /// The address, when the provider released one it says it verified.
    pub email: Option<String>,
    /// Whether the provider says it verified it.
    pub email_verified: bool,
    /// The provider's name for the user, for choosing a username.
    pub preferred_username: Option<String>,
    /// A display name, when there is one.
    pub name: Option<String>,
}

/// Why a provider's answer could not be turned into an identity.
#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    /// The provider could not be reached.
    #[error("could not reach {provider}: {reason}")]
    Unreachable {
        /// Which one.
        provider: &'static str,
        /// What went wrong.
        reason: String,
    },
    /// It answered, but with something unexpected.
    #[error("{provider} answered with something unexpected: {reason}")]
    Unreadable {
        /// Which one.
        provider: &'static str,
        /// What went wrong.
        reason: String,
    },
    /// It answered, and said no.
    #[error("{provider} refused: {status}")]
    Refused {
        /// Which one.
        provider: &'static str,
        /// The status it refused with.
        status: u16,
    },
}

/// What an access token buys, before it is read as an identity.
#[derive(Debug, Clone, Deserialize)]
pub struct AccessToken {
    /// The token itself.
    pub access_token: String,
}

/// Where a provider's browser redirect and code exchange live.
#[derive(Debug, Clone)]
pub struct Endpoints {
    /// Where the browser is sent.
    pub authorize: String,
    /// Where the code is exchanged, server to server.
    pub token: String,
}

impl Provider {
    /// The two URLs this provider's flow uses.
    ///
    /// Constants, except for a self-hosted GitHub. `None` for OIDC, whose
    /// endpoints come from discovery and depend on the issuer.
    #[must_use]
    pub fn endpoints(self, config: &Config) -> Option<Endpoints> {
        match self {
            Self::Oidc => None,
            Self::GitHub => Some(github::endpoints(self.base(config))),
            Self::Discord => Some(Endpoints {
                authorize: discord::AUTHORIZE.to_owned(),
                token: discord::TOKEN.to_owned(),
            }),
        }
    }

    /// The API root this provider's identity is read from.
    #[must_use]
    pub fn api(self, config: &Config) -> Option<String> {
        match self {
            Self::Oidc => None,
            Self::GitHub => Some(github::api(self.base(config))),
            Self::Discord => Some(discord::API.to_owned()),
        }
    }

    /// The provider's configured base URL, if any.
    ///
    /// Only GitHub reads it, for GitHub Enterprise Server.
    fn base(self, config: &Config) -> Option<&str> {
        self.oauth(config)
            .map(|oauth| oauth.base_url.trim().trim_end_matches('/'))
            .filter(|base| !base.is_empty())
    }

    /// Reads who signed in, given a token and where to ask.
    ///
    /// # Errors
    ///
    /// Returns [`IdentityError`] when the provider cannot be reached, refuses,
    /// or answers with something unexpected.
    pub async fn identify(self, api: &str, access_token: &str) -> Result<Identity, IdentityError> {
        match self {
            // An OIDC identity comes from the ID token that arrived with the
            // exchange. Nothing to ask.
            Self::Oidc => Err(IdentityError::Unreadable {
                provider: "oidc",
                reason: "an OpenID Connect identity is read from the id token".to_owned(),
            }),
            Self::GitHub => github::identify(api, access_token).await,
            Self::Discord => discord::identify(api, access_token).await,
        }
    }
}

/// Trades the authorisation code for an access token.
///
/// # Why this is not `auth::oidc::token::exchange`
///
/// That one requests and reads an ID token. This asks only for an access
/// token, and GitHub answers form-encoded unless told otherwise, so the
/// `Accept` header is required, not a courtesy.
///
/// # Errors
///
/// Returns [`IdentityError`] when the endpoint cannot be reached, refuses, or
/// answers with something that is not a token.
pub async fn exchange(
    provider: Provider,
    endpoints: &Endpoints,
    oauth: &OauthConfig,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<AccessToken, IdentityError> {
    let name = provider.as_str();
    let client = client(name)?;
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("code_verifier", verifier),
        ("client_id", oauth.client_id.trim()),
        ("client_secret", oauth.client_secret.trim()),
    ];

    read(
        name,
        client
            .post(&endpoints.token)
            .form(&form)
            // Without this GitHub returns `access_token=...&scope=...`, which a
            // JSON parser rejects with an unhelpful error.
            .header(reqwest::header::ACCEPT, "application/json"),
    )
    .await
}

/// A client with the timeout all of these calls use.
///
/// Short, because a user is waiting on a browser tab.
fn client(provider: &'static str) -> Result<reqwest::Client, IdentityError> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|error| IdentityError::Unreachable {
            provider,
            reason: error.to_string(),
        })
}

/// Sends a request and parses the answer, reporting the same three failures
/// the same way for both adapters.
async fn read<T: serde::de::DeserializeOwned>(
    provider: &'static str,
    request: reqwest::RequestBuilder,
) -> Result<T, IdentityError> {
    let response = request
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|error| IdentityError::Unreachable {
            provider,
            reason: error.to_string(),
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(IdentityError::Refused {
            provider,
            status: status.as_u16(),
        });
    }
    response
        .json()
        .await
        .map_err(|error| IdentityError::Unreadable {
            provider,
            reason: error.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_provider_round_trips_through_its_stored_spelling() {
        for provider in Provider::ALL {
            assert_eq!(Provider::parse(provider.as_str()), Some(provider));
        }
        assert_eq!(Provider::parse("nonsense"), None);
    }

    /// The name goes on a button, so an empty one is a blank button.
    #[test]
    fn an_unset_name_falls_back_to_the_providers_own() {
        let config = Config::default();

        assert_eq!(Provider::GitHub.display_name(&config), "GitHub");
        assert_eq!(Provider::Discord.display_name(&config), "Discord");
        assert_eq!(Provider::Oidc.display_name(&config), "Single sign-on");
    }

    #[test]
    fn a_configured_name_wins() {
        let mut config = Config::default();
        "Company GitHub".clone_into(&mut config.auth.github.display_name);

        assert_eq!(Provider::GitHub.display_name(&config), "Company GitHub");
    }

    /// Nothing is offered until configured, and each provider is configured
    /// independently.
    #[test]
    fn each_provider_is_offered_only_once_it_is_configured() {
        let mut config = Config::default();
        for provider in Provider::ALL {
            assert!(!provider.is_configured(&config), "{provider:?}");
        }

        "an-id".clone_into(&mut config.auth.github.client_id);
        "a-secret".clone_into(&mut config.auth.github.client_secret);
        assert!(Provider::GitHub.is_configured(&config));
        assert!(!Provider::Discord.is_configured(&config));
        assert!(!Provider::Oidc.is_configured(&config));
    }
}
