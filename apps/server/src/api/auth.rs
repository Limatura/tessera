//! Registration, sign-in and passwords.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::Cookie;
use serde::{Deserialize, Serialize};
use tessera_core::{
    AccountState, EmailAddress, Password, PasswordPolicy, Permission, PermissionSet, RoleId,
    UserId, Username,
};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    api::{AUTH_TAG, extract::ApiJson},
    auth::{
        cookie,
        extract::{ClientAddress, CurrentUser, SelfServiceUser},
        password,
        proxy::{Origin, UNPLACEABLE},
        throttle::Decision,
    },
    config::RegistrationPolicy,
    db::repository::{
        invite::Invite,
        pending_registration::Waiting,
        role::Role,
        session::{SessionEnding, SessionOrigin, StartedBy},
        user::User,
    },
    error::{Error, ErrorCode, Result},
    mail::password_reset_letter,
    state::AppState,
};

/// An account, as the API describes it. Never carries a hash.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccountResponse {
    /// Stable identifier.
    #[schema(example = "01K1ZQK9V8QF5S3T4M2N6P7R8S")]
    pub id: String,
    /// Normalised account name.
    #[schema(example = "alice")]
    pub username: String,
    /// Normalised address.
    #[schema(example = "alice@example.com")]
    pub email: String,
    /// Whether the account holds the seeded administrator role.
    ///
    /// Kept because clients read it, but derived and not stored, and narrower
    /// than `permissions`. An account in a role granting `users:read`
    /// and nothing else is not an administrator, yet reaches one administrative
    /// endpoint. New clients should read `permissions`.
    pub is_admin: bool,
    /// Everything this account is allowed to do.
    ///
    /// `resource:verb` strings, and what a client should decide from. A screen
    /// guessing from `is_admin` is wrong for every role between "member" and
    /// "everything".
    #[schema(example = json!(["shares:create", "shares:read"]))]
    pub permissions: Vec<String>,
    /// The roles it is in, by name.
    #[schema(example = json!(["member"]))]
    pub roles: Vec<String>,
    /// Whether the account must set a new password before doing anything else.
    ///
    /// Set on the administrator generated at first run. A client seeing this
    /// should send the user to a change-password screen and nowhere else.
    pub must_change_password: bool,
    /// A storage quota set on this one account, overriding its role's.
    ///
    /// `null` means the role's applies; `0` means this account has no ceiling
    /// of its own. What is actually left is `storage_remaining_bytes` on
    /// `/capabilities`, which accounts for what has been used.
    #[schema(example = 10_737_418_240_u64)]
    pub quota_bytes: Option<u64>,
    /// When the address was last proven to reach this user, or `null` if never.
    ///
    /// Set by confirming a registration, by a provider vouching for the
    /// address, or by following the link from
    /// `POST /account/email/verification`. Cleared when the address changes.
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime, example = "2026-09-26T08:00:00Z")]
    pub email_verified_at: Option<OffsetDateTime>,
}

impl AccountResponse {
    /// Builds the response from the account and the roles it is in.
    ///
    /// Computed from the roles, the only place the answer lives.
    /// `users.is_admin` mirrored this and has been dropped: reading it reported
    /// whatever the column said, not what the account can do.
    #[must_use]
    pub fn build(user: &User, roles: &[Role]) -> Self {
        let permissions: PermissionSet = roles
            .iter()
            .flat_map(|role| role.permissions.iter())
            .collect();
        Self {
            id: user.id.to_string(),
            username: user.username.to_string(),
            email: user.email.to_string(),
            // Either seeded role that administers the instance. Kept for
            // clients that still read it; neither role answers "may they
            // delete an account". That is what `permissions` is for.
            is_admin: roles.iter().any(|role| {
                let id = role.id.to_string();
                id == RoleId::OWNER || id == RoleId::ADMIN
            }),
            permissions: permissions
                .iter()
                .map(|permission| permission.as_str().to_owned())
                .collect(),
            roles: roles.iter().map(|role| role.name.clone()).collect(),
            must_change_password: user.must_change_password,
            quota_bytes: user.quota_bytes,
            email_verified_at: user.email_verified_at,
        }
    }
}

/// Body of `POST /auth/register`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterRequest {
    /// 3–32 characters: letters, digits, `_` and `-`, starting with a letter or
    /// digit. Stored lowercased.
    // The bounds and the pattern are `Username`'s own, so a generated client
    // rejects what the server would, and a fuzzer driven by this document stops
    // spending its budget on inputs that cannot be usernames.
    #[schema(
        example = "alice",
        min_length = 3,
        max_length = 32,
        pattern = "^[a-zA-Z0-9][a-zA-Z0-9_-]*$"
    )]
    pub username: String,
    /// Address to reach the account at.
    #[schema(example = "alice@example.com", format = Email)]
    pub email: String,
    /// At least `password_min_length` characters. See `/capabilities`.
    // 8 and 1024 are `PasswordPolicy`'s floor and ceiling, so they hold for
    // every deployment. The *configured* minimum is usually higher and cannot
    // be known here, since the document is generated at build time. That is
    // why `/capabilities` exists and 400 is a documented response.
    #[schema(
        example = "correct horse battery staple",
        min_length = 8,
        max_length = 1024
    )]
    pub password: String,
    /// An invite code, on an instance that asks for one.
    ///
    /// Ignored where registration is open, so a client does not have to know
    /// which kind of instance it is talking to before filling in the form.
    /// `/capabilities` says which it is.
    #[schema(example = "3f9a1c…")]
    #[serde(default)]
    pub invite_code: Option<String>,
}

/// Body of `POST /auth/login`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    /// Username or email address; either is accepted.
    #[schema(example = "alice")]
    pub identifier: String,
    /// The account's password.
    ///
    /// Bounded by what any instance could ever have accepted, not by the policy
    /// in force, so that raising the minimum cannot lock out an account
    /// whose password was valid when it was chosen.
    #[schema(
        example = "correct horse battery staple",
        min_length = 8,
        max_length = 1024
    )]
    pub password: String,
}

/// Body of `PATCH /auth/password`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    /// The password in force now. Required even though the caller is signed in:
    /// it is what stops a borrowed, unlocked browser from taking the account.
    #[schema(min_length = 8, max_length = 1024)]
    pub current_password: String,
    /// The replacement.
    #[schema(min_length = 8, max_length = 1024)]
    pub new_password: String,
}

