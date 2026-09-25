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
        oauth::{Identity as OauthIdentity, Provider as OauthProvider},
        oidc::{admit, claims, flow, logout, token},
    },
    config::ProviderRegistration,
    db::repository::{
        session::{SignedInWith, StartedBy},
        user::User,
    },
    error::{Error, Result},
    state::AppState,
};

/// The provider's name in `identities.provider`.
///
/// One provider today. The column exists so a second one is configuration
/// rather than a migration, and this is the placeholder that becomes a
/// The provider back-channel logout belongs to.
///
/// Only `OpenID` Connect has one: it is the mechanism by which a provider tells
/// us somebody signed out, and neither GitHub nor Discord has anything like
/// it. Everywhere else the provider comes from the flow row or the path.
const PROVIDER: &str = OauthProvider::Oidc.as_str();

/// How long a sign-in may take before the flow row stops being answerable.
///
/// Long enough for somebody to be asked for a second factor and go and find
/// their phone. Not so long that abandoned rows pile up between sweeps.
const FLOW_TTL_MINUTES: i64 = 15;

/// Where the browser is sent when it comes back.
const SIGN_IN_PAGE: &str = "/login";

/// Where a link attempt is sent when it comes back, signed in already.
const ACCOUNT_PAGE: &str = "/account";

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

/// Resolves where to send the browser, for whichever provider was asked for.
///
/// OIDC's endpoints come out of the document its issuer publishes; the other
/// two are constants in their adapters. Both arrive here as the same thing.
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
/// Answered whether or not one is configured, because a sign-in page has to ask
/// before it can know.
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

    // Every provider, including the ones that are off. A sign-in page has to
    // ask before it can know, and an instance that configures none gets a list
    // of three `enabled: false` rather than an empty one it has to interpret.
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
/// `404` when accounts are switched off. Everything else is a redirect back to
/// the sign-in page with a reason, because a browser is following this.
#[utoipa::path(
    get,
    path = "/auth/{provider}/start",
    tag = AUTH_TAG,
    params(
        ("provider" = String, Path, description = "`oidc`, `github` or `discord`"),
        ("invite" = Option<String>, Query, description = "An invitation to spend on success"),
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
    // Accepted so each provider has its own registered redirect URI, and then
    // ignored: the flow row says which provider this belongs to, and that was
    // written by the redirect rather than by whoever is calling now.
    axum::extract::Path(_name): axum::extract::Path<String>,
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

/// What the provider handed back, and what it has to be checked against.
#[derive(Debug, Clone, Copy)]
struct ComingBack<'a> {
    /// The row the redirect wrote, holding the verifier and the nonce.
    waiting: &'a crate::db::repository::oidc::Flow,
    /// The authorisation code.
    code: &'a str,
    /// The address the provider was told to come back to, which the token
    /// endpoint checks against what it saw.
    redirect_uri: &'a str,
}

