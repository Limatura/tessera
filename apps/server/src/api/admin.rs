//! What an administrator can do to other people's accounts.
//!
//! # Why a cookie and not a token
//!
//! Every endpoint here refuses a bearer token. A token exists so a script can
//! upload files; granting the administrator flag or deleting an account is the
//! largest thing this API does. A leaked token could otherwise make itself
//! permanent by promoting an account it controls, the same escalation the token
//! endpoints already refuse for minting another token.
//!
//! Automation against this surface waits for scoped tokens.
//!
//! # Why `features.admin = false` answers 404 instead of not mounting
//!
//! The contract describes the software, not one instance's configuration, so
//! `openapi.json` is the same file everywhere and a client can be built against
//! it without knowing which switches an operator threw. `/capabilities` is what
//! says which half of the software is live here.

// `params(SharePageParams)` makes utoipa emit a fully-qualified path to the
// type, and the generated code lands outside the function it decorates, so the
// suppression has to be module-wide. Dropping `params(..)` instead would take
// the paging parameters out of the contract. Same arrangement as
// `super::shares`.
#![expect(
    unused_qualifications,
    reason = "emitted by utoipa's params(..), not written here"
)]

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use tessera_core::{AccountState, InviteId, Permission, PermissionSet, RoleId, ShareId, UserId};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};

use crate::{
    api::{
        ADMIN_TAG, Patch,
        auth::AccountResponse,
        extract::{ApiJson, ApiPath, ApiQuery},
        patched,
        shares::{SharePageParams, page_bounds},
        tokens::format_timestamp,
    },
    auth::extract::CurrentUser,
    db::repository::{
        invite::Invite,
        role::{Role, RoleChange, RoleFields},
        scan::Scan,
        session::SessionEnding,
        share::{Availability, Content, Share},
    },
    error::{Error, ErrorCode, Result},
    state::AppState,
};

/// Body of `PATCH /admin/users/{id}`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateUserRequest {
    /// Whether this account administers the instance.
    ///
    /// Omit to leave the role unchanged. Sending it always rewrites the role
    /// (`true` to `owner`, `false` to none), so a client changing only `state`
    /// or a quota must omit it, or it demotes the account as a side effect.
    #[serde(default)]
    pub is_admin: Option<bool>,
    /// `active` to let it in, `disabled` to suspend it.
    ///
    /// Omitted leaves it as it is. `deleted` is refused here: removing an
    /// account scrubs it: `DELETE`, not a field on a patch.
    #[serde(default)]
    pub state: Option<String>,
    /// A storage quota for this one account, overriding its role's.
    ///
    /// Absent leaves it alone, `null` clears it so the role's applies again,
    /// and `0` gives this account no ceiling of its own. Three instructions,
    /// so absent and `null` are not the same here.
    ///
    /// For raising or lowering one account without inventing a role for it.
    /// `storage.max_total_bytes` still applies on top, so this cannot grant an
    /// account more room than the instance has.
    #[serde(default, deserialize_with = "patched")]
    #[schema(example = 10_737_418_240_u64)]
    pub quota_bytes: Patch<u64>,
    /// Largest single file this one account may send, overriding its role's.
    ///
    /// Absent leaves it alone, `null` clears it so the role's applies again,
    /// and `0` gives this account no limit of its own.
    ///
    /// `server.max_body_bytes` still applies on top; this cannot raise it.
    #[serde(default, deserialize_with = "patched")]
    #[schema(example = 5_368_709_120_u64)]
    pub max_file_bytes: Patch<u64>,
}

/// One account, as an administrator sees it.
///
/// What a user sees of themselves, plus the one thing only an administrator
/// needs: whether the account may sign in. Users never see their own `state`
/// as anything but active, since a disabled account cannot sign in to ask.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdminUserResponse {
    /// The account, as its owner sees it.
    #[serde(flatten)]
    pub account: AccountResponse,
    /// Whether it may sign in, or has been deleted.
    pub state: UserState,
}

impl AdminUserResponse {
    fn build(
        user: &crate::db::repository::user::User,
        roles: &[crate::db::repository::role::Role],
    ) -> Self {
        Self {
            account: AccountResponse::build(user, roles),
            state: user.state.into(),
        }
    }
}

