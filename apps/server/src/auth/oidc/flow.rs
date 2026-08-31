//! Leaving for the provider, and what has to be remembered until the browser
//! comes back.
//!
//! # The redirect URI comes from configuration, never from the request
//!
//! Everywhere else the origin falls back to the `Host` header when
//! `server.public_url` is unset, because getting it slightly wrong there means
//! a preview card with a broken image. Here it would mean sending the
//! authorisation code somewhere else: whoever controls the header controls
//! where the provider delivers it.
//!
//! So this reads `server.public_url` and refuses when it is empty. A provider
//! also matches the redirect URI by exact string equality against what was
//! registered with it, so a value that changes per request could not be
//! registered anyway.
//!
//! # PKCE is not optional here
//!
//! The specification lets a confidential client skip it, on the grounds that
//! the secret already proves who is exchanging the code. It is still sent,
//! because it is the only thing that helps when a code leaks through a redirect
//! chain, a proxy log or browser history, and it costs one hash.

use argon2::password_hash::rand_core::{OsRng, RngCore as _};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest as _, Sha256};
use url::Url;

use crate::config::Config;

/// Bytes behind each random value. 32 is what the rest of the server uses for
/// anything that must not be guessed.
const RANDOM_BYTES: usize = 32;

/// The path a provider sends the browser back to.
///
/// One per provider, because a provider matches this by exact string equality
/// against what was registered with it, and two of them sharing an address
/// would leave the callback guessing which one answered.
#[must_use]
pub fn callback_path(provider: &str) -> String {
    format!("/api/v1/auth/{provider}/callback")
}

/// What a sign-in that has left for the provider is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// Signing in, and registering first if this subject is unknown.
    Login,
    /// Attaching a provider to the account that is already signed in.
    Link,
}

impl Purpose {
    /// How the purpose is stored.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Link => "link",
        }
    }

    /// Reads back what [`Self::as_str`] wrote.
    ///
    /// An unrecognised value is [`Self::Login`], which is the narrower of the
    /// two: linking needs an account to link to, and treating a corrupt row as
    /// a link would attach an identity to whoever happened to be signed in.
    #[must_use]
    pub fn from_stored(value: &str) -> Self {
        if value == "link" {
            Self::Link
        } else {
            Self::Login
        }
    }
}

/// The secret that stays here and the challenge that goes to the provider.
#[derive(Debug, Clone)]
pub struct Pkce {
    /// Kept in `oidc_flows` and sent to the token endpoint.
    pub verifier: String,
    /// Sent to the authorisation endpoint. The SHA-256 of the verifier.
    pub challenge: String,
}

impl Pkce {
    /// A fresh pair.
    #[must_use]
    pub fn generate() -> Self {
        let verifier = random_token();
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        Self {
            verifier,
            challenge,
        }
    }
}

/// A high-entropy value in the alphabet a URL takes without escaping.
#[must_use]
pub fn random_token() -> String {
    let mut bytes = [0_u8; RANDOM_BYTES];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Why a sign-in could not be started.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// `server.public_url` is unset, so there is no address to come back to.
    #[error("set server.public_url before signing in with an identity provider")]
    NoPublicUrl,
    /// The provider's authorisation endpoint is not a URL.
    #[error("the identity provider's authorization endpoint is not a url: {0}")]
    BadEndpoint(String),
}

/// Where to send the browser, and what to remember while it is away.
#[derive(Debug, Clone)]
pub struct Start {
    /// The address to redirect to.
    pub url: String,
    /// Proves the callback belongs to a redirect we sent.
    pub state: String,
    /// Binds the returned token to this attempt, when there is a token.
    pub nonce: Option<String>,
    /// The PKCE secret, which never leaves this server.
    pub verifier: String,
}

/// Builds the redirect and the values it has to be checked against later.
///
/// # Errors
///
/// Returns [`StartError::NoPublicUrl`] when there is no configured address for
/// the provider to come back to.
pub fn start(leaving: Leaving<'_>) -> Result<Start, StartError> {
    let redirect_uri = redirect_uri(leaving.config, leaving.provider)?;
    let pkce = Pkce::generate();
    let state = random_token();
    // Generated for every provider and sent only to the ones that will echo it
    // back. A provider with no ID token has nothing to put it in, and storing
    // one it was never given would make the callback compare against a value
    // that could never match.
    let nonce = leaving.wants_nonce.then(random_token);

    // Appending to the parsed URL rather than to the string: an authorisation
    // endpoint is allowed to carry a query of its own, and at least one
    // provider uses it to select a tenant. Building the string by hand would
    // have to get the separator right and escape every value.
    let mut url = Url::parse(leaving.authorize)
        .map_err(|error| StartError::BadEndpoint(error.to_string()))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", leaving.client_id.trim())
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("scope", &leaving.scopes.join(" "))
        .append_pair("state", &state)
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256");
    if let Some(nonce) = nonce.as_deref() {
        url.query_pairs_mut().append_pair("nonce", nonce);
    }

    Ok(Start {
        url: url.into(),
        state,
        nonce,
        verifier: pkce.verifier,
    })
}

/// What a sign-in needs to know before it can leave.
///
/// A struct because the pieces come from three places: the provider's name,
/// its endpoints (a constant for some, discovered for others), and this
/// instance's own configuration.
#[derive(Debug, Clone, Copy)]
pub struct Leaving<'a> {
    /// Which provider, as it is stored and as it appears in the callback path.
    pub provider: &'a str,
    /// Where the browser is sent.
    pub authorize: &'a str,
    /// This installation's identifier at the provider.
    pub client_id: &'a str,
    /// What to ask for.
    pub scopes: &'a [String],
    /// This instance's configuration, for the address to come back to.
    pub config: &'a Config,
    /// Whether the provider will echo a nonce back inside a token.
    pub wants_nonce: bool,
}

