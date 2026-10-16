//! Passkeys: adding them to an account, naming and removing them, and signing
//! in with one.
//!
//! # What a passkey sign-in stands for
//!
//! The password and the second step together. An authenticator only answers
//! once the person has unlocked it with a fingerprint, a face or a PIN, and
//! every answer here is refused unless it says so. An account with two-step
//! sign-in on is therefore not asked for a code after one.
//!
//! # The shapes on the wire
//!
//! The options are the JSON forms of the WebAuthn dictionaries, which a client
//! hands to `PublicKeyCredential.parseCreationOptionsFromJSON` or
//! `parseRequestOptionsFromJSON` unchanged. The answers are what
//! `PublicKeyCredential.toJSON()` produces. Anything else a browser adds to
//! them is ignored.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    api::{
        AUTH_TAG,
        auth::{AccountResponse, reconfirm, require_accounts, start_session},
        extract::{ApiJson, ApiPath},
        tokens::format_timestamp,
    },
    auth::{
        extract::{ClientAddress, CurrentUser},
        throttle::Decision,
        webauthn::{self, ALGORITHMS, RelyingParty},
    },
    db::repository::{
        passkey::{ChallengePurpose, Passkey},
        session::StartedBy,
    },
    error::{Error, ErrorCode, Result},
    state::AppState,
};

/// How long a browser has to answer, in both ceremonies.
const CHALLENGE_TTL: time::Duration = time::Duration::minutes(5);

/// Longest name a passkey may be given, in characters.
const MAX_NAME: usize = 64;

/// What a passkey is called when its owner does not say.
const DEFAULT_NAME: &str = "Passkey";

/// A passkey, as its owner sees it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PasskeyResponse {
    /// Stable identifier, for renaming or removing it.
    #[schema(example = "01K1ZQK9V8QF5S3T4M2N6P7R8S")]
    pub id: String,
    /// What the owner calls it.
    #[schema(example = "iPhone")]
    pub name: String,
    /// Whether it syncs to the owner's other devices, as a phone's or a
    /// password manager's does; a hardware key does not.
    pub backed_up: bool,
    /// When it was added.
    pub created_at: String,
    /// When it last signed in, or `null` if it never has.
    pub last_used_at: Option<String>,
}

impl From<&Passkey> for PasskeyResponse {
    fn from(passkey: &Passkey) -> Self {
        Self {
            id: passkey.id.clone(),
            name: passkey.name.clone(),
            backed_up: passkey.backed_up,
            created_at: format_timestamp(passkey.created_at),
            last_used_at: passkey.last_used_at.map(format_timestamp),
        }
    }
}

/// `{ type, alg }` in the creation options.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CredentialParameter {
    /// Always `public-key`.
    #[serde(rename = "type")]
    pub kind: String,
    /// A COSE algorithm identifier.
    pub alg: i64,
}

/// `{ type, id }`: a credential the browser should not register again.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CredentialDescriptor {
    /// Always `public-key`.
    #[serde(rename = "type")]
    pub kind: String,
    /// The credential id, base64url.
    pub id: String,
}

/// The site, as the creation options name it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RelyingPartyEntity {
    /// The host name the passkey is scoped to.
    pub id: String,
    /// What the browser shows as the site's name.
    pub name: String,
}

/// The account, as the creation options name it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserEntity {
    /// An opaque handle for the account, base64url. Echoed back at sign-in.
    pub id: String,
    /// The account name.
    pub name: String,
    /// Shown by the browser beside it.
    pub display_name: String,
}

/// How the authenticator must behave.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticatorSelection {
    /// `required`: the credential must be findable without typing a name,
    /// which is what makes it a passkey.
    pub resident_key: String,
    /// The older spelling of the same thing, for older browsers.
    pub require_resident_key: bool,
    /// `required`: a fingerprint, a face or a PIN.
    pub user_verification: String,
}