/// Where an account stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UserState {
    /// Signs in, and does whatever its role allows.
    Active,
    /// Stopped from signing in. Its shares keep serving.
    Disabled,
    /// Removed. The remaining row keeps its shares attributable; the name is a
    /// tombstone that identifies no one.
    Deleted,
}

impl From<AccountState> for UserState {
    fn from(state: AccountState) -> Self {
        match state {
            AccountState::Active => Self::Active,
            AccountState::Disabled => Self::Disabled,
            AccountState::Deleted => Self::Deleted,
        }
    }
}

/// Refuses everything when the instance has no administration.
pub(super) fn require_admin_feature(state: &AppState) -> Result<()> {
    if state.config().features.admin {
        Ok(())
    } else {
        Err(Error::NotFound)
    }
}

/// Whether one account holds a permission.
///
/// Asked about an account other than the caller's, so it is a query and not a
/// look at [`CurrentUser::permissions`].
async fn holds(state: &AppState, user: UserId, permission: Permission) -> Result<bool> {
    Ok(state.roles.permissions_of(user).await?.allows(permission))
}

/// Refuses anything that did not arrive with a session cookie, then checks the
/// one permission this endpoint needs.
///
/// The two go together throughout: an administrative action is one a person
/// takes at a keyboard, and each names what it needs instead of sharing one
/// "is an administrator" answer.
pub(super) fn admitted(current: &CurrentUser, permission: Permission) -> Result<()> {
    if !current.credential.is_session() {
        return Err(Error::new(ErrorCode::SessionRequired));
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
        (status = 200, description = "A page of accounts", body = AdminUserPage),
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
) -> Result<Json<AdminUserPage>> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::UsersRead)?;

    let (limit, offset) = page_bounds(&crate::api::shares::SharePageParams {
        limit: params.limit,
        offset: params.offset,
    })?;
    let users = state
        .users
        .list(params.include_deleted, limit, offset)
        .await?;
    // One query for the whole page. Asking per account would make a list of
    // fifty accounts a hundred and one queries.
    let roles = state.roles.of_users(&users).await?;
    Ok(Json(AdminUserPage {
        data: users
            .iter()
            .map(|user| {
                let held = roles.get(&user.id).map_or(&[][..], Vec::as_slice);
                AdminUserResponse::build(user, held)
            })
            .collect(),
        meta: crate::api::shares::PageMeta {
            total: state.users.count(params.include_deleted).await?,
            limit,
            offset,
        },
    }))
}

/// Grants or removes the administrator flag.
///
/// # Why the last one cannot be removed
///
/// An instance with no administrator cannot be repaired through the API: no
/// endpoint is left that would grant the flag back. The way out is the command
/// line against the database file, which an operator locked out of a container
/// may not have.
///
/// Removing your own flag while another administrator still holds it is
/// allowed. That is stepping down, not an accident.
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

    // Counted over the permission that can restore every other one. Once no
    // account holds `roles:write`, the instance cannot be given an
    // administrator back.
    //
    // Both halves matter. Demoting an account that was never an administrator
    // does not reduce the count, and refusing it would leave the last
    // administrator unable to change anyone.
    let holds_the_keys = holds(&state, user.id, Permission::RolesWrite).await?;
    if holds_the_keys
        && body.is_admin == Some(false)
        && state.roles.holders_of(Permission::RolesWrite).await? <= 1
    {
        return Err(Error::new(ErrorCode::LastAdministrator).with_message(
            "this is the only administrator; grant the flag to somebody else first".to_owned(),
        ));
    }

    let now = OffsetDateTime::now_utc();
    if let Some(wanted) = body.state.as_deref() {
        let wanted: AccountState = wanted.parse().map_err(|_| {
            Error::invalid("state", "one_of").with_message("`state` must be `active` or `disabled`")
        })?;
        if wanted == AccountState::Deleted {
            return Err(Error::invalid("state", "use_delete")
                .with_message("removing an account scrubs it; use DELETE rather than this field"));
        }
        // Suspending an account that holds the keys is the same lock-out as
        // demoting it, reached by another door.
        if wanted != AccountState::Active
            && holds_the_keys
            && state.roles.holders_of(Permission::RolesWrite).await? <= 1
        {
            return Err(Error::new(ErrorCode::LastAdministrator).with_message(
                "this is the only administrator; grant the flag to somebody else first".to_owned(),
            ));
        }
        state.users.set_state(user_id, wanted, now).await?;
        if wanted != AccountState::Active {
            // Authentication already refuses a suspended account, so this does
            // not stop them; it makes the session list they see afterwards
            // true.
            state
                .sessions
                .end_all_for_user(user_id, None, SessionEnding::AccountDisabled, now)
                .await?;
        }
    }

    if let Some(is_admin) = body.is_admin {
        state.users.set_admin(user_id, is_admin, now).await?;
    }
    if let Some(quota) = body.quota_bytes {
        state.users.set_quota(user_id, quota, now).await?;
    }
    if let Some(limit) = body.max_file_bytes {
        state.users.set_max_file_bytes(user_id, limit, now).await?;
    }

    let updated = state
        .users
        .find_by_id(user_id)
        .await?
        .ok_or(Error::NotFound)?;
    let roles = state.roles.of_user(updated.id).await?;
    Ok(Json(AdminUserResponse::build(&updated, &roles)))
}