/// Turns an authorisation code into who just signed in.
///
/// The one step where the two kinds of provider differ. An `OpenID` Connect
/// exchange comes back with an ID token that already says who this is; a plain
/// OAuth 2.0 one comes back with an access token and nothing else, so the
/// answer has to be fetched from an API whose shape belongs to that provider.
///
/// Both leave as an [`OauthIdentity`], and nothing after this knows which it
/// was.
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
                    // Worth a log line: a wrong secret and an expired code
                    // look the same to whoever is signing in, and different
                    // to whoever set this up.
                    tracing::warn!(%error, "the token exchange failed");
                    Outcome::Rejected(Rejection::Refused)
                })?;

            // An OIDC flow always stored one. A row without it is one this
            // version did not write, and there is nothing to check against.
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

            // A provider that keeps the address, whether it checked it, or the
            // groups out of the token releases them here instead. Asked for
            // only when something is missing, so the ordinary case is one
            // round trip rather than two.
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
            // No session identifier: neither of these tells us about its own
            // session, so a back-channel logout naming one could never match.
            Ok((identity, None))
        }
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
    // nothing to prove and no session to start: the only question is whether
    // this subject is free to attach.
    if waiting.purpose == flow::Purpose::Link {
        let owner = waiting.user_id.ok_or(Outcome::Rejected(Rejection::Stale))?;
        return match existing {
            // Already attached to this very account. Nothing to do, and saying
            // so as a failure would be wrong.
            Some(attached) if attached.user_id == owner => Ok(to(ACCOUNT_PAGE, None)),
            // Attached to somebody else. Refused rather than moved: two people
            // are being told they are the same one.
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
        // Attaching to an account that already exists is how somebody who
        // registered with a password starts using the provider without losing
        // their shares. It is also how a stranger takes over that account if
        // the provider does not check addresses, which is why the operator
        // says how far this provider's addresses are to be trusted.
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

/// Creates an account for a subject nobody here has seen.
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
        // The provider letting somebody through is what admits them: that is
        // the whole point of saying so per provider, so there is no invitation
        // to ask for.
        ProviderRegistration::Always => None,
        ProviderRegistration::Inherit => {
            if config.auth.registration == crate::config::RegistrationPolicy::Closed {
                return Err(Outcome::Rejected(Rejection::NotInvited));
            }
            // The same rule as registering with a password: an instance that
            // asks for an invitation asks the provider's users for one too.
            // Being admitted by the provider is not being admitted here.
            check_invite(state, waiting.invite_code.as_deref())
                .await
                .map_err(|_ignored| Outcome::Rejected(Rejection::NotInvited))?
        }
    };

    // Collisions are settled by asking, one candidate at a time, so this
    // cannot be the pure function the rest of `admit` is.
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
        // The provider says it checked; that is the whole reason to trust it
        // enough to have got this far.
        email_verified_at: identity.email_verified.then_some(now),
        created_at: now,
        updated_at: now,
    };

    // No credential row: an account that has only ever used a provider has no
    // password, and that is what "no row" means.
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
    // `username_for` wants a synchronous predicate, so the names it would try
    // are collected first and looked up together.
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
    }
    // Filled on its own and not only with the address. A provider may put the
    // address in the token and say whether it checked it only in userinfo,
    // and reading the one without the other left it unverified for ever.
    //
    // Only when userinfo is talking about the same address: a verdict on some
    // other address says nothing about this one.
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
    // The same rule as changing a password. Attaching a provider is a way into
    // the account, so a leaked API token must not be able to add one.
    if !current.credential.is_session() {
        return Err(Error::Forbidden);
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

/// Reads `logout_token` out of a form-encoded body.
///
/// Parsed here rather than with `Form<T>`, which answers its own `422` in its
/// own words when a field is missing. This endpoint answers a provider, and a
/// provider reads `{"error", "error_description"}` or nothing; a `422` carrying
/// `Failed to deserialize form body` is neither documented nor parseable by
/// one, and it would keep retrying.
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
/// Every other endpoint here answers `{"code": ..., "message": ...}`. This one
/// answers `{"error": ..., "error_description": ...}`, because the caller is
/// not a client of this API. It is an identity provider following `OpenID`
/// Connect Back-Channel Logout 1.0, and section 2.8 says what it expects to
/// read when it is refused. A provider that cannot parse the refusal will keep
/// retrying a token that will never work.
///
/// # Why it is unauthenticated
///
/// The caller is a server with no account here. What stands in for a credential
/// is the token's signature, checked against keys fetched from the configured
/// issuer. See `auth::oidc::logout`.
///
/// # Errors
///
/// `404` when accounts are switched off. A token that cannot be believed is a
/// `400` with the reason, never a `500`.
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

/// The refusal shape section 2.8 says a provider reads.
fn refused(reason: &str) -> Response {
    // Logged, because a provider that cannot log anybody out is a silent
    // failure of the thing single sign-on is for, and the operator has to be
    // able to see why.
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
/// The error is a string rather than a type because every one of them leaves
/// here as the same `400` with the same shape, and the provider reads it as
/// prose.
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

    // A `sid` names one session; a `sub` names everything that person has open.
    // A token may carry both, and then the narrower one is what was meant.
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
            // Nobody here has ever signed in as that subject. Reported as
            // success: the provider asked for a state that already holds, and
            // a refusal would have it retry something that cannot change.
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
