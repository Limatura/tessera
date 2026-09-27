//! Exchanging the code for a token, server to server.
//!
//! The flow is built around this request. It is the only step the browser is
//! not part of, which is what makes the rest safe: see `claims.rs` for what
//! follows from the token arriving here and not through a redirect.
//!
//! # How this client authenticates
//!
//! With a secret configured, HTTP basic authentication, which every provider
//! must support and which keeps the secret out of the request body and so out
//! of anything that logs bodies.
//!
//! Without a secret, the client only identifies itself in the body. That is a
//! public client, safe only because PKCE is always sent: the code is useless
//! without the verifier, which never leaves this process.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::Deserialize;

use super::discovery::Provider;
use crate::config::Config;

/// How long to wait on the token endpoint. A user is waiting on a browser tab.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// What the token endpoint returned.
#[derive(Debug, Clone, Deserialize)]
pub struct Tokens {
    /// The token this flow exists to obtain.
    pub id_token: String,
    /// Fetches the claims missing from the ID token, from the userinfo endpoint.
    #[serde(default)]
    pub access_token: Option<String>,
}

/// What the token endpoint returned when it refused.
///
/// Kept structured, not flattened into one message: `invalid_grant` from an
/// already-used code and `invalid_client` from a wrong secret are different
/// problems for the operator.
#[derive(Debug, Clone, Deserialize)]
struct Refusal {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

/// Why a code could not be exchanged.
#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    /// The token endpoint could not be reached.
    #[error("could not reach the identity provider's token endpoint: {0}")]
    Unreachable(String),
    /// The provider refused the exchange.
    #[error("the identity provider refused the exchange: {0}")]
    Refused(String),
    /// The provider answered with something that is not a token response.
    #[error("the identity provider's token response could not be read: {0}")]
    Unreadable(String),
}

/// Trades the authorisation code for tokens.
///
/// # Errors
///
/// Returns [`TokenError`] when the endpoint cannot be reached, refuses, or
/// answers with something unreadable.
pub async fn exchange(
    provider: &Provider,
    config: &Config,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<Tokens, TokenError> {
    let client_id = config.auth.oidc.client_id.trim();
    let secret = config.auth.oidc.client_secret.trim();

    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("code_verifier", verifier),
        // Always sent, secret or not: a provider matches the code against the
        // client it was issued to, and basic auth is not the only place it may
        // look.
        ("client_id", client_id),
    ];
    if secret.is_empty() {
        // A public client. The verifier above stands in for the secret.
    } else {
        form.push(("client_secret", secret));
    }

    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|error| TokenError::Unreachable(error.to_string()))?;
    let mut request = client.post(&provider.token_endpoint).form(&form);
    if !secret.is_empty() {
        // Both are sent: providers must accept basic, and those configured for
        // `client_secret_post` read the body. Each uses the one it is set up
        // for and ignores the other.
        request = request.header(reqwest::header::AUTHORIZATION, basic(client_id, secret));
    }

    let response = request
        .send()
        .await
        .map_err(|error| TokenError::Unreachable(error.to_string()))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| TokenError::Unreadable(error.to_string()))?;

    if !status.is_success() {
        // The spec says a refusal is a JSON object naming the reason. When it
        // is not, the status is all there is to report.
        let refusal = serde_json::from_str::<Refusal>(&body).map_or_else(
            |_ignored| format!("it answered {status}"),
            |refusal| match refusal.error_description {
                Some(description) => format!("{} ({description})", refusal.error),
                None => refusal.error,
            },
        );
        return Err(TokenError::Refused(refusal));
    }

    serde_json::from_str(&body).map_err(|error| TokenError::Unreadable(error.to_string()))
}

/// Asks the userinfo endpoint for the claims the ID token did not carry.
///
/// Some providers leave `email` and the group claim out of the token to keep
/// it small and release them here. No userinfo endpoint, or a refusal, yields
/// nothing instead of failing the sign-in: the caller knows what it needed and
/// decides.
pub async fn userinfo(
    provider: &Provider,
    access_token: &str,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let endpoint = provider.userinfo_endpoint.as_deref()?;
    let client = reqwest::Client::builder().timeout(TIMEOUT).build().ok()?;
    let response = client.get(endpoint).bearer_auth(access_token).send().await;

    match response {
        Ok(response) if response.status().is_success() => response.json().await.ok(),
        Ok(response) => {
            tracing::debug!(status = %response.status(), "the userinfo endpoint refused");
            None
        }
        Err(error) => {
            tracing::debug!(%error, "the userinfo endpoint could not be reached");
            None
        }
    }
}

/// An HTTP basic authentication header value.
///
/// The spec requires both halves to be form-urlencoded before joining, since a
/// colon in either would split the pair in the wrong place.
fn basic(client_id: &str, secret: &str) -> String {
    let encode =
        |value: &str| form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>();
    let encoded = BASE64.encode(format!("{}:{}", encode(client_id), encode(secret)));
    format!("Basic {encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A colon in a secret would split the pair in the wrong place if the
    /// halves were joined raw, which is why the spec encodes them.
    #[test]
    fn the_basic_header_encodes_both_halves() {
        let header = basic("tessera", "se:cr et");

        let encoded = header.strip_prefix("Basic ").expect("the prefix");
        let decoded = BASE64.decode(encoded).expect("base64");
        assert_eq!(
            String::from_utf8(decoded).expect("utf-8"),
            "tessera:se%3Acr+et"
        );
    }

    #[test]
    fn an_ordinary_secret_survives_the_round_trip() {
        let header = basic("tessera", "plain-secret");

        let encoded = header.strip_prefix("Basic ").expect("the prefix");
        let decoded = BASE64.decode(encoded).expect("base64");
        assert_eq!(
            String::from_utf8(decoded).expect("utf-8"),
            "tessera:plain-secret"
        );
    }

    #[test]
    fn a_refusal_is_reported_with_its_reason() {
        let refusal: Refusal = serde_json::from_str(
            r#"{"error":"invalid_grant","error_description":"code already used"}"#,
        )
        .expect("a refusal");

        assert_eq!(refusal.error, "invalid_grant");
        assert_eq!(
            refusal.error_description.as_deref(),
            Some("code already used")
        );
    }

    #[test]
    fn a_token_response_reads_the_two_fields_that_matter() {
        let tokens: Tokens = serde_json::from_str(
            r#"{"id_token":"a.b.c","access_token":"at","token_type":"Bearer","expires_in":300}"#,
        )
        .expect("a token response");

        assert_eq!(tokens.id_token, "a.b.c");
        assert_eq!(tokens.access_token.as_deref(), Some("at"));
    }

    /// A provider that releases everything in the token need not send an
    /// access token at all.
    #[test]
    fn an_access_token_is_optional() {
        let tokens: Tokens =
            serde_json::from_str(r#"{"id_token":"a.b.c"}"#).expect("a token response");

        assert!(tokens.access_token.is_none());
    }
}