/// Removes an account.
///
/// # What goes with it and what does not
///
/// Sessions and API tokens are deleted by the database. Shares are not: their
/// owner column is `ON DELETE SET NULL`, so their uploads become anonymous
/// shares and keep working for whoever holds the code. Taking those files off
/// the internet in the same motion would be a second decision made silently.
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
        return Err(Error::new(ErrorCode::LastAdministrator).with_message(
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
    // a person: where they signed in from and on what. Removing the account
    // without it would leave the part that matters most.
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
    /// One code with several places is the point: something to send to a group
    /// instead of a code per person. `1` makes it single-use.
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
/// show; see [`CreatedInviteResponse`] for the one moment there is.
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
    /// Whether it would still admit an account.
    ///
    /// Sent, not left to the client to work out from `used`, `max_uses`
    /// and `expires_at` against its own clock, the same reason a session says
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
    /// be read back afterwards, not even by an administrator.
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
/// The command line is the rescue path: it works with the service stopped and
/// needs nothing but the database file. This is the ordinary path, because
/// inviting a friend should not need a shell on the box.
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
        return Err(Error::out_of_range("uses", Some(1), None)
            .with_message("an invite has to admit at least one"));
    }

    let now = OffsetDateTime::now_utc();
    let expires_at = body
        .expires_in_days
        .map(|days| {
            i64::try_from(days)
                .map(time::Duration::days)
                .map(|span| now.saturating_add(span))
                .map_err(|_| {
                    Error::out_of_range("expires_in_days", Some(1), None)
                        .with_message("that is more days than a calendar has")
                })
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
/// The accounts it already admitted stay, since an account does not depend on
/// its code still existing. This only stops the code letting anyone else in,
/// what an operator who posted it in the wrong chat window wants.
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
/// Four numbers an operator can act on, not a time series: "am I near the
/// quota", "how many people are on here", "is the sweep keeping up". Anything
/// wanting graphs wants Prometheus, a different endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StatsResponse {
    /// How many accounts exist.
    pub accounts: u64,
    /// How many of those administer the instance.
    pub administrators: u64,
    /// How many shares exist, including ones that can no longer be opened.
    pub shares: u64,
    /// How many a visitor could still open.
    ///
    /// The gap between this and `shares` is what the sweep has not reached. A
    /// large one means retention is longer than expected, or the sweep is not
    /// running.
    pub live_shares: u64,
    /// Bytes of uploaded content still on the store.
    pub stored_bytes: u64,
    /// How many bytes this instance has sent out.
    ///
    /// Bytes that left the process, not bytes that reached anybody: a proxy in
    /// front holds some, and a caching CDN answers requests this never sees.
    /// For what crossed the network, a reverse proxy's log is the authority.
    ///
    /// Counted over the shares still here, so a swept share takes its tally
    /// with it and this falls when one goes.
    pub bytes_served: u64,
    /// Bytes requested by downloads redirected straight to a store.
    ///
    /// Requested, not delivered: the store does not report what it sent, so an
    /// abandoned download counts in full. Kept separate so `bytes_served` still
    /// means what left this machine.
    pub bytes_redirected: u64,
    /// The ceiling from `storage.max_total_bytes`, or `null` when unset.
    ///
    /// `null` means unset, not unlimited. On an instance taking anonymous
    /// uploads, that combination is what the start-up warning is about.
    pub quota_bytes: Option<u64>,
    /// How many invites could still admit an account.
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
    let quota = state.config().storage.max_total_bytes;

    Ok(Json(StatsResponse {
        // Deleted accounts included, as this counted before `count` got the
        // filter. Whether a dashboard should count tombstones is a fair
        // question for a separate change, not a reason to silently shift a
        // number someone may be watching.
        accounts: state.users.count(true).await?,
        // Counted over the permission, not `users.is_admin`. That column is a
        // mirror kept for rollback and decides nothing; once roles can be
        // assigned directly it stops matching, and a dashboard number that
        // disagrees with who can administer the instance is worse than none.
        administrators: state.roles.holders_of(Permission::RolesWrite).await?,
        shares,
        live_shares,
        stored_bytes: state.shares.total_stored_bytes().await?,
        bytes_served: state.shares.total_bytes_served().await?,
        bytes_redirected: state.shares.total_bytes_redirected().await?,
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
    /// Those cannot be deleted: an instance whose administrator role was
    /// removed has no account left that could create one. They can be edited.
    pub is_system: bool,
    /// What it allows, as `resource:verb` strings.
    #[schema(example = json!(["users:read"]))]
    pub permissions: Vec<String>,
    /// How much storage each account in it may hold.
    ///
    /// `null` means the instance ceiling applies; `0` means no ceiling.
    #[schema(example = 10_737_418_240_u64)]
    pub quota_bytes: Option<u64>,
    /// Largest single file each account in it may send.
    ///
    /// `null` means `server.max_body_bytes` applies; `0` means no limit here.
    #[schema(example = 5_368_709_120_u64)]
    pub max_file_bytes: Option<u64>,
    /// How many accounts are in it.
    ///
    /// What an operator wants before editing. Changing an empty role is free;
    /// changing one held by forty accounts is not.
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
            quota_bytes: role.quota_bytes,
            max_file_bytes: role.max_file_bytes,
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
    /// How much storage each account in this role may hold.
    ///
    /// Omit or send `null` to fall back to the instance-wide ceiling; send `0`
    /// for no ceiling at all. Those are different: `null` follows
    /// `storage.max_total_bytes` as it changes, `0` ignores it.
    #[serde(default)]
    #[schema(example = 10_737_418_240_u64)]
    pub quota_bytes: Option<u64>,
    /// Largest single file accounts in this role may send.
    ///
    /// Omit or send `null` to fall back to `server.max_body_bytes`; send `0`
    /// for no limit of its own. `server.max_body_bytes` still applies on top,
    /// so this can only lower it.
    #[serde(default)]
    #[schema(example = 5_368_709_120_u64)]
    pub max_file_bytes: Option<u64>,
    /// Everything it should allow, as `resource:verb` strings.
    ///
    /// The whole set, not a change to it, so two operators editing the same
    /// role cannot interleave their edits into a set neither chose. The last
    /// one to press save wins, visibly.
    pub permissions: Vec<String>,
}

impl RoleRequest {
    /// Reads the permission names, refusing any this version does not have.
    ///
    /// Refused, not ignored. An operator told their tick was saved
    /// should get it. Dropping one silently produces a role that does less than
    /// the screen says.
    fn permissions(&self) -> Result<PermissionSet> {
        self.permissions
            .iter()
            .map(|name| {
                name.parse::<Permission>().map_err(|_| {
                    Error::new(ErrorCode::UnknownPermission)
                        .with_detail("permission", name.as_str())
                        .with_message(format!("`{name}` is not a permission this version has"))
                })
            })
            .collect()
    }

    fn name(&self) -> Result<String> {
        let name = self.name.trim();
        if name.is_empty() || name.len() > ROLE_NAME_LIMIT {
            return Err(Error::new(ErrorCode::ValueLength)
                .with_detail("field", "name")
                .with_detail("min", 1)
                .with_detail("max", ROLE_NAME_LIMIT)
                .with_message(format!(
                    "a role name must be between 1 and {ROLE_NAME_LIMIT} characters"
                )));
        }
        Ok(name.to_owned())
    }
}

/// The longest a role name may be.
///
/// Matches the `VARCHAR(191)` the MySQL migration uses. A name that fits two
/// dialects and is truncated on the third is worse than one refused everywhere.
const ROLE_NAME_LIMIT: usize = 191;

/// Body of `PUT /admin/users/{id}/roles`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UserRolesRequest {
    /// The role this account should be in, or `null` for none.
    ///
    /// `null` is signed in and allowed nothing: a real state, and what an empty
    /// list used to mean.
    #[serde(default)]
    pub role: Option<String>,
}

/// Every permission this version has, so a screen can offer them.
///
/// Sent, not hard-coded in the client, which would stop offering any
/// permission added since it was built and leave no way to grant them.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PermissionResponse {
    /// The `resource:verb` string.
    #[schema(example = "users:read")]
    pub name: String,
    /// Whether it reaches beyond the holder's own things.
    ///
    /// What an operator is really asking of a role they are about to grant:
    /// does this let them touch other people?
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
/// Otherwise `roles:write` is every permission. An account holding only that
/// could mint a role granting `users:delete` and join it. The check makes it
/// mean "may hand out what I have".
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
        return Err(Error::new(ErrorCode::RoleNameTaken));
    }
    let role = state
        .roles
        .create(
            RoleFields {
                name: &name,
                description: body.description.clone(),
                quota_bytes: body.quota_bytes,
                max_file_bytes: body.max_file_bytes,
                permissions: &permissions,
            },
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
/// identifier, `409` when the name is taken or the change would leave no
/// account able to administer the instance.
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
        return Err(Error::new(ErrorCode::RoleNameTaken));
    }

    // A built-in role means the same thing on every instance, and that is its
    // value: documentation, support conversations and `auth.default_role`
    // all name one expecting it to mean what it means elsewhere.
    //
    // The name is not part of that, since an operator who wants to read `管理員`
    // is changing a label, so only the permissions are refused. To have
    // ordinary accounts do less, make a role and point `auth.default_role` at
    // it.
    if existing.is_system && permissions != existing.permissions {
        return Err(Error::new(ErrorCode::BuiltinRoleImmutable).with_message(
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
        return Err(Error::new(ErrorCode::LastAdministrator));
    }

    let now = OffsetDateTime::now_utc();
    if state
        .roles
        .update(
            role_id,
            RoleFields {
                name: &name,
                description: body.description.clone(),
                quota_bytes: body.quota_bytes,
                max_file_bytes: body.max_file_bytes,
                permissions: &permissions,
            },
            now,
        )
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
/// Every member stops being one, which can leave an account in no role at all,
/// signed in and allowed nothing. That is a real state, not an error, and the
/// listing shows how many accounts it would affect.
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
        return Err(Error::new(ErrorCode::LastAdministrator));
    }

    match state.roles.delete(role_id).await? {
        RoleChange::Done => Ok(StatusCode::NO_CONTENT),
        RoleChange::NoSuchRole => Err(Error::NotFound),
        RoleChange::Seeded => Err(Error::new(ErrorCode::BuiltinRoleImmutable)
            .with_message("the roles this instance was seeded with cannot be removed".to_owned())),
    }
}

/// Puts an account in exactly these roles.
///
/// # Errors
///
/// `400` when a role identifier is not one, `401` when nothing was signed in,
/// `403` when the caller may not write roles, used a bearer token, or would be
/// granting a permission they do not hold, `404` when no such account or role,
/// `409` when it would leave no account able to administer the instance.
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

    let wanted = body
        .role
        .as_deref()
        // A name where an id belongs is a mistake in the request, not a role
        // that is missing, and the two want different fixes.
        .map(|role| RoleId::parse(role).map_err(|_| Error::invalid("role", "not_an_identifier")))
        .transpose()?;

    // What the account would end up holding, checked against what the caller
    // holds. Otherwise `roles:write` is a way to grant another account, and
    // then your own, anything at all.
    let resulting = match wanted {
        Some(role) => {
            let Some(found) = state.roles.find_by_id(role).await? else {
                return Err(Error::NotFound);
            };
            found.permissions
        }
        None => PermissionSet::default(),
    };
    refuse_escalation(&current, &resulting)?;

    // The same lock-out guard as everywhere else: this is the other way to take
    // the last administrator's keys away.
    let held = state.roles.permissions_of(user_id).await?;
    let losing_the_keys =
        held.allows(Permission::RolesWrite) && !resulting.allows(Permission::RolesWrite);
    if losing_the_keys && state.roles.holders_of(Permission::RolesWrite).await? <= 1 {
        return Err(Error::new(ErrorCode::LastAdministrator));
    }

    if state
        .roles
        .set_role_of(user_id, wanted, OffsetDateTime::now_utc())
        .await?
        == RoleChange::NoSuchRole
    {
        return Err(Error::NotFound);
    }
    let roles = state.roles.of_user(user_id).await?;
    Ok(Json(AdminUserResponse::build(&user, &roles)))
}

/// Refuses to hand out a permission the caller does not hold.
///
/// Without it, `roles:write` is every permission. See [`create_role`].
fn refuse_escalation(current: &CurrentUser, wanted: &PermissionSet) -> Result<()> {
    for permission in wanted.iter() {
        current.require(permission)?;
    }
    Ok(())
}

/// One share, as an administrator sees it.
///
/// Not the shape a person sees of their own. This listing exists to find things
/// that need removing, so it carries who uploaded it and how big
/// it is, and never the retrieval code: an administrator who can read every
/// code can open every share, a different power from deleting one.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdminShareResponse {
    /// Stable identifier, for [`delete_share`].
    #[schema(example = "01K1ZQK9V8QF5S3T4M2N6P7R8S")]
    pub id: String,
    /// Whether it holds a file or text.
    #[schema(example = "file")]
    pub kind: String,
    /// What the uploader called it, when the share holds exactly one file.
    ///
    /// `null` for text and for a share with several. See `file_count`.
    pub filename: Option<String>,
    /// How many files it holds. Zero for text.
    pub file_count: u32,
    /// How many bytes it holds.
    pub size_bytes: u64,
    /// How many bytes it has sent out, over its whole life.
    ///
    /// The instance-wide figure in `StatsResponse` answers "what is this
    /// costing me"; this answers which share to do something about. One link
    /// posted somewhere busy is usually where the transfer went, and the total
    /// cannot say which.
    ///
    /// Not `size_bytes` times `use_count`: a range request, a resumed download
    /// and an abandoned one all send some other amount.
    pub bytes_served: u64,
    /// Bytes requested by its downloads redirected straight to a store.
    /// Requested, not delivered.
    pub bytes_redirected: u64,
    /// Who uploaded it, or `null` for an anonymous upload.
    ///
    /// An account deleted since still appears here, because the row it left
    /// keeps this answerable. `null` means the upload was anonymous, a
    /// different fact worth telling apart.
    pub owner_id: Option<String>,
    /// The uploader's account name, or `null` for an anonymous upload.
    ///
    /// Included next to the id so the listing can show it without a request
    /// per row. An account deleted since returns its tombstone name, which
    /// starts with `gone-`.
    #[schema(example = "alice")]
    pub owner_username: Option<String>,
    /// Whether a visitor could still open it.
    pub active: bool,
    /// Why a visitor can or cannot open it. `active` is `true` only for
    /// `available`.
    pub status: AdminShareStatus,
    /// What the malware scanner said: `pending`, `deferred`, `clean`,
    /// `infected` or `skipped`, or `null` if it was never sent to one.
    ///
    /// For several files, the one that matters most: `infected` over
    /// `pending` over `deferred` over `skipped` over `clean`.
    #[schema(example = "infected")]
    pub scan_state: Option<String>,
    /// The threat the scanner named for `infected`, or why it was `skipped`
    /// (`encrypted` or `too_large`). `null` otherwise.
    #[schema(example = "Eicar-Test-Signature")]
    pub scan_detail: Option<String>,
    /// When it was uploaded, RFC 3339.
    pub created_at: String,
    /// When it stops being served, RFC 3339, or `null` if it never does.
    pub expires_at: Option<String>,
}

/// Why a share can or cannot be opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdminShareStatus {
    /// A visitor can open it, with the password if it has one.
    Available,
    /// Switched off, because the malware scanner found a threat in it. See
    /// `scan_detail`.
    Disabled,
    /// Past its expiry.
    Expired,
    /// Downloaded as many times as it allows.
    Exhausted,
}

