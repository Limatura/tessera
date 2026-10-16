//! Signing in through an identity provider.
//!
//! Three endpoints, all reached by browser navigation, not by script:
//!
//! - `GET /auth/oidc` says whether there is a provider and what to call it, so a sign-in page knows
//!   whether to draw the button.
//! - `GET /auth/oidc/start` redirects to the provider.
//! - `GET /auth/oidc/callback` is where the provider sends the browser back.
//!
//! # Why failures redirect instead of answering
//!
//! The callback is a top-level navigation, so a `403` with a JSON body would
//! show raw JSON in the address bar with no way back to the app. Every
//! outcome, refusals included, redirects to the sign-in page with a stable
//! code in the query string, which the frontend turns into a sentence.
//!
//! The codes are coarse. "Your provider did not release an address" is worth
//! saying because the user can act on it. Distinguishing a wrong nonce from an
//! expired token is not, and would tell a prober which guess was closer.

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
        oauth::{Identity as OauthIdentity, Provider as OauthProvider},
        oidc::{admit, claims, flow, logout, token},
    },
    config::ProviderRegistration,
    db::repository::{
        session::{SignedInWith, StartedBy},
        user::User,
    },
    error::{Error, ErrorCode, Result},
    state::AppState,
};

/// The provider back-channel logout belongs to.
///
/// Only `OpenID` Connect has one: it is how a provider reports that a user signed
/// out, and GitHub and Discord have nothing like it. Everywhere else the
/// provider comes from the flow row or the path.
const PROVIDER: &str = OauthProvider::Oidc.as_str();

/// How long a sign-in may take before its flow row expires.
///
/// Long enough to be asked for a second factor and go find a phone; short
/// enough that abandoned rows do not pile up between sweeps.
const FLOW_TTL_MINUTES: i64 = 15;

/// Where the browser is sent when it comes back.
const SIGN_IN_PAGE: &str = "/login";

/// Where a link attempt returns to, already signed in.
///
/// The web app's account page. `tab` is omitted: the page opens on its account
/// tab by default, which lists the linked identities.
const ACCOUNT_PAGE: &str = "/user";

/// One provider a sign-in page may offer.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProviderResponse {
    /// Which one, and the segment to put in the start and link paths.
    #[schema(example = "github")]
    pub provider: String,
    /// Whether it is configured and usable.
    pub enabled: bool,
    /// What to write on the button.
    pub display_name: String,
}

/// The query a sign-in starts with.
#[derive(Debug, Deserialize)]
pub struct StartQuery {
    /// An invitation to spend once the sign-in succeeds.
    ///
    /// Kept in the flow row; the provider never sees it.
    #[serde(default)]
    pub invite: Option<String>,
    /// A path on this site to land on once signed in, such as `/user`.
    ///
    /// Anything that is not a plain path here (another origin, `//host`, a
    /// backslash) is ignored and the browser lands on the home page, so this
    /// cannot turn our own sign-in into an open redirect.
    #[serde(default)]
    pub return_to: Option<String>,
}

/// Longest `return_to` kept.
const MAX_RETURN_TO: usize = 512;

/// `value` if it is a path on this site, and nothing otherwise.
///
/// Browsers resolve a `Location` starting with `//` or `/\` against the scheme
/// alone, so those name another host; only a single slash followed by
/// something else stays on this one.
fn site_path(value: Option<String>) -> Option<String> {
    value.filter(|path| {
        path.len() <= MAX_RETURN_TO
            && path.starts_with('/')
            && !path.starts_with("//")
            && !path.contains('\\')
            && !path.chars().any(char::is_control)
    })
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
    /// This provider identity is already linked to another account.
    AlreadyLinked,
    /// Linking needs a sign-in within the last few minutes, and this session's
    /// is older. The user must sign in again, then link.
    SignInAgain,
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
            Self::SignInAgain => "sign_in_again",
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
    // 303, not 302: the browser must follow with a GET whatever method brought
    // it here, and 302 leaves that to the implementation.
    (
        axum::http::StatusCode::SEE_OTHER,
        [(header::LOCATION, location)],
    )
        .into_response()
}

