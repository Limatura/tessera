//! Reading an ID token, and deciding what may be believed from it.
//!
//! # Why the signature is not checked, and when that stops being allowed
//!
//! An ID token is a JWT. The usual handling fetches the provider's JWKS,
//! matches the `kid`, verifies the signature and only then reads the claims.
//! This code skips that under a conditional exemption; it is not a shortcut.
//!
//! `OpenID` Connect Core section 3.1.3.7 rule 6: when the ID token is received
//! through direct communication between the client and the token endpoint, TLS
//! server validation may be used to validate the issuer in place of checking
//! the token signature. That is this code path. The token arrives in the
//! response to a request this server made, to an `https` URL taken from a
//! document fetched from the configured issuer, over a connection whose
//! certificate was checked. Nothing untrusted has carried it.
//!
//! **The exemption ends as soon as a token arrives any other way.** An
//! implicit or hybrid flow returns the token through the browser, which is the
//! party the signature protects against. A back-channel logout token is pushed
//! to us unprompted. Either needs JWKS fetching, key rotation and signature
//! verification. Adding such a flow without them turns a documented exemption
//! into a hole.
//!
//! Everything else the spec requires is checked here: the issuer, the
//! audience, expiry, and the nonce tying the token to its sign-in.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use time::OffsetDateTime;

/// How much clock difference with the provider is tolerated.
///
/// Kept small: it covers clocks that are slightly out of step, not expired
/// tokens.
const CLOCK_SKEW_SECONDS: i64 = 60;

/// What an ID token says, before any of it is believed.
#[derive(Debug, Clone, Deserialize)]
pub struct Claims {
    /// Who issued it.
    pub iss: String,
    /// The provider's identifier for the user. Stable, and never reused within
    /// one issuer.
    pub sub: String,
    /// Who it was issued for. A string or a list, hence [`Audience`].
    pub aud: Audience,
    /// When it stops being valid, as a Unix timestamp.
    pub exp: i64,
    /// When it was issued.
    #[serde(default)]
    pub iat: Option<i64>,
    /// Ties the token to the sign-in that asked for it.
    #[serde(default)]
    pub nonce: Option<String>,
    /// The provider's own identifier for the session it just started.
    ///
    /// Optional, and only some providers send it. Stored with the session so a
    /// back-channel logout naming this `sid` ends that session only, instead of
    /// everything the account has open.
    #[serde(default)]
    pub sid: Option<String>,
    /// The address, when the provider released one.
    #[serde(default)]
    pub email: Option<String>,
    /// Whether the provider says it verified that address.
    #[serde(default)]
    pub email_verified: Option<bool>,
    /// A display name, when there is one.
    #[serde(default)]
    pub name: Option<String>,
    /// The provider's name for the user, when there is one.
    #[serde(default)]
    pub preferred_username: Option<String>,
    /// Everything else, so a configured group claim can be looked up by name
    /// without this struct knowing it.
    #[serde(flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

/// `aud` is a string when there is one audience and a list when there are more.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Audience {
    /// The ordinary case.
    One(String),
    /// A token issued for several clients at once.
    Many(Vec<String>),
}

impl Audience {
    /// Whether this token was issued for the given client.
    #[must_use]
    pub fn contains(&self, client_id: &str) -> bool {
        match self {
            Self::One(one) => one == client_id,
            Self::Many(many) => many.iter().any(|entry| entry == client_id),
        }
    }
}

/// Why an ID token could not be believed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ClaimsError {
    /// It is not three base64url parts separated by dots.
    #[error("the identity provider returned something that is not an id token")]
    Malformed,
    /// The payload is not JSON, or is missing a required claim.
    #[error("the id token's claims could not be read: {0}")]
    Unreadable(String),
    /// It names a different issuer than the configured one.
    #[error("the id token was issued by `{found}`, not `{expected}`")]
    WrongIssuer {
        /// What the token says.
        found: String,
        /// What was configured.
        expected: String,
    },
    /// It was issued for a different client.
    #[error("the id token was not issued for this client")]
    WrongAudience,
    /// It has expired.
    #[error("the id token has expired")]
    Expired,
    /// Its nonce is missing, or is not the one this sign-in sent.
    #[error("the id token does not belong to this sign-in")]
    WrongNonce,
}