impl From<Availability> for AdminShareStatus {
    fn from(availability: Availability) -> Self {
        match availability {
            Availability::Available => Self::Available,
            Availability::Disabled => Self::Disabled,
            Availability::Expired => Self::Expired,
            Availability::Exhausted => Self::Exhausted,
        }
    }
}

/// The verdict that says most about a share, from every file or note in it.
fn weightiest_scan(content: &Content) -> Option<&Scan> {
    let weight = |scan: &Scan| match scan {
        Scan::Infected(_) => 4,
        Scan::Pending => 3,
        Scan::Deferred => 2,
        Scan::Skipped(_) => 1,
        Scan::Clean => 0,
    };
    let scans: Vec<&Scan> = match content {
        Content::Files(files) => files.iter().filter_map(|file| file.scan.as_ref()).collect(),
        Content::Text(text) => text.scan.iter().collect(),
        Content::Gone => Vec::new(),
    };
    scans.into_iter().max_by_key(|scan| weight(scan))
}

impl AdminShareResponse {
    fn build(
        share: &Share,
        content: &Content,
        owners: &std::collections::HashMap<tessera_core::UserId, tessera_core::Username>,
        now: OffsetDateTime,
    ) -> Self {
        let status = share.availability_at(now);
        let (scan_state, scan_detail) = weightiest_scan(content)
            .map(Scan::columns)
            .map_or((None, None), |(state, detail)| {
                (Some(state.to_owned()), detail)
            });
        Self {
            id: share.id.to_string(),
            kind: share.kind.as_str().to_owned(),
            // The one file's name, or how many there are. An administrator
            // scanning for something to remove wants to recognise it, and "3
            // files" is what a multi-file share is.
            filename: content.single_file().and_then(|file| file.filename.clone()),
            file_count: u32::try_from(content.files().len()).unwrap_or(u32::MAX),
            size_bytes: content.size_bytes(),
            bytes_served: share.bytes_served,
            bytes_redirected: share.bytes_redirected,
            owner_id: share.user_id.map(|id| id.to_string()),
            owner_username: share
                .user_id
                .and_then(|id| owners.get(&id))
                .map(ToString::to_string),
            active: matches!(status, Availability::Available),
            status: status.into(),
            scan_state,
            scan_detail,
            created_at: format_timestamp(share.created_at),
            expires_at: share.expires_at.map(format_timestamp),
        }
    }
}

