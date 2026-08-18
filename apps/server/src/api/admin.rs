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

// `params(SharePageParams)` makes utoipa emit a fully-qualified path to the
// type. The generated code lands outside the function it decorates, so the
// suppression has to be module-wide; dropping `params(..)` instead would take
// the paging parameters out of the contract, which is the point of declaring
// them. Same arrangement as `super::shares`.
#![expect(
    unused_qualifications,
    reason = "emitted by utoipa's params(..), not written here"
)]

use axum::{Json, extract::State, http::StatusCode};
use object_store::{ObjectStoreExt as _, path::Path as ObjectPath};
use serde::{Deserialize, Serialize};
use tessera_core::{AccountState, InviteId, Permission, PermissionSet, RoleId, ShareId, UserId};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};

use crate::{
    api::{
        ADMIN_TAG,
        auth::AccountResponse,
        extract::{ApiJson, ApiPath, ApiQuery},
        tokens::format_timestamp,
    },
    auth::extract::CurrentUser,
    db::repository::{
        invite::Invite,
        role::{Role, RoleChange},
        session::SessionEnding,
        share::{Availability, Content, Share},
    },
    error::{Error, Result},
    state::AppState,
};

/// Body of `PATCH /admin/users/{id}`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateUserRequest {
    /// Whether this account administers the instance.
    pub is_admin: bool,
    /// `active` to let it in, `disabled` to suspend it.
    ///
    /// Omitted leaves it as it is. `deleted` is refused here — removing an
    /// account scrubs it, and that is `DELETE` rather than a field on a patch.
    #[serde(default)]
    pub state: Option<String>,
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
    params(UserListParams),
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
    ApiQuery(params): ApiQuery<UserListParams>,
) -> Result<Json<Vec<AdminUserResponse>>> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::UsersRead)?;

    let users = state.users.list(params.include_deleted).await?;
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

    let now = OffsetDateTime::now_utc();
    if let Some(wanted) = body.state.as_deref() {
        let wanted: AccountState = wanted
            .parse()
            .map_err(|_| Error::bad_request("`state` must be `active` or `disabled`"))?;
        if wanted == AccountState::Deleted {
            return Err(Error::bad_request(
                "removing an account scrubs it; use DELETE rather than this field",
            ));
        }
        // Suspending somebody who holds the keys is the same lock-out as
        // demoting them, reached by another door.
        if wanted != AccountState::Active
            && holds_the_keys
            && state.roles.holders_of(Permission::RolesWrite).await? <= 1
        {
            return Err(Error::Conflict(
                "this is the only administrator; grant the flag to somebody else first".to_owned(),
            ));
        }
        state.users.set_state(user_id, wanted, now).await?;
        if wanted != AccountState::Active {
            // Authentication already refuses a suspended account, so this is
            // not what stops them — it is what makes the session list they see
            // afterwards true.
            state
                .sessions
                .end_all_for_user(user_id, None, SessionEnding::AccountDisabled, now)
                .await?;
        }
    }

    state.users.set_admin(user_id, body.is_admin, now).await?;

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

    // Already gone answers the same as gone: the caller asked for the account
    // to stop existing, and it does not.
    if user.state == AccountState::Deleted {
        return Ok(StatusCode::NO_CONTENT);
    }

    let now = OffsetDateTime::now_utc();
    if !state
        .users
        .soft_delete(user_id, current.user.id, now)
        .await?
    {
        return Err(Error::NotFound);
    }

    // The login history is the most identifying thing this service holds about
    // a person — where they signed in from and on what. Removing the account
    // without removing that would leave the part that matters most.
    state.sessions.delete_all_for_user(user_id).await?;
    state.tokens.delete_all_for_user(user_id).await?;

    tracing::info!(account = %user_id, by = %current.user.id, "an account was removed");
    Ok(StatusCode::NO_CONTENT)
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

/// One role, as the API describes it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RoleResponse {
    /// Stable identifier.
    #[schema(example = "01K1ZQK9V8QF5S3T4M2N6P7R8S")]
    pub id: String,
    /// What an operator types and reads.
    #[schema(example = "auditor")]
    pub name: String,
    /// What it is for, in the operator's own words.
    pub description: Option<String>,
    /// Whether it is one of the two the instance was seeded with.
    ///
    /// Those cannot be deleted — an instance whose administrator role has been
    /// removed has nobody who can create one. They can still be edited.
    pub is_system: bool,
    /// What it allows, as `resource:verb` strings.
    #[schema(example = json!(["users:read"]))]
    pub permissions: Vec<String>,
    /// How many accounts are in it.
    ///
    /// The number an operator wants before editing one: changing a role that
    /// nobody holds is free, and changing one that forty people hold is not.
    pub members: u64,
    /// When it was created, RFC 3339.
    pub created_at: String,
    /// When its permissions last changed, RFC 3339.
    pub updated_at: String,
}

