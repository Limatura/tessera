//! Signing in a program that has no browser, by approving it from one that has.
//!
//! The device authorization grant of RFC 8628, in this API's own shapes. The
//! command line client is what it is for: it may be running over SSH on a
//! machine with no browser at all, so it cannot send one to a sign-in page and
//! wait for it to come back.
//!
//! 1. The program calls `POST /auth/device` and gets a device code, which it keeps, and a short
//!    user code, which it shows.
//! 2. The person opens `/device` on the site, signs in as usual, enters the user code and approves.
//! 3. The program, polling `POST /auth/device/token` with its device code all along, gets an API
//!    token on the first poll after the approval.
//!
//! Anyone can start a request, and only a signed-in browser can answer one.
//! The approval page shows what the program calls itself and where the request
//! came from, because the attack on this flow is a stranger starting one and
//! sending you the code.

use std::time::Duration;

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use tessera_core::{Permission, PermissionSet};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    api::{
        AUTH_TAG,
        auth::require_accounts,
        extract::{ApiJson, ApiPath},
        tokens::{CreatedTokenResponse, format_timestamp},
    },
    auth::{
        extract::{ClientAddress, CurrentUser},
        secret::Secret,
        throttle::Decision,
    },
    db::repository::device_authorization::{
        DeviceAuthorization, Redemption, USER_CODE_LEN, normalize_user_code,
    },
    error::{Error, ErrorCode, Result},
    state::AppState,
};

/// How long a request waits for an answer. Long enough to find the tab and
/// sign in, short enough that a code read out of a screenshot is stale.
const DEVICE_CODE_TTL: Duration = Duration::from_mins(10);

/// Seconds a program waits between polls.
const POLL_INTERVAL_SECS: u64 = 5;

/// Longest name a program may give itself. The token is named after it, so
/// this is the token name's limit.
const MAX_CLIENT_NAME_LENGTH: usize = 64;

/// What a token minted this way may do, at most: what the command line client
/// needs to send, list and remove its owner's shares. The approver's own
/// permissions narrow it further.
const DEVICE_PERMISSIONS: [Permission; 3] = [
    Permission::SharesCreate,
    Permission::SharesRead,
    Permission::SharesDelete,
];

/// Body of `POST /auth/device`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceAuthorizationRequest {
    /// What the program calls itself, such as `tessera CLI on laptop`. Shown on
    /// the approval page and used as the token's name.
    #[schema(example = "tessera CLI on laptop")]
    pub client_name: String,
}

/// A started request, as the program needs it.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DeviceAuthorizationResponse {
    /// The program keeps this and polls with it. Never shown to the person.
    pub device_code: String,
    /// What the person types on the site's `/device` page.
    #[schema(example = "WDJB-MJHT")]
    pub user_code: String,
    /// Seconds until both codes stop working.
    #[schema(example = 600)]
    pub expires_in: u64,
    /// Seconds to wait between polls.
    #[schema(example = 5)]
    pub interval: u64,
}

/// Body of `POST /auth/device/token`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceTokenRequest {
    /// The device code from `POST /auth/device`.
    pub device_code: String,
}

/// A request waiting for an answer, as the approval page shows it.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PendingDeviceResponse {
    /// The code, as it was shown to the person.
    #[schema(example = "WDJB-MJHT")]
    pub user_code: String,
    /// What the program says it is. Its own claim, not checked.
    #[schema(example = "tessera CLI on laptop")]
    pub client_name: String,
    /// The address the request came from.
    pub requested_from: String,
    /// When it was started, RFC 3339.
    pub created_at: String,
    /// When it stops working, RFC 3339.
    pub expires_at: String,
}

impl From<DeviceAuthorization> for PendingDeviceResponse {
    fn from(request: DeviceAuthorization) -> Self {
        Self {
            user_code: display_user_code(&request.user_code),
            client_name: request.client_name,
            requested_from: request.requested_from,
            created_at: format_timestamp(request.created_at),
            expires_at: format_timestamp(request.expires_at),
        }
    }
}

/// `WDJBMJHT` as `WDJB-MJHT`, which is easier to read out and to copy.
fn display_user_code(code: &str) -> String {
    let (head, tail) = code.split_at(code.len().min(USER_CODE_LEN / 2));
    format!("{head}-{tail}")
}