/// Whether a listing should include accounts that have been removed.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct UserListParams {
    /// Include accounts that have been deleted.
    ///
    /// Off by default. What is left of one identifies no person and cannot be
    /// acted on, so it belongs only in an answer that asked for it.
    #[serde(default)]
    pub include_deleted: bool,
    /// How many to return. Capped, and defaulted when omitted.
    #[serde(default)]
    pub limit: Option<u64>,
    /// How many to skip.
    #[serde(default)]
    pub offset: Option<u64>,
}

/// A page of accounts.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdminUserPage {
    /// The accounts on this page, newest first.
    pub data: Vec<AdminUserResponse>,
    /// The collection they came from.
    pub meta: crate::api::shares::PageMeta,
}

/// A page of shares.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdminSharePage {
    /// The shares on this page.
    pub data: Vec<AdminShareResponse>,
    /// The collection they came from.
    pub meta: crate::api::shares::PageMeta,
}

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
        (status = 400, description = "`limit` or `offset` is not a number", body = crate::error::ErrorBody),
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

    let (limit, offset) = page_bounds(&params)?;
    let shares = state.shares.list_all(limit, offset).await?;
    let contents = state.shares.contents_for(&shares).await?;
    let mut owner_ids: Vec<_> = shares.iter().filter_map(|share| share.user_id).collect();
    owner_ids.sort_unstable();
    owner_ids.dedup();
    let owners = state.users.usernames_of(&owner_ids).await?;
    let now = OffsetDateTime::now_utc();

    Ok(Json(AdminSharePage {
        data: shares
            .iter()
            .filter_map(|share| {
                contents
                    .get(&share.id)
                    .map(|content| AdminShareResponse::build(share, content, &owners, now))
            })
            .collect(),
        meta: crate::api::shares::PageMeta {
            total: state.shares.count_all().await?,
            limit,
            offset,
        },
    }))
}

