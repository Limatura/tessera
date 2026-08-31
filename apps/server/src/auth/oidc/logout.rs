//! Reading a logout token, which is the one token here whose signature matters.
//!
//! # Why this one is verified and the ID token is not
//!
//! An ID token arrives in the body of a response to a request this server made,
//! to an address that came out of the configured issuer's own document, over a
//! connection whose certificate was checked. `claims.rs` sets out why the
//! specification lets that stand in for checking the signature.
//!
//! A logout token arrives the other way round: somebody posts it to a public
//! endpoint and says it is from the provider. Nothing about the connection says
//! who sent it, so the signature is the only thing that does. That is why
//! `jwks.rs` exists, and why this refuses `alg: none` and anything symmetric
//! rather than trusting what the header asks for.
//!
//! Everything checked here is from `OpenID` Connect Back-Channel Logout 1.0
//! section 2.6.

use std::collections::HashSet;

use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use time::OffsetDateTime;

/// The claim that says this is a logout and not some other token the provider
/// happens to have signed for us.
const LOGOUT_EVENT: &str = "http://schemas.openid.net/event/backchannel-logout";

/// How far in the past an `iat` may be.
///
/// A logout is acted on within seconds of being issued. A token older than this
/// is one somebody kept, and replaying it is the thing this bounds.
const MAX_AGE_SECONDS: i64 = 300;

/// How much the two clocks may disagree.
const CLOCK_SKEW_SECONDS: u64 = 60;

/// Which signatures are accepted.
///
/// Asymmetric only. A shared secret would let anyone who can read the
/// configuration mint one of these, and the client secret is exactly that.
const ALLOWED: [Algorithm; 6] = [
    Algorithm::RS256,
    Algorithm::RS384,
    Algorithm::RS512,
    Algorithm::ES256,
    Algorithm::ES384,
    Algorithm::PS256,
];

/// What a logout token says.
#[derive(Debug, Clone, Deserialize)]
pub struct LogoutClaims {
    /// The provider's identifier for the person, when it named one.
    #[serde(default)]
    pub sub: Option<String>,
    /// The provider's own session identifier, when it named one.
    ///
    /// Present when the provider is ending one session rather than all of
    /// somebody's. It is the `sid` that was in the ID token.
    #[serde(default)]
    pub sid: Option<String>,
    /// When the provider issued it.
    pub iat: i64,
    /// What kind of token this is.
    #[serde(default)]
    events: serde_json::Map<String, serde_json::Value>,
    /// Must be absent. See [`LogoutError::LooksLikeAnIdToken`].
    #[serde(default)]
    nonce: Option<String>,
}

/// Why a logout token could not be believed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LogoutError {
    /// It is not a JWT, or its header cannot be read.
    #[error("that is not a logout token")]
    Malformed,
    /// It is signed with something this will not accept, or not signed at all.
    #[error("a logout token must be signed with an asymmetric algorithm")]
    BadAlgorithm,
    /// Its header names no key.
    #[error("the logout token does not say which key signed it")]
    NoKeyId,
    /// The signature, issuer, audience or expiry did not hold up.
    #[error("the logout token could not be verified: {0}")]
    NotVerified(String),
    /// It carries no `events` claim naming a back-channel logout.
    #[error("that token is not a logout")]
    NotALogout,
    /// It names neither a subject nor a session, so there is nothing to end.
    #[error("the logout token says nothing about who to sign out")]
    NoSubject,
    /// It carries a `nonce`, which a logout token must not.
    ///
    /// The specification forbids it precisely so that an ID token cannot be
    /// replayed here as a logout.
    #[error("that looks like an id token, not a logout token")]
    LooksLikeAnIdToken,
    /// It was issued too long ago to be acted on.
    #[error("the logout token is too old to act on")]
    TooOld,
}

/// The key id a token says signed it.
///
/// Read before verification, because the key has to be found before anything
/// can be checked with it. Nothing from the header is trusted beyond choosing
/// which key to try and refusing an algorithm outright.
///
/// # Errors
///
/// Returns [`LogoutError`] when the header is unreadable, names no key, or
/// asks for an algorithm this will not accept.
pub fn key_id(token: &str) -> Result<String, LogoutError> {
    let header = jsonwebtoken::decode_header(token).map_err(|_ignored| LogoutError::Malformed)?;
    if !ALLOWED.contains(&header.alg) {
        return Err(LogoutError::BadAlgorithm);
    }
    header.kid.ok_or(LogoutError::NoKeyId)
}

/// Verifies a logout token and returns what it says.
///
/// # Errors
///
/// Returns [`LogoutError`] naming the check that failed.
pub fn verify(
    token: &str,
    key: &DecodingKey,
    issuer: &str,
    client_id: &str,
    now: OffsetDateTime,
) -> Result<LogoutClaims, LogoutError> {
    // Pinned to the one the header names, which `key_algorithm` has already
    // checked is in `ALLOWED`. Listing the whole set here instead fails with
    // `InvalidAlgorithm`, because a key belongs to one family and the set spans
    // two, and it would be no stricter: what keeps `none` and a shared secret
    // out is the check on the way in, not the size of this list.
    let mut validation = Validation::new(key_algorithm(token)?);
    validation.leeway = CLOCK_SKEW_SECONDS;
    validation.set_issuer(&[issuer.trim().trim_end_matches('/')]);
    validation.set_audience(&[client_id.trim()]);
    // `exp` is optional in a logout token; `iat` is what bounds it, below.
    validation.validate_exp = false;
    validation.required_spec_claims = HashSet::from(["iss".to_owned(), "aud".to_owned()]);

    let decoded = jsonwebtoken::decode::<LogoutClaims>(token, key, &validation)
        .map_err(|error| LogoutError::NotVerified(error.to_string()))?;
    let claims = decoded.claims;

    // Rule 5: an ID token replayed here would otherwise end the session it was
    // issued for.
    if claims.nonce.is_some() {
        return Err(LogoutError::LooksLikeAnIdToken);
    }
    // Rule 3.
    let is_logout = claims
        .events
        .get(LOGOUT_EVENT)
        .is_some_and(serde_json::Value::is_object);
    if !is_logout {
        return Err(LogoutError::NotALogout);
    }
    // Rule 4.
    let named = |value: &Option<String>| value.as_deref().is_some_and(|it| !it.trim().is_empty());
    if !named(&claims.sub) && !named(&claims.sid) {
        return Err(LogoutError::NoSubject);
    }
    // Not in the list, but the specification says to reject one issued too far
    // in the past, and this is what makes a kept copy useless.
    let age = now.unix_timestamp() - claims.iat;
    if age > MAX_AGE_SECONDS + i64::from(u32::try_from(CLOCK_SKEW_SECONDS).unwrap_or(0)) {
        return Err(LogoutError::TooOld);
    }

    Ok(claims)
}

/// The algorithm the token asks for, once it is known to be one we accept.
fn key_algorithm(token: &str) -> Result<Algorithm, LogoutError> {
    let header = jsonwebtoken::decode_header(token).map_err(|_ignored| LogoutError::Malformed)?;
    if ALLOWED.contains(&header.alg) {
        Ok(header.alg)
    } else {
        Err(LogoutError::BadAlgorithm)
    }
}
