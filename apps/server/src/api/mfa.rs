//! Two-step sign-in: setting up an authenticator app, finishing a sign-in with
//! one, and the recovery codes for when it is lost.
//!
//! # Which sign-ins ask for a code
//!
//! Only the password. A sign-in through GitHub, Discord or an OIDC provider has
//! already passed whatever second step that provider asks for, and asking again
//! here would protect nothing the provider does not. The code guards the one
//! credential this instance holds itself.
//!
//! # Why a cookie and not a token
//!
//! Every account endpoint here refuses a bearer token. Turning the second step
//! on or off, or reading new recovery codes, decides who can sign in; a leaked
//! token exists to upload files and must not be able to lock the owner out.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tessera_core::UserId;
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    api::{
        AUTH_TAG,
        auth::{AccountResponse, reconfirm, require_accounts, start_session},
        extract::ApiJson,
    },
    auth::{
        extract::{ClientAddress, CurrentUser},
        mfa::{TotpSecret, generate_recovery_codes, looks_like_totp},
        proxy::Origin,
        secret::Secret,
        throttle::Decision,
    },
    db::repository::session::StartedBy,
    error::{Error, ErrorCode, Result},
    state::AppState,
};

/// How long the ticket from the password step lasts.
///
/// Long enough to find the phone and open the app; short enough that a ticket
/// left in a closed tab is not a half-open door for long.
pub const CHALLENGE_TTL: time::Duration = time::Duration::minutes(5);

/// What `POST /auth/login` answers when the password is right and a code is
/// still needed.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MfaChallengeResponse {
    /// Sent back with the code to `POST /auth/login/mfa`. Works once.
    #[schema(example = "9b2f…")]
    pub ticket: String,
    /// When the ticket stops working, and the password has to be entered again.
    pub expires_at: String,
}

/// Body of `POST /auth/login/mfa`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LoginCodeRequest {
    /// The ticket `POST /auth/login` returned.
    pub ticket: String,
    /// Six digits from the authenticator app, or one of the recovery codes.
    #[schema(example = "123456")]
    pub code: String,
}

/// Whether the account asks for a second step, as `GET /account/mfa` reports.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MfaStatusResponse {
    /// Whether signing in with the password asks for a code.
    pub totp_enabled: bool,
    /// How many recovery codes are left unused.
    pub recovery_codes_left: u64,
}

/// What `POST /account/mfa/totp` answers: the secret to put in the app.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TotpSetupResponse {
    /// The secret, for typing into an app that cannot scan.
    #[schema(example = "JBSWY3DPEHPK3PXP")]
    pub secret: String,
    /// The `otpauth://` link, for a client to draw as a QR code.
    #[schema(example = "otpauth://totp/Tessera:alice?secret=JBSWY3DPEHPK3PXP&issuer=Tessera")]
    pub uri: String,
}

/// New recovery codes. Shown once: only their digests are kept.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecoveryCodesResponse {
    /// Each works once, in place of a code from the app.
    #[schema(example = json!(["3k9qa-7xm2d", "h4tw0-p8c1z"]))]
    pub recovery_codes: Vec<String>,
}

/// Body of `POST /account/mfa/totp`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BeginTotpRequest {
    /// The current password. Omitted by an account without one, which confirms
    /// by a recent sign-in instead.
    #[schema(min_length = 8, max_length = 1024)]
    #[serde(default)]
    pub current_password: Option<String>,
}

/// Body of `POST /account/mfa/totp/confirm`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfirmTotpRequest {
    /// A code the app shows for the secret just set up.
    #[schema(example = "123456")]
    pub code: String,
}

/// Body of `DELETE /account/mfa/totp` and `POST /account/mfa/recovery-codes`.
///
/// Either proves it is the owner asking: the password, or a code.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfirmOwnerRequest {
    /// The current password. Omitted by an account without one, which confirms
    /// by a recent sign-in instead, or when `code` is given.
    #[schema(min_length = 8, max_length = 1024)]
    #[serde(default)]
    pub current_password: Option<String>,
    /// A code from the app, or a recovery code, instead of the password.
    #[schema(example = "123456")]
    #[serde(default)]
    pub code: Option<String>,
}

/// The throttle key for guessing at one account's codes.
///
/// The account alone, not the account and the address. Whoever reaches this
/// step already has the password, so spreading guesses across addresses must
/// not buy more of them; the cost is that they can stop the owner signing in
/// for a while, and the owner should be changing that password anyway.
fn throttle_key(user: UserId) -> String {
    format!("mfa|{user}")
}