/// One recorded retrieval, as an administrator sees it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccessEventResponse {
    /// When the content was handed over, RFC 3339.
    #[schema(example = "2026-08-22T09:41:00Z")]
    pub occurred_at: String,
    /// Where the request came from, or `null` when there was no honest answer.
    ///
    /// Behind a proxy this instance does not trust, the visible address is the
    /// proxy's. A null says so; recording the proxy's address would record the
    /// wrong person.
    #[schema(example = "203.0.113.9")]
    pub ip_address: Option<String>,
    /// What the client called itself, as presented and truncated.
    #[schema(example = "Mozilla/5.0")]
    pub user_agent: Option<String>,
}

/// A share's recorded retrievals.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccessLogResponse {
    /// The events on this page, most recent first.
    pub data: Vec<AccessEventResponse>,
    /// The collection they came from, and whether anything is being recorded.
    pub meta: AccessLogMeta,
}

/// [`PageMeta`](crate::api::shares::PageMeta) plus the one extra fact this list
/// needs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
pub struct AccessLogMeta {
    /// How many events this share has, not how many are on this page.
    pub total: u64,
    /// The page size used.
    pub limit: u64,
    /// How many were skipped.
    pub offset: u64,
    /// Whether this instance is recording at all.
    ///
    /// An empty list means the share was never fetched, or that recording is
    /// off. A screen that cannot tell those apart reports the second as the
    /// first.
    pub enabled: bool,
}