impl RoleResponse {
    fn build(role: &Role, members: u64) -> Self {
        Self {
            id: role.id.to_string(),
            name: role.name.clone(),
            description: role.description.clone(),
            is_system: role.is_system,
            permissions: role
                .permissions
                .iter()
                .map(|permission| permission.as_str().to_owned())
                .collect(),
            members,
            created_at: format_timestamp(role.created_at),
            updated_at: format_timestamp(role.updated_at),
        }
    }
}

/// Body of `POST /admin/roles` and `PUT /admin/roles/{id}`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RoleRequest {
    /// What to call it.
    pub name: String,
    /// What it is for.
    #[serde(default)]
    pub description: Option<String>,
    /// Everything it should allow, as `resource:verb` strings.
    ///
    /// The whole set, not a change to it. Two operators editing the same role
    /// from two screens cannot interleave their edits into a set neither of
    /// them chose — the last one to press save wins, visibly.
    pub permissions: Vec<String>,
}

impl RoleRequest {
    /// Reads the permission names, refusing any this version does not have.
    ///
    /// Refused rather than ignored: an operator who ticks something and is told
    /// it was saved should get what they ticked. Silently dropping one produces
    /// a role that does less than the screen says it does.
    fn permissions(&self) -> Result<PermissionSet> {
        self.permissions
            .iter()
            .map(|name| {
                name.parse::<Permission>().map_err(|_| {
                    Error::bad_request(format!("`{name}` is not a permission this version has"))
                })
            })
            .collect()
    }

    fn name(&self) -> Result<String> {
        let name = self.name.trim();
        if name.is_empty() || name.len() > ROLE_NAME_LIMIT {
            return Err(Error::bad_request(format!(
                "a role name must be between 1 and {ROLE_NAME_LIMIT} characters"
            )));
        }
        Ok(name.to_owned())
    }
}

/// The longest a role name may be.
///
/// Matches the `VARCHAR(191)` the MySQL migration uses — a name that fits on
/// two of the three dialects and is truncated on the third is worse than one
/// that is refused everywhere.
const ROLE_NAME_LIMIT: usize = 191;

/// Body of `PUT /admin/users/{id}/roles`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UserRolesRequest {
    /// The roles this account should be in. The whole list, not a change to it.
    pub roles: Vec<String>,
}

/// Every permission this version has, so a screen can offer them.
///
/// Sent rather than hard-coded in the client: a client listing permissions it
/// was built against would silently stop offering ones added since, and the
/// operator would have no way to grant them.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PermissionResponse {
    /// The `resource:verb` string.
    #[schema(example = "users:read")]
    pub name: String,
    /// Whether it reaches beyond the holder's own things.
    ///
    /// The question an operator is actually asking of a role they are about to
    /// grant: does this let them touch other people?
    pub privileged: bool,
}

/// Lists every role, by name.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the caller may not read roles
/// or used a bearer token, `404` when this instance has no administration.
#[utoipa::path(
    get,
    path = "/admin/roles",
    operation_id = "list_roles",
    tag = ADMIN_TAG,
    responses(
        (status = 200, description = "Every role on this instance", body = Vec<RoleResponse>),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not read roles, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no administration", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn list_roles(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<RoleResponse>>> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::RolesRead)?;

    let roles = state.roles.list().await?;
    let members = state.roles.member_counts().await?;
    Ok(Json(
        roles
            .iter()
            .map(|role| RoleResponse::build(role, members.get(&role.id).copied().unwrap_or(0)))
            .collect(),
    ))
}

/// Lists every permission this version has.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the caller may not read roles
/// or used a bearer token, `404` when this instance has no administration.
#[utoipa::path(
    get,
    path = "/admin/permissions",
    operation_id = "list_permissions",
    tag = ADMIN_TAG,
    responses(
        (status = 200, description = "Every permission this version has", body = Vec<PermissionResponse>),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not read roles, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no administration", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn list_permissions(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<PermissionResponse>>> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::RolesRead)?;

    Ok(Json(
        Permission::ALL
            .into_iter()
            .map(|permission| PermissionResponse {
                name: permission.as_str().to_owned(),
                privileged: permission.is_privileged(),
            })
            .collect(),
    ))
}