/// The key a per-address limit counts under.
///
/// Folded, where [`EmailAddress`] is not. The local part is case-sensitive by
/// the standard, so `Alice@example.com` and `alice@example.com` stay separate
/// accounts. A limit is about one mailbox
/// and not identity, and every provider delivers both to the same one.
///
/// Counting the unfolded address lets the limit be walked around by changing
/// case. Measured against a running instance: thirty-two attempts at eight
/// spellings of a six-letter local part were answered twenty-four times against
/// a stated limit of three, and every extra letter doubles that.
fn address_limit_key(email: &EmailAddress) -> String {
    email.as_str().to_lowercase()
}

/// The one answer an invite ever gets wrong.
///
/// Unknown, spent and expired are one message. Telling them apart would let the
/// holder of a code they were never given learn that it was once real, turning
/// the registration form into a way to test codes instead of use one.
pub(super) fn invite_refused() -> Error {
    Error::new(ErrorCode::InviteUnusable)
}

/// Reads the invite a registration came in on, when this instance asks for one.
///
/// # Why the place is not taken here
///
/// The invite is only looked at. Charging a place here would let one visitor
/// spend every place on a code by submitting the form five times without ever
/// confirming an address, with no account to show for it. So the code is
/// checked now, remembered on the pending row, and charged once an account
/// exists.
///
/// The check still belongs here. Reporting a bad invite after the password is
/// chosen and the inbox checked is worse.
///
/// # Errors
///
/// `400` when the instance asks for a code and the one given is unknown, spent,
/// expired or absent.
pub(super) async fn check_invite(state: &AppState, code: Option<&str>) -> Result<Option<Invite>> {
    if state.config().auth.registration != RegistrationPolicy::Invite {
        // An open instance ignores one instead of refusing it, so a client can
        // send the same body to either kind.
        return Ok(None);
    }

    let code = code.map(str::trim).filter(|code| !code.is_empty());
    let Some(code) = code else {
        return Err(invite_refused());
    };

    let found = state
        .invites
        .find_by_code(&crate::auth::secret::Secret::from_client(code.to_owned()))
        .await?;
    let now = OffsetDateTime::now_utc();
    found
        .filter(|invite| invite.is_usable_at(now))
        .map(Some)
        .ok_or_else(invite_refused)
}

/// Refuses everything when the instance has no accounts.
///
/// `404`, not `403`: the endpoint genuinely is not part of this deployment. The
/// contract still describes it, because it describes the software and not one
/// instance's configuration. `/capabilities` tells a client which half
/// it is talking to.
pub(super) fn require_accounts(state: &AppState) -> Result<()> {
    if state.config().features.accounts {
        Ok(())
    } else {
        Err(Error::NotFound)
    }
}

/// Which role a brand-new account goes into.
///
/// The only place the choice is made, so "what can a new account do" is one
/// function instead of a property of whichever registration path it came
/// through.
///
/// The first account administers the instance whatever the configuration says.
/// An instance whose first account cannot administer it has no way to fix
/// that.
///
/// # Errors
///
/// `500` when `auth.default_role` names a role that is not there. Startup
/// refuses that configuration, so reaching it means the role was deleted while
/// the server ran, and quietly using `member` instead would overrule a stated
/// policy.
pub(super) async fn seed_role(state: &AppState, first: bool) -> Result<RoleId> {
    if first {
        // The owner role, not `admin`. The first account belongs to whoever
        // runs the instance, and `admin` cannot grant permissions, so an
        // instance without an owner can never admit a second account.
        return Ok(RoleId::parse(RoleId::OWNER)?);
    }
    let wanted = &state.config().auth.default_role;
    let Some(role) = state.roles.find_by_name(wanted).await? else {
        tracing::error!(role = %wanted, "auth.default_role names a role that is not there");
        return Err(Error::new(ErrorCode::RegistrationClosed).with_message(
            "this instance is not configured to accept new accounts right now".to_owned(),
        ));
    };
    Ok(role.id)
}

/// Creates an account and signs it in.
///
/// # Errors
///
/// `404` when the instance has no accounts, `403` when registration is closed,
/// `409` when the name or address is taken, `400` when a value fails its rule.
#[utoipa::path(
    post,
    path = "/auth/register",
    tag = AUTH_TAG,
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Account created and signed in; only where this instance sends no mail", body = AccountResponse),
        (status = 202, description = "Accepted; a confirmation link is sent if that address is free", body = ()),
        (status = 400, description = "A value failed its rule", body = crate::error::ErrorBody),
        (status = 403, description = "Registration is closed", body = crate::error::ErrorBody),
        (status = 409, description = "That username is taken", body = crate::error::ErrorBody),
        (status = 429, description = "Too many messages have been sent to this address", body = crate::error::ErrorBody),
    ),
)]
pub async fn register(
    State(state): State<AppState>,
    ClientAddress(origin): ClientAddress,
    headers: HeaderMap,
    ApiJson(body): ApiJson<RegisterRequest>,
) -> Result<Response> {
    require_accounts(&state)?;

    // Checked before anything is parsed, so a closed instance says so without
    // reading what was sent.
    if state.config().auth.registration == RegistrationPolicy::Closed {
        return Err(Error::new(ErrorCode::RegistrationClosed));
    }

    let username = Username::parse(&body.username)?;
    let email = EmailAddress::parse(&body.email)?;
    let password = Password::parse(body.password, state.config().auth.password)?;
    crate::auth::pwned::refuse_if_breached(&state, &password).await?;

    // Checked before inserting so the caller gets "that name is taken" rather
    // than a constraint violation. The unique index is still the guarantee;
    // this check races, and losing the race lands on the same 409 below.
    //
    // A name is answered plainly where an address is not, which does tell any
    // caller whether a given name is taken. A form that cannot say "try another
    // one" cannot be completed.
    //
    // A name is a handle chosen for this instance; an address identifies a
    // person outside it. "alice has an account here" is a smaller fact than
    // "the person reachable at this address has an account here", and
    // only the second is a question strangers get to ask. An instance that does
    // not want even the first asked closes registration, refused above.
    if state.users.find_by_username(&username).await?.is_some() {
        return Err(Error::new(ErrorCode::UsernameTaken));
    }

    let invite = check_invite(&state, body.invite_code.as_deref()).await?;

    // With a relay configured, nothing is created until the address answers.
    if state.mailer().is_some() {
        return start_verified_registration(&state, username, email, password, invite);
    }

    // Without one there is nothing to verify with, so this is all of it.
    // The ordinary case here: one person on a NAS who set the address.
    if state.users.find_by_email(&email).await?.is_some() {
        return Err(Error::new(ErrorCode::EmailTaken)
            .with_message("an account already exists for that address".to_owned()));
    }

    let now = OffsetDateTime::now_utc();
    // Taken before the account is written: a place charged only once everything
    // else succeeded is a place two people can hold at once. If the insert then
    // fails, the operator loses one slot on a code they can reissue. Cheaper
    // than a code that admits more people than it says.
    if let Some(invite) = invite
        && !state.invites.spend(invite.id, now).await?
    {
        return Err(invite_refused());
    }

    // The first account to exist becomes the administrator. Where none was
    // bootstrapped, that is whoever set the instance up.
    let first = !state.users.any_exist().await?;
    let user = User {
        id: UserId::generate(),
        username,
        email,
        role_id: Some(seed_role(&state, first).await?),
        quota_bytes: None,
        max_file_bytes: None,
        must_change_password: false,
        state: AccountState::Active,
        deleted_at: None,
        deleted_by: None,
        email_verified_at: None,
        created_at: now,
        updated_at: now,
    };

    let hash = password::hash(password, state.config().auth.hashing).await?;
    if let Err(error) = state.users.insert(&user, Some(&hash)).await {
        // The check above raced with another registration, or something else
        // went wrong. Decided by looking, not by assuming every insert failure
        // is a conflict, which would report an outage as a taken name.
        if state
            .users
            .find_by_username(&user.username)
            .await?
            .is_some()
            || state.users.find_by_email(&user.email).await?.is_some()
        {
            return Err(Error::new(ErrorCode::AccountExists));
        }
        return Err(error.into());
    }

    let cookie =
        start_session(&state, &user, &headers, &origin, StartedBy::password(), now).await?;
    Ok((
        StatusCode::CREATED,
        [(header::SET_COOKIE, cookie.to_string())],
        Json(AccountResponse::build(
            &user,
            &state.roles.of_user(user.id).await?,
        )),
    )
        .into_response())
}