/// Refuses when this account or address has guessed at codes too often.
fn check_throttle(state: &AppState, user: UserId, origin: &Origin) -> Result<()> {
    let throttles = &state.throttles;
    if let Decision::Deny { retry_after } =
        throttles.check(&throttles.login, &throttle_key(user), &origin.source)
    {
        return Err(Error::TooManyRequests {
            retry_after_secs: retry_after.as_secs(),
        });
    }
    Ok(())
}

/// Checks a second-step code for an account whose second step is on, using it
/// up if it is right.
///
/// Six digits are an authenticator code, anything else a recovery code; the
/// two never look alike. An authenticator code is refused a second time within
/// its step, so one read over a shoulder cannot be replayed.
async fn spend_code(
    state: &AppState,
    user: UserId,
    code: &str,
    now: OffsetDateTime,
) -> Result<bool> {
    if !looks_like_totp(code) {
        return Ok(state.mfa.spend_recovery_code(user, code, now).await?);
    }
    let Some(authenticator) = state.mfa.authenticator(user).await? else {
        return Ok(false);
    };
    if !authenticator.confirmed {
        return Ok(false);
    }
    match authenticator.secret.verify(code, now.unix_timestamp()) {
        Some(step) => Ok(state.mfa.use_step(user, step).await?),
        None => Ok(false),
    }
}

/// Checks a code with the throttle around it: refused when guessing too fast,
/// counted when wrong.
async fn spend_code_throttled(
    state: &AppState,
    user: UserId,
    origin: &Origin,
    code: &str,
    now: OffsetDateTime,
) -> Result<()> {
    check_throttle(state, user, origin)?;
    if spend_code(state, user, code, now).await? {
        state.throttles.login.reset(&throttle_key(user));
        Ok(())
    } else {
        state
            .throttles
            .record_failure(&state.throttles.login, &throttle_key(user), &origin.source);
        Err(Error::new(ErrorCode::MfaCodeIncorrect))
    }
}

/// Refuses anything that did not arrive with a session cookie.
fn require_session(current: &CurrentUser) -> Result<()> {
    if current.credential.is_session() {
        Ok(())
    } else {
        Err(Error::new(ErrorCode::SessionRequired))
    }
}

/// Finishes a sign-in that `POST /auth/login` answered with a ticket.
///
/// # Errors
///
/// `401` `mfa_challenge_unusable` when the ticket is unknown, used or expired,
/// `401` `mfa_code_incorrect` when the code is wrong, `429` when too many codes
/// have been wrong.
#[utoipa::path(
    post,
    path = "/auth/login/mfa",
    tag = AUTH_TAG,
    request_body = LoginCodeRequest,
    responses(
        (status = 200, description = "Signed in", body = AccountResponse),
        (status = 400, description = "The body is not the shape this expects", body = crate::error::ErrorBody),
        (status = 401, description = "The ticket is not usable, or the code is wrong", body = crate::error::ErrorBody),
        (status = 429, description = "Too many wrong codes", body = crate::error::ErrorBody),
    ),
)]
pub async fn login_code(
    State(state): State<AppState>,
    ClientAddress(origin): ClientAddress,
    headers: HeaderMap,
    ApiJson(body): ApiJson<LoginCodeRequest>,
) -> Result<Response> {
    require_accounts(&state)?;

    let now = OffsetDateTime::now_utc();
    let ticket = Secret::from_client(body.ticket);
    let Some(user_id) = state.mfa.challenge(&ticket, now).await? else {
        return Err(Error::new(ErrorCode::MfaChallengeUnusable));
    };

    spend_code_throttled(&state, user_id, &origin, &body.code, now).await?;

    // Deleted only now, so a mistyped code leaves the ticket usable for the
    // next try. Whoever deletes it is the one sign-in that goes ahead.
    if !state.mfa.close_challenge(&ticket).await? {
        return Err(Error::new(ErrorCode::MfaChallengeUnusable));
    }

    // Checked again: the account may have been suspended between the two
    // steps, and the password step's check is minutes old.
    let Some(user) = state
        .users
        .find_by_id(user_id)
        .await?
        .filter(|user| user.state.may_sign_in())
    else {
        return Err(Error::new(ErrorCode::InvalidCredentials));
    };

    let cookie =
        start_session(&state, &user, &headers, &origin, StartedBy::password(), now).await?;
    Ok((
        [(header::SET_COOKIE, cookie.to_string())],
        Json(AccountResponse::build(
            &user,
            &state.roles.of_user(user.id).await?,
            true,
        )),
    )
        .into_response())
}

