//! Signing in with a provider that speaks plain OAuth 2.0.
//!
//! # Why these are hand-written and OIDC is not
//!
//! An `OpenID` Connect provider is configured by naming it. It publishes a
//! document saying where its endpoints are, and it answers with an ID token
//! whose shape the specification fixes, so `auth::oidc` works against any of
//! them without knowing which.
//!
//! GitHub and Discord publish no such document and issue no such token. Their
//! endpoints are constants, and the only way to find out who just signed in is
//! to call an API whose shape is theirs alone. So each is an adapter rather
//! than a row of configuration, and adding a third means writing a third.
//!
//! # What is shared and what is not
//!
//! Everything after "who is this" is shared with OIDC: the flow row, the
//! one-shot `state`, PKCE, the registration policy, choosing a username,
//! attaching to an existing account. See `api::oidc`.
//!
//! What is not shared is the absence of a nonce. An ID token carries one and
//! it is what ties the token to the sign-in that asked for it. There is no
//! token here to carry one, so the callback is tied to its sign-in by `state`
//! alone, which the flow table makes single-use by deleting the row.

pub mod discord;
pub mod github;

use serde::Deserialize;

use crate::config::{Config, OauthConfig};

/// Which provider a flow belongs to.
///
/// The string form is what `identities.provider` and `sessions.origin` hold,
/// so the two line up and a session can be traced to the identity that started
/// it.
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
    /// Every provider this version knows, in the order a sign-in page lists
    /// them.
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
    /// `None` for OIDC, which is configured by [`OidcConfig`] instead and has
    /// an issuer where these have an adapter.
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
                // No good guess at what an operator calls their own provider.
                Self::Oidc => "Single sign-on".to_owned(),
                Self::GitHub => "GitHub".to_owned(),
                Self::Discord => "Discord".to_owned(),
            }
        } else {
            configured.to_owned()
        }
    }

    /// Whether a first sign-in through this provider makes an account.
    #[must_use]
    pub fn registration(self, config: &Config) -> crate::config::ProviderRegistration {
        self.oauth(config)
            .map_or(config.auth.oidc.allow_registration, |oauth| {
                oauth.allow_registration
            })
    }

    /// Whether an address from this provider may attach to an account that
    /// already has it, given whether the provider said it checked it.
    #[must_use]
    pub fn may_link(self, config: &Config, verified: bool) -> bool {
        self.oauth(config).map_or_else(
            || config.auth.oidc.link_by_email.allows(verified),
            |oauth| oauth.link_by_verified_email && verified,
        )
    }
}

/// Who signed in, in the one shape everything after this understands.
///
/// Deliberately the same fields `auth::oidc::claims` produces, so the code that
/// turns one of these into an account does not know which kind of provider it
/// came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The provider's own identifier, and never a name.
    ///
    /// Discord's documentation says a username is "not unique across the
    /// platform". A GitHub login can be changed, and a released one can be
    /// claimed by somebody else. Keying on either is an account takeover
    /// waiting for somebody to notice, so this is the numeric id and the
    /// snowflake.
    pub subject: String,
    /// The address, when the provider released one it says it has checked.
    pub email: Option<String>,
    /// Whether the provider says it checked it.
    pub email_verified: bool,
    /// What the provider calls them, for choosing a username.
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
    /// It answered, but not with something this understands.
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
    /// The two addresses this provider's flow uses.
    ///
    /// Constants, except where an operator runs their own GitHub. `None` for
    /// OIDC, whose endpoints come out of discovery and change with the issuer.
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

    /// The provider's own address, when one is configured.
    ///
    /// Only GitHub reads it, and only because GitHub Enterprise Server exists.
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
            // An OIDC identity comes out of the ID token, which arrived with
            // the exchange. Nothing to ask.
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
/// That one asks for an ID token and reads one back. This asks for nothing but
/// an access token, and GitHub answers form-encoded unless told otherwise, so
/// the `Accept` header is doing real work rather than being polite.
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
            // GitHub returns `access_token=...&scope=...` without this, which a
            // JSON parser reports as a failure with nothing useful in it.
            .header(reqwest::header::ACCEPT, "application/json"),
    )
    .await
}

/// A client with the timeout every one of these calls wants.
///
/// Short, because somebody is watching a browser tab.
fn client(provider: &'static str) -> Result<reqwest::Client, IdentityError> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|error| IdentityError::Unreachable {
            provider,
            reason: error.to_string(),
        })
}

/// Sends a request and reads the answer, with the same three failures named
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

    /// A name is what goes on a button, so an empty one is a button with
    /// nothing written on it.
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

    /// Nothing is offered until it is configured, and each is configured on its
    /// own.
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