/// Signs in.
///
/// # Errors
///
/// `401` when the credentials do not match, `429` when too many attempts have
/// failed, `404` when the instance has no accounts.
#[utoipa::path(
    post,
    path = "/auth/login",
    tag = AUTH_TAG,
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Signed in", body = AccountResponse),
        (status = 400, description = "The body is not the shape this expects", body = crate::error::ErrorBody),
        (status = 401, description = "Wrong credentials", body = crate::error::ErrorBody),
        (status = 429, description = "Too many failed attempts", body = crate::error::ErrorBody),
    ),
)]
pub async fn login(
    State(state): State<AppState>,
    ClientAddress(origin): ClientAddress,
    headers: HeaderMap,
    ApiJson(body): ApiJson<LoginRequest>,
) -> Result<Response> {
    require_accounts(&state)?;

    // Keyed on the account *and* where the request came from. Account alone
    // lets anyone lock a known user out by guessing at them; address alone lets
    // a botnet spread its guesses and never trip the limit. A second counter
    // watches the address by itself, since naming a different account each time
    // otherwise produces a fresh key and no limit at all.
    let key = format!("{}|{}", body.identifier.to_lowercase(), origin.source);
    let throttles = &state.throttles;
    if let Decision::Deny { retry_after } = throttles.check(&throttles.login, &key, &origin.source)
    {
        return Err(Error::TooManyRequests {
            retry_after_secs: retry_after.as_secs(),
        });
    }

    // Parsed against the widest bounds, not this instance's policy. The
    // password being presented was chosen under whatever policy was in force
    // then, and rejecting it here would show "wrong password" for a password
    // that is right. See `PasswordPolicy::widest`.
    let password = Password::parse(body.password, PasswordPolicy::widest()).ok();
    let user = find_by_identifier(&state, &body.identifier).await?;

    let stored = match user.as_ref() {
        Some(found) => state.users.password_of(found.id).await?,
        None => None,
    };
    let authenticated = if let (Some(stored), Some(password)) = (stored, password) {
        // The hash is cloned because verification moves it onto the blocking
        // pool, and `user` is still needed to build the response.
        password::verify(password, stored).await?
    } else {
        // No account, a password the policy rejects, or a provider-only
        // account. All three pay for a hash: returning early would make them
        // measurably faster than "wrong password", an account-enumeration
        // oracle. The third is easy to miss, and would reveal which addresses
        // belong to single sign-on users.
        verify_against_nothing().await?;
        false
    };

    // The state is checked here, not before the hash, and answers the same
    // `401` as a wrong password. Checking earlier returns faster for a
    // suspended account, which reopens the oracle the work above closes.
    let Some(user) = user.filter(|found| authenticated && found.state.may_sign_in()) else {
        throttles.record_failure(&throttles.login, &key, &origin.source);
        return Err(Error::new(ErrorCode::InvalidCredentials));
    };

    throttles.login.reset(&key);
    let now = OffsetDateTime::now_utc();
    let cookie =
        start_session(&state, &user, &headers, &origin, StartedBy::password(), now).await?;

    Ok((
        [(header::SET_COOKIE, cookie.to_string())],
        Json(AccountResponse::build(
            &user,
            &state.roles.of_user(user.id).await?,
        )),
    )
        .into_response())
}

/// Ends the current session.
///
/// # Errors
///
/// `401` when nothing was signed in.
#[utoipa::path(
    post,
    path = "/auth/logout",
    tag = AUTH_TAG,
    responses(
        (status = 204, description = "Signed out"),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
    ),
    security(("session" = []), ("bearer" = [])),
)]
pub async fn logout(
    State(state): State<AppState>,
    ClientAddress(origin): ClientAddress,
    headers: HeaderMap,
    SelfServiceUser(current): SelfServiceUser,
) -> Result<Response> {
    // Only a session can be logged out of. A bearer token is revoked, not
    // signed out: treating logout as revocation would let a stolen token
    // destroy the owner's automation by calling this.
    if let Some(id) = current.credential.session_id() {
        state
            .sessions
            .end(id, SessionEnding::SignedOut, OffsetDateTime::now_utc())
            .await?;
    }

    let secure = state
        .config()
        .auth
        .cookie_secure
        .applies(&headers, origin.via_trusted_proxy);
    Ok((
        StatusCode::NO_CONTENT,
        [(
            header::SET_COOKIE,
            cookie::clearing_cookie(secure).to_string(),
        )],
    )
        .into_response())
}

