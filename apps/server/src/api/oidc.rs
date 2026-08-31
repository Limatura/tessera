//! Signing in through an identity provider.
//!
//! Three endpoints, all reached by a browser navigating rather than by script:
//!
//! - `GET /auth/oidc` says whether there is a provider and what to call it, so a sign-in page knows
//!   whether to draw the button.
//! - `GET /auth/oidc/start` redirects to the provider.
//! - `GET /auth/oidc/callback` is where the provider sends the browser back.
//!
//! # Why failures redirect instead of answering
//!
//! The callback is a top-level navigation. A `403` with a JSON body would be
//! shown as raw JSON in the address bar, with no way back to the application.
//! So every outcome, including every refusal, ends in a redirect to the sign-in
//! page carrying a stable code in the query string, and the frontend turns that
//! into a sentence.
//!
//! The codes are deliberately coarse. "Your provider did not release an
//! address" is worth saying because somebody can act on it; the difference
//! between a wrong nonce and an expired token is not, and spelling it out tells
//! whoever is probing which of their guesses was closer.

use axum::{
    extract::{Query, State},
    http::{HeaderMap, header},
    response::{IntoResponse as _, Response},
};
use serde::{Deserialize, Serialize};
use tessera_core::{AccountState, EmailAddress, UserId};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    api::{
        AUTH_TAG,
        auth::{check_invite, invite_refused, require_accounts, seed_role, start_session},
    },
    auth::{
        extract::{ClientAddress, SelfServiceUser},
        oidc::{admit, claims, flow, token},
    },
    db::repository::{session::SignedInWith, user::User},
    error::{Error, Result},
    state::AppState,
};

/// The provider's name in `identities.provider`.
///
/// One provider today. The column exists so a second one is configuration
/// rather than a migration, and this is the placeholder that becomes a
/// configured name when that happens.
const PROVIDER: &str = "oidc";

/// How long a sign-in may take before the flow row stops being answerable.
///
/// Long enough for somebody to be asked for a second factor and go and find
/// their phone. Not so long that abandoned rows pile up between sweeps.
const FLOW_TTL_MINUTES: i64 = 15;

/// Where the browser is sent when it comes back.
const SIGN_IN_PAGE: &str = "/login";

/// Where a link attempt is sent when it comes back, signed in already.
const ACCOUNT_PAGE: &str = "/account";

/// What a sign-in page needs to know before it draws anything.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProviderResponse {
    /// Whether an identity provider is configured and usable.
    pub enabled: bool,
    /// What to write on the button.
    pub display_name: String,
}

/// The query a sign-in starts with.
#[derive(Debug, Deserialize)]
pub struct StartQuery {
    /// An invitation to spend once the sign-in succeeds.
    ///
    /// Carried in the flow row rather than through the provider, which never
    /// sees it.
    #[serde(default)]
    pub invite: Option<String>,
}

/// What the provider sends back.
#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    /// The authorisation code, when the provider is answering with one.
    #[serde(default)]
    pub code: Option<String>,
    /// The value that ties this callback to a redirect we sent.
    #[serde(default)]
    pub state: Option<String>,
    /// Set instead of `code` when the provider refused.
    #[serde(default)]
    pub error: Option<String>,
}

/// Why a sign-in did not finish, in a form a sign-in page can translate.
#[derive(Debug, Clone, Copy)]
enum Rejection {
    /// No provider is configured, or it cannot be reached.
    Unavailable,
    /// The callback does not belong to a sign-in we started, or took too long.
    Stale,
    /// The provider refused, or its answer could not be believed.
    Refused,
    /// The provider released no address, and an account needs one.
    NoAddress,
    /// The person is not in the group this instance requires.
    NotPermitted,
    /// Registration is closed, or the invitation was not usable.
    NotInvited,
    /// An account already exists for that address and was not linked.
    AddressTaken,
    /// The account exists here but may not sign in.
    Disabled,
    /// This provider identity already belongs to somebody else.
    AlreadyLinked,
}

