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

/// The key a per-address limit counts under.
///
/// Folded, where [`EmailAddress`] deliberately is not: the local part is
/// case-sensitive by the standard, so `Alice@example.com` and `alice@example.com`
/// stay separate accounts. A limit is not about identity though — it is about
/// one mailbox, and in practice every provider delivers both to the same one.
///
/// Counting the unfolded address means the limit can be walked around by
/// changing case, which is not a small margin: measured against a running
/// instance, thirty-two attempts at eight spellings of a six-letter local part
/// were answered twenty-four times against a stated limit of three, and a longer
/// local part doubles that for every extra letter.
fn address_limit_key(email: &EmailAddress) -> String {
    email.as_str().to_lowercase()
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
    //
    // A name is answered plainly, unlike an address, and that asymmetry is a
    // decision rather than an oversight. It does mean this endpoint will tell
    // anyone whether a given name is taken — but a form that cannot say "try
    // another one" is a form nobody can complete, so the alternative is finding
    // out after a round trip through an inbox, or not at all.
    //
    // What makes it an acceptable trade and an address not: a name is a handle
    // somebody chose for this instance, while an address identifies a person
    // outside it. "alice has an account here" is a smaller fact than "the person
    // reachable at this address has an account here", and only the second is a
    // question strangers get to ask. An instance that does not want even the
    // first one asked closes registration, which is refused above before any of
    // this is reached.
    if state.users.find_by_username(&username).await?.is_some() {
        return Err(Error::Conflict("that username is taken".to_owned()));
    }

    // With a relay configured, nothing is created until the address answers.
    if state.mailer.is_some() {
        return start_verified_registration(&state, username, email, password);
    }

    // Without one there is nothing to verify with, so this is the whole of it.
    // That is the ordinary case for the instance this was built for: one person
    // on a NAS, who is also the one who set the address.
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

    let cookie = start_session(&state, &user, &headers, origin.via_trusted_proxy, now).await?;
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
    ClientAddress(origin): ClientAddress,
    headers: HeaderMap,
    ApiJson(body): ApiJson<LoginRequest>,
) -> Result<Response> {
    require_accounts(&state)?;

    // Keyed on the account *and* where the request came from. Account alone
    // would let anyone lock a known user out by guessing at them; address alone
    // would let a botnet spread its guesses and never trip the limit. A second
    // counter watches the address by itself, because otherwise naming a
    // different account each time produces a fresh key and no limit at all.
    let key = format!("{}|{}", body.identifier.to_lowercase(), origin.source);
    let throttles = &state.throttles;
    if let Decision::Deny { retry_after } = throttles.check(&throttles.login, &key, &origin.source)
    {
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
        throttles.record_failure(&throttles.login, &key, &origin.source);
        return Err(Error::Unauthorized);
    };

    throttles.login.reset(&key);
    let now = OffsetDateTime::now_utc();
    let cookie = start_session(&state, &user, &headers, origin.via_trusted_proxy, now).await?;

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
    ClientAddress(origin): ClientAddress,
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
    ClientAddress(origin): ClientAddress,
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
    // `false`: this password was chosen by the person who owns the account,
    // so there is nothing left to force a change of.
    state
        .users
        .set_password(current.user.id, hash, false, now)
        .await?;
    state.sessions.delete_for_user(current.user.id).await?;

    let cookie = start_session(
        &state,
        &current.user,
        &headers,
        origin.via_trusted_proxy,
        now,
    )
    .await?;
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
    via_trusted_proxy: bool,
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
        .applies(headers, via_trusted_proxy);
    Ok(cookie::session_cookie(&secret, ttl, secure))
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
/// address is taken" would make the registration form a way of asking whether
/// somebody has an account here — the same question `POST /auth/password-reset`
/// is careful to refuse, reachable through a different door.
///
/// The person who owns the address still finds out: they get a message saying
/// somebody tried, which is also where they learn that repeated attempts are
/// worth noticing. That is the one place the fact is disclosed, and it goes
/// only to the one person entitled to it.
///
/// # Why this is not `async`
///
/// Nothing here awaits, and that is the property worth keeping rather than an
/// accident: every step whose cost depends on whether the address is known runs
/// in the detached task below. If this ever needs an `await`, that is the moment
/// to ask what it is waiting for and whether a stopwatch could tell.
fn start_verified_registration(
    state: &AppState,
    username: Username,
    email: EmailAddress,
    password: Password,
) -> Result<Response> {
    let Some(mailer) = state.mailer.clone() else {
        return Err(Error::NotFound);
    };

    let counter = &state.throttles.registration;
    let limit_key = address_limit_key(&email);
    if let Decision::Deny { retry_after } = counter.check(&limit_key) {
        return Err(Error::TooManyRequests {
            retry_after_secs: retry_after.as_secs(),
        });
    }
    // Counted here, before anything is looked up, and not after a message goes
    // out. Counting on send would mean the limit is only ever reached by
    // addresses that have one — so the fourth attempt would answer `429` for a
    // member and `202` for a stranger, which is the membership question this
    // endpoint refuses to answer, asked four times instead of once. The same
    // mistake was live in the reset handler and CI was green for it.
    counter.record_failure(&limit_key);

    // Detached, so the response goes out before any of it. Everything below is
    // work a free address does and a taken one skips — hashing a password, and
    // handing a message to a relay — so leaving it in the request path would
    // make the two answer at measurably different speeds. That is the same
    // membership question the `202` refuses, read with a stopwatch instead.
    //
    // Not a theoretical margin. The instance that still does this synchronously
    // (the one with no relay, where there is nothing to hide) answers in 22 ms
    // for a free address and 0.6 ms for a taken one. With the work detached,
    // both answer in 0.4 ms and the medians differ by 0.03 ms — noise.
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
                detached.config.mail.public_url.trim_end_matches('/')
            );
            crate::mail::already_registered_letter(email.as_str(), &reset)
        } else {
            let hash = match password::hash(password).await {
                Ok(hash) => hash,
                Err(error) => {
                    tracing::error!(?error, "could not hash a password during registration");
                    return;
                }
            };
            let now = OffsetDateTime::now_utc();
            let ttl = time::Duration::try_from(detached.config.mail.registration_ttl())
                .unwrap_or_else(|_| time::Duration::hours(24));
            let secret = match detached
                .pending
                .start(&username, &email, hash, now, now.saturating_add(ttl))
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
                detached.config.mail.public_url.trim_end_matches('/'),
                secret.expose()
            );
            crate::mail::registration_letter(
                email.as_str(),
                &link,
                detached.config.mail.registration_ttl_hours,
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
/// would have made "register and never confirm" a free way to lock every good
/// name away, with nothing an operator could do about it. The price is this:
/// the name is checked against today's accounts, and somebody can be told at
/// this point that it went to another person. That is rare, it is recoverable,
/// and it is a much smaller problem than the alternative.
///
/// # Errors
///
/// `400` when the link is unknown, spent or expired, `403` when registration was
/// closed while this was waiting, `409` when the name went to somebody else in
/// the meantime, `404` when this instance sends no mail.
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
    if state.mailer.is_none() {
        return Err(Error::NotFound);
    }

    // Read again here, and not only where the link was sent. Closing
    // registration has to mean no more accounts, and without this it would mean
    // "no more accounts except from the links already in inboxes" — up to
    // `registration_ttl_hours` of them, which an operator cannot see, cannot
    // count and cannot cancel. Somebody closing registration is usually reacting
    // to something, and a day of accounts still arriving is the opposite of what
    // they asked for.
    match state.config.auth.registration {
        RegistrationPolicy::Open => {}
        RegistrationPolicy::Invite | RegistrationPolicy::Closed => {
            return Err(Error::Forbidden);
        }
    }

    let now = OffsetDateTime::now_utc();
    let secret = crate::auth::secret::Secret::from_client(body.token);
    let found = state.pending.find_by_secret(&secret).await?;

    // One message for unknown and expired alike: telling them apart would let
    // somebody holding a stale link learn that it was once real.
    let Some(pending) = found.filter(|pending| pending.is_usable_at(now)) else {
        return Err(Error::bad_request(
            "this confirmation link is not usable; register again",
        ));
    };

    if state
        .users
        .find_by_username(&pending.username)
        .await?
        .is_some()
    {
        return Err(Error::Conflict(
            "that username was taken while this link was waiting; register again with another"
                .to_owned(),
        ));
    }
    // Somebody else confirmed the same address first, or an account was made
    // for it by other means. Answered like an unusable link rather than as a
    // conflict: the address is not this caller's to be told about, and they
    // have a working password for it either way.
    if state.users.find_by_email(&pending.email).await?.is_some() {
        state.pending.finish(pending.id).await?;
        return Err(Error::bad_request(
            "this confirmation link is not usable; register again",
        ));
    }

    let user = User {
        id: UserId::generate(),
        username: pending.username.clone(),
        email: pending.email.clone(),
        password_hash: pending.password_hash.clone(),
        // Decided here rather than when the link was sent, because "first
        // account" has to mean first to *exist*. Two people confirming at once
        // both read no accounts and both claim it — which the unique index does
        // not prevent, since it is not a uniqueness question. It is also not
        // worth a transaction: an instance whose first two accounts are created
        // in the same instant is one nobody has bootstrapped, and both of those
        // people are its operator.
        is_admin: !state.users.any_exist().await?,
        must_change_password: false,
        // The whole point of the wait.
        email_verified_at: Some(now),
        created_at: now,
        updated_at: now,
    };
    if let Err(error) = state.users.insert(&user).await {
        // The checks above raced with another confirmation. Decided by looking
        // rather than by assuming every insert failure is a conflict, which
        // would report a database outage as "that name is taken" — and the
        // window is real: two links naming the same username, opened at the same
        // moment, both read a free name.
        if state
            .users
            .find_by_username(&user.username)
            .await?
            .is_some()
            || state.users.find_by_email(&user.email).await?.is_some()
        {
            return Err(Error::Conflict(
                "that username or address was taken while this link was waiting; \
                 register again"
                    .to_owned(),
            ));
        }
        return Err(error.into());
    }
    state.pending.finish(pending.id).await?;

    let cookie = start_session(&state, &user, &headers, origin.via_trusted_proxy, now).await?;
    Ok((
        StatusCode::CREATED,
        [(header::SET_COOKIE, cookie.to_string())],
        Json(AccountResponse::from(&user)),
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
/// the mail could be handed to the relay. Any other shape turns this endpoint
/// into a way of asking "does this person have an account here" — which for a
/// file courier is a question about who uses it, answerable by anyone, at any
/// rate the throttle allows.
///
/// The cost is that a typo produces no feedback. That is the right way round:
/// somebody who mistyped their own address will notice when no mail arrives,
/// and nobody learns anything from the response either way.
///
/// # Errors
///
/// `404` when this instance sends no mail — the route is not mounted at all, so
/// this is the router's answer rather than the handler's. `429` when this
/// address has already been sent as many links as the window allows.
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
    let Some(mailer) = state.mailer.clone() else {
        // Unreachable in practice: the route is only mounted when a relay
        // exists. Kept so that a future caller cannot turn it into a panic.
        return Err(Error::NotFound);
    };

    // Normalised before it is used as a throttle key, or `A@b.com` and
    // `a@b.com` would each get their own budget.
    let Ok(email) = EmailAddress::parse(&body.email) else {
        // A malformed address is answered like every other case. Saying "that
        // is not an address" is harmless on its own, but it is one more way for
        // the responses to differ from each other.
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
    // Counting only when a message was actually sent looks more careful — it
    // keeps a stranger's address from spending somebody else's budget — and it
    // is exactly the mistake this endpoint exists to avoid: four requests would
    // then answer `429` for a member and `202` for everyone else, which is the
    // membership oracle the `202` was supposed to close. Measured, not
    // reasoned: it took one loop of `curl` to read the difference.
    //
    // The cost of counting always is that somebody who knows an address can
    // stop it being reset for an hour. That is a nuisance with a way out (the
    // `reset-password` subcommand, which needs no mail at all), whereas the
    // oracle would be a property of the service nobody could work around.
    throttles.reset.record_failure(&limit_key);

    // Everything that depends on the account happening to exist runs detached,
    // and the response goes out before any of it.
    //
    // Not tidiness — the same defence again. Handing a message to a relay takes
    // an SMTP handshake, so doing it inline makes a request for a real address
    // measurably slower than one for a stranger's: a clean read of who has an
    // account here, from the outside, with a stopwatch. Even against a relay
    // that fails instantly the difference was consistently double.
    let detached = state.clone();
    tokio::spawn(async move {
        let Ok(Some(user)) = detached.users.find_by_email(&email).await else {
            return;
        };

        let now = OffsetDateTime::now_utc();
        let ttl = time::Duration::try_from(detached.config.mail.reset_ttl())
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
            detached.config.mail.public_url.trim_end_matches('/'),
            secret.expose()
        );
        let letter = password_reset_letter(
            user.email.as_str(),
            &link,
            detached.config.mail.reset_ttl_minutes,
        );

        if let Err(error) = mailer.send(letter).await {
            // Logged, not returned — the caller is already gone, and the relay
            // being down is the operator's problem to find in the logs.
            tracing::error!(?error, "could not send a password reset link");
        }
    });

    Ok((StatusCode::ACCEPTED, ()).into_response())
}

/// Sets a new password from a reset link.
///
/// Every session belonging to the account is ended. Somebody asking to reset a
/// password is usually saying they have lost control of it, and leaving the
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
    if state.mailer.is_none() {
        return Err(Error::NotFound);
    }

    let now = OffsetDateTime::now_utc();
    let secret = crate::auth::secret::Secret::from_client(body.token);
    let found = state.resets.find_by_secret(&secret).await?;

    // One message for unknown, spent and expired alike. They are different to
    // us — which is why the row survives being used — but telling them apart
    // out loud would let somebody with a spent link learn that it was once
    // real, and that is a fact about somebody else's account.
    let Some(token) = found.filter(|token| token.is_usable_at(now)) else {
        return Err(Error::bad_request(
            "this reset link is not usable; ask for a new one",
        ));
    };

    // Parsed before the token is spent: a password that fails the policy should
    // leave the link usable, or a typo costs somebody the whole round trip
    // through their inbox.
    let password = Password::parse(body.password, state.config.auth.password)?;

    // The database decides who wins, so two clicks arriving together set the
    // password once.
    if !state.resets.spend(token.id, now).await? {
        return Err(Error::bad_request(
            "this reset link is not usable; ask for a new one",
        ));
    }

    let hash = password::hash(password).await?;
    // `false`: the person just chose this one, so there is nothing to force a
    // change of.
    state
        .users
        .set_password(token.user_id, hash, false, now)
        .await?;

    // Both, and in this order. Any other outstanding link is a key somebody may
    // still be holding; any existing session belongs to whoever had the old
    // password.
    state.resets.spend_all_for(token.user_id, now).await?;
    state.sessions.delete_for_user(token.user_id).await?;

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