/// Reports whether the caller's sign-in asks for a code.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when a bearer token was used.
#[utoipa::path(
    get,
    path = "/account/mfa",
    operation_id = "mfa_status",
    tag = AUTH_TAG,
    responses(
        (status = 200, description = "Whether two-step sign-in is on", body = MfaStatusResponse),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token, or a password change is due", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn status(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<MfaStatusResponse>> {
    require_session(&current)?;
    let totp_enabled = state.mfa.is_enabled(current.user.id).await?;
    let recovery_codes_left = if totp_enabled {
        state.mfa.recovery_codes_left(current.user.id).await?
    } else {
        0
    };
    Ok(Json(MfaStatusResponse {
        totp_enabled,
        recovery_codes_left,
    }))
}

/// Starts setting up an authenticator app.
///
/// Nothing changes at sign-in yet. The account is asked for a code only once
/// `POST /account/mfa/totp/confirm` has seen one, so a QR code scanned wrong
/// cannot lock anybody out. Calling this again replaces a secret that was
/// never confirmed.
///
/// # Why it asks for the password
///
/// Turning the second step on is a way to lock the owner out: a borrowed,
/// unlocked browser could set it up with the borrower's phone.
///
/// # Errors
///
/// `401` when the password is wrong, `403` when a bearer token was used, `409`
/// when two-step sign-in is already on.
#[utoipa::path(
    post,
    path = "/account/mfa/totp",
    operation_id = "begin_totp",
    tag = AUTH_TAG,
    request_body = BeginTotpRequest,
    responses(
        (status = 200, description = "The secret to put in the app", body = TotpSetupResponse),
        (status = 400, description = "The body is not the shape this expects", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in, the password is wrong, or an account with none signed in too long ago", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token, or a password change is due", body = crate::error::ErrorBody),
        (status = 409, description = "Two-step sign-in is already on", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn begin_totp(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(body): ApiJson<BeginTotpRequest>,
) -> Result<Json<TotpSetupResponse>> {
    reconfirm(&state, &current, body.current_password).await?;

    let secret = TotpSecret::generate();
    if !state
        .mfa
        .begin(current.user.id, &secret, OffsetDateTime::now_utc())
        .await?
    {
        return Err(Error::new(ErrorCode::MfaAlreadyEnabled));
    }

    let issuer = {
        let config = state.config();
        let title = config.site.title.trim();
        if title.is_empty() {
            "Tessera".to_owned()
        } else {
            title.to_owned()
        }
    };
    Ok(Json(TotpSetupResponse {
        secret: secret.to_base32(),
        uri: secret.uri(&issuer, current.user.username.as_str()),
    }))
}

/// Turns the second step on, by showing a code from the app just set up.
///
/// Answers with the recovery codes, the only time they are shown.
///
/// # Errors
///
/// `401` when the code is wrong, `403` when a bearer token was used, `404` when
/// nothing is being set up, `409` when two-step sign-in is already on, `429`
/// when too many codes have been wrong.
#[utoipa::path(
    post,
    path = "/account/mfa/totp/confirm",
    operation_id = "confirm_totp",
    tag = AUTH_TAG,
    request_body = ConfirmTotpRequest,
    responses(
        (status = 200, description = "On; the recovery codes, shown once", body = RecoveryCodesResponse),
        (status = 400, description = "The body is not the shape this expects", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in, or the code is wrong", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token, or a password change is due", body = crate::error::ErrorBody),
        (status = 404, description = "Nothing is being set up", body = crate::error::ErrorBody),
        (status = 409, description = "Two-step sign-in is already on", body = crate::error::ErrorBody),
        (status = 429, description = "Too many wrong codes", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn confirm_totp(
    State(state): State<AppState>,
    ClientAddress(origin): ClientAddress,
    current: CurrentUser,
    ApiJson(body): ApiJson<ConfirmTotpRequest>,
) -> Result<Json<RecoveryCodesResponse>> {
    require_session(&current)?;
    let user = current.user.id;

    let Some(authenticator) = state.mfa.authenticator(user).await? else {
        return Err(Error::new(ErrorCode::NotFound)
            .with_message("nothing is being set up; start again".to_owned()));
    };
    if authenticator.confirmed {
        return Err(Error::new(ErrorCode::MfaAlreadyEnabled));
    }

    check_throttle(&state, user, &origin)?;
    let now = OffsetDateTime::now_utc();
    let Some(step) = authenticator
        .secret
        .verify(&body.code, now.unix_timestamp())
    else {
        state
            .throttles
            .record_failure(&state.throttles.login, &throttle_key(user), &origin.source);
        return Err(Error::new(ErrorCode::MfaCodeIncorrect));
    };
    state.throttles.login.reset(&throttle_key(user));

    let recovery_codes = generate_recovery_codes();
    if !state.mfa.confirm(user, step, &recovery_codes, now).await? {
        // Confirmed by a request that arrived at the same moment.
        return Err(Error::new(ErrorCode::MfaAlreadyEnabled));
    }
    tracing::info!(account = %user, "two-step sign-in turned on");
    Ok(Json(RecoveryCodesResponse { recovery_codes }))
}

/// Confirms the caller is the owner, by a code or else the way every other
/// account change does.
pub(super) async fn confirm_owner(
    state: &AppState,
    origin: &Origin,
    current: &CurrentUser,
    body: ConfirmOwnerRequest,
) -> Result<()> {
    require_session(current)?;
    match body.code {
        Some(code) => {
            spend_code_throttled(
                state,
                current.user.id,
                origin,
                &code,
                OffsetDateTime::now_utc(),
            )
            .await
        }
        None => reconfirm(state, current, body.current_password).await,
    }
}

/// Turns the second step off, and throws away the recovery codes.
///
/// # Errors
///
/// `401` when the password or code is wrong, `403` when a bearer token was
/// used, `404` when two-step sign-in is not on, `429` when too many codes have
/// been wrong.
#[utoipa::path(
    delete,
    path = "/account/mfa/totp",
    operation_id = "disable_totp",
    tag = AUTH_TAG,
    request_body = ConfirmOwnerRequest,
    responses(
        (status = 204, description = "Off"),
        (status = 400, description = "The body is not the shape this expects", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in, or the password or code is wrong", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token, or a password change is due", body = crate::error::ErrorBody),
        (status = 404, description = "Two-step sign-in is not on", body = crate::error::ErrorBody),
        (status = 429, description = "Too many wrong codes", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn disable_totp(
    State(state): State<AppState>,
    ClientAddress(origin): ClientAddress,
    current: CurrentUser,
    ApiJson(body): ApiJson<ConfirmOwnerRequest>,
) -> Result<StatusCode> {
    require_session(&current)?;
    // Asked before the proof, so a code is not spent on an account that has
    // nothing to turn off.
    if !state.mfa.is_enabled(current.user.id).await? {
        return Err(Error::NotFound);
    }
    confirm_owner(&state, &origin, &current, body).await?;
    state.mfa.remove(current.user.id).await?;
    tracing::info!(account = %current.user.id, "two-step sign-in turned off");
    Ok(StatusCode::NO_CONTENT)
}

/// Replaces the recovery codes with a new set, and answers with them.
///
/// For when they have been used up or may have been seen. Every old code stops
/// working.
///
/// # Errors
///
/// `401` when the password or code is wrong, `403` when a bearer token was
/// used, `404` when two-step sign-in is not on, `429` when too many codes have
/// been wrong.
#[utoipa::path(
    post,
    path = "/account/mfa/recovery-codes",
    operation_id = "regenerate_recovery_codes",
    tag = AUTH_TAG,
    request_body = ConfirmOwnerRequest,
    responses(
        (status = 200, description = "The new codes, shown once", body = RecoveryCodesResponse),
        (status = 400, description = "The body is not the shape this expects", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in, or the password or code is wrong", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token, or a password change is due", body = crate::error::ErrorBody),
        (status = 404, description = "Two-step sign-in is not on", body = crate::error::ErrorBody),
        (status = 429, description = "Too many wrong codes", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn regenerate_recovery_codes(
    State(state): State<AppState>,
    ClientAddress(origin): ClientAddress,
    current: CurrentUser,
    ApiJson(body): ApiJson<ConfirmOwnerRequest>,
) -> Result<Json<RecoveryCodesResponse>> {
    require_session(&current)?;
    if !state.mfa.is_enabled(current.user.id).await? {
        return Err(Error::NotFound);
    }
    confirm_owner(&state, &origin, &current, body).await?;
    let recovery_codes = generate_recovery_codes();
    state
        .mfa
        .replace_recovery_codes(current.user.id, &recovery_codes, OffsetDateTime::now_utc())
        .await?;
    Ok(Json(RecoveryCodesResponse { recovery_codes }))
}