/// The address the provider sends the browser back to.
///
/// # Errors
///
/// Returns [`StartError::NoPublicUrl`] when `server.public_url` is empty.
pub fn redirect_uri(config: &Config, provider: &str) -> Result<String, StartError> {
    let base = config.server.public_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(StartError::NoPublicUrl);
    }
    Ok(format!("{base}{}", callback_path(provider)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An OIDC sign-in leaving for the given endpoint.
    fn leaving<'a>(authorize: &'a str, config: &'a Config, scopes: &'a [String]) -> Leaving<'a> {
        Leaving {
            provider: "oidc",
            authorize,
            client_id: "tessera",
            scopes,
            config,
            wants_nonce: true,
        }
    }

    fn scopes() -> Vec<String> {
        vec!["openid".to_owned(), "email".to_owned()]
    }

    fn config() -> Config {
        let mut config = Config::default();
        config.server.public_url = "https://tessera.example/".to_owned();
        config.auth.oidc.issuer = "https://id.example".to_owned();
        config.auth.oidc.client_id = "tessera".to_owned();
        config
    }

    /// The challenge is the hash of the verifier, which is the whole point: the
    /// provider sees only the hash, so a stolen redirect cannot be exchanged.
    #[test]
    fn the_challenge_is_the_hash_of_the_verifier() {
        let pkce = Pkce::generate();

        let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(pkce.verifier.as_bytes()));
        assert_eq!(pkce.challenge, expected);
        assert_ne!(pkce.challenge, pkce.verifier);
    }

    #[test]
    fn two_flows_share_nothing() {
        let first = start(leaving(
            "https://id.example/authorize",
            &config(),
            &scopes(),
        ))
        .expect("a start");
        let second = start(leaving(
            "https://id.example/authorize",
            &config(),
            &scopes(),
        ))
        .expect("a start");

        assert_ne!(first.state, second.state);
        assert_ne!(first.nonce, second.nonce);
        assert_ne!(first.verifier, second.verifier);
    }

    #[test]
    fn the_redirect_carries_everything_the_provider_needs() {
        let start = start(leaving(
            "https://id.example/authorize",
            &config(),
            &scopes(),
        ))
        .expect("a start");

        for pair in [
            "response_type=code",
            "client_id=tessera",
            "code_challenge_method=S256",
            "redirect_uri=https%3A%2F%2Ftessera.example%2Fapi%2Fv1%2Fauth%2Foidc%2Fcallback",
        ] {
            assert!(
                start.url.contains(pair),
                "{pair} missing from {}",
                start.url
            );
        }
        assert!(start.url.contains(&format!("state={}", start.state)));
        let nonce = start.nonce.as_deref().expect("an OIDC start carries one");
        assert!(start.url.contains(&format!("nonce={nonce}")));
        // The verifier is the half that stays here.
        assert!(!start.url.contains(&start.verifier));
    }

    /// An endpoint that already carries a query keeps it.
    #[test]
    fn an_endpoint_with_a_query_keeps_it() {
        let start = start(leaving(
            "https://id.example/authorize?tenant=acme",
            &config(),
            &scopes(),
        ))
        .expect("a start");

        assert!(start.url.contains("tenant=acme"));
        assert!(start.url.contains("&response_type=code"));
    }

    /// Without a configured address there is nowhere safe to come back to, and
    /// guessing from the request is how a code gets delivered to somebody else.
    #[test]
    fn no_public_url_is_a_refusal() {
        let mut config = config();
        config.server.public_url = String::new();

        assert!(matches!(
            start(leaving("https://id.example/authorize", &config, &scopes())),
            Err(StartError::NoPublicUrl)
        ));
    }

    #[test]
    fn a_trailing_slash_does_not_double_up() {
        let mut config = config();
        config.server.public_url = "https://tessera.example/".to_owned();

        assert_eq!(
            redirect_uri(&config, "oidc").expect("a redirect uri"),
            "https://tessera.example/api/v1/auth/oidc/callback"
        );
    }

    /// A provider with no ID token gets no nonce, and the redirect does not
    /// carry one. Sending it anyway would be asking for an echo that cannot
    /// come back.
    #[test]
    fn a_provider_with_no_token_gets_no_nonce() {
        let config = config();
        let scopes = scopes();
        let mut leaving = leaving("https://github.com/login/oauth/authorize", &config, &scopes);
        leaving.provider = "github";
        leaving.wants_nonce = false;

        let start = start(leaving).expect("a start");

        assert!(start.nonce.is_none());
        assert!(!start.url.contains("nonce="), "{}", start.url);
        // The state and the verifier are still there: they are what is left
        // tying the callback to this sign-in.
        assert!(start.url.contains(&format!("state={}", start.state)));
        assert!(start.url.contains("code_challenge_method=S256"));
        assert!(start.url.contains(
            "redirect_uri=https%3A%2F%2Ftessera.example%2Fapi%2Fv1%2Fauth%2Fgithub%2Fcallback"
        ));
    }

    #[test]
    fn an_unknown_purpose_reads_back_as_login() {
        assert_eq!(Purpose::from_stored("link"), Purpose::Link);
        assert_eq!(Purpose::from_stored("login"), Purpose::Login);
        assert_eq!(Purpose::from_stored("nonsense"), Purpose::Login);
    }
}
