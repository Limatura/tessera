//! Registration, sign-in and passwords.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::Cookie;
use serde::{Deserialize, Serialize};
use tessera_core::{EmailAddress, Password, PasswordPolicy, UserId, Username};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    api::{AUTH_TAG, extract::ApiJson},
    auth::{
        cookie,
        extract::{ClientAddress, Credential, SelfServiceUser},
        password,
        throttle::Decision,
    },
    config::RegistrationPolicy,
    db::repository::user::User,
    error::{Error, Result},
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
    /// Whether the account may reach the administrative endpoints.
    pub is_admin: bool,
    /// Whether the account must set a new password before doing anything else.
    ///
    /// Set on the administrator generated at first run. A client seeing this
    /// should send the user to a change-password screen and nowhere else.
    pub must_change_password: bool,
}

impl From<&User> for AccountResponse {
    fn from(user: &User) -> Self {
        Self {
            id: user.id.to_string(),
            username: user.username.to_string(),
            email: user.email.to_string(),
            is_admin: user.is_admin,
            must_change_password: user.must_change_password,
        }
    }
}

/// Body of `POST /auth/register`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterRequest {
    /// 3–32 characters: letters, digits, `_` and `-`, starting with a letter or
    /// digit. Stored lowercased.
    // The bounds and the pattern are `Username`'s own, so a generated client
    // rejects what the server would have rejected — and a fuzzer driven by this
    // document stops spending its budget on inputs that cannot be usernames.
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
    /// At least `password_min_length` characters — see `/capabilities`.
    // 8 and 1024 are `PasswordPolicy`'s floor and ceiling: no instance can be
    // configured to accept anything outside them, so they are true of every
    // deployment. The *configured* minimum is usually higher and is not
    // knowable here — the document is generated at build time — which is why
    // `/capabilities` exists and why 400 is a documented response.
    #[schema(
        example = "correct horse battery staple",
        min_length = 8,
        max_length = 1024
    )]
    pub password: String,
}

/// Body of `POST /auth/login`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    /// Username or email address; either is accepted.
    #[schema(example = "alice")]
    pub identifier: String,
    /// The account's password.
    ///
    /// Bounded by what any instance could ever have accepted rather than by the
    /// policy in force, so that raising the minimum cannot lock out an account
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