/// `PublicKeyCredentialCreationOptionsJSON`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreationOptions {
    /// Base64url; answered once, within `timeout`.
    pub challenge: String,
    /// The site.
    pub rp: RelyingPartyEntity,
    /// The account.
    pub user: UserEntity,
    /// The algorithms this accepts, preferred first.
    pub pub_key_cred_params: Vec<CredentialParameter>,
    /// Milliseconds.
    pub timeout: u64,
    /// The account's existing passkeys, so the same authenticator is not added
    /// twice.
    pub exclude_credentials: Vec<CredentialDescriptor>,
    /// See [`AuthenticatorSelection`].
    pub authenticator_selection: AuthenticatorSelection,
    /// Always `none`; see `auth::webauthn`.
    pub attestation: String,
}

/// `PublicKeyCredentialRequestOptionsJSON`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestOptions {
    /// Base64url; answered once, within `timeout`.
    pub challenge: String,
    /// The host name the passkey must be scoped to.
    pub rp_id: String,
    /// Milliseconds.
    pub timeout: u64,
    /// Always `required`.
    pub user_verification: String,
    /// Always empty: the browser offers whichever passkeys it holds for the
    /// site, and the answer says which account.
    pub allow_credentials: Vec<CredentialDescriptor>,
}

/// The `response` of a registration, as `toJSON()` writes it.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AttestationResponse {
    /// Base64url.
    #[serde(rename = "clientDataJSON")]
    pub client_data_json: String,
    /// Base64url.
    #[serde(rename = "attestationObject")]
    pub attestation_object: String,
}

/// A new passkey, as `PublicKeyCredential.toJSON()` writes it.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RegistrationCredential {
    /// The credential id, base64url.
    pub id: String,
    /// The authenticator's answer.
    pub response: AttestationResponse,
}

/// The `response` of a sign-in, as `toJSON()` writes it.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AssertionResponse {
    /// Base64url.
    #[serde(rename = "clientDataJSON")]
    pub client_data_json: String,
    /// Base64url.
    #[serde(rename = "authenticatorData")]
    pub authenticator_data: String,
    /// Base64url.
    pub signature: String,
    /// The account handle the passkey was registered with, base64url.
    #[serde(rename = "userHandle", default)]
    pub user_handle: Option<String>,
}

/// A sign-in answer, as `PublicKeyCredential.toJSON()` writes it.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AuthenticationCredential {
    /// The credential id, base64url.
    pub id: String,
    /// The authenticator's answer.
    pub response: AssertionResponse,
}

/// Body of `POST /account/passkeys/options`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BeginRegistrationRequest {
    /// The current password. Omitted by an account without one, which confirms
    /// by a recent sign-in instead.
    #[schema(min_length = 8, max_length = 1024)]
    #[serde(default)]
    pub current_password: Option<String>,
}

/// Body of `POST /account/passkeys`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterPasskeyRequest {
    /// What to call it. `Passkey` when omitted.
    #[schema(example = "iPhone", max_length = 64)]
    #[serde(default)]
    pub name: Option<String>,
    /// What the browser returned from `navigator.credentials.create`.
    pub credential: RegistrationCredential,
}

/// Body of `PATCH /account/passkeys/{id}`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RenamePasskeyRequest {
    /// The new name.
    #[schema(example = "Work laptop", min_length = 1, max_length = 64)]
    pub name: String,
}

/// Body of `POST /auth/passkey`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PasskeySignInRequest {
    /// What the browser returned from `navigator.credentials.get`.
    pub credential: AuthenticationCredential,
}

/// The site, from this instance's address.
///
/// From `server.public_url` when set, otherwise from the request, the same
/// answer link previews use. A passkey is scoped to the host it was made on, so
/// an instance that moves to a new address leaves its passkeys behind; the
/// accounts sign in another way and add new ones.
fn relying_party(state: &AppState, headers: &HeaderMap) -> Result<RelyingParty> {
    crate::api::preview::origin(&state.config(), headers)
        .as_deref()
        .and_then(RelyingParty::from_origin)
        .ok_or_else(|| {
            tracing::warn!("no address to scope a passkey to; set server.public_url");
            Error::Unavailable
        })
}