/// Reports who the caller is.
///
/// # Errors
///
/// `401` when nothing was signed in.
#[utoipa::path(
    get,
    path = "/auth/session",
    tag = AUTH_TAG,
    responses(
        (status = 200, description = "The signed-in account", body = AccountResponse),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
    ),
    security(("session" = []), ("bearer" = [])),
)]
pub async fn session(
    State(state): State<AppState>,
    SelfServiceUser(current): SelfServiceUser,
) -> Result<Json<AccountResponse>> {
    let roles = state.roles.of_user(current.user.id).await?;
    Ok(Json(AccountResponse::build(&current.user, &roles)))
}

/// Changes the caller's password.
///
/// Every other session is ended, and a fresh one issued so the caller stays
/// signed in where they are. Changing a password is how a person says that
/// whoever knew the old one should stop being signed in.
///
/// # Errors
///
/// `401` when the current password is wrong, `403` when authenticated with an
/// API token, `400` when the new password fails the policy.
#[utoipa::path(
    patch,
    path = "/auth/password",
    tag = AUTH_TAG,
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "Changed; other sessions ended", body = AccountResponse),
        (status = 400, description = "The new password fails the policy", body = crate::error::ErrorBody),
        (status = 401, description = "The current password is wrong", body = crate::error::ErrorBody),
        (status = 403, description = "Not permitted with an API token", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn change_password(
    State(state): State<AppState>,
    ClientAddress(origin): ClientAddress,
    headers: HeaderMap,
    SelfServiceUser(current): SelfServiceUser,
    ApiJson(body): ApiJson<ChangePasswordRequest>,
) -> Result<Response> {
    // A leaked API token must not be able to take the account. Requiring the
    // browser credential keeps a leaked token to what the token can do.
    if !current.credential.is_session() {
        return Err(Error::new(ErrorCode::SessionRequired));
    }

    // Widest bounds for the password being *presented*, the instance's policy
    // for the one being *chosen*. Otherwise raising `min_length` would leave
    // everyone below it unable to log in and unable to fix it.
    let supplied = Password::parse(body.current_password, PasswordPolicy::widest())
        .map_err(|_| Error::new(ErrorCode::CurrentPasswordIncorrect))?;
    // An account with no password cannot confirm one. `Unauthorized`, not a
    // distinct answer: the caller is signed in, so there is nothing to
    // enumerate, and the fix is to set a password, not to guess again.
    let stored = state
        .users
        .password_of(current.user.id)
        .await?
        .ok_or_else(|| Error::new(ErrorCode::CurrentPasswordIncorrect))?;
    if !password::verify(supplied, stored).await? {
        return Err(Error::new(ErrorCode::CurrentPasswordIncorrect));
    }

    let replacement = Password::parse(body.new_password, state.config().auth.password)?;
    crate::auth::pwned::refuse_if_breached(&state, &replacement).await?;
    let hash = password::hash(replacement, state.config().auth.hashing).await?;

    let now = OffsetDateTime::now_utc();
    // `false`: this password was chosen by the person who owns the account,
    // so there is nothing left to force a change of.
    state
        .users
        .set_password(current.user.id, hash, false, now)
        .await?;
    // Every one of them, this one included. The browser gets a fresh session
    // below, and the old credential should work nowhere.
    state
        .sessions
        .end_all_for_user(current.user.id, None, SessionEnding::PasswordChanged, now)
        .await?;

    let cookie = start_session(
        &state,
        &current.user,
        &headers,
        &origin,
        StartedBy::password(),
        now,
    )
    .await?;
    let roles = state.roles.of_user(current.user.id).await?;
    let mut account = AccountResponse::build(&current.user, &roles);
    account.must_change_password = false;

    Ok(([(header::SET_COOKIE, cookie.to_string())], Json(account)).into_response())
}

/// A syntactically valid Argon2 hash that no password matches.
///
/// Verified against when no account was found, so that the failing path costs
/// the same as the succeeding one.
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$\
                          c29tZXNhbHRzb21lc2FsdA$\
                          UEhDIHN0cmluZyB0aGF0IG5vdGhpbmcgbWF0Y2hlcw";

/// Spends one Argon2 verification on nothing, so that a failed sign-in costs the
/// same whether or not the account exists.
async fn verify_against_nothing() -> Result<()> {
    let filler = "x".repeat(PasswordPolicy::FLOOR);
    let Ok(password) = Password::parse(filler, PasswordPolicy::widest()) else {
        // Unreachable: the filler is exactly the widest policy's minimum.
        // Skipping the work is still better than panicking.
        return Ok(());
    };
    password::verify(password, DUMMY_HASH.to_owned()).await?;
    Ok(())
}

async fn find_by_identifier(state: &AppState, identifier: &str) -> Result<Option<User>> {
    // Either form is accepted, because people do not remember which they used.
    // Parsing decides which lookup to run: a value with an `@` can only be an
    // address, and one without can only be a name.
    if let Ok(email) = EmailAddress::parse(identifier) {
        return Ok(state.users.find_by_email(&email).await?);
    }
    // Privileged parsing, because this is a *lookup* and not a claim. The
    // bootstrap administrator is called `admin`, which the reserved list
    // refuses, so the ordinary parser would lock out the one account every
    // instance has.
    if let Ok(username) = Username::parse_privileged(identifier) {
        return Ok(state.users.find_by_username(&username).await?);
    }
    Ok(None)
}

pub(super) async fn start_session(
    state: &AppState,
    user: &User,
    headers: &HeaderMap,
    origin: &Origin,
    started_by: StartedBy<'_>,
    now: OffsetDateTime,
) -> Result<Cookie<'static>> {
    // `std::time::Duration` is unsigned and `time::Duration` is not, so this
    // fails only for a TTL beyond any calendar. Falling back to the default
    // keeps an absurd configuration from breaking sign-in; start-up validation
    // already refuses zero.
    let ttl = time::Duration::try_from(state.config().auth.session_ttl())
        .unwrap_or_else(|_| time::Duration::days(30));

    let (_, secret) = state
        .sessions
        .create(
            user.id,
            &session_origin(headers, origin),
            started_by,
            now,
            now.saturating_add(ttl),
        )
        .await?;

    // The cookie's `Max-Age` matches the row's expiry, so a browser stops
    // sending a session at the same moment the server stops accepting it.
    let secure = state
        .config()
        .auth
        .cookie_secure
        .applies(headers, origin.via_trusted_proxy);
    Ok(cookie::session_cookie(&secret, ttl, secure))
}