/// Creates a role.
///
/// # Why a caller cannot grant what they do not hold
///
/// Otherwise `roles:write` is every permission: somebody holding only that
/// could mint a role granting `users:delete`, put themselves in it, and have
/// it. The check makes `roles:write` mean "may hand out what I have" rather
/// than "may hand out anything".
///
/// # Errors
///
/// `400` when the name or a permission is not usable, `401` when nothing was
/// signed in, `403` when the caller may not write roles, used a bearer token,
/// or asked for a permission they do not hold, `409` when the name is taken,
/// `404` when this instance has no administration.
#[utoipa::path(
    post,
    path = "/admin/roles",
    operation_id = "create_role",
    tag = ADMIN_TAG,
    request_body = RoleRequest,
    responses(
        (status = 201, description = "The role as it now stands", body = RoleResponse),
        (status = 400, description = "The name or a permission is not usable", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not write roles, used an API token, or asked for a permission not held", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no administration", body = crate::error::ErrorBody),
        (status = 409, description = "A role already has that name", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn create_role(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(body): ApiJson<RoleRequest>,
) -> Result<(StatusCode, Json<RoleResponse>)> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::RolesWrite)?;

    let name = body.name()?;
    let permissions = body.permissions()?;
    refuse_escalation(&current, &permissions)?;

    if state.roles.find_by_name(&name).await?.is_some() {
        return Err(Error::Conflict("a role already has that name".to_owned()));
    }
    let role = state
        .roles
        .create(
            name,
            body.description.clone(),
            &permissions,
            OffsetDateTime::now_utc(),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(RoleResponse::build(&role, 0))))
}

/// Replaces what a role allows.
///
/// # Errors
///
/// `400` when the name or a permission is not usable, `401` when nothing was
/// signed in, `403` when the caller may not write roles, used a bearer token,
/// or asked for a permission they do not hold, `404` when no role has that
/// identifier, `409` when the name is taken or the change would leave nobody
/// able to administer the instance.
#[utoipa::path(
    put,
    path = "/admin/roles/{id}",
    operation_id = "update_role",
    tag = ADMIN_TAG,
    params(("id" = String, Path, description = "Which role")),
    request_body = RoleRequest,
    responses(
        (status = 200, description = "The role as it now stands", body = RoleResponse),
        (status = 400, description = "The name or a permission is not usable", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not write roles, used an API token, or asked for a permission not held", body = crate::error::ErrorBody),
        (status = 404, description = "No such role", body = crate::error::ErrorBody),
        (status = 409, description = "The name is taken, or nobody would be left to administer the instance", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn update_role(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<String>,
    ApiJson(body): ApiJson<RoleRequest>,
) -> Result<Json<RoleResponse>> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::RolesWrite)?;

    let role_id = RoleId::parse(&id).map_err(|_| Error::NotFound)?;
    let name = body.name()?;
    let permissions = body.permissions()?;
    refuse_escalation(&current, &permissions)?;

    let Some(existing) = state.roles.find_by_id(role_id).await? else {
        return Err(Error::NotFound);
    };
    if let Some(clash) = state.roles.find_by_name(&name).await?
        && clash.id != role_id
    {
        return Err(Error::Conflict("a role already has that name".to_owned()));
    }

    // A built-in role means the same thing on every instance, and that is the
    // whole of its value: documentation, support conversations and the
    // `auth.default_role` setting all name one expecting it to mean what it
    // means everywhere else.
    //
    // The name is not part of that — an operator who wants to read `管理員` is
    // changing a label, not a meaning — so only the permissions are refused.
    // The way to have ordinary accounts do less is a role of your own, and
    // `auth.default_role` to send new accounts into it.
    if existing.is_system && permissions != existing.permissions {
        return Err(Error::Conflict(
            "a built-in role's permissions cannot be changed; make a role of your own and point              auth.default_role at it"
                .to_owned(),
        ));
    }

    // Taking `roles:write` off the role that carries it for everybody is the
    // one edit that cannot be undone through any screen.
    if existing.permissions.allows(Permission::RolesWrite)
        && !permissions.allows(Permission::RolesWrite)
        && state.roles.holders_of(Permission::RolesWrite).await?
            <= state.roles.member_count(role_id).await?
    {
        return Err(Error::Conflict(
            "that would leave nobody able to administer this instance".to_owned(),
        ));
    }

    let now = OffsetDateTime::now_utc();
    if state
        .roles
        .update(role_id, &name, body.description.clone(), &permissions, now)
        .await?
        == RoleChange::NoSuchRole
    {
        return Err(Error::NotFound);
    }
    let Some(updated) = state.roles.find_by_id(role_id).await? else {
        return Err(Error::NotFound);
    };
    let members = state.roles.member_count(role_id).await?;
    Ok(Json(RoleResponse::build(&updated, members)))
}

/// Removes a role.
///
/// Anybody in it stops being in it, which can leave somebody in no role at all
/// — signed in and allowed nothing. That is a real state and not an error; the
/// listing shows how many people it would affect.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the caller may not write roles
/// or used a bearer token, `404` when no role has that identifier, `409` when
/// it is a seeded role or the last one able to administer the instance.
#[utoipa::path(
    delete,
    path = "/admin/roles/{id}",
    operation_id = "delete_role",
    tag = ADMIN_TAG,
    params(("id" = String, Path, description = "Which role")),
    responses(
        (status = 204, description = "It is gone"),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not write roles, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "No such role", body = crate::error::ErrorBody),
        (status = 409, description = "A seeded role, or nobody would be left to administer the instance", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn delete_role(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<String>,
) -> Result<StatusCode> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::RolesWrite)?;

    let role_id = RoleId::parse(&id).map_err(|_| Error::NotFound)?;
    let Some(existing) = state.roles.find_by_id(role_id).await? else {
        return Err(Error::NotFound);
    };
    if existing.permissions.allows(Permission::RolesWrite)
        && state.roles.holders_of(Permission::RolesWrite).await?
            <= state.roles.member_count(role_id).await?
    {
        return Err(Error::Conflict(
            "that would leave nobody able to administer this instance".to_owned(),
        ));
    }

    match state.roles.delete(role_id).await? {
        RoleChange::Done => Ok(StatusCode::NO_CONTENT),
        RoleChange::NoSuchRole => Err(Error::NotFound),
        RoleChange::Seeded => Err(Error::Conflict(
            "the roles this instance was seeded with cannot be removed".to_owned(),
        )),
    }
}