/// Resolves where to send the browser for the requested provider.
///
/// OIDC endpoints come from the issuer's discovery document; the other two are
/// constants in their adapters. Both arrive here in the same form.
async fn leaving_for(
    state: &AppState,
    config: &crate::config::Config,
    provider: OauthProvider,
    now: OffsetDateTime,
) -> Option<flow::Start> {
    match provider {
        OauthProvider::Oidc => {
            let document = state
                .oidc
                .provider(&config.auth.oidc.issuer, now)
                .await
                .ok()?;
            flow::start(flow::Leaving {
                provider: provider.as_str(),
                authorize: &document.authorization_endpoint,
                client_id: config.auth.oidc.client_id.trim(),
                scopes: &config.auth.oidc.effective_scopes(),
                config,
                wants_nonce: true,
            })
            .ok()
        }
        OauthProvider::GitHub | OauthProvider::Discord => {
            let oauth = provider.oauth(config)?;
            let endpoints = provider.endpoints(config)?;
            flow::start(flow::Leaving {
                provider: provider.as_str(),
                authorize: &endpoints.authorize,
                client_id: oauth.client_id.trim(),
                scopes: &oauth.scopes,
                config,
                // No ID token to carry one. `state` is what ties the callback
                // to this sign-in, and the flow table makes it single-use.
                wants_nonce: false,
            })
            .ok()
        }
    }
}

/// Whether this instance has an identity provider.
///
/// Answers even when none is configured, since a sign-in page cannot know
/// without asking.
///
/// # Errors
///
/// `404` when accounts are switched off.
#[utoipa::path(
    get,
    path = "/auth/providers",
    tag = AUTH_TAG,
    responses(
        (status = 200, description = "Every provider, and whether it is configured", body = Vec<ProviderResponse>),
        (status = 404, description = "Accounts are switched off", body = crate::error::ErrorBody),
    ),
)]
pub async fn provider(State(state): State<AppState>) -> Result<Response> {
    require_accounts(&state)?;
    let config = state.config();

    // Every provider, including disabled ones, so an instance with none
    // configured returns three `enabled: false` entries instead of an empty list.
    let listed: Vec<_> = OauthProvider::ALL
        .into_iter()
        .map(|provider| ProviderResponse {
            provider: provider.as_str().to_owned(),
            enabled: provider.is_configured(&config),
            display_name: provider.display_name(&config),
        })
        .collect();
    Ok(axum::Json(listed).into_response())
}