/// What gets recorded about the request that started a session.
///
/// # Why the address can be absent
///
/// [`Origin::source`] always holds something, since a throttle needs a key even
/// with no address to be had, and says `unknown` then. A person reads this
/// column to decide whether a session is theirs, and `unknown` there reads like
/// a place. Nothing beats a word that looks like an answer.
///
/// # Why a user agent can be absent too
///
/// `to_str` refuses a header value outside visible ASCII, and nothing is stored
/// when it does. Every real browser sends ASCII, so this only drops a client
/// that went out of its way, and a session showing no browser is truthful where
/// a mangled string would not be.
fn session_origin(headers: &HeaderMap, origin: &Origin) -> SessionOrigin {
    SessionOrigin {
        ip: (origin.source != UNPLACEABLE).then(|| origin.source.clone()),
        user_agent: headers
            .get(header::USER_AGENT)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|agent| !agent.is_empty())
            .map(str::to_owned),
    }
}

/// Body of `POST /auth/register/confirm`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterConfirmRequest {
    /// The token from the link.
    pub token: String,
}

/// Takes a registration as far as an email, and no further.
///
/// # Why the answer is the same whichever it is
///
/// `202` whether or not the address already has an account. Saying "that
/// address is taken" would make the registration form a way to ask who has an
/// account here, the same question `POST /auth/password-reset` refuses.
///
/// The owner of the address still finds out, by mail. That is the one place
/// the fact is disclosed, and it reaches only the person entitled to it.
///
/// # Why this is not `async`
///
/// Nothing here awaits. Every step whose cost depends on whether the address is
/// known runs in the detached task below. If this ever needs an `await`, ask
/// what it is waiting for and whether a stopwatch could tell.
fn start_verified_registration(
    state: &AppState,
    username: Username,
    email: EmailAddress,
    password: Password,
    invite: Option<Invite>,
) -> Result<Response> {
    let Some(mailer) = state.mailer().as_ref().clone() else {
        return Err(Error::NotFound);
    };

    let counter = &state.throttles.registration;
    let limit_key = address_limit_key(&email);
    if let Decision::Deny { retry_after } = counter.check(&limit_key) {
        return Err(Error::TooManyRequests {
            retry_after_secs: retry_after.as_secs(),
        });
    }
    // Counted before anything is looked up, not after a message goes out.
    // Counting on send means only addresses that get one ever reach the limit,
    // so the fourth attempt answers `429` for a member and `202` for a
    // stranger: the membership question this endpoint refuses, asked four times
    // instead of once. The reset handler had the same bug with CI green.
    counter.record_failure(&limit_key);

    // Detached, so the response goes out first. Everything below is work a free
    // address does and a taken one skips (hashing a password, handing a message
    // to a relay), so leaving it in the request path lets a stopwatch ask the
    // membership question the `202` refuses.
    //
    // Measured. Done synchronously, a free address answers in 22 ms and a taken
    // one in 0.6 ms; detached, both answer in 0.4 ms with medians 0.03 ms apart.
    let detached = state.clone();
    tokio::spawn(async move {
        let taken = match detached.users.find_by_email(&email).await {
            Ok(found) => found.is_some(),
            Err(error) => {
                tracing::error!(?error, "could not check an address during registration");
                return;
            }
        };

        let letter = if taken {
            let reset = format!(
                "{}/reset-password",
                detached.config().mail.public_url.trim_end_matches('/')
            );
            crate::mail::already_registered_letter(email.as_str(), &reset)
        } else {
            let hash = match password::hash(password, detached.config().auth.hashing).await {
                Ok(hash) => hash,
                Err(error) => {
                    tracing::error!(?error, "could not hash a password during registration");
                    return;
                }
            };
            let now = OffsetDateTime::now_utc();
            let ttl = time::Duration::try_from(detached.config().mail.registration_ttl())
                .unwrap_or_else(|_| time::Duration::hours(24));
            let secret = match detached
                .pending
                .start(
                    Waiting {
                        username: &username,
                        email: &email,
                        password_hash: hash,
                        invite_id: invite.map(|invite| invite.id),
                    },
                    now,
                    now.saturating_add(ttl),
                )
                .await
            {
                Ok(secret) => secret,
                Err(error) => {
                    tracing::error!(?error, "could not record a pending registration");
                    return;
                }
            };
            let link = format!(
                "{}/register/confirm?token={}",
                detached.config().mail.public_url.trim_end_matches('/'),
                secret.expose()
            );
            crate::mail::registration_letter(
                email.as_str(),
                &link,
                detached.config().mail.registration_ttl_hours,
            )
        };

        if let Err(error) = mailer.send(letter).await {
            tracing::error!(?error, "could not send a registration message");
        }
    });

    Ok((StatusCode::ACCEPTED, ()).into_response())
}