/// Puts an account in exactly these roles.
///
/// # Errors
///
/// `400` when a role identifier is not one, `401` when nothing was signed in,
/// `403` when the caller may not write roles, used a bearer token, or would be
/// granting a permission they do not hold, `404` when no such account or role,
/// `409` when it would leave nobody able to administer the instance.
#[utoipa::path(
    put,
    path = "/admin/users/{id}/roles",
    operation_id = "set_user_roles",
    tag = ADMIN_TAG,
    params(("id" = String, Path, description = "Which account")),
    request_body = UserRolesRequest,
    responses(
        (status = 200, description = "The account as it now stands", body = AdminUserResponse),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not write roles, used an API token, or would grant a permission not held", body = crate::error::ErrorBody),
        (status = 404, description = "No such account or role", body = crate::error::ErrorBody),
        (status = 409, description = "Nobody would be left to administer the instance", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn set_user_roles(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<String>,
    ApiJson(body): ApiJson<UserRolesRequest>,
) -> Result<Json<AdminUserResponse>> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::RolesWrite)?;

    let user_id = UserId::parse(&id).map_err(|_| Error::NotFound)?;
    let Some(user) = state.users.find_by_id(user_id).await? else {
        return Err(Error::NotFound);
    };

    let mut wanted = Vec::with_capacity(body.roles.len());
    for role in &body.roles {
        wanted.push(RoleId::parse(role).map_err(|_| Error::NotFound)?);
    }

    // What the account would end up holding, checked against what the caller
    // holds — otherwise `roles:write` is a way to give somebody else, and then
    // oneself, anything at all.
    let mut resulting: Vec<Permission> = Vec::new();
    for role in &wanted {
        let Some(found) = state.roles.find_by_id(*role).await? else {
            return Err(Error::NotFound);
        };
        resulting.extend(found.permissions.iter());
    }
    refuse_escalation(&current, &resulting.into_iter().collect())?;

    // The same lock-out guard as everywhere else: this is the other way to take
    // the last administrator's keys away.
    let held = state.roles.permissions_of(user_id).await?;
    let losing_the_keys =
        held.allows(Permission::RolesWrite) && !wanted_grants_keys(&state, &wanted).await?;
    if losing_the_keys && state.roles.holders_of(Permission::RolesWrite).await? <= 1 {
        return Err(Error::Conflict(
            "that would leave nobody able to administer this instance".to_owned(),
        ));
    }

    if state
        .roles
        .set_roles_of(user_id, &wanted, OffsetDateTime::now_utc())
        .await?
        == RoleChange::NoSuchRole
    {
        return Err(Error::NotFound);
    }
    let roles = state.roles.of_user(user_id).await?;
    Ok(Json(AdminUserResponse(AccountResponse::build(
        &user, &roles,
    ))))
}

