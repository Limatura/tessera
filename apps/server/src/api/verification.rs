//! Proving an account can receive mail at its address.
//!
//! Registration proves it once by making the account wait for a link. Nothing
//! proved it for an address changed later, or for an account that never
//! waited: the first administrator, or one registered while mail was not
//! configured. These two endpoints cover that: a link sent on request, and
//! the page it opens.
//!
//! Nothing on the server depends on the result yet. It is shown to the user as
//! `email_verified_at` on their account, so they can see which address they
//! have confirmed.

use axum::{extract::State, http::StatusCode};
use serde::Deserialize;
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    api::{AUTH_TAG, auth::require_accounts, extract::ApiJson},
    auth::{extract::CurrentUser, secret::Secret, throttle::Decision},
    error::{Error, ErrorCode, Result},
    mail::verification_letter,
    state::AppState,
};

/// Body of `POST /account/email/verification/confirm`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VerificationConfirmRequest {
    /// The token from the link.
    pub token: String,
}

/// The single answer for a link that cannot be used.
///
/// Unknown, used, expired, and "sent to an address the account no longer has"
/// all get the same message. Telling them apart would let the holder of a
/// stale link learn it was once valid.
fn not_usable() -> Error {
    Error::new(ErrorCode::VerificationLinkUnusable)
}

/// Sends the caller a link proving their address is theirs.
///
/// Sent while the request waits, unlike a password reset: the caller is signed
/// in and knows their own address, so timing reveals nothing, and a down relay
/// is better reported than hidden.
///
/// # Errors
///
/// `401` when not signed in, `403` when an API token was used, `404` when this
/// instance has no accounts or sends no mail, `409` when the address is already
/// confirmed, `429` when that address has hit the send limit for the window,
/// `503` when the relay would not take the message.
#[utoipa::path(
    post,
    path = "/account/email/verification",
    operation_id = "request_email_verification",
    tag = AUTH_TAG,
    responses(
        (status = 202, description = "A link is on its way to the account's address"),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no accounts, or sends no mail", body = crate::error::ErrorBody),
        (status = 409, description = "The address is already confirmed", body = crate::error::ErrorBody),
        (status = 429, description = "Too many links have been sent to this address", body = crate::error::ErrorBody),
        (status = 503, description = "The mail relay would not take the message", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn request(State(state): State<AppState>, current: CurrentUser) -> Result<StatusCode> {
    require_accounts(&state)?;
    let Some(mailer) = state.mailer().as_ref().clone() else {
        return Err(Error::NotFound);
    };
    // A leaked API token must not be usable to mail the account owner
    // repeatedly, and no script needs this.
    if !current.credential.is_session() {
        return Err(Error::new(ErrorCode::SessionRequired));
    }
    if current.user.email_verified_at.is_some() {
        return Err(Error::new(ErrorCode::EmailAlreadyVerified));
    }

    // Keyed on the address, since that is what receives the mail. An account
    // can set its address to anyone's, and this must not become a way to flood
    // a stranger's inbox.
    let email = current.user.email.clone();
    let key = email.as_str().to_lowercase();
    let counter = &state.throttles.verification;
    if let Decision::Deny { retry_after } = counter.check(&key) {
        return Err(Error::TooManyRequests {
            retry_after_secs: retry_after.as_secs(),
        });
    }
    counter.record_failure(&key);

    let now = OffsetDateTime::now_utc();
    let (ttl, hours, public_url) = {
        let config = state.config();
        (
            time::Duration::try_from(config.mail.registration_ttl())
                .unwrap_or_else(|_| time::Duration::hours(24)),
            config.mail.registration_ttl_hours,
            config.mail.public_url.trim_end_matches('/').to_owned(),
        )
    };
    let secret = state
        .verifications
        .issue(current.user.id, &email, now, now.saturating_add(ttl))
        .await?;
    let link = format!("{public_url}/verify-email?token={}", secret.expose());

    mailer
        .send(verification_letter(email.as_str(), &link, hours))
        .await
        .map_err(|error| {
            tracing::error!(?error, "could not send an address verification link");
            Error::Unavailable
        })?;
    Ok(StatusCode::ACCEPTED)
}

/// Confirms an address from the link sent to it.
///
/// Needs no session: the link is opened wherever the mail is read, often not
/// where the account is signed in. Holding the link is the proof.
///
/// # Errors
///
/// `400` when the link is unknown, used, expired, or was sent to an address the
/// account no longer has. `404` when this instance has no accounts or sends no
/// mail.
#[utoipa::path(
    post,
    path = "/account/email/verification/confirm",
    operation_id = "confirm_email_verification",
    tag = AUTH_TAG,
    request_body = VerificationConfirmRequest,
    responses(
        (status = 204, description = "The address is confirmed"),
        (status = 400, description = "The link is not usable", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no accounts, or sends no mail", body = crate::error::ErrorBody),
    ),
)]
pub async fn confirm(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<VerificationConfirmRequest>,
) -> Result<StatusCode> {
    require_accounts(&state)?;
    if state.mailer().is_none() {
        return Err(Error::NotFound);
    }

    let now = OffsetDateTime::now_utc();
    let found = state
        .verifications
        .find_by_secret(&Secret::from_client(body.token))
        .await?;
    let Some(link) = found.filter(|link| link.is_usable_at(now)) else {
        return Err(not_usable());
    };

    // The database arbitrates, so two clicks confirm only once.
    if !state.verifications.spend(link.id, now).await? {
        return Err(not_usable());
    }
    // Only if the account still has the address the link was sent to.
    // Otherwise mail that went to the old address would confirm the new one.
    if !state
        .users
        .mark_email_verified(link.user_id, &link.email, now)
        .await?
    {
        return Err(not_usable());
    }
    Ok(StatusCode::NO_CONTENT)
}
