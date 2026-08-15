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
use tessera_core::UserId;
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    api::{
        ADMIN_TAG,
        auth::AccountResponse,
        extract::{ApiJson, ApiPath},
    },
    auth::extract::AdminUser,
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

/// Refuses anything that did not arrive with a session cookie.
const fn require_session(admin: &AdminUser) -> Result<()> {
    if admin.0.credential.is_session() {
        Ok(())
    } else {
        Err(Error::Forbidden)
    }
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
    admin: AdminUser,
) -> Result<Json<Vec<AdminUserResponse>>> {
    require_admin_feature(&state)?;
    require_session(&admin)?;

    let users = state.users.list().await?;
    Ok(Json(
        users
            .iter()
            .map(|user| AdminUserResponse(AccountResponse::from(user)))
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
    admin: AdminUser,
    ApiPath(id): ApiPath<String>,
    ApiJson(body): ApiJson<UpdateUserRequest>,
) -> Result<Json<AdminUserResponse>> {
    require_admin_feature(&state)?;
    require_session(&admin)?;

    let user_id = UserId::parse(&id).map_err(|_| Error::NotFound)?;
    let Some(user) = state.users.find_by_id(user_id).await? else {
        return Err(Error::NotFound);
    };

    if user.is_admin && !body.is_admin && state.users.count_admins().await? <= 1 {
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
    Ok(Json(AdminUserResponse(AccountResponse::from(&updated))))
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
    admin: AdminUser,
    ApiPath(id): ApiPath<String>,
) -> Result<StatusCode> {
    require_admin_feature(&state)?;
    require_session(&admin)?;

    let user_id = UserId::parse(&id).map_err(|_| Error::NotFound)?;
    let Some(user) = state.users.find_by_id(user_id).await? else {
        return Err(Error::NotFound);
    };

    if user.is_admin && state.users.count_admins().await? <= 1 {
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
