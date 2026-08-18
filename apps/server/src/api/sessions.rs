//! Seeing and ending your own sessions.
//!
//! # Why a cookie and not a token
//!
//! All three endpoints refuse a bearer token, which is stricter than the token
//! endpoints next door. Two different reasons, and both matter:
//!
//! Ending sessions with a token would mean a leaked token can throw the owner
//! off every device they have — the opposite of what a person does when they
//! discover a leak.
//!
//! Listing them with a token would hand it the addresses and browsers the owner
//! signs in from. That is the most identifying thing this service stores about
//! anybody, and a token exists to upload files.

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use tessera_core::SessionId;
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    api::{SESSIONS_TAG, extract::ApiPath, tokens::format_timestamp},
    auth::extract::CurrentUser,
    db::repository::session::{Revocation, Session, SessionEnding},
    error::{Error, Result},
    state::AppState,
};

/// One sign-in, as the API describes it.
///
/// Covers sessions that are over as well as the ones that work. Somebody reading
/// this list is usually asking whether anybody else has been in, and a session
/// that was revoked last Tuesday is exactly what they need to see.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionResponse {
    /// Stable identifier, for [`revoke`].
    #[schema(example = "01K1ZQK9V8QF5S3T4M2N6P7R8S")]
    pub id: String,
    /// Whether this is the session reading the list.
    ///
    /// The one thing a person cannot work out for themselves, and the one they
    /// need before ending anything.
    pub current: bool,
    /// Whether it still works.
    ///
    /// Sent rather than left to the client to derive from `expires_at`: a
    /// browser's clock can be wrong by more than a session's remaining life, and
    /// being told "this is still live" by a machine with a fast clock is how
    /// somebody decides not to revoke something they should have.
    pub active: bool,
    /// When the sign-in happened.
    pub created_at: String,
    /// When this session was last used.
    pub last_seen_at: String,
    /// When it stops, or stopped, working.
    pub expires_at: String,
    /// Where the sign-in came from, when there was an answer worth recording.
    ///
    /// `null` on an instance that sits behind an undeclared proxy, and on
    /// sessions that predate this being recorded.
    #[schema(example = "203.0.113.9")]
    pub created_ip: Option<String>,
    /// The `User-Agent` the sign-in arrived with, exactly as it was sent.
    ///
    /// Not parsed into "Chrome on macOS": doing that reliably is famously hard,
    /// and getting it wrong tells somebody a session is theirs when it is not.
    #[schema(example = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Safari/605.1.15")]
    pub created_user_agent: Option<String>,
    /// Why it ended early, if somebody ended it.
    ///
    /// `null` means it simply ran out. The values are `signed_out`, `revoked`,
    /// `password_changed`, `password_reset` and `account_disabled`.
    #[schema(example = "revoked")]
    pub ended_reason: Option<String>,
}

impl SessionResponse {
    fn build(session: &Session, current: Option<SessionId>, now: OffsetDateTime) -> Self {
        Self {
            id: session.id.to_string(),
            current: current == Some(session.id),
            active: session.is_valid_at(now),
            created_at: format_timestamp(session.created_at),
            last_seen_at: format_timestamp(session.last_seen_at),
            expires_at: format_timestamp(session.expires_at),
            created_ip: session.created_ip.clone(),
            created_user_agent: session.created_user_agent.clone(),
            ended_reason: session
                .ended_reason
                .map(|reason| reason.as_str().to_owned()),
        }
    }
}

/// Refuses anything that did not arrive with a session cookie.
///
/// Returns the calling session's identifier, which every handler here needs
/// anyway — to mark the current row, or to spare it.
fn require_session(current: &CurrentUser) -> Result<SessionId> {
    current.credential.session_id().ok_or(Error::Forbidden)
}

/// Lists the caller's sign-ins, newest first.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when a bearer token was used or the
/// account still owes a password change.
#[utoipa::path(
    get,
    path = "/sessions",
    operation_id = "list_sessions",
    tag = SESSIONS_TAG,
    responses(
        (status = 200, description = "The caller's sign-ins", body = Vec<SessionResponse>),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token, or a password change is due", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<SessionResponse>>> {
    let here = require_session(&current)?;
    let now = OffsetDateTime::now_utc();
    let sessions = state.sessions.find_for_user(current.user.id).await?;
    Ok(Json(
        sessions
            .iter()
            .map(|session| SessionResponse::build(session, Some(here), now))
            .collect(),
    ))
}

/// Ends one of the caller's sessions.
///
/// # Why an identifier that is not yours is a `404`
///
/// The same answer as one that never existed, and as one that is not an
/// identifier at all. Telling them apart would make this endpoint a way to ask
/// which identifiers exist and who they belong to.
///
/// # Why a session that had already finished is a `204`
///
/// The caller asked for it to stop working, and it does not work. Answering
/// `404` for a row they can see in their own list would be a puzzle, and would
/// mean a list that went stale between loading and clicking reports a problem
/// where there is none. Nothing is written, so the time it ended does not move.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when a bearer token was used, `404`
/// when no session of the caller's has that identifier.
#[utoipa::path(
    delete,
    path = "/sessions/{id}",
    operation_id = "revoke_session",
    tag = SESSIONS_TAG,
    params(("id" = String, Path, description = "Which session to end")),
    responses(
        (status = 204, description = "It no longer works"),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token, or a password change is due", body = crate::error::ErrorBody),
        (status = 404, description = "No session of yours has that id", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn revoke(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<String>,
) -> Result<StatusCode> {
    require_session(&current)?;

    // An unparsable identifier and somebody else's are the same answer.
    let session_id = SessionId::parse(&id).map_err(|_| Error::NotFound)?;

    match state
        .sessions
        .revoke(session_id, current.user.id, OffsetDateTime::now_utc())
        .await?
    {
        Revocation::Ended | Revocation::AlreadyOver => Ok(StatusCode::NO_CONTENT),
        Revocation::NotYours => Err(Error::NotFound),
    }
}

/// Ends every session except the one asking.
///
/// The button somebody presses when they think another person is signed in as
/// them. It spares the browser doing the asking, because being logged out by
/// your own attempt to log everybody else out is a poor way to find out it
/// worked.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when a bearer token was used.
#[utoipa::path(
    delete,
    path = "/sessions",
    operation_id = "revoke_other_sessions",
    tag = SESSIONS_TAG,
    responses(
        (status = 204, description = "Every other session is over"),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token, or a password change is due", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn revoke_others(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<StatusCode> {
    let here = require_session(&current)?;
    state
        .sessions
        .end_all_for_user(
            current.user.id,
            Some(here),
            SessionEnding::Revoked,
            OffsetDateTime::now_utc(),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