/// Reads who has fetched one share.
///
/// # Why this is administrative and not the uploader's
///
/// The switch belongs to whoever runs the instance, so the record does too.
/// Handing visitor addresses to every uploader would be a second decision about
/// a visitor's privacy, made on behalf of an operator who agreed only to the
/// first. On a shared instance the uploader is not the operator.
///
/// Letting uploaders see their own is a deliberate change to make, not one to
/// arrive at by leaving this open.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the caller may not read every
/// share or used a bearer token, `404` when there is no such share or this
/// instance has no administration.
#[utoipa::path(
    get,
    path = "/admin/shares/{id}/access-log",
    operation_id = "read_share_access_log",
    tag = ADMIN_TAG,
    params(
        ("id" = String, Path, description = "Which share"),
        SharePageParams,
    ),
    responses(
        (status = 200, description = "What has been recorded", body = AccessLogResponse),
        (status = 400, description = "`limit` or `offset` is not a number", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not read every share, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "No such share", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn share_access_log(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<String>,
    ApiQuery(params): ApiQuery<SharePageParams>,
) -> Result<Json<AccessLogResponse>> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::SharesReadAny)?;

    let share_id = ShareId::parse(&id).map_err(|_| Error::NotFound)?;
    if state.shares.find_by_id(share_id).await?.is_none() {
        return Err(Error::NotFound);
    }

    // `offset` used to be read and ignored, so the docs advertised a parameter
    // that did nothing and the second page returned the first.
    let (limit, offset) = page_bounds(&params)?;
    let events = state.events.for_share(share_id, limit, offset).await?;

    Ok(Json(AccessLogResponse {
        meta: AccessLogMeta {
            total: state.events.count_for_share(share_id).await?,
            limit,
            offset,
            enabled: state.config().share.access_log.enabled,
        },
        data: events
            .into_iter()
            .map(|event| AccessEventResponse {
                occurred_at: format_timestamp(event.occurred_at),
                ip_address: event.ip_address,
                user_agent: event.user_agent,
            })
            .collect(),
    }))
}

