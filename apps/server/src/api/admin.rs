//! What an administrator can do to other people's accounts.
//!
//! # Why a cookie and not a token
//!
//! Every endpoint here refuses a bearer token. Granting somebody the
//! administrator flag, or deleting an account, is the largest thing this API
//! does — and a token exists so a script can upload files. One that leaked would
//! otherwise be able to make itself permanent by promoting an account it
//! controls, which is exactly the escalation the token endpoints already refuse
//! for the smaller case of minting another token.
//!
//! Automation against this surface waits for scoped tokens.
//!
//! # Why `features.admin = false` answers 404 rather than not mounting
//!
//! The contract describes the software, not one instance's configuration, so
//! `openapi.json` is the same file everywhere and a client can be built against
//! it without knowing which switches an operator threw. `/capabilities` is what
//! says which half of the software is live here.

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use tessera_core::{InviteId, Permission, UserId};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    api::{
        ADMIN_TAG,
        auth::AccountResponse,
        extract::{ApiJson, ApiPath},
        tokens::format_timestamp,
    },
    auth::extract::CurrentUser,
    db::repository::invite::Invite,
    error::{Error, Result},
    state::AppState,
};

/// Body of `PATCH /admin/users/{id}`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateUserRequest {
    /// Whether this account administers the instance.
    pub is_admin: bool,
}

/// One account, as an administrator sees it.
///
/// The same shape a person sees of themselves. There is nothing extra to show:
/// this service stores no profile, and the one thing an administrator can change
/// is already a field.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdminUserResponse(pub AccountResponse);

/// Refuses everything when the instance has no administration.
fn require_admin_feature(state: &AppState) -> Result<()> {
    if state.config.features.admin {
        Ok(())
    } else {
        Err(Error::NotFound)
    }
}

/// Whether one account holds a permission.
///
/// Asked about somebody other than the caller, which is why it is a query
/// rather than a look at [`CurrentUser::permissions`].
async fn holds(state: &AppState, user: UserId, permission: Permission) -> Result<bool> {
    Ok(state.roles.permissions_of(user).await?.allows(permission))
}

/// Refuses anything that did not arrive with a session cookie, then checks the
/// one permission this endpoint needs.
///
/// The two go together everywhere here: an administrative action is one a
/// person takes at a keyboard, and every one of them names what it needs rather
/// than sharing a single "is an administrator" answer.
fn admitted(current: &CurrentUser, permission: Permission) -> Result<()> {
    if !current.credential.is_session() {
        return Err(Error::Forbidden);
    }
    current.require(permission)
}

/// Lists every account, newest first.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the caller is not an
/// administrator or used a bearer token, `404` when this instance has no
/// administration.
#[utoipa::path(
    get,
    path = "/admin/users",
    operation_id = "list_users",
    tag = ADMIN_TAG,
    responses(
        (status = 200, description = "Every account on this instance", body = Vec<AdminUserResponse>),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Not an administrator, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no administration", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn list_users(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<AdminUserResponse>>> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::UsersRead)?;

    let users = state.users.list().await?;
    // One query for the whole page. Asking per account would make a list of
    // fifty accounts a hundred and one queries.
    let roles = state.roles.of_users(&users).await?;
    Ok(Json(
        users
            .iter()
            .map(|user| {
                let held = roles.get(&user.id).map_or(&[][..], Vec::as_slice);
                AdminUserResponse(AccountResponse::build(user, held))
            })
            .collect(),
    ))
}