/// A name for a passkey, trimmed and bounded.
fn passkey_name(raw: Option<&str>) -> Result<String> {
    let name = raw.map_or(DEFAULT_NAME, str::trim);
    let length = name.chars().count();
    if length == 0 || length > MAX_NAME {
        return Err(Error::new(ErrorCode::ValueLength)
            .with_detail("field", "name")
            .with_detail("max", MAX_NAME)
            .with_message(format!("`name` must be 1 to {MAX_NAME} characters")));
    }
    Ok(name.to_owned())
}

/// Refuses anything that did not arrive with a session cookie.
fn require_session(current: &CurrentUser) -> Result<()> {
    if current.credential.is_session() {
        Ok(())
    } else {
        Err(Error::new(ErrorCode::SessionRequired))
    }
}

/// Starts adding a passkey to the caller's account.
///
/// # Why it asks for the password
///
/// A passkey is a way in. A borrowed, unlocked browser must not be able to add
/// one the borrower holds.
///
/// # Errors
///
/// `401` when the password is wrong, `403` when a bearer token was used, `503`
/// when this instance cannot tell what address it is on.
#[utoipa::path(
    post,
    path = "/account/passkeys/options",
    operation_id = "begin_passkey_registration",
    tag = AUTH_TAG,
    request_body = BeginRegistrationRequest,
    responses(
        (status = 200, description = "Options for `navigator.credentials.create`", body = CreationOptions),
        (status = 400, description = "The body is not the shape this expects", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in, the password is wrong, or an account with none signed in too long ago", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token, or a password change is due", body = crate::error::ErrorBody),
        (status = 503, description = "This instance does not know its own address", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn begin_registration(
    State(state): State<AppState>,
    headers: HeaderMap,
    current: CurrentUser,
    ApiJson(body): ApiJson<BeginRegistrationRequest>,
) -> Result<Json<CreationOptions>> {
    reconfirm(&state, &current, body.current_password).await?;
    let rp = relying_party(&state, &headers)?;

    let now = OffsetDateTime::now_utc();
    let challenge = state
        .passkeys
        .open_challenge(
            ChallengePurpose::Register,
            Some(current.user.id),
            now,
            now.saturating_add(CHALLENGE_TTL),
        )
        .await?;
    let existing = state.passkeys.list(current.user.id).await?;
    let site = {
        let config = state.config();
        let title = config.site.title.trim();
        if title.is_empty() {
            "Tessera".to_owned()
        } else {
            title.to_owned()
        }
    };

    let username = current.user.username.to_string();
    Ok(Json(CreationOptions {
        challenge,
        rp: RelyingPartyEntity {
            id: rp.id,
            name: site,
        },
        user: UserEntity {
            id: webauthn::encode(current.user.id.to_string().as_bytes()),
            name: username.clone(),
            display_name: username,
        },
        pub_key_cred_params: ALGORITHMS
            .iter()
            .map(|&alg| CredentialParameter {
                kind: "public-key".to_owned(),
                alg,
            })
            .collect(),
        timeout: timeout_millis(),
        exclude_credentials: existing
            .iter()
            .map(|passkey| CredentialDescriptor {
                kind: "public-key".to_owned(),
                id: webauthn::encode(&passkey.credential_id),
            })
            .collect(),
        authenticator_selection: AuthenticatorSelection {
            resident_key: "required".to_owned(),
            require_resident_key: true,
            user_verification: "required".to_owned(),
        },
        attestation: "none".to_owned(),
    }))
}

/// [`CHALLENGE_TTL`] in the milliseconds the options carry.
fn timeout_millis() -> u64 {
    u64::try_from(CHALLENGE_TTL.whole_milliseconds()).unwrap_or(300_000)
}

/// Adds the passkey the browser just made.
///
/// # Errors
///
/// `400` when the answer does not check out or its challenge is gone, `403`
/// when a bearer token was used, `409` when that passkey is already
/// registered.
#[utoipa::path(
    post,
    path = "/account/passkeys",
    operation_id = "register_passkey",
    tag = AUTH_TAG,
    request_body = RegisterPasskeyRequest,
    responses(
        (status = 201, description = "Added", body = PasskeyResponse),
        (status = 400, description = "The answer does not check out, or a value is not usable", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token, or a password change is due", body = crate::error::ErrorBody),
        (status = 409, description = "That passkey is already registered", body = crate::error::ErrorBody),
        (status = 503, description = "This instance does not know its own address", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    current: CurrentUser,
    ApiJson(body): ApiJson<RegisterPasskeyRequest>,
) -> Result<(StatusCode, Json<PasskeyResponse>)> {
    require_session(&current)?;
    let name = passkey_name(body.name.as_deref())?;
    let rp = relying_party(&state, &headers)?;
    let now = OffsetDateTime::now_utc();

    let refused = |reason: webauthn::Rejection| {
        tracing::info!(account = %current.user.id, %reason, "a passkey was not added");
        Error::new(ErrorCode::PasskeyInvalid)
    };
    let response = &body.credential.response;
    let client_data = webauthn::decode(&response.client_data_json).map_err(refused)?;
    let attestation = webauthn::decode(&response.attestation_object).map_err(refused)?;
    let challenge = webauthn::challenge_of(&client_data).map_err(refused)?;

    // Issued to this account, for this purpose, and not yet answered.
    let issued_to = state
        .passkeys
        .take_challenge(&challenge, ChallengePurpose::Register, now)
        .await?;
    if issued_to != Some(Some(current.user.id)) {
        return Err(Error::new(ErrorCode::PasskeyInvalid));
    }

    let credential =
        webauthn::verify_registration(&rp, &client_data, &attestation).map_err(refused)?;
    if state
        .passkeys
        .find_by_credential(&credential.id)
        .await?
        .is_some()
    {
        return Err(Error::new(ErrorCode::PasskeyAlreadyRegistered));
    }

    let passkey = state
        .passkeys
        .add(current.user.id, &credential, &name, now)
        .await?;
    tracing::info!(account = %current.user.id, passkey = %passkey.id, "a passkey was added");
    Ok((StatusCode::CREATED, Json(PasskeyResponse::from(&passkey))))
}

/// Lists the caller's passkeys, oldest first.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when a bearer token was used.
#[utoipa::path(
    get,
    path = "/account/passkeys",
    operation_id = "list_passkeys",
    tag = AUTH_TAG,
    responses(
        (status = 200, description = "The caller's passkeys", body = Vec<PasskeyResponse>),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token, or a password change is due", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<PasskeyResponse>>> {
    require_session(&current)?;
    let passkeys = state.passkeys.list(current.user.id).await?;
    Ok(Json(passkeys.iter().map(PasskeyResponse::from).collect()))
}

/// Renames one of the caller's passkeys.
///
/// # Errors
///
/// `400` when the name is empty or too long, `403` when a bearer token was
/// used, `404` when no passkey of the caller's has that id.
#[utoipa::path(
    patch,
    path = "/account/passkeys/{id}",
    operation_id = "rename_passkey",
    tag = AUTH_TAG,
    params(("id" = String, Path, description = "Which passkey")),
    request_body = RenamePasskeyRequest,
    responses(
        (status = 204, description = "Renamed"),
        (status = 400, description = "The name is not usable", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token, or a password change is due", body = crate::error::ErrorBody),
        (status = 404, description = "No passkey of yours has that id", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn rename(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<String>,
    ApiJson(body): ApiJson<RenamePasskeyRequest>,
) -> Result<StatusCode> {
    require_session(&current)?;
    let name = passkey_name(Some(&body.name))?;
    if state.passkeys.rename(current.user.id, &id, &name).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(Error::NotFound)
    }
}

/// Removes one of the caller's passkeys.
///
/// # Why the last way in cannot be removed
///
/// An account with no password, no linked provider and one passkey would be
/// locked out the moment that passkey went, and would only find out at the
/// next sign-in. The same rule unlinking a provider follows.
///
/// # Errors
///
/// `403` when a bearer token was used, `404` when no passkey of the caller's
/// has that id, `409` when it is the account's only way in.
#[utoipa::path(
    delete,
    path = "/account/passkeys/{id}",
    operation_id = "remove_passkey",
    tag = AUTH_TAG,
    params(("id" = String, Path, description = "Which passkey")),
    responses(
        (status = 204, description = "Removed"),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token, or a password change is due", body = crate::error::ErrorBody),
        (status = 404, description = "No passkey of yours has that id", body = crate::error::ErrorBody),
        (status = 409, description = "It is the only way left into the account", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn remove(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<String>,
) -> Result<StatusCode> {
    require_session(&current)?;
    let user = current.user.id;
    let passkeys = state.passkeys.list(user).await?;
    if !passkeys.iter().any(|passkey| passkey.id == id) {
        return Err(Error::NotFound);
    }
    if passkeys.len() == 1
        && !state.users.has_password(user).await?
        && state.identities.identities_of(user).await?.is_empty()
    {
        return Err(Error::new(ErrorCode::LastSignInMethod));
    }
    state.passkeys.remove(user, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Starts a sign-in with a passkey.
///
/// Nobody is named: the browser offers whichever passkeys it holds for this
/// site, and the answer says whose account it is.
///
/// # Errors
///
/// `404` when this instance has no accounts, `503` when it cannot tell what
/// address it is on.
#[utoipa::path(
    post,
    path = "/auth/passkey/options",
    operation_id = "begin_passkey_sign_in",
    tag = AUTH_TAG,
    responses(
        (status = 200, description = "Options for `navigator.credentials.get`", body = RequestOptions),
        (status = 404, description = "This instance has no accounts", body = crate::error::ErrorBody),
        (status = 503, description = "This instance does not know its own address", body = crate::error::ErrorBody),
    ),
)]
pub async fn begin_sign_in(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<RequestOptions>> {
    require_accounts(&state)?;
    let rp = relying_party(&state, &headers)?;
    let now = OffsetDateTime::now_utc();
    let challenge = state
        .passkeys
        .open_challenge(
            ChallengePurpose::SignIn,
            None,
            now,
            now.saturating_add(CHALLENGE_TTL),
        )
        .await?;
    Ok(Json(RequestOptions {
        challenge,
        rp_id: rp.id,
        timeout: timeout_millis(),
        user_verification: "required".to_owned(),
        allow_credentials: Vec::new(),
    }))
}

/// Signs in with a passkey.
///
/// A passkey stands for the password and the second step together, so an
/// account with two-step sign-in on is signed in straight away.
///
/// # Errors
///
/// `401` `passkey_rejected` when the answer cannot sign anybody in, whatever
/// the reason; `429` when too many have failed from this address.
#[utoipa::path(
    post,
    path = "/auth/passkey",
    operation_id = "sign_in_with_passkey",
    tag = AUTH_TAG,
    request_body = PasskeySignInRequest,
    responses(
        (status = 200, description = "Signed in", body = AccountResponse),
        (status = 400, description = "The body is not the shape this expects", body = crate::error::ErrorBody),
        (status = 401, description = "The passkey could not sign anybody in", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no accounts", body = crate::error::ErrorBody),
        (status = 429, description = "Too many failed attempts", body = crate::error::ErrorBody),
        (status = 503, description = "This instance does not know its own address", body = crate::error::ErrorBody),
    ),
)]
pub async fn sign_in(
    State(state): State<AppState>,
    ClientAddress(origin): ClientAddress,
    headers: HeaderMap,
    ApiJson(body): ApiJson<PasskeySignInRequest>,
) -> Result<Response> {
    require_accounts(&state)?;
    let rp = relying_party(&state, &headers)?;

    // Keyed on the address alone: nothing names an account until the answer
    // has been checked.
    let key = format!("passkey|{}", origin.source);
    let throttles = &state.throttles;
    if let Decision::Deny { retry_after } = throttles.check(&throttles.login, &key, &origin.source)
    {
        return Err(Error::TooManyRequests {
            retry_after_secs: retry_after.as_secs(),
        });
    }

    let now = OffsetDateTime::now_utc();
    let Some((user, passkey)) = check_sign_in(&state, &rp, &body.credential, now).await? else {
        throttles.record_failure(&throttles.login, &key, &origin.source);
        return Err(Error::new(ErrorCode::PasskeyRejected));
    };
    throttles.login.reset(&key);

    tracing::info!(account = %user.id, passkey = %passkey, "signed in with a passkey");
    let cookie = start_session(&state, &user, &headers, &origin, StartedBy::passkey(), now).await?;
    let has_password = state.users.has_password(user.id).await?;
    Ok((
        [(header::SET_COOKIE, cookie.to_string())],
        Json(AccountResponse::build(
            &user,
            &state.roles.of_user(user.id).await?,
            has_password,
        )),
    )
        .into_response())
}

/// Every check a sign-in answer has to pass, in order. `None` for any failure:
/// the caller answers them all the same way.
async fn check_sign_in(
    state: &AppState,
    rp: &RelyingParty,
    credential: &AuthenticationCredential,
    now: OffsetDateTime,
) -> Result<Option<(crate::db::repository::user::User, String)>> {
    let response = &credential.response;
    let decoded = (
        webauthn::decode(&credential.id),
        webauthn::decode(&response.client_data_json),
        webauthn::decode(&response.authenticator_data),
        webauthn::decode(&response.signature),
    );
    let (Ok(credential_id), Ok(client_data), Ok(authenticator_data), Ok(signature)) = decoded
    else {
        return Ok(None);
    };
    let Ok(challenge) = webauthn::challenge_of(&client_data) else {
        return Ok(None);
    };
    // Spent before anything else is looked at, so each challenge is tried once
    // whatever happens next.
    if state
        .passkeys
        .take_challenge(&challenge, ChallengePurpose::SignIn, now)
        .await?
        .is_none()
    {
        return Ok(None);
    }

    let Some(passkey) = state.passkeys.find_by_credential(&credential_id).await? else {
        return Ok(None);
    };
    // The handle the browser says the passkey was made for has to be the
    // account the passkey is stored against.
    if let Some(handle) = response.user_handle.as_deref() {
        let matches = webauthn::decode(handle)
            .is_ok_and(|handle| handle == passkey.user_id.to_string().as_bytes());
        if !matches {
            return Ok(None);
        }
    }

    let assertion = match webauthn::verify_assertion(
        rp,
        &passkey.public_key,
        &client_data,
        &authenticator_data,
        &signature,
    ) {
        Ok(assertion) => assertion,
        Err(reason) => {
            tracing::info!(passkey = %passkey.id, %reason, "a passkey sign-in was refused");
            return Ok(None);
        }
    };
    // A counter that does not move forward means two copies of one key. Zero
    // on both sides is a passkey that keeps no counter, which most synced ones
    // do not.
    if (assertion.sign_count != 0 || passkey.sign_count != 0)
        && assertion.sign_count <= passkey.sign_count
    {
        tracing::warn!(passkey = %passkey.id, "a passkey's counter went backwards; it may have been copied");
        return Ok(None);
    }

    let Some(user) = state
        .users
        .find_by_id(passkey.user_id)
        .await?
        .filter(|user| user.state.may_sign_in())
    else {
        return Ok(None);
    };
    state
        .passkeys
        .record_use(&passkey.id, assertion.sign_count, assertion.backed_up, now)
        .await?;
    Ok(Some((user, passkey.id)))
}