/// Turns a confirmed registration into an account.
///
/// # Why the checks happen here and not when the link was sent
///
/// Nothing was reserved while the message sat in an inbox. Reserving the name
/// would make "register and never confirm" a free way to lock every good name
/// away. The price is that the name is checked against today's accounts, so a
/// confirmation can fail here because the name went elsewhere. Rare, and
/// recoverable.
///
/// # Errors
///
/// `400` when the link is unknown, spent or expired, `403` when registration was
/// closed while this was waiting, `409` when the name was taken in the
/// meantime, `404` when this instance sends no mail.
#[utoipa::path(
    post,
    path = "/auth/register/confirm",
    tag = AUTH_TAG,
    request_body = RegisterConfirmRequest,
    responses(
        (status = 201, description = "Account created and signed in", body = AccountResponse),
        (status = 400, description = "The link is not usable, or the body is not the shape this expects", body = crate::error::ErrorBody),
        (status = 403, description = "Registration was closed while this link was waiting", body = crate::error::ErrorBody),
        (status = 404, description = "This instance sends no mail", body = crate::error::ErrorBody),
        (status = 409, description = "The name was taken while this was waiting", body = crate::error::ErrorBody),
    ),
)]
pub async fn confirm_registration(
    State(state): State<AppState>,
    ClientAddress(origin): ClientAddress,
    headers: HeaderMap,
    ApiJson(body): ApiJson<RegisterConfirmRequest>,
) -> Result<Response> {
    require_accounts(&state)?;
    if state.mailer().is_none() {
        return Err(Error::NotFound);
    }

    // Read again here, not only where the link was sent. Closing registration
    // has to mean no more accounts; without this it means "none except from the
    // links already in inboxes", up to `registration_ttl_hours` of them, which
    // an operator cannot see, count or cancel.
    //
    // `Invite` still passes. The code was checked when the link was sent and is
    // charged below, so this link was genuinely issued.
    if state.config().auth.registration == RegistrationPolicy::Closed {
        return Err(Error::new(ErrorCode::RegistrationClosed));
    }

    let now = OffsetDateTime::now_utc();
    let secret = crate::auth::secret::Secret::from_client(body.token);
    let found = state.pending.find_by_secret(&secret).await?;

    // One message for unknown and expired alike. Telling them apart would let
    // the holder of a stale link learn that it was once real.
    let Some(pending) = found.filter(|pending| pending.is_usable_at(now)) else {
        return Err(Error::new(ErrorCode::RegistrationLinkUnusable));
    };

    if state
        .users
        .find_by_username(&pending.username)
        .await?
        .is_some()
    {
        return Err(Error::new(ErrorCode::UsernameTaken).with_message(
            "that username was taken while this link was waiting; register again with another"
                .to_owned(),
        ));
    }
    // The address was confirmed first by another registration, or an account
    // was made for it another way. Answered like an unusable link, not a
    // conflict, since the address is not this caller's to be told about.
    if state.users.find_by_email(&pending.email).await?.is_some() {
        state.pending.finish(pending.id).await?;
        return Err(Error::new(ErrorCode::RegistrationLinkUnusable));
    }

    // The place is charged here, where an account is about to exist. Two links
    // for the last place on a code both reach this point; the condition lives in
    // the `UPDATE`, so exactly one of them gets it.
    if let Some(invite) = pending.invite_id
        && !state.invites.spend(invite, now).await?
    {
        state.pending.finish(pending.id).await?;
        return Err(invite_refused());
    }

    let first = !state.users.any_exist().await?;
    let user = User {
        id: UserId::generate(),
        username: pending.username.clone(),
        email: pending.email.clone(),
        role_id: Some(seed_role(&state, first).await?),
        quota_bytes: None,
        max_file_bytes: None,
        // Decided here, not when the link was sent, because "first
        // account" has to mean first to *exist*. Two people confirming at once
        // both read no accounts and both claim it, which no unique index
        // prevents. Not worth a transaction either: an instance whose first two
        // accounts arrive in the same instant belongs to both of them.
        must_change_password: false,
        state: AccountState::Active,
        deleted_at: None,
        deleted_by: None,
        // The whole point of the wait.
        email_verified_at: Some(now),
        created_at: now,
        updated_at: now,
    };
    if let Err(error) = state
        .users
        .insert(&user, Some(&pending.password_hash))
        .await
    {
        // The checks above raced with another confirmation. Decided by looking
        // and not by assuming every insert failure is a conflict, which
        // would report an outage as a taken name. The window is real: two links
        // naming the same username, opened together, both read a free name.
        if state
            .users
            .find_by_username(&user.username)
            .await?
            .is_some()
            || state.users.find_by_email(&user.email).await?.is_some()
        {
            return Err(Error::new(ErrorCode::AccountExists).with_message(
                "that username or address was taken while this link was waiting; \
                 register again"
                    .to_owned(),
            ));
        }
        return Err(error.into());
    }
    state.pending.finish(pending.id).await?;

    let cookie =
        start_session(&state, &user, &headers, &origin, StartedBy::password(), now).await?;
    Ok((
        StatusCode::CREATED,
        [(header::SET_COOKIE, cookie.to_string())],
        Json(AccountResponse::build(
            &user,
            &state.roles.of_user(user.id).await?,
        )),
    )
        .into_response())
}

/// Body of `POST /auth/password-reset`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PasswordResetRequest {
    /// The address to send the link to.
    #[schema(example = "someone@example.com")]
    pub email: String,
}

/// Body of `POST /auth/password-reset/confirm`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PasswordResetConfirmRequest {
    /// The token from the link.
    pub token: String,
    /// What to set the password to.
    pub password: String,
}