/// Grants or removes the administrator flag.
///
/// # Why the last one cannot be removed
///
/// An instance with no administrator cannot be repaired through the API — there
/// is no endpoint left that would let anybody grant the flag back. The way out
/// is the command line against the database file, which an operator who locked
/// themselves out of a container may not have. So the case is refused rather
/// than allowed and regretted.
///
/// Removing your own flag while somebody else still has it is allowed: that is
/// a person stepping down, not an accident.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the caller is not an
/// administrator or used a bearer token, `404` when no account has that
/// identifier, `409` when this would leave the instance with none.
#[utoipa::path(
    patch,
    path = "/admin/users/{id}",
    operation_id = "update_user",
    tag = ADMIN_TAG,
    params(("id" = String, Path, description = "Which account to change")),
    request_body = UpdateUserRequest,
    responses(
        (status = 200, description = "The account as it now stands", body = AdminUserResponse),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Not an administrator, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "No account has that id", body = crate::error::ErrorBody),
        (status = 409, description = "That would leave the instance with no administrator", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn update_user(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<String>,
    ApiJson(body): ApiJson<UpdateUserRequest>,
) -> Result<Json<AdminUserResponse>> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::UsersWrite)?;

    let user_id = UserId::parse(&id).map_err(|_| Error::NotFound)?;
    let Some(user) = state.users.find_by_id(user_id).await? else {
        return Err(Error::NotFound);
    };

    // Counted over the permission that can restore every other one: an instance
    // where nobody holds `roles:write` cannot be given an administrator back by
    // anybody, through any screen.
    //
    // Both halves matter. Demoting somebody who was never an administrator does
    // not reduce the count, and refusing it would make the last administrator
    // unable to change anybody at all.
    let holds_the_keys = holds(&state, user.id, Permission::RolesWrite).await?;
    if holds_the_keys
        && !body.is_admin
        && state.roles.holders_of(Permission::RolesWrite).await? <= 1
    {
        return Err(Error::Conflict(
            "this is the only administrator; grant the flag to somebody else first".to_owned(),
        ));
    }

    state
        .users
        .set_admin(user_id, body.is_admin, OffsetDateTime::now_utc())
        .await?;

    let updated = state
        .users
        .find_by_id(user_id)
        .await?
        .ok_or(Error::NotFound)?;
    let roles = state.roles.of_user(updated.id).await?;
    Ok(Json(AdminUserResponse(AccountResponse::build(
        &updated, &roles,
    ))))
}

/// Removes an account.
///
/// # What goes with it and what does not
///
/// Sessions and API tokens are deleted by the database. Shares are not: their
/// owner column is `ON DELETE SET NULL`, so what somebody uploaded becomes an
/// anonymous share and keeps working for whoever holds the code. Deleting an
/// account is about the account, and taking somebody's files off the internet in
/// the same motion would be a second decision made silently.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the caller is not an
/// administrator or used a bearer token, `404` when no account has that
/// identifier, `409` when this would leave the instance with no administrator.
#[utoipa::path(
    delete,
    path = "/admin/users/{id}",
    operation_id = "delete_user",
    tag = ADMIN_TAG,
    params(("id" = String, Path, description = "Which account to remove")),
    responses(
        (status = 204, description = "Removed"),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Not an administrator, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "No account has that id", body = crate::error::ErrorBody),
        (status = 409, description = "That would leave the instance with no administrator", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn delete_user(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<String>,
) -> Result<StatusCode> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::UsersDelete)?;

    let user_id = UserId::parse(&id).map_err(|_| Error::NotFound)?;
    let Some(user) = state.users.find_by_id(user_id).await? else {
        return Err(Error::NotFound);
    };

    let holds_the_keys = holds(&state, user.id, Permission::RolesWrite).await?;
    if holds_the_keys && state.roles.holders_of(Permission::RolesWrite).await? <= 1 {
        return Err(Error::Conflict(
            "this is the only administrator; grant the flag to somebody else first".to_owned(),
        ));
    }

    if state.users.delete(user_id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(Error::NotFound)
    }
}

/// Body of `POST /admin/invites`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateInviteRequest {
    /// How many accounts this code may create.
    ///
    /// One code with several places is what the feature is for: something to
    /// send to a group rather than a code per person. `1` makes it single-use.
    #[schema(example = 3, minimum = 1)]
    pub uses: i64,
    /// How many days it lasts, or `null` for one that never expires.
    #[schema(example = 7)]
    #[serde(default)]
    pub expires_in_days: Option<u64>,
    /// What to call it, so a list of them means something.
    #[schema(example = "family")]
    #[serde(default)]
    pub note: Option<String>,
}