/// Whether any of these roles carries `roles:write`.
async fn wanted_grants_keys(state: &AppState, wanted: &[RoleId]) -> Result<bool> {
    for role in wanted {
        if let Some(found) = state.roles.find_by_id(*role).await?
            && found.permissions.allows(Permission::RolesWrite)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Refuses to hand out a permission the caller does not hold.
///
/// Without it, `roles:write` is every permission — see [`create_role`].
fn refuse_escalation(current: &CurrentUser, wanted: &PermissionSet) -> Result<()> {
    for permission in wanted.iter() {
        current.require(permission)?;
    }
    Ok(())
}

/// One share, as an administrator sees it.
///
/// Deliberately not the same shape a person sees of their own: this listing
/// exists to find things that need removing, so it carries who uploaded it and
/// how big it is, and it never carries the retrieval code. An administrator who
/// can read every code can open every share, and that is a different power from
/// being able to delete one.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdminShareResponse {
    /// Stable identifier, for [`delete_share`].
    #[schema(example = "01K1ZQK9V8QF5S3T4M2N6P7R8S")]
    pub id: String,
    /// Whether it holds a file or text.
    #[schema(example = "file")]
    pub kind: String,
    /// What the uploader called it, when it is a file.
    pub filename: Option<String>,
    /// How many bytes it holds.
    pub size_bytes: u64,
    /// Who uploaded it, or `null` when nobody was signed in.
    ///
    /// An account that has since been deleted still appears here — the row it
    /// left behind is what keeps this answerable. `null` means the upload was
    /// anonymous, which is a different fact and worth telling apart.
    pub owner_id: Option<String>,
    /// Whether a visitor could still open it.
    pub active: bool,
    /// When it was uploaded, RFC 3339.
    pub created_at: String,
    /// When it stops being served, RFC 3339, or `null` if it never does.
    pub expires_at: Option<String>,
}

impl AdminShareResponse {
    fn build(share: &Share, content: &Content, now: OffsetDateTime) -> Self {
        Self {
            id: share.id.to_string(),
            kind: share.kind.as_str().to_owned(),
            filename: content.filename.clone(),
            size_bytes: content.size_bytes,
            owner_id: share.user_id.map(|id| id.to_string()),
            active: matches!(share.availability_at(now), Availability::Available),
            created_at: format_timestamp(share.created_at),
            expires_at: share.expires_at.map(format_timestamp),
        }
    }
}

/// Whether a listing should include accounts that have been removed.
#[derive(Debug, Deserialize, IntoParams)]
pub struct UserListParams {
    /// Include accounts that have been deleted.
    ///
    /// Off by default: what is left of one identifies nobody and cannot be
    /// acted on, so it belongs in an answer somebody asked for rather than in
    /// every listing.
    #[serde(default)]
    pub include_deleted: bool,
}

/// A page of shares.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdminSharePage {
    /// The shares on this page.
    pub shares: Vec<AdminShareResponse>,
    /// How many exist in total, so a client can render pagination.
    pub total: u64,
}

/// Which slice of the listing to return.
#[derive(Debug, Deserialize, IntoParams)]
pub struct SharePageParams {
    /// How many to return. Capped, and defaulted when omitted.
    #[serde(default)]
    pub limit: Option<u64>,
    /// How many to skip.
    #[serde(default)]
    pub offset: Option<u64>,
}

/// The most shares one request will return.
const MAX_SHARE_PAGE: u64 = 200;
/// How many it returns when the caller does not say.
const DEFAULT_SHARE_PAGE: u64 = 50;