/// Sends the browser to the identity provider.
///
/// # Errors
///
/// `404` when accounts are switched off. Everything else redirects back to the
/// sign-in page with a reason, since a browser is following this.
#[utoipa::path(
    get,
    path = "/auth/{provider}/start",
    tag = AUTH_TAG,
    params(
        ("provider" = String, Path, description = "`oidc`, `github` or `discord`"),
        ("invite" = Option<String>, Query, description = "An invitation to spend on success"),
        ("return_to" = Option<String>, Query, description = "A path on this site to land on once signed in, such as `/user`; anything else lands on `/`"),
    ),
    responses(
        (status = 303, description = "Redirect to the identity provider"),
        (status = 404, description = "Accounts are switched off", body = crate::error::ErrorBody),
    ),
)]
pub async fn start(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Query(query): Query<StartQuery>,
) -> Result<Response> {
    require_accounts(&state)?;
    let config = state.config();
    let Some(provider) = OauthProvider::parse(&name).filter(|it| it.is_configured(&config)) else {
        return Ok(back(Some(Rejection::Unavailable)));
    };

    let now = OffsetDateTime::now_utc();
    let Some(leaving) = leaving_for(&state, &config, provider, now).await else {
        return Ok(back(Some(Rejection::Unavailable)));
    };

    state
        .identities
        .begin(
            &crate::db::repository::oidc::Flow {
                state: leaving.state,
                provider: provider.as_str().to_owned(),
                nonce: leaving.nonce,
                verifier: leaving.verifier,
                purpose: flow::Purpose::Login,
                user_id: None,
                invite_code: query.invite,
                return_to: site_path(query.return_to),
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
/// `404` when accounts are switched off. Everything else is a redirect; see the
/// module documentation.
#[utoipa::path(
    get,
    path = "/auth/{provider}/callback",
    tag = AUTH_TAG,
    params(
        ("provider" = String, Path, description = "`oidc`, `github` or `discord`"),
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
    // Accepted so each provider can register its own redirect URI, then
    // ignored: the flow row, written at redirect time, says which provider it is.
    axum::extract::Path(_name): axum::extract::Path<String>,
    ClientAddress(origin): ClientAddress,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Result<Response> {
    require_accounts(&state)?;

    // Consume the state first, whatever else the query says. A refusing
    // provider still sends one, and a leftover row would keep an abandoned
    // sign-in answerable until the sweep.
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

    // A link attempt comes from a signed-in user, so its failures go back to
    // the page it started from. The sign-in page would look like a sign-out.
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

/// Either a reason to send the browser back, or an internal failure.
///
/// Kept apart because a database outage is a `500` with a request id, not a
/// sign-in page saying the provider refused.
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

/// What the provider handed back, and what it has to be checked against.
#[derive(Debug, Clone, Copy)]
struct ComingBack<'a> {
    /// The row the redirect wrote, holding the verifier and the nonce.
    waiting: &'a crate::db::repository::oidc::Flow,
    /// The authorisation code.
    code: &'a str,
    /// The redirect URI sent to the provider; the token endpoint checks it
    /// matches.
    redirect_uri: &'a str,
}

/// Turns an authorisation code into the identity that signed in.
///
/// The one step where the two kinds of provider differ. An `OpenID` Connect
/// exchange returns an ID token that already identifies the user; plain OAuth
/// 2.0 returns only an access token, so identity has to be fetched from a
/// provider-specific API. Both end up as an [`OauthIdentity`].
async fn identify(
    state: &AppState,
    config: &crate::config::Config,
    provider: OauthProvider,
    coming_back: ComingBack<'_>,
    now: OffsetDateTime,
) -> std::result::Result<(OauthIdentity, Option<String>), Outcome> {
    let ComingBack {
        waiting,
        code,
        redirect_uri,
    } = coming_back;
    match provider {
        OauthProvider::Oidc => {
            let document = state
                .oidc
                .provider(&config.auth.oidc.issuer, now)
                .await
                .map_err(|_ignored| Outcome::Rejected(Rejection::Unavailable))?;
            let tokens = token::exchange(&document, config, code, &waiting.verifier, redirect_uri)
                .await
                .map_err(|error| {
                    // Worth logging: a wrong secret and an expired code look
                    // the same to the user but not to the operator.
                    tracing::warn!(%error, "the token exchange failed");
                    Outcome::Rejected(Rejection::Refused)
                })?;

            // An OIDC flow always stores one. A row without it was written by
            // another version, and there is nothing to check against.
            let nonce = waiting
                .nonce
                .as_deref()
                .ok_or(Outcome::Rejected(Rejection::Stale))?;
            let mut claims = claims::verify(
                &tokens.id_token,
                &config.auth.oidc.issuer,
                config.auth.oidc.client_id.trim(),
                nonce,
                now,
            )
            .map_err(|error| {
                tracing::warn!(%error, "the id token could not be believed");
                Outcome::Rejected(Rejection::Refused)
            })?;

            // Some providers put the address, its verified flag or the groups
            // in userinfo instead of the token. Only fetched when something is
            // missing, so the usual case is one round trip.
            let wants_groups = !config.auth.oidc.required_group.trim().is_empty()
                && !admit::is_in_required_group(&claims, &config.auth.oidc);
            if (claims.email.is_none() || claims.email_verified.is_none() || wants_groups)
                && let Some(access_token) = tokens.access_token.as_deref()
                && let Some(extra) = token::userinfo(&document, access_token).await
            {
                merge(&mut claims, extra);
            }

            // The only provider with groups, so the only one this applies to.
            if !admit::is_in_required_group(&claims, &config.auth.oidc) {
                return Err(Outcome::Rejected(Rejection::NotPermitted));
            }

            let sid = claims.sid.clone();
            Ok((
                OauthIdentity {
                    subject: claims.sub,
                    email: claims.email,
                    email_verified: claims.email_verified == Some(true),
                    preferred_username: claims.preferred_username,
                    name: claims.name,
                },
                sid,
            ))
        }
        OauthProvider::GitHub | OauthProvider::Discord => {
            let (Some(oauth), Some(endpoints), Some(api)) = (
                provider.oauth(config),
                provider.endpoints(config),
                provider.api(config),
            ) else {
                return Err(Outcome::Rejected(Rejection::Unavailable));
            };
            let token = crate::auth::oauth::exchange(
                provider,
                &endpoints,
                oauth,
                code,
                &waiting.verifier,
                redirect_uri,
            )
            .await
            .map_err(|error| {
                tracing::warn!(%error, "the token exchange failed");
                Outcome::Rejected(Rejection::Refused)
            })?;

            let identity = provider
                .identify(&api, &token.access_token)
                .await
                .map_err(|error| {
                    tracing::warn!(%error, "the provider would not say who signed in");
                    Outcome::Rejected(Rejection::Refused)
                })?;
            // No session id: these providers do not report their own sessions,
            // so a back-channel logout could never match one.
            Ok((identity, None))
        }
    }
}

/// Everything after the callback has been matched to one of our sign-ins.
async fn finish(
    state: &AppState,
    waiting: &crate::db::repository::oidc::Flow,
    code: &str,
    headers: &HeaderMap,
    origin: &crate::auth::proxy::Origin,
    now: OffsetDateTime,
) -> std::result::Result<Response, Outcome> {
    let config = state.config();
    let provider =
        OauthProvider::parse(&waiting.provider).ok_or(Outcome::Rejected(Rejection::Stale))?;
    let redirect_uri = flow::redirect_uri(&config, provider.as_str())
        .map_err(|_ignored| Outcome::Rejected(Rejection::Unavailable))?;

    let (identity, session_sid) = identify(
        state,
        &config,
        provider,
        ComingBack {
            waiting,
            code,
            redirect_uri: &redirect_uri,
        },
        now,
    )
    .await?;

    let existing = state
        .identities
        .find_identity(provider.as_str(), &identity.subject)
        .await?;

    // A link attempt ends here. The account is already signed in, so there is
    // no session to start; the only question is whether this subject is free.
    if waiting.purpose == flow::Purpose::Link {
        let owner = waiting.user_id.ok_or(Outcome::Rejected(Rejection::Stale))?;
        return match existing {
            // Already linked to this account. Nothing to do, and not a failure.
            Some(attached) if attached.user_id == owner => Ok(to(ACCOUNT_PAGE, None)),
            // Linked to another account. Refused, not moved: that would merge
            // two people into one.
            Some(_taken) => Err(Outcome::Rejected(Rejection::AlreadyLinked)),
            None => {
                state
                    .identities
                    .link(provider.as_str(), &identity.subject, owner, now)
                    .await?;
                Ok(to(ACCOUNT_PAGE, None))
            }
        };
    }

    let user = match existing {
        Some(attached) => state
            .users
            .find_by_id(attached.user_id)
            .await?
            .ok_or(Outcome::Rejected(Rejection::Stale))?,
        None => attach(state, provider, &identity, waiting, now).await?,
    };

    if !user.state.may_sign_in() {
        return Err(Outcome::Rejected(Rejection::Disabled));
    }

    let cookie = start_session(
        state,
        &user,
        headers,
        origin,
        StartedBy {
            credential: SignedInWith::Provider,
            provider: provider.as_str(),
            provider_sid: session_sid.as_deref(),
        },
        now,
    )
    .await?;
    Ok((
        axum::http::StatusCode::SEE_OTHER,
        [
            (
                header::LOCATION,
                waiting.return_to.clone().unwrap_or_else(|| "/".to_owned()),
            ),
            (header::SET_COOKIE, cookie.to_string()),
        ],
    )
        .into_response())
}

/// Finds or creates the account a subject signs in as, and attaches the
/// identity to it.
async fn attach(
    state: &AppState,
    provider: OauthProvider,
    identity: &OauthIdentity,
    waiting: &crate::db::repository::oidc::Flow,
    now: OffsetDateTime,
) -> std::result::Result<User, Outcome> {
    let config = state.config();
    let address = identity
        .email
        .as_deref()
        .and_then(|raw| EmailAddress::parse(raw).ok())
        .ok_or(Outcome::Rejected(Rejection::NoAddress))?;

    let existing = state.users.find_by_email(&address).await?;

    if let Some(found) = existing {
        // Linking to an existing account lets a password user start using the
        // provider without losing their shares. It is also how a stranger
        // takes over that account if the provider does not verify addresses,
        // hence the per-provider trust setting.
        if !provider.may_link(&config, identity.email_verified) {
            return Err(Outcome::Rejected(Rejection::AddressTaken));
        }
        state
            .identities
            .link(provider.as_str(), &identity.subject, found.id, now)
            .await?;
        return Ok(found);
    }

    register(state, provider, identity, address, waiting, now).await
}

/// Creates an account for a subject not seen here before.
async fn register(
    state: &AppState,
    provider: OauthProvider,
    identity: &OauthIdentity,
    email: EmailAddress,
    waiting: &crate::db::repository::oidc::Flow,
    now: OffsetDateTime,
) -> std::result::Result<User, Outcome> {
    let config = state.config();
    let invite = match provider.registration(&config) {
        ProviderRegistration::Never => return Err(Outcome::Rejected(Rejection::NotInvited)),
        // Admission by the provider is the point of setting this per provider,
        // so no invitation is asked for.
        ProviderRegistration::Always => None,
        ProviderRegistration::Inherit => {
            if config.auth.registration == crate::config::RegistrationPolicy::Closed {
                return Err(Outcome::Rejected(Rejection::NotInvited));
            }
            // Same rule as password registration: an instance that requires
            // invitations requires them from provider users too.
            check_invite(state, waiting.invite_code.as_deref())
                .await
                .map_err(|_ignored| Outcome::Rejected(Rejection::NotInvited))?
        }
    };

    // Collisions are resolved by querying one candidate at a time, so this
    // cannot be a pure function like the rest of `admit`.
    let username = pick_username(state, identity)
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
        // The provider says it verified the address, which is why we trust it.
        email_verified_at: identity.email_verified.then_some(now),
        created_at: now,
        updated_at: now,
    };

    // No credential row: a provider-only account has no password, and that is
    // what a missing row means.
    state.users.insert(&user, None).await?;
    state
        .identities
        .link(provider.as_str(), &identity.subject, user.id, now)
        .await?;
    Ok(user)
}

/// Asks the database whether each candidate name is free.
async fn pick_username(
    state: &AppState,
    identity: &OauthIdentity,
) -> std::result::Result<Option<tessera_core::Username>, Outcome> {
    // `username_for` takes a synchronous predicate, so the candidate names are
    // collected first and looked up together.
    let mut taken = std::collections::BTreeSet::new();
    for _round in 0..8 {
        let chosen = admit::username_for(identity, |candidate| taken.contains(candidate.as_str()));
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

/// Folds the userinfo response into the claims.
///
/// The token wins where both have a value: the checks above ran against it,
/// and userinfo only fills gaps.
fn merge(claims: &mut claims::Claims, extra: serde_json::Map<String, serde_json::Value>) {
    if claims.email.is_none() {
        claims.email = extra
            .get("email")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
    }
    // Filled independently of the address. A provider may put the address in
    // the token and the verified flag only in userinfo; reading one without
    // the other left accounts unverified for good.
    //
    // Only when userinfo refers to the same address: a verdict on another
    // address says nothing about this one.
    let same_address = extra
        .get("email")
        .and_then(serde_json::Value::as_str)
        .is_none_or(|address| Some(address) == claims.email.as_deref());
    if claims.email_verified.is_none() && same_address {
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
    /// The provider that vouched for this.
    pub provider: String,
    /// When it was attached.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// Whether removing this one would leave the account with no way in.
    ///
    /// Computed here because the client cannot tell whether a password exists.
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
/// token. Everything else is a redirect, since a browser is following this.
#[utoipa::path(
    get,
    path = "/auth/{provider}/link",
    tag = AUTH_TAG,
    params(("provider" = String, Path, description = "`oidc`, `github` or `discord`")),
    responses(
        (status = 303, description = "Redirect to the identity provider"),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Not permitted with an API token", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn link(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    SelfServiceUser(current): SelfServiceUser,
) -> Result<Response> {
    require_accounts(&state)?;
    // Same rule as changing a password: linking a provider opens a way into
    // the account, so a leaked API token must not be able to do it.
    if !current.credential.is_session() {
        return Err(Error::new(ErrorCode::SessionRequired));
    }
    // A linked provider is a way in that outlasts every session and survives a
    // password change. Without this check, a stolen session cookie could link
    // the thief's own GitHub and sign in with it indefinitely, or sign in once
    // to look freshly authenticated and then delete the account. A browser
    // follows this link, so there is no password to ask for; a recent sign-in
    // is the proof, as for an account without a password everywhere else.
    let since = OffsetDateTime::now_utc() - crate::api::auth::REAUTHENTICATION_WINDOW;
    if !current.credential.signed_in_since(since) {
        return Ok(to(ACCOUNT_PAGE, Some(Rejection::SignInAgain)));
    }

    let config = state.config();
    let Some(provider) = OauthProvider::parse(&name).filter(|it| it.is_configured(&config)) else {
        return Ok(to(ACCOUNT_PAGE, Some(Rejection::Unavailable)));
    };

    let now = OffsetDateTime::now_utc();
    let Some(leaving) = leaving_for(&state, &config, provider, now).await else {
        return Ok(to(ACCOUNT_PAGE, Some(Rejection::Unavailable)));
    };

    state
        .identities
        .begin(
            &crate::db::repository::oidc::Flow {
                state: leaving.state,
                provider: provider.as_str().to_owned(),
                nonce: leaving.nonce,
                verifier: leaving.verifier,
                purpose: flow::Purpose::Link,
                user_id: Some(current.user.id),
                invite_code: None,
                return_to: None,
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
        return Err(Error::new(ErrorCode::SessionRequired));
    }

    let attached = state.identities.identities_of(current.user.id).await?;
    if !attached
        .iter()
        .any(|identity| identity.provider == provider)
    {
        return Err(Error::NotFound);
    }

    // An account with no password, no other identity and no passkey would be locked out,
    // unnoticed until the next sign-in attempt. Checked here, not in the
    // repository, because it needs the credentials table too.
    //
    // The recovery path is `reset-password` on the server, which creates a
    // credential row for an account that never had one.
    let has_password = state.users.password_of(current.user.id).await?.is_some();
    if attached.len() == 1 && !has_password && state.passkeys.count(current.user.id).await? == 0 {
        return Err(Error::new(ErrorCode::LastSignInMethod));
    }

    state.identities.unlink(current.user.id, &provider).await?;
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

/// Reads `logout_token` from a form-encoded body.
///
/// Not `Form<T>`, which answers a missing field with its own `422`. The caller
/// is a provider that expects `{"error", "error_description"}` or nothing, and
/// would keep retrying on a `422` saying `Failed to deserialize form body`.
fn logout_token_in(body: &str) -> Option<String> {
    form_urlencoded::parse(body.as_bytes())
        .find(|(key, _value)| key == "logout_token")
        .map(|(_key, value)| value.into_owned())
        .filter(|token| !token.trim().is_empty())
}

/// Ends the sessions a provider says are over.
///
/// # Why this answers in the OAuth error shape
///
/// Every other endpoint answers `{"code": ..., "message": ...}`. This one
/// answers `{"error": ..., "error_description": ...}` because the caller is an
/// identity provider following `OpenID` Connect Back-Channel Logout 1.0, whose
/// section 2.8 defines the refusal format. A provider that cannot parse the
/// refusal keeps retrying a token that will never work.
///
/// # Why it is unauthenticated
///
/// The caller is a server with no account here. The token's signature, checked
/// against keys from the configured issuer, stands in for a credential. See
/// `auth::oidc::logout`.
///
/// # Errors
///
/// `404` when accounts are switched off. An untrustworthy token is a `400`
/// with the reason, never a `500`.
#[utoipa::path(
    post,
    path = "/auth/oidc/backchannel-logout",
    tag = AUTH_TAG,
    request_body(content = String, content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 200, description = "The sessions it named are over"),
        (status = 400, description = "The token could not be believed, or the body carried none"),
        (status = 404, description = "Accounts are switched off", body = crate::error::ErrorBody),
    ),
)]
pub async fn backchannel_logout(State(state): State<AppState>, body: String) -> Response {
    if !state.config().features.accounts {
        return Error::NotFound.into_response();
    }
    let Some(token) = logout_token_in(&body) else {
        return refused("the body carries no `logout_token`");
    };
    match end_sessions(&state, &token).await {
        Ok(()) => (
            axum::http::StatusCode::OK,
            [(header::CACHE_CONTROL, "no-store")],
        )
            .into_response(),
        Err(reason) => refused(&reason),
    }
}

/// The refusal format a provider reads, per section 2.8.
fn refused(reason: &str) -> Response {
    // Logged: a provider unable to log users out is a silent failure of what
    // single sign-on is for, and the operator needs to see why.
    tracing::warn!(%reason, "refused a back-channel logout");
    (
        axum::http::StatusCode::BAD_REQUEST,
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(serde_json::json!({
            "error": "invalid_request",
            "error_description": reason,
        })),
    )
        .into_response()
}

/// Verifies a logout token and ends what it names.
///
/// The error is a plain string: every failure leaves as the same `400`, and the
/// provider reads it as prose.
async fn end_sessions(state: &AppState, token: &str) -> std::result::Result<(), String> {
    let config = state.config();
    if !config.auth.oidc.is_configured() {
        return Err("no identity provider is configured".to_owned());
    }
    let now = OffsetDateTime::now_utc();

    let provider = state
        .oidc
        .provider(&config.auth.oidc.issuer, now)
        .await
        .map_err(|error| error.to_string())?;
    let kid = logout::key_id(token).map_err(|error| error.to_string())?;
    let key = state
        .oidc_keys
        .key_for(provider.jwks_uri.as_deref(), &kid, now)
        .await
        .map_err(|error| error.to_string())?;

    let claims = logout::verify(
        token,
        &key,
        &config.auth.oidc.issuer,
        config.auth.oidc.client_id.trim(),
        now,
    )
    .map_err(|error| error.to_string())?;

    let issued_at = OffsetDateTime::from_unix_timestamp(claims.iat)
        .map_err(|_ignored| "the logout token's `iat` is not a time".to_owned())?;

    // `sid` names one session; `sub` names all of that user's sessions. When a
    // token carries both, the narrower one is meant.
    let ended = if let Some(sid) = claims.sid.as_deref().filter(|sid| !sid.trim().is_empty()) {
        state
            .sessions
            .end_provider_session(PROVIDER, sid, issued_at, now)
            .await
            .map_err(|error| error.to_string())?
    } else {
        let subject = claims.sub.as_deref().unwrap_or_default();
        let Some(identity) = state
            .identities
            .find_identity(PROVIDER, subject)
            .await
            .map_err(|error| error.to_string())?
        else {
            // No account has ever signed in as that subject. Reported as
            // success: the requested state already holds, and a refusal would
            // make the provider retry forever.
            return Ok(());
        };
        state
            .sessions
            .end_provider_sessions(identity.user_id, PROVIDER, issued_at, now)
            .await
            .map_err(|error| error.to_string())?
    };

    tracing::info!(ended, "a provider signed somebody out");
    Ok(())
}