/// Reads the claims from a token without checking any of them.
///
/// Separate from [`verify`] so the checks read as a list, not tangled with
/// base64 and JSON handling.
///
/// # Errors
///
/// Returns [`ClaimsError::Malformed`] or [`ClaimsError::Unreadable`] when the
/// token is not a JWT whose payload deserialises.
pub fn read(id_token: &str) -> Result<Claims, ClaimsError> {
    // header.payload.signature. The signature is unused; see the module docs
    // for why, and for when that stops being true.
    let mut parts = id_token.split('.');
    let (Some(_header), Some(payload), Some(_signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(ClaimsError::Malformed);
    };

    let decoded = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_ignored| ClaimsError::Malformed)?;
    serde_json::from_slice(&decoded).map_err(|error| ClaimsError::Unreadable(error.to_string()))
}

/// Reads a token and checks everything the spec asks for except the
/// signature.
///
/// # Errors
///
/// Returns [`ClaimsError`] naming the check that failed.
pub fn verify(
    id_token: &str,
    issuer: &str,
    client_id: &str,
    nonce: &str,
    now: OffsetDateTime,
) -> Result<Claims, ClaimsError> {
    let claims = read(id_token)?;

    // Exact string equality, as the spec requires. A trailing slash would make
    // two issuers differ, so it is trimmed on both sides.
    let expected = issuer.trim().trim_end_matches('/');
    if claims.iss.trim_end_matches('/') != expected {
        return Err(ClaimsError::WrongIssuer {
            found: claims.iss,
            expected: expected.to_owned(),
        });
    }
    if !claims.aud.contains(client_id) {
        return Err(ClaimsError::WrongAudience);
    }
    if claims.exp + CLOCK_SKEW_SECONDS <= now.unix_timestamp() {
        return Err(ClaimsError::Expired);
    }
    // A missing nonce is a failure. The sign-in sent one, so a token without it
    // is not the answer to that sign-in.
    if claims.nonce.as_deref() != Some(nonce) {
        return Err(ClaimsError::WrongNonce);
    }
    if claims.sub.trim().is_empty() {
        return Err(ClaimsError::Unreadable("it has no subject".to_owned()));
    }

    Ok(claims)
}