/// Starts a sign-in for a program, to be approved from a browser.
///
/// # Errors
///
/// `400` when the name is empty or too long, `404` when this instance has no
/// accounts, `429` when this address has started too many.
#[utoipa::path(
    post,
    path = "/auth/device",
    operation_id = "start_device_authorization",
    tag = AUTH_TAG,
    request_body = DeviceAuthorizationRequest,
    responses(
        (status = 201, description = "Started; show the user code and poll with the device code", body = DeviceAuthorizationResponse),
        (status = 400, description = "The name is empty or too long", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no accounts", body = crate::error::ErrorBody),
        (status = 429, description = "Too many started from this address", body = crate::error::ErrorBody),
    ),
)]
pub async fn start(
    State(state): State<AppState>,
    ClientAddress(origin): ClientAddress,
    ApiJson(body): ApiJson<DeviceAuthorizationRequest>,
) -> Result<(StatusCode, Json<DeviceAuthorizationResponse>)> {
    require_accounts(&state)?;

    let client_name = body.client_name.trim();
    if client_name.is_empty() {
        return Err(Error::new(ErrorCode::ValueEmpty).with_detail("field", "client_name"));
    }
    if client_name.chars().count() > MAX_CLIENT_NAME_LENGTH {
        return Err(Error::new(ErrorCode::ValueLength)
            .with_detail("field", "client_name")
            .with_detail("max", MAX_CLIENT_NAME_LENGTH)
            .with_message(format!(
                "`client_name` must be at most {MAX_CLIENT_NAME_LENGTH} characters"
            )));
    }

    // Every request writes a row anyone can ask for, so every one counts, not
    // only failures. A person signs in a program a few times a day.
    let counter = &state.throttles.source;
    if let Decision::Deny { retry_after } = counter.check(&origin.source) {
        return Err(Error::TooManyRequests {
            retry_after_secs: retry_after.as_secs(),
        });
    }
    counter.record_failure(&origin.source);

    let now = OffsetDateTime::now_utc();
    let ttl = time::Duration::try_from(DEVICE_CODE_TTL).unwrap_or(time::Duration::MINUTE);
    let (device_code, user_code) = state
        .devices
        .open(
            client_name.to_owned(),
            origin.source,
            now,
            now.saturating_add(ttl),
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DeviceAuthorizationResponse {
            device_code: device_code.expose().to_owned(),
            user_code: display_user_code(&user_code),
            expires_in: DEVICE_CODE_TTL.as_secs(),
            interval: POLL_INTERVAL_SECS,
        }),
    ))
}

/// Polls a started sign-in, and collects its API token once approved.
///
/// The token is handed over once. The request is gone after that, so a
/// second poll gets `device_code_unusable`.
///
/// # Errors
///
/// `400` with `device_authorization_pending` while nobody has answered, and
/// with `device_code_unusable` when the code is unknown, expired or already
/// redeemed. `403` with `device_authorization_denied` when the person refused.
/// `404` when this instance has no accounts.
#[utoipa::path(
    post,
    path = "/auth/device/token",
    operation_id = "redeem_device_code",
    tag = AUTH_TAG,
    request_body = DeviceTokenRequest,
    responses(
        (status = 200, description = "Approved; the token's secret is shown only here", body = CreatedTokenResponse),
        (status = 400, description = "Not answered yet, or the code is not usable", body = crate::error::ErrorBody),
        (status = 403, description = "The person refused", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no accounts", body = crate::error::ErrorBody),
    ),
)]
pub async fn redeem(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<DeviceTokenRequest>,
) -> Result<Json<CreatedTokenResponse>> {
    require_accounts(&state)?;

    // No throttle: the device code is 256 bits, so there is nothing to guess,
    // and each poll is one indexed lookup.
    let now = OffsetDateTime::now_utc();
    match state
        .devices
        .redeem(&Secret::from_client(body.device_code), now)
        .await?
    {
        Redemption::Unknown => Err(Error::new(ErrorCode::DeviceCodeUnusable)),
        Redemption::Pending => Err(Error::new(ErrorCode::DeviceAuthorizationPending)
            .with_detail("interval", POLL_INTERVAL_SECS)),
        Redemption::Denied => Err(Error::new(ErrorCode::DeviceAuthorizationDenied)),
        Redemption::Approved {
            user_id,
            permissions,
            client_name,
        } => {
            let (token, secret) = state
                .tokens
                .create(user_id, client_name, &permissions, now, None)
                .await?;
            Ok(Json(CreatedTokenResponse {
                token: token.into(),
                secret: secret.expose().to_owned(),
            }))
        }
    }
}