/// Sends a password reset link, if there is an account and mail is configured.
///
/// # Why this always answers the same way
///
/// `202` whether or not the address belongs to an account, and whether or not
/// the mail reached the relay. Any other shape turns this into a way to ask
/// "does this person have an account here", answerable by anyone at whatever
/// rate the throttle allows.
///
/// The cost is that a typo produces no feedback. A mistyped address is noticed
/// when no mail arrives.
///
/// # Errors
///
/// `404` when this instance sends no mail. The route is not mounted, so that is
/// the router's answer. `429` when this address has already been sent as
/// many links as the window allows.
#[utoipa::path(
    post,
    path = "/auth/password-reset",
    tag = AUTH_TAG,
    request_body = PasswordResetRequest,
    responses(
        (status = 202, description = "Accepted; a link is sent if that address has an account"),
        (status = 400, description = "The body is not the shape this expects", body = crate::error::ErrorBody),
        (status = 404, description = "This instance sends no mail", body = crate::error::ErrorBody),
        (status = 429, description = "Too many links have been sent to this address", body = crate::error::ErrorBody),
    ),
)]
pub async fn request_password_reset(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<PasswordResetRequest>,
) -> Result<Response> {
    let Some(mailer) = state.mailer().as_ref().clone() else {
        // Unreachable in practice: the route is only mounted when a relay
        // exists. Kept so that a future caller cannot turn it into a panic.
        return Err(Error::NotFound);
    };

    // Normalised before it is used as a throttle key, or `A@b.com` and
    // `a@b.com` would each get their own budget.
    let Ok(email) = EmailAddress::parse(&body.email) else {
        // A malformed address is answered like every other case: saying "that
        // is not an address" is one more way for the responses to differ.
        return Ok((StatusCode::ACCEPTED, ()).into_response());
    };

    let throttles = &state.throttles;
    let limit_key = address_limit_key(&email);
    if let Decision::Deny { retry_after } = throttles.reset.check(&limit_key) {
        return Err(Error::TooManyRequests {
            retry_after_secs: retry_after.as_secs(),
        });
    }
    // Counted before the account is looked up, so that being refused says
    // nothing about whether the address is known.
    //
    // Counting only when a message was sent looks more careful, and is the
    // mistake this endpoint exists to avoid: four requests then answer `429`
    // for a member and `202` for everyone else: the membership oracle the
    // `202` closes. One loop of `curl` reads the difference.
    //
    // The cost is that anyone who knows an address can block its reset for an
    // hour. That is a nuisance with a way out, the `reset-password` subcommand;
    // the oracle would have none.
    throttles.reset.record_failure(&limit_key);

    // Everything that depends on the account existing runs detached, and the
    // response goes out first. The same defence as above: handing a message to
    // a relay takes an SMTP handshake, so inline it makes a request for a real
    // address measurably slower than one for a stranger's. Even against a relay
    // that fails instantly the difference was consistently double.
    let detached = state.clone();
    tokio::spawn(async move {
        let Ok(Some(user)) = detached.users.find_by_email(&email).await else {
            return;
        };

        let now = OffsetDateTime::now_utc();
        let ttl = time::Duration::try_from(detached.config().mail.reset_ttl())
            .unwrap_or_else(|_| time::Duration::minutes(30));
        let secret = match detached
            .resets
            .issue(user.id, now, now.saturating_add(ttl))
            .await
        {
            Ok(secret) => secret,
            Err(error) => {
                tracing::error!(?error, "could not record a password reset link");
                return;
            }
        };

        let link = format!(
            "{}/reset-password?token={}",
            detached.config().mail.public_url.trim_end_matches('/'),
            secret.expose()
        );
        let letter = password_reset_letter(
            email.as_str(),
            &link,
            detached.config().mail.reset_ttl_minutes,
        );

        if let Err(error) = mailer.send(letter).await {
            // Logged, not returned: the caller is already gone, and a relay
            // that is down is the operator's problem to find in the logs.
            tracing::error!(?error, "could not send a password reset link");
        }
    });

    Ok((StatusCode::ACCEPTED, ()).into_response())
}

/// Sets a new password from a reset link.
///
/// Every session belonging to the account is ended. A reset request usually
/// means control of the password has been lost, and leaving the
/// sessions alone would leave whoever had it signed in.
///
/// # Errors
///
/// `400` when the token is unknown, spent or expired, or the new password fails
/// the policy. `404` when this instance sends no mail.
#[utoipa::path(
    post,
    path = "/auth/password-reset/confirm",
    tag = AUTH_TAG,
    request_body = PasswordResetConfirmRequest,
    responses(
        (status = 204, description = "Password set; every session ended"),
        (status = 400, description = "The link is not usable, or the password fails the policy", body = crate::error::ErrorBody),
        (status = 404, description = "This instance sends no mail", body = crate::error::ErrorBody),
    ),
)]
pub async fn confirm_password_reset(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<PasswordResetConfirmRequest>,
) -> Result<Response> {
    if state.mailer().is_none() {
        return Err(Error::NotFound);
    }

    let now = OffsetDateTime::now_utc();
    let secret = crate::auth::secret::Secret::from_client(body.token);
    let found = state.resets.find_by_secret(&secret).await?;

    // One message for unknown, spent and expired alike. The three differ here,
    // and that is why the row survives being used, but saying so would let the
    // holder of a spent link learn it was once real.
    let Some(token) = found.filter(|token| token.is_usable_at(now)) else {
        return Err(Error::new(ErrorCode::ResetLinkUnusable));
    };

    // Parsed before the token is spent. A password that fails the policy should
    // leave the link usable, or a typo costs the whole round trip through the
    // inbox.
    let password = Password::parse(body.password, state.config().auth.password)?;
    crate::auth::pwned::refuse_if_breached(&state, &password).await?;

    // The database decides who wins, so two clicks arriving together set the
    // password once.
    if !state.resets.spend(token.id, now).await? {
        return Err(Error::new(ErrorCode::ResetLinkUnusable));
    }

    let hash = password::hash(password, state.config().auth.hashing).await?;
    // `false`: the person just chose this one, so there is nothing to force a
    // change of.
    state
        .users
        .set_password(token.user_id, hash, false, now)
        .await?;

    // Both, in this order. Any other outstanding link is a key still in
    // circulation; any existing session belongs to whoever had the old
    // password.
    state.resets.spend_all_for(token.user_id, now).await?;
    state
        .sessions
        .end_all_for_user(token.user_id, None, SessionEnding::PasswordReset, now)
        .await?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dummy_hash_is_a_parsable_phc_string() {
        // If it were malformed, verification against it would short-circuit and
        // the timing equalisation it exists for would silently stop working.
        assert!(
            argon2::password_hash::PasswordHash::new(DUMMY_HASH).is_ok(),
            "the dummy hash must parse, or the timing defence is a no-op"
        );
    }
}

/// Body of `PATCH /account`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateAccountRequest {
    /// A new name, or omitted to keep the current one.
    #[schema(
        example = "alice",
        min_length = 3,
        max_length = 32,
        pattern = "^[a-zA-Z0-9][a-zA-Z0-9_-]*$"
    )]
    #[serde(default)]
    pub username: Option<String>,
    /// A new address, or omitted to keep the current one.
    ///
    /// Changing it clears the verification: the new address has not been proven
    /// reachable, and carrying the old flag over would mean an unverified
    /// address wearing a verified one's badge.
    #[schema(example = "alice@example.com", format = Email)]
    #[serde(default)]
    pub email: Option<String>,
    /// The password in force now.
    ///
    /// Required even though the caller is signed in. An unlocked browser left
    /// on a desk is otherwise enough to move the account's address to one the
    /// finder controls, and from there the reset flow hands them the account.
    #[schema(min_length = 8, max_length = 1024)]
    pub current_password: String,
}