/// Refuses everything when the instance has no accounts.
///
/// `404`, not `403`. The endpoint genuinely is not part of this deployment, and
/// the contract still describes it because the contract describes the software
/// rather than one instance's configuration — `/capabilities` is what tells a
/// client which half it is talking to.
fn require_accounts(state: &AppState) -> Result<()> {
    if state.config.features.accounts {
        Ok(())
    } else {
        Err(Error::NotFound)
    }
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
        (status = 201, description = "Account created and signed in", body = AccountResponse),
        (status = 400, description = "A value failed its rule", body = crate::error::ErrorBody),
        (status = 403, description = "Registration is closed", body = crate::error::ErrorBody),
        (status = 409, description = "Name or address is taken", body = crate::error::ErrorBody),
    ),
)]
pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<RegisterRequest>,
) -> Result<Response> {
    require_accounts(&state)?;

    match state.config.auth.registration {
        RegistrationPolicy::Open => {}
        RegistrationPolicy::Invite | RegistrationPolicy::Closed => {
            return Err(Error::Forbidden);
        }
    }

    let username = Username::parse(&body.username)?;
    let email = EmailAddress::parse(&body.email)?;
    let password = Password::parse(body.password, state.config.auth.password)?;

    // Checked before inserting so the caller gets "that name is taken" rather
    // than a constraint violation. The unique index is still what guarantees it
    // — this check races, and losing the race lands on the same 409 below.
    if state.users.find_by_username(&username).await?.is_some() {
        return Err(Error::Conflict("that username is taken".to_owned()));
    }
    if state.users.find_by_email(&email).await?.is_some() {
        return Err(Error::Conflict(
            "an account already exists for that address".to_owned(),
        ));
    }

    let now = OffsetDateTime::now_utc();
    let user = User {
        id: UserId::generate(),
        username,
        email,
        password_hash: password::hash(password).await?,
        // The first account to exist becomes the administrator. On an instance
        // where nobody bootstrapped one, that is the person who set it up.
        is_admin: !state.users.any_exist().await?,
        must_change_password: false,
        email_verified_at: None,
        created_at: now,
        updated_at: now,
    };

    if let Err(error) = state.users.insert(&user).await {
        // The check above raced with another registration, or something else
        // went wrong. Which one is decided by looking, rather than by assuming
        // every insert failure is a conflict — that would report a database
        // outage as "that name is taken".
        if state
            .users
            .find_by_username(&user.username)
            .await?
            .is_some()
            || state.users.find_by_email(&user.email).await?.is_some()
        {
            return Err(Error::Conflict(
                "that username or address is already taken".to_owned(),
            ));
        }
        return Err(error.into());
    }

    let cookie = start_session(&state, &user, &headers, now).await?;
    Ok((
        StatusCode::CREATED,
        [(header::SET_COOKIE, cookie.to_string())],
        Json(AccountResponse::from(&user)),
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
    ClientAddress(address): ClientAddress,
    headers: HeaderMap,
    ApiJson(body): ApiJson<LoginRequest>,
) -> Result<Response> {
    require_accounts(&state)?;

    // Keyed on the account *and* where the request came from. Account alone
    // would let anyone lock a known user out by guessing at them; address alone
    // would let a botnet spread its guesses and never trip the limit. A second
    // counter watches the address by itself, because otherwise naming a
    // different account each time produces a fresh key and no limit at all.
    let key = format!("{}|{address}", body.identifier.to_lowercase());
    let throttles = &state.throttles;
    if let Decision::Deny { retry_after } = throttles.check(&throttles.login, &key, &address) {
        return Err(Error::TooManyRequests {
            retry_after_secs: retry_after.as_secs(),
        });
    }

    // Parsed against the widest bounds rather than this instance's policy: the
    // password being presented was chosen under whatever policy was in force
    // then, and rejecting it here would show "wrong password" for a password
    // that is right. See `PasswordPolicy::widest`.
    let password = Password::parse(body.password, PasswordPolicy::widest()).ok();
    let user = find_by_identifier(&state, &body.identifier).await?;

    let authenticated = if let (Some(user), Some(password)) = (user.as_ref(), password) {
        // The hash is cloned because verification moves it onto the blocking
        // pool, and `user` is still needed to build the response.
        password::verify(password, user.password_hash.clone()).await?
    } else {
        // No account, or a password the policy would not even accept. Both still
        // pay for a hash: returning early would make "no such user" measurably
        // faster than "wrong password", which is a free account-enumeration
        // oracle on an endpoint anyone can call.
        verify_against_nothing().await?;
        false
    };

    let Some(user) = user.filter(|_| authenticated) else {
        throttles.record_failure(&throttles.login, &key, &address);
        return Err(Error::Unauthorized);
    };

    throttles.login.reset(&key);
    let now = OffsetDateTime::now_utc();
    let cookie = start_session(&state, &user, &headers, now).await?;

    Ok((
        [(header::SET_COOKIE, cookie.to_string())],
        Json(AccountResponse::from(&user)),
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
    headers: HeaderMap,
    SelfServiceUser(current): SelfServiceUser,
) -> Result<Response> {
    // Only a session can be logged out of. A bearer token is revoked, not
    // signed out — treating logout as revocation would let a stolen token
    // destroy the owner's automation by calling this.
    if current.credential == Credential::Session
        && let Some(secret) = cookie::session_secret(&headers)
        && let Some(session) = state.sessions.find_by_secret(&secret).await?
    {
        state.sessions.delete(session.id).await?;
    }

    let secure = state
        .config
        .auth
        .cookie_secure
        .applies(&headers, state.config.server.trusted_proxy);
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
pub async fn session(SelfServiceUser(current): SelfServiceUser) -> Result<Json<AccountResponse>> {
    Ok(Json(AccountResponse::from(&current.user)))
}

/// Changes the caller's password.
///
/// Every other session is ended, and a fresh one is issued so the caller stays
/// signed in where they are. Whoever knew the old password may be holding a
/// session, and changing a password is how a person says they want that to stop.
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
    headers: HeaderMap,
    SelfServiceUser(current): SelfServiceUser,
    ApiJson(body): ApiJson<ChangePasswordRequest>,
) -> Result<Response> {
    // A leaked API token must not be able to take the account. Requiring the
    // browser credential keeps the blast radius of a leaked token to what the
    // token can do, rather than everything.
    if current.credential != Credential::Session {
        return Err(Error::Forbidden);
    }

    // Widest bounds for the password being *presented*, the instance's policy
    // for the one being *chosen*. Otherwise raising `min_length` would leave
    // everyone below it unable to log in and unable to fix it.
    let supplied = Password::parse(body.current_password, PasswordPolicy::widest())
        .map_err(|_| Error::Unauthorized)?;
    if !password::verify(supplied, current.user.password_hash.clone()).await? {
        return Err(Error::Unauthorized);
    }

    let replacement = Password::parse(body.new_password, state.config.auth.password)?;
    let hash = password::hash(replacement).await?;

    let now = OffsetDateTime::now_utc();
    state.users.set_password(current.user.id, hash, now).await?;
    state.sessions.delete_for_user(current.user.id).await?;

    let cookie = start_session(&state, &current.user, &headers, now).await?;
    let mut account = AccountResponse::from(&current.user);
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
    // Privileged parsing: this is a *lookup*, not a claim. The bootstrap
    // administrator is called `admin`, which the reserved list refuses — so the
    // ordinary parser would make the one account every instance has unable to
    // sign in.
    if let Ok(username) = Username::parse_privileged(identifier) {
        return Ok(state.users.find_by_username(&username).await?);
    }
    Ok(None)
}

async fn start_session(
    state: &AppState,
    user: &User,
    headers: &HeaderMap,
    now: OffsetDateTime,
) -> Result<Cookie<'static>> {
    // `std::time::Duration` is unsigned and `time::Duration` is not, so the
    // conversion is fallible only for a TTL beyond any calendar. Falling back to
    // the default rather than failing keeps an absurd configuration from
    // breaking sign-in; start-up validation already refuses zero.
    let ttl = time::Duration::try_from(state.config.auth.session_ttl())
        .unwrap_or_else(|_| time::Duration::days(30));

    let (_, secret) = state
        .sessions
        .create(user.id, now, now.saturating_add(ttl))
        .await?;

    // The cookie's `Max-Age` matches the row's expiry, so a browser stops
    // sending a session at the same moment the server stops accepting it.
    let secure = state
        .config
        .auth
        .cookie_secure
        .applies(headers, state.config.server.trusted_proxy);
    Ok(cookie::session_cookie(&secret, ttl, secure))
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