/// Shows a waiting request, so the person can see what they are approving.
///
/// # Errors
///
/// `401` when not signed in, `403` when an API token was used, `404` when no
/// request is waiting under that code or this instance has no accounts, `429`
/// when this address has tried too many codes that do not exist.
#[utoipa::path(
    get,
    path = "/auth/device/{user_code}",
    operation_id = "describe_device_authorization",
    tag = AUTH_TAG,
    params(("user_code" = String, Path, description = "The code the program showed, with or without its dash")),
    responses(
        (status = 200, description = "Waiting for an answer", body = PendingDeviceResponse),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "No request is waiting under that code", body = crate::error::ErrorBody),
        (status = 429, description = "Too many codes tried that do not exist", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn describe(
    State(state): State<AppState>,
    ClientAddress(origin): ClientAddress,
    current: CurrentUser,
    ApiPath(user_code): ApiPath<String>,
) -> Result<Json<PendingDeviceResponse>> {
    let (code, now) = answerable(&state, &origin.source, &current, &user_code)?;
    let request = state.devices.find_pending(&code, now).await?;
    let request = found(&state, &origin.source, request)?;
    Ok(Json(request.into()))
}

/// Approves a waiting request, so the program's next poll gets a token.
///
/// The token may create, list and delete the approver's own shares, less
/// whatever of that the approver may not do themselves.
///
/// # Errors
///
/// `401` when not signed in, `403` when an API token was used or the account
/// may not issue tokens, `404` when no request is waiting under that code or
/// this instance has no accounts, `429` when this address has tried too many
/// codes that do not exist.
#[utoipa::path(
    post,
    path = "/auth/device/{user_code}/approve",
    operation_id = "approve_device_authorization",
    tag = AUTH_TAG,
    params(("user_code" = String, Path, description = "The code the program showed, with or without its dash")),
    responses(
        (status = 204, description = "Approved"),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token, or not allowed to issue tokens", body = crate::error::ErrorBody),
        (status = 404, description = "No request is waiting under that code", body = crate::error::ErrorBody),
        (status = 429, description = "Too many codes tried that do not exist", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn approve(
    State(state): State<AppState>,
    ClientAddress(origin): ClientAddress,
    current: CurrentUser,
    ApiPath(user_code): ApiPath<String>,
) -> Result<StatusCode> {
    let (code, now) = answerable(&state, &origin.source, &current, &user_code)?;
    current.require(Permission::TokensManage)?;
    let permissions: PermissionSet = DEVICE_PERMISSIONS
        .into_iter()
        .filter(|permission| current.permissions.allows(*permission))
        .collect();
    if permissions.iter().next().is_none() {
        return Err(Error::Forbidden);
    }
    let approved = state
        .devices
        .approve(&code, current.user.id, &permissions, now)
        .await?;
    found(&state, &origin.source, approved.then_some(()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// Refuses a waiting request, so the program's next poll is told no.
///
/// # Errors
///
/// `401` when not signed in, `403` when an API token was used, `404` when no
/// request is waiting under that code or this instance has no accounts, `429`
/// when this address has tried too many codes that do not exist.
#[utoipa::path(
    post,
    path = "/auth/device/{user_code}/deny",
    operation_id = "deny_device_authorization",
    tag = AUTH_TAG,
    params(("user_code" = String, Path, description = "The code the program showed, with or without its dash")),
    responses(
        (status = 204, description = "Denied"),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "No request is waiting under that code", body = crate::error::ErrorBody),
        (status = 429, description = "Too many codes tried that do not exist", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn deny(
    State(state): State<AppState>,
    ClientAddress(origin): ClientAddress,
    current: CurrentUser,
    ApiPath(user_code): ApiPath<String>,
) -> Result<StatusCode> {
    let (code, now) = answerable(&state, &origin.source, &current, &user_code)?;
    let denied = state.devices.deny(&code, current.user.id, now).await?;
    found(&state, &origin.source, denied.then_some(()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// The checks every answering endpoint makes first, and the code it names in
/// stored form.
fn answerable(
    state: &AppState,
    source: &str,
    current: &CurrentUser,
    user_code: &str,
) -> Result<(String, OffsetDateTime)> {
    require_accounts(state)?;
    // Approving mints a token, and a token may not mint another: one leaked
    // token would otherwise be a way to keep access after it is revoked.
    if !current.credential.is_session() {
        return Err(Error::new(ErrorCode::SessionRequired));
    }
    if let Decision::Deny { retry_after } = state.throttles.lookup.check(source) {
        return Err(Error::TooManyRequests {
            retry_after_secs: retry_after.as_secs(),
        });
    }
    Ok((normalize_user_code(user_code), OffsetDateTime::now_utc()))
}

/// `Some` as it is, and `None` as a `404` counted against the source, the
/// same budget mistyped retrieval codes spend.
fn found<T>(state: &AppState, source: &str, value: Option<T>) -> Result<T> {
    value.ok_or_else(|| {
        state.throttles.lookup.record_failure(source);
        Error::NotFound
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_user_code_is_shown_with_a_dash() {
        assert_eq!(display_user_code("WDJBMJHT"), "WDJB-MJHT");
    }
}