/// Removes any share, whoever uploaded it.
///
/// # Why this exists
///
/// The ordinary delete route asks whether the caller owns the share. An upload
/// from a since-deleted account has no owner, so nothing could remove it. It
/// kept serving, kept its place in the storage quota, and was never swept if it
/// had no expiry and no use limit. This is the only way to reach one.
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
    let Some(_) = state.shares.find_by_id(share_id).await? else {
        return Err(Error::NotFound);
    };

    // Objects first, and every object, since a share can hold several. A row
    // without its object is a broken share; an object without its row is an
    // orphan a sweep can still find by listing.
    crate::api::shares::remove_objects(&state, share_id).await?;
    state.shares.delete(share_id).await?;
    tracing::info!(share = %share_id, by = %current.user.id, "an administrator removed a share");
    Ok(StatusCode::NO_CONTENT)
}

/// How many shares an operation ended.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RemovedSharesResponse {
    /// How many were still live and are now finished.
    ///
    /// The bytes go as the reaper reaches them, so this is not how much space
    /// has come back.
    pub removed: u64,
}

/// Ends every share one account uploaded.
///
/// # Why this is not part of deleting the account
///
/// Removing an account and destroying what it uploaded are different acts. An
/// operator clearing space wants this and not that; one removing a person who
/// has left wants the reverse. Welding them together makes one impossible and
/// the other a surprise.
///
/// It works on an account in any state, including one still in use. Reclaiming
/// space from an active account should not require suspending it first.
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