/// Body of `DELETE /account`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteAccountRequest {
    /// The password in force now, for the same reason as above.
    #[schema(min_length = 8, max_length = 1024)]
    pub current_password: String,
    /// Whether to end everything this account uploaded.
    ///
    /// Defaults to ending it. "Delete my account" usually means "and the things
    /// I sent"; wanting the handed-out links to keep working is the rarer case,
    /// worth one explicit `false`.
    ///
    /// # What "delete" means here
    ///
    /// Every link stops working the moment this returns. The bytes go later,
    /// when the housekeeping pass reaches them, because removing hundreds of
    /// objects inside one request times out half way through with no record of
    /// where it got to. Nothing serves the content in the meantime; what
    /// arrives after the answer is the disk space.
    #[serde(default = "delete_shares_by_default")]
    pub delete_shares: bool,
}

const fn delete_shares_by_default() -> bool {
    true
}

/// Confirms the caller's password, for an act that is worth confirming.
///
/// # Errors
///
/// `401` when it is wrong, `403` when the caller used an API token.
async fn reconfirm(state: &AppState, current: &CurrentUser, supplied: String) -> Result<()> {
    // A leaked API token must not be able to rename or remove the account, for
    // the same reason as changing a password.
    if !current.credential.is_session() {
        return Err(Error::new(ErrorCode::SessionRequired));
    }
    let supplied = Password::parse(supplied, PasswordPolicy::widest())
        .map_err(|_| Error::new(ErrorCode::CurrentPasswordIncorrect))?;
    let stored = state
        .users
        .password_of(current.user.id)
        .await?
        .ok_or_else(|| Error::new(ErrorCode::CurrentPasswordIncorrect))?;
    if password::verify(supplied, stored).await? {
        Ok(())
    } else {
        Err(Error::new(ErrorCode::CurrentPasswordIncorrect))
    }
}

/// Changes the caller's own name or address.
///
/// # Errors
///
/// `400` when a value fails its rule, `401` when the password is wrong, `403`
/// when an API token was used, `409` when the name or address is taken.
#[utoipa::path(
    patch,
    path = "/account",
    operation_id = "update_account",
    tag = AUTH_TAG,
    request_body = UpdateAccountRequest,
    responses(
        (status = 200, description = "The account as it now stands", body = AccountResponse),
        (status = 400, description = "A value is not usable", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in, or the password is wrong", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token", body = crate::error::ErrorBody),
        (status = 409, description = "The name or address is taken", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn update_account(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(body): ApiJson<UpdateAccountRequest>,
) -> Result<Json<AccountResponse>> {
    reconfirm(&state, &current, body.current_password).await?;

    let username = match body.username {
        // Not `parse_privileged`. No account may rename itself to `admin`; the
        // reserved list guards this door as well as registration.
        Some(wanted) => Username::parse(&wanted)?,
        None => current.user.username.clone(),
    };
    let email = match body.email {
        Some(wanted) => EmailAddress::parse(&wanted)?,
        None => current.user.email.clone(),
    };

    let renamed = username != current.user.username;
    let moved = email != current.user.email;
    if renamed && state.users.find_by_username(&username).await?.is_some() {
        return Err(
            Error::new(ErrorCode::UsernameTaken).with_message("that name is taken".to_owned())
        );
    }
    if moved && state.users.find_by_email(&email).await?.is_some() {
        return Err(Error::new(ErrorCode::EmailTaken));
    }

    // Cleared when the address moves, kept otherwise. Correcting a typo in a
    // name should not mean proving the address again.
    let verified_at = if moved {
        None
    } else {
        current.user.email_verified_at
    };
    if !state
        .users
        .rename(
            current.user.id,
            &username,
            &email,
            verified_at,
            OffsetDateTime::now_utc(),
        )
        .await?
    {
        return Err(Error::NotFound);
    }

    let updated = state
        .users
        .find_by_id(current.user.id)
        .await?
        .ok_or(Error::NotFound)?;
    let roles = state.roles.of_user(updated.id).await?;
    Ok(Json(AccountResponse::build(&updated, &roles)))
}

/// Removes the caller's own account.
///
/// # Why the shares are the caller's decision and not an administrator's
///
/// They are the account's own uploads. An administrator removing another
/// account does not touch its content, a separate and explicit act,
/// but an account removing itself may say what happens to what it sent.
///
/// # Errors
///
/// `401` when the password is wrong, `403` when an API token was used or the
/// caller is the last one able to administer the instance.
#[utoipa::path(
    delete,
    path = "/account",
    operation_id = "delete_account",
    tag = AUTH_TAG,
    request_body = DeleteAccountRequest,
    responses(
        (status = 204, description = "The account is gone"),
        (status = 401, description = "Not signed in, or the password is wrong", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token", body = crate::error::ErrorBody),
        (status = 409, description = "Nobody would be left to administer the instance", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn delete_account(
    State(state): State<AppState>,
    ClientAddress(origin): ClientAddress,
    headers: HeaderMap,
    current: CurrentUser,
    ApiJson(body): ApiJson<DeleteAccountRequest>,
) -> Result<Response> {
    reconfirm(&state, &current, body.current_password).await?;

    // The same guard the administrative route has, from the other side: an
    // instance whose last administrator removes themselves cannot be
    // administered, and nothing undoes that.
    if current.permissions.allows(Permission::RolesWrite)
        && state.roles.holders_of(Permission::RolesWrite).await? <= 1
    {
        return Err(Error::new(ErrorCode::LastAdministrator).with_message(
            "you are the only administrator; give somebody else the role first".to_owned(),
        ));
    }

    let now = OffsetDateTime::now_utc();
    if body.delete_shares {
        // Before the account goes, while the rows still point at it.
        state
            .shares
            .expire_all_for_user(current.user.id, now)
            .await?;
    }
    if !state
        .users
        .soft_delete(current.user.id, current.user.id, now)
        .await?
    {
        return Err(Error::NotFound);
    }
    state.sessions.delete_all_for_user(current.user.id).await?;
    state.tokens.delete_all_for_user(current.user.id).await?;

    tracing::info!(account = %current.user.id, "an account removed itself");

    // The rows are gone, so the cookie is already dead, but leaving it in the
    // browser makes the next request carry a credential for an account that
    // does not exist, and the client has to work out that `401` means "you are
    // finished" and not "sign in again". `logout` clears it too.
    let secure = state
        .config()
        .auth
        .cookie_secure
        .applies(&headers, origin.via_trusted_proxy);
    Ok((
        StatusCode::NO_CONTENT,
        [(
            header::SET_COOKIE,
            cookie::clearing_cookie(secure).to_string(),
        )],
    )
        .into_response())
}