/// One invite code, as an administrator sees it.
///
/// Never carries the code. Only its digest is stored, so there is nothing to
/// show — see [`CreatedInviteResponse`] for the one moment there is.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InviteResponse {
    /// Stable identifier, for revoking it.
    #[schema(example = "01K1ZQK9V8QF5S3T4M2N6P7R8S")]
    pub id: String,
    /// What the operator called it.
    pub note: Option<String>,
    /// How many accounts it may create.
    pub max_uses: i64,
    /// How many it has created.
    pub used: i64,
    /// Whether it would still admit somebody.
    ///
    /// Sent rather than left to the client to work out from `used`, `max_uses`
    /// and `expires_at` against its own clock — the same reason a session says
    /// whether it is active.
    pub active: bool,
    /// When it was made.
    pub created_at: String,
    /// When it stops working, or `null` for one that does not.
    pub expires_at: Option<String>,
}

/// A freshly minted invite, with its code.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreatedInviteResponse {
    /// The invite itself.
    #[serde(flatten)]
    pub invite: InviteResponse,
    /// **Shown once.** Only its SHA-256 digest is stored, so this value cannot
    /// be read back afterwards — not by an administrator, not by anybody.
    #[schema(example = "3f9a1c…")]
    pub code: String,
}

impl InviteResponse {
    fn build(invite: &Invite, now: OffsetDateTime) -> Self {
        Self {
            id: invite.id.to_string(),
            note: invite.note.clone(),
            max_uses: invite.max_uses,
            used: invite.used,
            active: invite.is_usable_at(now),
            created_at: format_timestamp(invite.created_at),
            expires_at: invite.expires_at.map(format_timestamp),
        }
    }
}

/// Lists every invite, newest first.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the caller is not an
/// administrator or used a bearer token, `404` when this instance has no
/// administration.
#[utoipa::path(
    get,
    path = "/admin/invites",
    operation_id = "list_invites",
    tag = ADMIN_TAG,
    responses(
        (status = 200, description = "Every invite on this instance", body = Vec<InviteResponse>),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Not an administrator, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no administration", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn list_invites(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<InviteResponse>>> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::InvitesRead)?;

    let now = OffsetDateTime::now_utc();
    let invites = state.invites.list().await?;
    Ok(Json(
        invites
            .iter()
            .map(|invite| InviteResponse::build(invite, now))
            .collect(),
    ))
}

/// Mints an invite code and returns it, once.
///
/// # Why this exists when the CLI already does it
///
/// The command line is the rescue path — it works with the service stopped and
/// needs nothing but the database file. This is the ordinary path: somebody
/// letting a friend in should not have to reach a shell on the box.
///
/// # Errors
///
/// `400` when `uses` is below one or the lifetime is beyond a calendar, `401`
/// when nothing was signed in, `403` when the caller is not an administrator or
/// used a bearer token, `404` when this instance has no administration.
#[utoipa::path(
    post,
    path = "/admin/invites",
    operation_id = "create_invite",
    tag = ADMIN_TAG,
    request_body = CreateInviteRequest,
    responses(
        (status = 201, description = "The invite, with its code", body = CreatedInviteResponse),
        (status = 400, description = "A value failed its rule", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Not an administrator, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no administration", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn create_invite(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(body): ApiJson<CreateInviteRequest>,
) -> Result<(StatusCode, Json<CreatedInviteResponse>)> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::InvitesCreate)?;

    if body.uses < 1 {
        return Err(Error::bad_request("an invite has to admit at least one"));
    }

    let now = OffsetDateTime::now_utc();
    let expires_at = body
        .expires_in_days
        .map(|days| {
            i64::try_from(days)
                .map(time::Duration::days)
                .map(|span| now.saturating_add(span))
                .map_err(|_| Error::bad_request("that is more days than a calendar has"))
        })
        .transpose()?;

    let (invite, code) = state
        .invites
        .issue(body.note, body.uses, now, expires_at)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(CreatedInviteResponse {
            invite: InviteResponse::build(&invite, now),
            code: code.expose().to_owned(),
        }),
    ))
}

