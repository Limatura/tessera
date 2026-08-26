//! Storage backends, from the panel.
//!
//! The same operations `tessera-server backends` offers, over HTTP. The command
//! line exists for the instance that has no panel — `features.admin` off, or a
//! configuration that broke the panel — and this exists because adding a bucket
//! from a terminal is not how most operators want to spend an afternoon.
//!
//! # What is never sent
//!
//! The URL and the options. Both carry credentials, and a listing is the sort of
//! thing that ends up pasted into a support thread. The panel can *set* them and
//! can see everything else; it cannot read them back. That is also what makes
//! the sealing worth anything — a surface that echoed them would leave the
//! encryption protecting the database against everybody except the one request
//! that asks nicely.

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    api::{
        ADMIN_TAG,
        admin::{admitted, require_admin_feature},
        extract::{ApiJson, ApiPath},
    },
    auth::extract::CurrentUser,
    db::repository::storage_backend::{StorageBackend, StorageBackendError},
    error::{Error, Result},
    state::AppState,
};

/// One backend, as the panel draws it.
///
/// Four booleans, and they are four unrelated questions: where new uploads go,
/// whether this one still takes them, whether the process can reach it, and
/// whether it comes from the environment. Collapsing any pair into an enum
/// would invent a state the system does not have — a read-only backend can be
/// the default, and an unreachable one can be either.
#[expect(
    clippy::struct_excessive_bools,
    reason = "four independent facts about one backend; see above"
)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackendResponse {
    /// What `file_segments.backend` records. Stable, and how everything else
    /// refers to it.
    #[schema(example = "cold")]
    pub name: String,
    /// Whether new uploads go here.
    pub is_default: bool,
    /// How much it may hold, or `null` for no limit.
    ///
    /// Typed in by the operator: object stores have no capacity API, and on a
    /// free tier the limit belongs to the account's plan, which no API reports.
    #[schema(example = 10_737_418_240_u64)]
    pub capacity_bytes: Option<u64>,
    /// How much it is holding.
    ///
    /// Not typed in and not asked of the store — summed from the segments this
    /// instance wrote.
    #[schema(example = 4_294_967_296_u64)]
    pub used_bytes: u64,
    /// Whether it still takes new content. Reads are unaffected.
    pub read_only: bool,
    /// Whether the process could open it.
    ///
    /// `false` means shares held there answer `503` until the operator fixes
    /// the credentials or the address — worth showing prominently, because
    /// nothing else about the row will look wrong.
    pub reachable: bool,
    /// Whether it comes from `storage.url` rather than this table.
    ///
    /// The one that does cannot be edited or removed from here: it is where
    /// everything written before backends existed lives, and it is configured
    /// in the environment.
    pub from_environment: bool,
}

/// Body of `POST /admin/storage/backends`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BackendRequest {
    /// What to call it. Reusing an existing name replaces that backend.
    #[schema(example = "cold")]
    pub name: String,
    /// Where it is, credentials and all. Sealed before it is stored.
    #[schema(example = "s3://my-bucket/tessera")]
    pub url: String,
    /// Backend-specific options, passed to `object_store` unchanged.
    #[serde(default)]
    pub options: std::collections::BTreeMap<String, String>,
    /// How much it may hold. Omit or send `null` for no limit.
    #[serde(default)]
    #[schema(example = 10_737_418_240_u64)]
    pub capacity_bytes: Option<u64>,
    /// Whether it should stop taking new content.
    #[serde(default)]
    pub read_only: bool,
}