impl Rejection {
    /// The code the sign-in page branches on.
    const fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Stale => "stale",
            Self::Refused => "refused",
            Self::NoAddress => "no_address",
            Self::NotPermitted => "not_permitted",
            Self::NotInvited => "not_invited",
            Self::AddressTaken => "address_taken",
            Self::Disabled => "disabled",
            Self::AlreadyLinked => "already_linked",
        }
    }
}

/// Sends the browser back to the sign-in page.
fn back(query: Option<Rejection>) -> Response {
    to(SIGN_IN_PAGE, query)
}

/// Sends the browser back to a page, with a reason when there is one.
fn to(page: &str, query: Option<Rejection>) -> Response {
    let location = match query {
        Some(rejection) => format!("{page}?oidc_error={}", rejection.code()),
        None => page.to_owned(),
    };
    // 303, not 302: the browser must follow this with a GET whatever it used
    // to get here, and 302 leaves that to the implementation.
    (
        axum::http::StatusCode::SEE_OTHER,
        [(header::LOCATION, location)],
    )
        .into_response()
}

/// Whether this instance has an identity provider.
///
/// Answered whether or not one is configured, because a sign-in page has to ask
/// before it can know.
///
/// # Errors
///
/// `404` when accounts are switched off.
#[utoipa::path(
    get,
    path = "/auth/oidc",
    tag = AUTH_TAG,
    responses(
        (status = 200, description = "Whether a provider is configured", body = ProviderResponse),
        (status = 404, description = "Accounts are switched off", body = crate::error::ErrorBody),
    ),
)]
pub async fn provider(State(state): State<AppState>) -> Result<Response> {
    require_accounts(&state)?;
    let config = state.config();

    // A name is what goes on the button, so an empty one is a button with
    // nothing written on it. `auth.oidc.display_name` is left empty by default
    // because there is no good guess at what an operator calls their provider,
    // and this is the fallback for having not guessed.
    let display_name = config.auth.oidc.display_name.trim();
    Ok(axum::Json(ProviderResponse {
        enabled: config.auth.oidc.is_configured(),
        display_name: if display_name.is_empty() {
            "Single sign-on".to_owned()
        } else {
            display_name.to_owned()
        },
    })
    .into_response())
}

/// Sends the browser to the identity provider.
///
/// # Errors
///
/// `404` when accounts are switched off. Everything else is a redirect back to
/// the sign-in page with a reason, because a browser is following this.
#[utoipa::path(
    get,
    path = "/auth/oidc/start",
    tag = AUTH_TAG,
    params(("invite" = Option<String>, Query, description = "An invitation to spend on success")),
    responses(
        (status = 303, description = "Redirect to the identity provider"),
        (status = 404, description = "Accounts are switched off", body = crate::error::ErrorBody),
    ),
)]
pub async fn start(
    State(state): State<AppState>,
    Query(query): Query<StartQuery>,
) -> Result<Response> {
    require_accounts(&state)?;
    let config = state.config();
    if !config.auth.oidc.is_configured() {
        return Ok(back(Some(Rejection::Unavailable)));
    }

    let now = OffsetDateTime::now_utc();
    let Ok(provider) = state.oidc.provider(&config.auth.oidc.issuer, now).await else {
        return Ok(back(Some(Rejection::Unavailable)));
    };
    let Ok(leaving) = flow::start(&provider, &config) else {
        return Ok(back(Some(Rejection::Unavailable)));
    };

    state
        .identities
        .begin(
            &crate::db::repository::oidc::Flow {
                state: leaving.state,
                nonce: leaving.nonce,
                verifier: leaving.verifier,
                purpose: flow::Purpose::Login,
                user_id: None,
                invite_code: query.invite,
                expires_at: now + time::Duration::minutes(FLOW_TTL_MINUTES),
            },
            now,
        )
        .await?;

    Ok((
        axum::http::StatusCode::SEE_OTHER,
        [(header::LOCATION, leaving.url)],
    )
        .into_response())
}