/// Revokes an invite.
///
/// The accounts it already admitted stay: an account created through a code does
/// not depend on the code still existing. What this stops is the code letting
/// anybody else in, which is what somebody who posted one in the wrong chat
/// window wants.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the caller is not an
/// administrator or used a bearer token, `404` when no invite has that
/// identifier.
#[utoipa::path(
    delete,
    path = "/admin/invites/{id}",
    operation_id = "revoke_invite",
    tag = ADMIN_TAG,
    params(("id" = String, Path, description = "Which invite to revoke")),
    responses(
        (status = 204, description = "It no longer admits anybody"),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Not an administrator, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "No invite has that id", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn revoke_invite(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<String>,
) -> Result<StatusCode> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::InvitesDelete)?;

    let invite_id = InviteId::parse(&id).map_err(|_| Error::NotFound)?;
    if state.invites.delete(invite_id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(Error::NotFound)
    }
}

/// What one instance is holding, at a glance.
///
/// # Why this is not a metrics endpoint
///
/// Four numbers an operator can act on, not a time series. "Am I near the
/// quota", "how many people are on here", "is the sweep keeping up" — the
/// questions somebody asks before deciding whether to change a setting. Anything
/// that wants graphs wants Prometheus, and that is a different endpoint with a
/// different shape.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StatsResponse {
    /// How many accounts exist.
    pub accounts: u64,
    /// How many of those administer the instance.
    pub administrators: u64,
    /// How many shares exist, including ones nobody can open any more.
    pub shares: u64,
    /// How many a visitor could still open.
    ///
    /// The gap between this and `shares` is what the sweep has not reached yet,
    /// which is worth seeing: a large one means retention is longer than
    /// somebody expected, or the sweep is not running.
    pub live_shares: u64,
    /// Bytes of uploaded content still on the store.
    pub stored_bytes: u64,
    /// The ceiling from `storage.max_total_bytes`, or `null` when unset.
    ///
    /// `null` is not "unlimited" so much as "nobody said" — and on an instance
    /// that takes anonymous uploads, that is the combination the start-up
    /// warning is about.
    pub quota_bytes: Option<u64>,
    /// How many invites could still admit somebody.
    pub live_invites: u64,
}

/// Reports what this instance is holding.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the caller is not an
/// administrator or used a bearer token, `404` when this instance has no
/// administration.
#[utoipa::path(
    get,
    path = "/admin/stats",
    operation_id = "instance_stats",
    tag = ADMIN_TAG,
    responses(
        (status = 200, description = "What this instance is holding", body = StatsResponse),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Not an administrator, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no administration", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn stats(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<StatsResponse>> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::StatsRead)?;

    let now = OffsetDateTime::now_utc();
    let (shares, live_shares) = state.shares.counts_at(now).await?;
    let quota = state.config.storage.max_total_bytes;

    Ok(Json(StatsResponse {
        accounts: state.users.count().await?,
        // Counted over the permission, not over `users.is_admin`. The column is
        // a mirror kept for rollback and nothing reads it to decide anything;
        // once roles can be assigned directly it stops matching, and a number on
        // a dashboard that quietly disagrees with who can actually administer
        // the instance is worse than no number.
        administrators: state.roles.holders_of(Permission::RolesWrite).await?,
        shares,
        live_shares,
        stored_bytes: state.shares.total_stored_bytes().await?,
        // Zero is how the configuration spells "no ceiling", but a client
        // drawing a usage bar against zero would show every instance as full.
        quota_bytes: (quota > 0).then_some(quota),
        live_invites: state.invites.count_usable_at(now).await?,
    }))
}