/// Every configured backend, with what it is holding.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the caller may not read
/// settings, `404` when this instance has no administration.
#[utoipa::path(
    get,
    path = "/admin/storage/backends",
    operation_id = "list_storage_backends",
    tag = ADMIN_TAG,
    responses(
        (status = 200, description = "What is configured, without the credentials", body = Vec<BackendResponse>),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not read settings", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no administration", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<BackendResponse>>> {
    require_admin_feature(&state)?;
    admitted(&current, tessera_core::Permission::SettingsRead)?;
    describe_all(&state).await
}

/// Adds a backend, or replaces one of the same name.
///
/// The new one is opened and reachable to the next request — the pool is
/// rebuilt here rather than at the next restart, because "add a bucket" that
/// needs a restart is the thing this whole feature exists to stop.
///
/// # Errors
///
/// `400` when the URL will not open or the name is the environment's, `401`
/// when nothing was signed in, `403` when the caller may not write settings,
/// `404` when this instance has no administration, `409` when no
/// `storage.secret_key` is set.
#[utoipa::path(
    post,
    path = "/admin/storage/backends",
    operation_id = "add_storage_backend",
    tag = ADMIN_TAG,
    request_body = BackendRequest,
    responses(
        (status = 200, description = "The backend as it now stands", body = BackendResponse),
        (status = 400, description = "The URL will not open, or the name is reserved", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not write settings", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no administration", body = crate::error::ErrorBody),
        (status = 409, description = "No storage.secret_key, so credentials cannot be stored", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn add(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(body): ApiJson<BackendRequest>,
) -> Result<Json<BackendResponse>> {
    require_admin_feature(&state)?;
    admitted(&current, tessera_core::Permission::SettingsWrite)?;
    refuse_reserved(&body.name)?;

    let backend = StorageBackend {
        name: body.name.clone(),
        url: body.url,
        options: body.options,
        // Left alone: a name that is already the default stays it, and a new
        // one does not steal the flag by being written. That is its own call.
        is_default: state
            .backends
            .load()
            .configured
            .iter()
            .any(|existing| existing.name == body.name && existing.is_default),
        capacity_bytes: body.capacity_bytes,
        read_only: body.read_only,
    };

    // Opened before it is stored. A backend that will not open is a typo the
    // operator can fix while they still have the form in front of them —
    // storing it first would mean the failure shows up as somebody's upload
    // going wrong later.
    open_or_refuse(&backend)?;

    state
        .storage_backends
        .put(&backend, OffsetDateTime::now_utc())
        .await
        .map_err(describe_failure)?;
    reload(&state).await?;
    one(&state, &body.name).await
}

/// Removes a backend.
///
/// # Errors
///
/// `400` when the name is the environment's, `401` when nothing was signed in,
/// `403` when the caller may not write settings, `404` when no backend has that
/// name, `409` when it still holds content.
#[utoipa::path(
    delete,
    path = "/admin/storage/backends/{name}",
    operation_id = "remove_storage_backend",
    tag = ADMIN_TAG,
    params(("name" = String, Path, description = "Which backend")),
    responses(
        (status = 204, description = "Removed", body = ()),
        (status = 400, description = "The name is the environment's", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not write settings", body = crate::error::ErrorBody),
        (status = 404, description = "No such backend", body = crate::error::ErrorBody),
        (status = 409, description = "It still holds content; move it off first", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn remove(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(name): ApiPath<String>,
) -> Result<StatusCode> {
    require_admin_feature(&state)?;
    admitted(&current, tessera_core::Permission::SettingsWrite)?;
    refuse_reserved(&name)?;

    if !state
        .storage_backends
        .remove(&name)
        .await
        .map_err(describe_failure)?
    {
        return Err(Error::NotFound);
    }
    reload(&state).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Sends new uploads to one backend.
///
/// Existing files stay where they are — each records its own backend — so this
/// changes what happens next and nothing about what already happened.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the caller may not write
/// settings, `404` when no backend has that name.
#[utoipa::path(
    put,
    path = "/admin/storage/backends/{name}/default",
    operation_id = "set_default_storage_backend",
    tag = ADMIN_TAG,
    params(("name" = String, Path, description = "Which backend")),
    responses(
        (status = 200, description = "The backend as it now stands", body = BackendResponse),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not write settings", body = crate::error::ErrorBody),
        (status = 404, description = "No such backend", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn set_default(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(name): ApiPath<String>,
) -> Result<Json<BackendResponse>> {
    require_admin_feature(&state)?;
    admitted(&current, tessera_core::Permission::SettingsWrite)?;

    // The environment's one becomes the default by nothing else claiming it,
    // which is what clearing the flag everywhere does.
    let chosen = if name == crate::storage::DEFAULT_BACKEND {
        state
            .storage_backends
            .clear_default(OffsetDateTime::now_utc())
            .await
            .map_err(describe_failure)?;
        true
    } else {
        state
            .storage_backends
            .set_default(&name, OffsetDateTime::now_utc())
            .await
            .map_err(describe_failure)?
    };
    if !chosen {
        return Err(Error::NotFound);
    }
    reload(&state).await?;
    one(&state, &name).await
}

/// Refuses a name that belongs to the environment.
///
/// The backend from `storage.url` has no row, so writing one under its name
/// would create a second definition of the same thing — and the configured one
/// wins at start-up, so the stored one would be silently ignored.
fn refuse_reserved(name: &str) -> Result<()> {
    if name == crate::storage::DEFAULT_BACKEND {
        return Err(Error::bad_request(
            "`default` is the backend from storage.url; change it in the environment",
        ));
    }
    Ok(())
}

/// Proves the backend opens before it is written down.
fn open_or_refuse(backend: &StorageBackend) -> Result<()> {
    crate::storage::build(&crate::config::StorageConfig {
        url: backend.url.clone(),
        options: backend.options.clone(),
        ..crate::config::StorageConfig::default()
    })
    .map(|_| ())
    .map_err(|error| Error::bad_request(format!("that backend will not open: {error}")))
}

/// Rebuilds the pool so a change is live for the next request.
///
/// The whole point of storing these rather than configuring them: an operator
/// adding a bucket should not have to restart the instance to use it.
async fn reload(state: &AppState) -> Result<()> {
    let configured = state
        .storage_backends
        .all()
        .await
        .map_err(describe_failure)?;
    let (open, _) = crate::storage::build_all(&state.config().storage, &configured)
        .map_err(|error| Error::bad_request(format!("could not reopen the backends: {error}")))?;
    state.replace_backends(crate::storage::Pool { open, configured });
    Ok(())
}

/// One backend's row from the freshly reloaded pool.
async fn one(state: &AppState, name: &str) -> Result<Json<BackendResponse>> {
    let Json(listed) = describe_all(state).await?;
    listed
        .into_iter()
        .find(|backend| backend.name == name)
        .map(Json)
        .ok_or(Error::NotFound)
}

/// Every backend, with what it is holding.
///
/// Separate from the handler so the write endpoints can answer with the same
/// shape they just changed, rather than a second description that could drift
/// from the one the listing gives.
async fn describe_all(state: &AppState) -> Result<Json<Vec<BackendResponse>>> {
    let used = state.shares.bytes_per_backend().await?;
    let pool = state.backends.load();
    let taken = |name: &str| used.get(name).copied().unwrap_or(0);

    let mut listed = vec![BackendResponse {
        name: crate::storage::DEFAULT_BACKEND.to_owned(),
        is_default: !pool.configured.iter().any(|backend| backend.is_default),
        capacity_bytes: Some(state.config().storage.capacity_bytes).filter(|bytes| *bytes > 0),
        used_bytes: taken(crate::storage::DEFAULT_BACKEND),
        read_only: false,
        reachable: pool.open.contains_key(crate::storage::DEFAULT_BACKEND),
        from_environment: true,
    }];
    listed.extend(pool.configured.iter().map(|backend| BackendResponse {
        name: backend.name.clone(),
        is_default: backend.is_default,
        capacity_bytes: backend.capacity_bytes.filter(|bytes| *bytes > 0),
        used_bytes: taken(&backend.name),
        read_only: backend.read_only,
        reachable: pool.open.contains_key(&backend.name),
        from_environment: false,
    }));
    Ok(Json(listed))
}

/// Turns a repository failure into the answer the caller should get.
///
/// The two that are not internal errors are the two an operator can act on: no
/// key configured, and a backend that is still holding somebody's files.
fn describe_failure(error: StorageBackendError) -> Error {
    match error {
        StorageBackendError::Secret(inner) => Error::Conflict(inner.to_string()),
        StorageBackendError::StillInUse { .. } => Error::Conflict(error.to_string()),
        // A malformed row or a database failure is ours, not the caller's.
        other => {
            tracing::error!(error = %other, "a storage backend could not be read or written");
            Error::Unavailable
        }
    }
}