/// Where the provider sends the browser back.
///
/// # Errors
///
/// `404` when accounts are switched off. Everything else is a redirect, see the
/// module documentation.
#[utoipa::path(
    get,
    path = "/auth/oidc/callback",
    tag = AUTH_TAG,
    params(
        ("code" = Option<String>, Query, description = "The authorisation code"),
        ("state" = Option<String>, Query, description = "The value that ties this to a redirect we sent"),
        ("error" = Option<String>, Query, description = "Set instead of `code` when the provider refused"),
    ),
    responses(
        (status = 303, description = "Redirect back to the application, signed in or with a reason"),
        (status = 404, description = "Accounts are switched off", body = crate::error::ErrorBody),
    ),
)]
pub async fn callback(
    State(state): State<AppState>,
    ClientAddress(origin): ClientAddress,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Result<Response> {
    require_accounts(&state)?;

    // The state is taken back first, whatever else the query says. A provider
    // that refused still sends one, and leaving the row behind would mean an
    // abandoned sign-in is answerable until the sweep runs.
    let Some(state_value) = query.state else {
        return Ok(back(Some(Rejection::Stale)));
    };
    let Some(waiting) = state.identities.consume(&state_value).await? else {
        return Ok(back(Some(Rejection::Stale)));
    };

    let now = OffsetDateTime::now_utc();
    if waiting.expires_at <= now {
        return Ok(back(Some(Rejection::Stale)));
    }
    if query.error.is_some() {
        return Ok(back(Some(Rejection::Refused)));
    }
    let Some(code) = query.code else {
        return Ok(back(Some(Rejection::Refused)));
    };

    // A link attempt was started by somebody already signed in, so its failures
    // belong on the page they started from. Sending them to the sign-in page
    // would read as having been signed out.
    let page = match waiting.purpose {
        flow::Purpose::Login => SIGN_IN_PAGE,
        flow::Purpose::Link => ACCOUNT_PAGE,
    };
    match finish(&state, &waiting, &code, &headers, &origin, now).await {
        Ok(response) => Ok(response),
        Err(Outcome::Rejected(rejection)) => Ok(to(page, Some(rejection))),
        Err(Outcome::Failed(error)) => Err(error),
    }
}

/// Either a reason to send the browser back, or something that went wrong here.
///
/// Kept apart because the first is the person's business and the second is
/// ours: a database that is down is a `500` with a request id, not a sentence
/// on a sign-in page saying the provider refused.
enum Outcome {
    Rejected(Rejection),
    Failed(Error),
}

impl From<Error> for Outcome {
    fn from(error: Error) -> Self {
        Self::Failed(error)
    }
}

impl From<crate::db::repository::user::UserRepositoryError> for Outcome {
    fn from(error: crate::db::repository::user::UserRepositoryError) -> Self {
        Self::Failed(Error::from(error))
    }
}

/// Everything after the callback has been matched to a sign-in we started.
async fn finish(
    state: &AppState,
    waiting: &crate::db::repository::oidc::Flow,
    code: &str,
    headers: &HeaderMap,
    origin: &crate::auth::proxy::Origin,
    now: OffsetDateTime,
) -> std::result::Result<Response, Outcome> {
    let config = state.config();
    let provider = state
        .oidc
        .provider(&config.auth.oidc.issuer, now)
        .await
        .map_err(|_ignored| Outcome::Rejected(Rejection::Unavailable))?;
    let redirect_uri = flow::redirect_uri(&config)
        .map_err(|_ignored| Outcome::Rejected(Rejection::Unavailable))?;

    let tokens = token::exchange(&provider, &config, code, &waiting.verifier, &redirect_uri)
        .await
        .map_err(|error| {
            // Worth a log line: a wrong secret and an expired code look the
            // same to whoever is signing in, and different to whoever set this
            // up.
            tracing::warn!(%error, "the token exchange failed");
            Outcome::Rejected(Rejection::Refused)
        })?;

    let mut claims = claims::verify(
        &tokens.id_token,
        &config.auth.oidc.issuer,
        config.auth.oidc.client_id.trim(),
        &waiting.nonce,
        now,
    )
    .map_err(|error| {
        tracing::warn!(%error, "the id token could not be believed");
        Outcome::Rejected(Rejection::Refused)
    })?;

    // A provider that keeps the address and the groups out of the token
    // releases them here instead. Asked for only when something is missing, so
    // the ordinary case is one round trip rather than two.
    let wants_groups = !config.auth.oidc.required_group.trim().is_empty()
        && !admit::is_in_required_group(&claims, &config.auth.oidc);
    if (claims.email.is_none() || wants_groups)
        && let Some(access_token) = tokens.access_token.as_deref()
        && let Some(extra) = token::userinfo(&provider, access_token).await
    {
        merge(&mut claims, extra);
    }

    if !admit::is_in_required_group(&claims, &config.auth.oidc) {
        return Err(Outcome::Rejected(Rejection::NotPermitted));
    }

    let existing = state
        .identities
        .find_identity(PROVIDER, &claims.sub)
        .await?;

    // A link attempt ends here. The account is already signed in, so there is
    // nothing to prove and no session to start: the only question is whether
    // this subject is free to attach.
    if waiting.purpose == flow::Purpose::Link {
        let owner = waiting.user_id.ok_or(Outcome::Rejected(Rejection::Stale))?;
        return match existing {
            // Already attached to this very account. Nothing to do, and saying
            // so as a failure would be wrong.
            Some(identity) if identity.user_id == owner => Ok(to(ACCOUNT_PAGE, None)),
            // Attached to somebody else. Refused rather than moved: two people
            // are being told they are the same one.
            Some(_taken) => Err(Outcome::Rejected(Rejection::AlreadyLinked)),
            None => {
                state
                    .identities
                    .link(PROVIDER, &claims.sub, owner, now)
                    .await?;
                Ok(to(ACCOUNT_PAGE, None))
            }
        };
    }

    let user = match existing {
        Some(identity) => state
            .users
            .find_by_id(identity.user_id)
            .await?
            .ok_or(Outcome::Rejected(Rejection::Stale))?,
        None => attach(state, &claims, waiting, now).await?,
    };

    if !user.state.may_sign_in() {
        return Err(Outcome::Rejected(Rejection::Disabled));
    }

    let cookie = start_session(state, &user, headers, origin, SignedInWith::Provider, now).await?;
    Ok((
        axum::http::StatusCode::SEE_OTHER,
        [
            (header::LOCATION, "/".to_owned()),
            (header::SET_COOKIE, cookie.to_string()),
        ],
    )
        .into_response())
}

/// Finds or creates the account a subject signs in as, and attaches the
/// identity to it.
async fn attach(
    state: &AppState,
    claims: &claims::Claims,
    waiting: &crate::db::repository::oidc::Flow,
    now: OffsetDateTime,
) -> std::result::Result<User, Outcome> {
    let config = state.config();
    let address = claims
        .email
        .as_deref()
        .and_then(|raw| EmailAddress::parse(raw).ok())
        .ok_or(Outcome::Rejected(Rejection::NoAddress))?;

    let existing = state.users.find_by_email(&address).await?;

    if let Some(found) = existing {
        // Attaching to an account that already exists is how somebody who
        // registered with a password starts using the provider without losing
        // their shares. It is also how a stranger takes over that account if
        // the provider does not check addresses, which is why both the switch
        // and the provider's own verification have to agree.
        if !config.auth.oidc.link_by_verified_email || claims.email_verified != Some(true) {
            return Err(Outcome::Rejected(Rejection::AddressTaken));
        }
        state
            .identities
            .link(PROVIDER, &claims.sub, found.id, now)
            .await?;
        return Ok(found);
    }

    register(state, claims, address, waiting, now).await
}

/// Creates an account for a subject nobody here has seen.
async fn register(
    state: &AppState,
    claims: &claims::Claims,
    email: EmailAddress,
    waiting: &crate::db::repository::oidc::Flow,
    now: OffsetDateTime,
) -> std::result::Result<User, Outcome> {
    let config = state.config();
    if config.auth.registration == crate::config::RegistrationPolicy::Closed {
        return Err(Outcome::Rejected(Rejection::NotInvited));
    }
    // The same rule as registering with a password: an instance that asks for
    // an invitation asks the provider's users for one too. Being admitted by
    // the provider is not being admitted here.
    let invite = check_invite(state, waiting.invite_code.as_deref())
        .await
        .map_err(|_ignored| Outcome::Rejected(Rejection::NotInvited))?;

    // Collisions are settled by asking, one candidate at a time, so this
    // cannot be the pure function the rest of `admit` is.
    let username = pick_username(state, claims)
        .await?
        .ok_or(Outcome::Rejected(Rejection::AddressTaken))?;

    if let Some(invite) = invite
        && !state.invites.spend(invite.id, now).await?
    {
        return Err(Outcome::Failed(invite_refused()));
    }

    let first = !state.users.any_exist().await?;
    let user = User {
        id: UserId::generate(),
        username,
        email,
        role_id: Some(seed_role(state, first).await?),
        quota_bytes: None,
        max_file_bytes: None,
        // There is no password to change.
        must_change_password: false,
        state: AccountState::Active,
        deleted_at: None,
        deleted_by: None,
        // The provider says it checked; that is the whole reason to trust it
        // enough to have got this far.
        email_verified_at: (claims.email_verified == Some(true)).then_some(now),
        created_at: now,
        updated_at: now,
    };

    // No credential row: an account that has only ever used a provider has no
    // password, and that is what "no row" means.
    state.users.insert(&user, None).await?;
    state
        .identities
        .link(PROVIDER, &claims.sub, user.id, now)
        .await?;
    Ok(user)
}

/// Asks the database whether each candidate name is free.
async fn pick_username(
    state: &AppState,
    claims: &claims::Claims,
) -> std::result::Result<Option<tessera_core::Username>, Outcome> {
    // `username_for` wants a synchronous predicate, so the names it would try
    // are collected first and looked up together.
    let mut taken = std::collections::BTreeSet::new();
    for _round in 0..8 {
        let chosen = admit::username_for(claims, |candidate| taken.contains(candidate.as_str()));
        let Some(candidate) = chosen else {
            return Ok(None);
        };
        if state.users.find_by_username(&candidate).await?.is_none() {
            return Ok(Some(candidate));
        }
        taken.insert(candidate.as_str().to_owned());
    }
    Ok(None)
}

/// Folds what the userinfo endpoint said into the claims.
///
/// The token wins where both have a value: it is the document the checks above
/// were run against, and userinfo is a second call that only fills gaps.
fn merge(claims: &mut claims::Claims, extra: serde_json::Map<String, serde_json::Value>) {
    if claims.email.is_none() {
        claims.email = extra
            .get("email")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        claims.email_verified = extra
            .get("email_verified")
            .and_then(serde_json::Value::as_bool);
    }
    if claims.preferred_username.is_none() {
        claims.preferred_username = extra
            .get("preferred_username")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
    }
    if claims.name.is_none() {
        claims.name = extra
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
    }
    for (key, value) in extra {
        claims.rest.entry(key).or_insert(value);
    }
}

/// One way an account signs in through a provider.
#[derive(Debug, Serialize, ToSchema)]
pub struct IdentityResponse {
    /// Which provider vouched for this.
    pub provider: String,
    /// When it was attached.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// Whether removing this one would leave the account with no way in.
    ///
    /// Sent rather than left for the client to work out: the client would need
    /// to know whether a password exists, which nothing else tells it.
    pub is_last_way_in: bool,
}

/// What this account signs in through.
///
/// # Errors
///
/// `401` when nothing was signed in.
#[utoipa::path(
    get,
    path = "/auth/oidc/identities",
    tag = AUTH_TAG,
    responses(
        (status = 200, description = "What is attached", body = Vec<IdentityResponse>),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn identities(
    State(state): State<AppState>,
    SelfServiceUser(current): SelfServiceUser,
) -> Result<Response> {
    require_accounts(&state)?;

    let attached = state.identities.identities_of(current.user.id).await?;
    let has_password = state.users.password_of(current.user.id).await?.is_some();
    let only_one = attached.len() == 1;

    let listed: Vec<_> = attached
        .into_iter()
        .map(|identity| IdentityResponse {
            provider: identity.provider,
            created_at: identity.created_at,
            is_last_way_in: only_one && !has_password,
        })
        .collect();
    Ok(axum::Json(listed).into_response())
}

/// Sends the browser to the provider to attach it to this account.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when authenticated with an API
/// token. Everything else is a redirect, because a browser is following this.
#[utoipa::path(
    get,
    path = "/auth/oidc/link",
    tag = AUTH_TAG,
    responses(
        (status = 303, description = "Redirect to the identity provider"),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Not permitted with an API token", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn link(
    State(state): State<AppState>,
    SelfServiceUser(current): SelfServiceUser,
) -> Result<Response> {
    require_accounts(&state)?;
    // The same rule as changing a password. Attaching a provider is a way into
    // the account, so a leaked API token must not be able to add one.
    if !current.credential.is_session() {
        return Err(Error::Forbidden);
    }

    let config = state.config();
    if !config.auth.oidc.is_configured() {
        return Ok(back(Some(Rejection::Unavailable)));
    }

    let now = OffsetDateTime::now_utc();
    let Ok(provider) = state.oidc.provider(&config.auth.oidc.issuer, now).await else {
        return Ok(back(Some(Rejection::Unavailable)));
    };
    let Ok(leaving) = flow::start(&provider, &config) else {
        return Ok(back(Some(Rejection::Unavailable)));
    };

    state
        .identities
        .begin(
            &crate::db::repository::oidc::Flow {
                state: leaving.state,
                nonce: leaving.nonce,
                verifier: leaving.verifier,
                purpose: flow::Purpose::Link,
                user_id: Some(current.user.id),
                invite_code: None,
                expires_at: now + time::Duration::minutes(FLOW_TTL_MINUTES),
            },
            now,
        )
        .await?;

    Ok((
        axum::http::StatusCode::SEE_OTHER,
        [(header::LOCATION, leaving.url)],
    )
        .into_response())
}

/// Detaches a provider from this account.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when authenticated with an API
/// token, `404` when nothing was attached, `409` when this is the only way
/// left into the account.
#[utoipa::path(
    delete,
    path = "/auth/oidc/identities/{provider}",
    tag = AUTH_TAG,
    params(("provider" = String, Path, description = "Which provider to detach")),
    responses(
        (status = 204, description = "Detached"),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Not permitted with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "Nothing was attached", body = crate::error::ErrorBody),
        (status = 409, description = "This is the only way left into the account", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn unlink(
    State(state): State<AppState>,
    SelfServiceUser(current): SelfServiceUser,
    axum::extract::Path(provider): axum::extract::Path<String>,
) -> Result<Response> {
    require_accounts(&state)?;
    if !current.credential.is_session() {
        return Err(Error::Forbidden);
    }

    let attached = state.identities.identities_of(current.user.id).await?;
    if !attached
        .iter()
        .any(|identity| identity.provider == provider)
    {
        return Err(Error::NotFound);
    }

    // An account with no password and no other identity would have no way back
    // in, and nothing above this layer would notice until somebody tried to
    // sign in. Checked here rather than in the repository because it needs the
    // credentials table as well as this one.
    //
    // The way out is `reset-password` on the machine, which creates a
    // credential row for an account that never had one.
    let has_password = state.users.password_of(current.user.id).await?.is_some();
    if attached.len() == 1 && !has_password {
        return Err(Error::Conflict(
            "that is the only way left into this account; set a password first".to_owned(),
        ));
    }

    state.identities.unlink(current.user.id, &provider).await?;
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}