/// The groups a claim holds, in whatever shape the provider used.
///
/// Providers differ: a list of strings, a single string, or a list of objects
/// with a `name`. Anything else yields nothing, which fails closed when a
/// required group is configured.
#[must_use]
pub fn groups(value: Option<&serde_json::Value>) -> Vec<String> {
    match value {
        Some(serde_json::Value::String(one)) => vec![one.clone()],
        Some(serde_json::Value::Array(many)) => many
            .iter()
            .filter_map(|entry| match entry {
                serde_json::Value::String(name) => Some(name.clone()),
                serde_json::Value::Object(fields) => fields
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a token as a provider would, minus the signature, which nothing
    /// here reads.
    fn token(payload: &serde_json::Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
        let body = URL_SAFE_NO_PAD.encode(payload.to_string());
        format!("{header}.{body}.not-checked")
    }

    fn now() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_785_000_000).expect("a valid timestamp")
    }

    fn payload() -> serde_json::Value {
        serde_json::json!({
            "iss": "https://id.example",
            "sub": "sub-1",
            "aud": "tessera",
            "exp": now().unix_timestamp() + 300,
            "nonce": "nonce-1",
            "email": "alice@example.com",
            "email_verified": true,
        })
    }

    fn check(payload: &serde_json::Value) -> Result<Claims, ClaimsError> {
        verify(
            token(payload).as_str(),
            "https://id.example",
            "tessera",
            "nonce-1",
            now(),
        )
    }

    #[test]
    fn a_good_token_gives_up_its_claims() {
        let claims = check(&payload()).expect("the claims");

        assert_eq!(claims.sub, "sub-1");
        assert_eq!(claims.email.as_deref(), Some("alice@example.com"));
        assert_eq!(claims.email_verified, Some(true));
    }

    #[test]
    fn something_that_is_not_a_jwt_is_refused() {
        for bad in ["", "one.two", "one.two.three.four", "one.!!!.three"] {
            assert_eq!(
                verify(bad, "https://id.example", "tessera", "nonce-1", now()).unwrap_err(),
                ClaimsError::Malformed,
                "{bad} was accepted"
            );
        }
    }

    /// A token from another issuer is the beginning of a mix-up attack.
    #[test]
    fn another_issuers_token_is_refused() {
        let mut claims = payload();
        claims["iss"] = serde_json::json!("https://elsewhere.example");

        assert!(matches!(
            check(&claims),
            Err(ClaimsError::WrongIssuer { .. })
        ));
    }

    /// A trailing slash does not make it another issuer.
    #[test]
    fn a_trailing_slash_is_the_same_issuer() {
        let mut claims = payload();
        claims["iss"] = serde_json::json!("https://id.example/");

        assert!(check(&claims).is_ok());
    }

    /// A token issued for another client at the same provider must not work
    /// here. That is what the audience check is for.
    #[test]
    fn another_clients_token_is_refused() {
        let mut claims = payload();
        claims["aud"] = serde_json::json!("somebody-else");

        assert_eq!(check(&claims).unwrap_err(), ClaimsError::WrongAudience);
    }

    #[test]
    fn a_list_of_audiences_counts_if_we_are_in_it() {
        let mut claims = payload();
        claims["aud"] = serde_json::json!(["somebody-else", "tessera"]);

        assert!(check(&claims).is_ok());
    }

    #[test]
    fn an_expired_token_is_refused() {
        let mut claims = payload();
        claims["exp"] = serde_json::json!(now().unix_timestamp() - 3600);

        assert_eq!(check(&claims).unwrap_err(), ClaimsError::Expired);
    }

    /// Clocks drift, so a token that expired a moment ago is still accepted.
    /// One that expired an hour ago is not.
    #[test]
    fn a_token_just_past_expiry_is_still_taken() {
        let mut claims = payload();
        claims["exp"] = serde_json::json!(now().unix_timestamp() - 5);

        assert!(check(&claims).is_ok());
    }

    /// The nonce ties the token to the sign-in that asked for it. Without it, a
    /// token obtained elsewhere could be replayed into a callback here.
    #[test]
    fn a_token_from_another_sign_in_is_refused() {
        let mut claims = payload();
        claims["nonce"] = serde_json::json!("some-other-nonce");

        assert_eq!(check(&claims).unwrap_err(), ClaimsError::WrongNonce);
    }

    #[test]
    fn a_token_with_no_nonce_at_all_is_refused() {
        let mut claims = payload();
        claims.as_object_mut().expect("an object").remove("nonce");

        assert_eq!(check(&claims).unwrap_err(), ClaimsError::WrongNonce);
    }

    #[test]
    fn a_token_with_no_subject_is_refused() {
        let mut claims = payload();
        claims["sub"] = serde_json::json!("   ");

        assert!(matches!(check(&claims), Err(ClaimsError::Unreadable(_))));
    }

    /// The signature is not read, so its contents change nothing. A test so
    /// that adding verification later fails here first and sends the author to
    /// the module docs.
    #[test]
    fn the_signature_is_not_read() {
        let good = check(&payload()).expect("the claims");
        let mut tampered = token(&payload());
        tampered.truncate(tampered.rfind('.').expect("a dot") + 1);
        tampered.push_str("a-completely-different-signature");

        let still_good = verify(&tampered, "https://id.example", "tessera", "nonce-1", now())
            .expect("the claims");
        assert_eq!(good.sub, still_good.sub);
    }

    #[test]
    fn groups_are_read_in_every_shape_providers_send() {
        assert_eq!(groups(Some(&serde_json::json!("admins"))), ["admins"]);
        assert_eq!(
            groups(Some(&serde_json::json!(["admins", "staff"]))),
            ["admins", "staff"]
        );
        assert_eq!(
            groups(Some(&serde_json::json!([{"name": "admins"}]))),
            ["admins"]
        );
    }

    /// Anything unrecognised yields nothing, so a configured required group
    /// refuses instead of admitting.
    #[test]
    fn an_unrecognised_shape_yields_no_groups() {
        assert!(groups(None).is_empty());
        assert!(groups(Some(&serde_json::json!(42))).is_empty());
        assert!(groups(Some(&serde_json::json!([{"id": 1}]))).is_empty());
    }

    /// A claim this struct does not name is still reachable. That is how a
    /// configured group claim is looked up without hard-coding its name.
    #[test]
    fn an_unnamed_claim_is_still_reachable() {
        let mut claims = payload();
        claims["https://authentik/groups"] = serde_json::json!(["admins"]);

        let read = check(&claims).expect("the claims");
        assert_eq!(
            groups(read.rest.get("https://authentik/groups")),
            ["admins"]
        );
    }
}