/// Lists every share on the instance, newest first.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the caller may not read every
/// share or used a bearer token, `404` when this instance has no
/// administration.
#[utoipa::path(
    get,
    path = "/admin/shares",
    operation_id = "list_all_shares",
    tag = ADMIN_TAG,
    params(SharePageParams),
    responses(
        (status = 200, description = "A page of shares", body = AdminSharePage),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not read every share, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no administration", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn list_shares(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiQuery(params): ApiQuery<SharePageParams>,
) -> Result<Json<AdminSharePage>> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::SharesReadAny)?;

    let limit = params
        .limit
        .unwrap_or(DEFAULT_SHARE_PAGE)
        .min(MAX_SHARE_PAGE);
    let offset = params.offset.unwrap_or(0);
    let shares = state.shares.list_all(limit, offset).await?;
    let contents = state.shares.contents_for(&shares).await?;
    let now = OffsetDateTime::now_utc();

    Ok(Json(AdminSharePage {
        shares: shares
            .iter()
            .filter_map(|share| {
                contents
                    .get(&share.id)
                    .map(|content| AdminShareResponse::build(share, content, now))
            })
            .collect(),
        total: state.shares.count_all().await?,
    }))
}

/// Removes any share, whoever uploaded it.
///
/// # Why this exists
///
/// The ordinary delete route asks whether the caller owns the share. An upload
/// from an account that has since been deleted is owned by nobody, so nothing
/// could remove it — it kept serving its content, kept its place in the storage
/// quota, and was never swept if it had no expiry and no use limit. This is the
/// only way to reach one.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the caller may not delete every
/// share or used a bearer token, `404` when no share has that identifier.
#[utoipa::path(
    delete,
    path = "/admin/shares/{id}",
    operation_id = "delete_any_share",
    tag = ADMIN_TAG,
    params(("id" = String, Path, description = "Which share")),
    responses(
        (status = 204, description = "It is gone"),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not delete every share, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "No such share", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn delete_share(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<String>,
) -> Result<StatusCode> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::SharesDeleteAny)?;

    let share_id = ShareId::parse(&id).map_err(|_| Error::NotFound)?;
    let Some(share) = state.shares.find_by_id(share_id).await? else {
        return Err(Error::NotFound);
    };
    let content = state.shares.content_of(&share).await?;

    // The object first, for the same reason the owner's own delete does it in
    // this order: a row without its object is a broken share, an object without
    // its row is an orphan a sweep can still find by listing.
    if let Some(key) = content.storage.object_key() {
        state.storage.delete(&ObjectPath::from(key)).await?;
    }
    state.shares.delete(share_id).await?;
    tracing::info!(share = %share_id, by = %current.user.id, "an administrator removed a share");
    Ok(StatusCode::NO_CONTENT)
}

/// How many shares an operation ended.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RemovedSharesResponse {
    /// How many were still live and are now finished.
    ///
    /// The bytes go as the reaper reaches them, so this number is not how much
    /// space came back yet.
    pub removed: u64,
}

/// Ends every share one account uploaded.
///
/// # Why this is not part of deleting the account
///
/// Removing somebody and destroying what they uploaded are different acts. An
/// operator clearing space wants this and not that; one removing a person who
/// has left wants that and not this. Welding them together would make one of
/// those two impossible and the other a surprise.
///
/// It works on an account in any state, including one still in use — an
/// operator reclaiming space from somebody who is still here should not have to
/// suspend them first.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the caller may not delete every
/// share or used a bearer token, `404` when no account has that identifier.
#[utoipa::path(
    delete,
    path = "/admin/users/{id}/shares",
    operation_id = "remove_user_shares",
    tag = ADMIN_TAG,
    params(("id" = String, Path, description = "Whose shares to end")),
    responses(
        (status = 200, description = "How many were ended", body = RemovedSharesResponse),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not delete every share, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "No account has that id", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn remove_user_shares(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<String>,
) -> Result<Json<RemovedSharesResponse>> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::SharesDeleteAny)?;

    let user_id = UserId::parse(&id).map_err(|_| Error::NotFound)?;
    if state.users.find_by_id(user_id).await?.is_none() {
        return Err(Error::NotFound);
    }

    let removed = state
        .shares
        .expire_all_for_user(user_id, OffsetDateTime::now_utc())
        .await?;
    tracing::info!(account = %user_id, by = %current.user.id, removed, "an administrator ended an account's shares");
    Ok(Json(RemovedSharesResponse { removed }))
}
