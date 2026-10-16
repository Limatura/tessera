//! Storage backends, from the panel.
//!
//! The same operations `tessera-server backends` offers, over HTTP. The command
//! line exists for an instance with no panel (`features.admin` off, or a
//! configuration that broke it); this exists because adding a bucket from a
//! terminal is not how most operators want to spend an afternoon.
//!
//! # What is never sent
//!
//! The credentials, and any option not on [`StoreConfig`]'s allowlist. A
//! listing is the sort of thing that ends up pasted into a support thread. The
//! panel sees the location and endpoint with their credentials removed, the
//! region, and the access key ID masked, which is enough to tell two buckets
//! apart and to edit one, but cannot read a secret back. That is what makes the
//! sealing worth anything: a surface that echoed them would leave the
//! encryption protecting the database against everybody except the one request
//! that asks nicely. See [`crate::storage::location`].

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    api::{
        ADMIN_TAG,
        admin::{admitted, require_admin_feature},
        extract::{ApiJson, ApiPath},
        mfa::{ConfirmOwnerRequest, confirm_owner},
    },
    auth::extract::{ClientAddress, CurrentUser},
    db::repository::storage_backend::{StorageBackend, StorageBackendError},
    error::{Error, ErrorCode, Result},
    state::AppState,
    storage::location,
};

/// One backend, as the panel draws it.
///
/// Four booleans for four unrelated questions: where new uploads go, whether
/// this one still accepts them, whether the process can reach it, and whether
/// browsers may be redirected to it. Merging any pair into an enum would
/// invent a state that does not exist, since a read-only backend can be the
/// default and an unreachable one can be either.
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
    /// Where it is, without credentials: no user, password, query or
    /// fragment. A local directory is its path.
    #[schema(example = "s3://my-bucket/tessera")]
    pub url: String,
    /// The access key ID with all but four characters at each end hidden, or
    /// `null` when there is none or it is too short to show any of.
    #[schema(example = "AKIA…MPLE")]
    pub access_key: Option<String>,
    /// The options that say where the store is, for the panel to show and
    /// edit. Credentials are never among them.
    pub config: StoreConfig,
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
    /// Not typed in and not asked of the store, but summed from the segments
    /// this instance wrote.
    #[schema(example = 4_294_967_296_u64)]
    pub used_bytes: u64,
    /// Whether it still takes new content. Reads are unaffected.
    pub read_only: bool,
    /// Whether it answered the last time it was called, or `true` before it
    /// has been.
    ///
    /// `false` means shares held there fail until the operator fixes the
    /// credentials or the address, or the store comes back. Worth showing
    /// prominently, because nothing else about the row looks wrong.
    ///
    /// Learned from the calls uploads and downloads make, so a backend in use
    /// is always current; one left idle is asked every fifteen minutes, and one
    /// that failed every five, until it answers.
    pub reachable: bool,
    /// When it was last called, or `null` before it has been since start-up.
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime, example = "2026-10-15T08:00:00Z")]
    pub last_checked_at: Option<OffsetDateTime>,
    /// Why the last call failed, or `null` when it answered.
    #[schema(example = "Generic S3 error: error sending request")]
    pub last_error: Option<String>,
    /// Whether browsers are redirected to it directly, to download a file or
    /// write an upload's parts, instead of going through this server.
    ///
    /// Only S3 and GCS support this. On anything else it may be `true` and
    /// change nothing.
    pub direct_access: bool,
}

/// The options that place a store, as opposed to the ones that unlock it.
///
/// An allowlist, not every option but the credentials: a key this does not
/// know is not sent, so a secret under a name nobody thought of stays sealed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct StoreConfig {
    /// The server an S3-compatible store talks to (Garage, R2, `SeaweedFS`...), without
    /// credentials, or `null` for the provider's own.
    #[schema(example = "http://garage:3900")]
    pub endpoint: Option<String>,
    /// The region, when one is set.
    #[schema(example = "us-east-1")]
    pub region: Option<String>,
    /// Whether plain HTTP to the endpoint is allowed.
    pub allow_http: bool,
    /// An Azure storage account's name.
    pub account: Option<String>,
}

impl StoreConfig {
    fn of(options: &std::collections::BTreeMap<String, String>) -> Self {
        Self {
            endpoint: location::redacted_endpoint(options),
            region: location::option(options, location::REGION_KEYS).map(str::to_owned),
            allow_http: location::option(options, location::ALLOW_HTTP_KEYS)
                .is_some_and(|value| value.eq_ignore_ascii_case("true")),
            account: location::option(options, location::ACCOUNT_KEYS).map(str::to_owned),
        }
    }
}

/// Body of `POST /admin/storage/backends`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BackendRequest {
    /// What to call it. A name already in use is refused with `409`; replace
    /// a backend with `PUT /admin/storage/backends/{name}`.
    #[schema(example = "cold")]
    pub name: String,
    /// Where it is. Sealed before it is stored.
    ///
    /// For S3, GCS and Azure the credentials go in `options`: `object_store`
    /// takes only the bucket from an `s3://` URL and ignores a `key:secret@`.
    #[schema(example = "s3://my-bucket/tessera")]
    pub url: String,
    /// Backend-specific options, passed to `object_store` unchanged:
    /// `aws_endpoint`, `aws_region`, `aws_access_key_id`,
    /// `aws_secret_access_key` and the like.
    #[serde(default)]
    pub options: std::collections::BTreeMap<String, String>,
    /// How much it may hold. Omit or send `null` for no limit.
    #[serde(default)]
    #[schema(example = 10_737_418_240_u64)]
    pub capacity_bytes: Option<u64>,
    /// Whether it should stop taking new content.
    #[serde(default)]
    pub read_only: bool,
    /// Whether browsers may be redirected to it directly. Only if browsers can
    /// reach it; see [`BackendResponse::direct_access`].
    #[serde(default)]
    pub direct_access: bool,
}

/// Body of `PUT /admin/storage/backends/{name}`: everything in
/// [`BackendRequest`] but the name, which the path gives.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReplaceBackendRequest {
    /// Where it is. Sealed before it is stored.
    ///
    /// Without credentials, here or in `options`, the stored ones are kept,
    /// as long as the backend stays on the same store: same scheme, same
    /// server, same options. Moving to another bucket keeps them; moving to
    /// another endpoint is refused until credentials are sent.
    #[schema(example = "s3://my-bucket/tessera")]
    pub url: String,
    /// Backend-specific options, passed to `object_store` unchanged. Omit to
    /// keep the stored ones; `{}` clears them.
    #[serde(default)]
    pub options: Option<std::collections::BTreeMap<String, String>>,
    /// How much it may hold. Omit or send `null` for no limit.
    #[serde(default)]
    #[schema(example = 10_737_418_240_u64)]
    pub capacity_bytes: Option<u64>,
    /// Whether it should stop taking new content.
    #[serde(default)]
    pub read_only: bool,
    /// Whether browsers may be redirected to it directly.
    #[serde(default)]
    pub direct_access: bool,
}

/// Body of `PATCH /admin/storage/backends/{name}`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateBackendRequest {
    /// Whether browsers may be redirected to it directly.
    pub direct_access: bool,
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

/// Adds a backend. A name that is already taken is refused.
///
/// The new backend is open and usable from the next request. The pool is
/// rebuilt now, not at the next restart, since avoiding "add a bucket, then
/// restart" is the point of this feature.
///
/// It is asked whether it answers before the response, which says so in
/// `reachable` and `last_error`. One that does not is stored anyway: it may
/// not be up yet. Only a URL that cannot be a store at all is refused.
///
/// # Errors
///
/// `400` when the URL will not open, `401` when nothing was signed in, `403`
/// when the caller may not write settings, `404` when this instance has no
/// administration, `409` when the name is taken.
#[utoipa::path(
    post,
    path = "/admin/storage/backends",
    operation_id = "add_storage_backend",
    tag = ADMIN_TAG,
    request_body = BackendRequest,
    responses(
        (status = 200, description = "The backend as it now stands", body = BackendResponse),
        (status = 400, description = "The URL will not open", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not write settings", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no administration", body = crate::error::ErrorBody),
        (status = 409, description = "A backend with that name already exists", body = crate::error::ErrorBody),
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

    let backend = StorageBackend {
        name: body.name.clone(),
        url: body.url,
        options: body.options,
        // A new row never takes the flag by being written; that is its own call.
        is_default: false,
        capacity_bytes: body.capacity_bytes,
        read_only: body.read_only,
        direct_access: body.direct_access,
    };

    // Opened before it is stored. A backend that will not open is a typo the
    // operator can fix with the form still in front of them; storing it first
    // surfaces the failure later, as a broken upload.
    open_or_refuse(&backend)?;

    state
        .storage_backends
        .create(&backend, OffsetDateTime::now_utc())
        .await
        .map_err(describe_failure)?;
    reload(&state).await?;
    check(&state, &backend).await;
    one(&state, &body.name).await
}

/// Replaces a backend's location, credentials and limits.
///
/// Credentials left out are kept when the backend stays on the same store (see
/// [`ReplaceBackendRequest::url`]). Which backend is the default is left
/// alone. Files already written stay where they were: moving the location does
/// not move them, so this is for a store whose contents were copied first.
///
/// Asked whether it answers before the response, as when it was added.
///
/// # Errors
///
/// `400` when the URL will not open or moved to another store without
/// credentials, `401` when nothing was signed in, `403`
/// when the caller may not write settings, `404` when no backend has that name.
#[utoipa::path(
    put,
    path = "/admin/storage/backends/{name}",
    operation_id = "replace_storage_backend",
    tag = ADMIN_TAG,
    params(("name" = String, Path, description = "Which backend")),
    request_body = ReplaceBackendRequest,
    responses(
        (status = 200, description = "The backend as it now stands", body = BackendResponse),
        (status = 400, description = "The URL will not open, or moved to another store without credentials", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not write settings", body = crate::error::ErrorBody),
        (status = 404, description = "No such backend", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn replace(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(name): ApiPath<String>,
    ApiJson(body): ApiJson<ReplaceBackendRequest>,
) -> Result<Json<BackendResponse>> {
    require_admin_feature(&state)?;
    admitted(&current, tessera_core::Permission::SettingsWrite)?;

    let pool = state.backends.load();
    let stored = pool.row(&name).ok_or(Error::NotFound)?;
    let is_default = stored.is_default;
    let (url, options) =
        location::carry_credentials(&stored.url, &stored.options, body.url, body.options).map_err(
            |location::CredentialsRequired| Error::new(ErrorCode::BackendCredentialsRequired),
        )?;
    drop(pool);
    let backend = StorageBackend {
        name: name.clone(),
        url,
        options,
        is_default,
        capacity_bytes: body.capacity_bytes,
        read_only: body.read_only,
        direct_access: body.direct_access,
    };
    open_or_refuse(&backend)?;

    if !state
        .storage_backends
        .replace(&backend, OffsetDateTime::now_utc())
        .await
        .map_err(describe_failure)?
    {
        return Err(Error::NotFound);
    }
    reload(&state).await?;
    check(&state, &backend).await;
    one(&state, &name).await
}

/// Removes a backend.
///
/// Only forgets the backend: its row goes and the pool is rebuilt without it.
/// Nothing is deleted from the bucket or directory behind it, and a backend
/// that still holds a file refuses with `409`, so removal never strands one.
///
/// The caller proves it is the account's owner first, by password or by a
/// two-step code (see [`ConfirmOwnerRequest`]): a session left open on a desk
/// should not be enough to unplug the place files are kept.
///
/// # Errors
///
/// `401` when nothing was signed in or the password or code is wrong, `403`
/// when the caller may not write settings or used an API token, `404` when no
/// backend has that name, `409` when it still holds content, `429` when too
/// many codes have been wrong.
#[utoipa::path(
    delete,
    path = "/admin/storage/backends/{name}",
    operation_id = "remove_storage_backend",
    tag = ADMIN_TAG,
    params(("name" = String, Path, description = "Which backend")),
    request_body = ConfirmOwnerRequest,
    responses(
        (status = 204, description = "Removed", body = ()),
        (status = 401, description = "Not signed in, or the password or code is wrong", body = crate::error::ErrorBody),
        (status = 403, description = "May not write settings", body = crate::error::ErrorBody),
        (status = 404, description = "No such backend", body = crate::error::ErrorBody),
        (status = 409, description = "It still holds content; move it off first", body = crate::error::ErrorBody),
        (status = 429, description = "Too many wrong codes", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn remove(
    State(state): State<AppState>,
    ClientAddress(origin): ClientAddress,
    current: CurrentUser,
    ApiPath(name): ApiPath<String>,
    ApiJson(body): ApiJson<ConfirmOwnerRequest>,
) -> Result<StatusCode> {
    require_admin_feature(&state)?;
    admitted(&current, tessera_core::Permission::SettingsWrite)?;
    confirm_owner(&state, &origin, &current, body).await?;

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

/// Turns direct access on or off for a backend.
///
/// # Why not another `POST` of the whole backend
///
/// That works and replaces the row, but it needs the URL and credentials sent
/// again. Those are sealed and never shown, so flipping one switch would mean
/// digging up the bucket's secret key again.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the caller may not write
/// settings, `404` when no backend has that name.
#[utoipa::path(
    patch,
    path = "/admin/storage/backends/{name}",
    operation_id = "update_storage_backend",
    tag = ADMIN_TAG,
    params(("name" = String, Path, description = "Which backend")),
    request_body = UpdateBackendRequest,
    responses(
        (status = 200, description = "The backend as it now stands", body = BackendResponse),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not write settings", body = crate::error::ErrorBody),
        (status = 404, description = "No such backend", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn update(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(name): ApiPath<String>,
    ApiJson(body): ApiJson<UpdateBackendRequest>,
) -> Result<Json<BackendResponse>> {
    require_admin_feature(&state)?;
    admitted(&current, tessera_core::Permission::SettingsWrite)?;

    if !state
        .storage_backends
        .set_direct_access(&name, body.direct_access, OffsetDateTime::now_utc())
        .await
        .map_err(describe_failure)?
    {
        return Err(Error::NotFound);
    }
    reload(&state).await?;
    one(&state, &name).await
}

/// Sends new uploads to one backend.
///
/// Existing files stay where they are, since each records its own backend, so
/// this changes what happens next and nothing about what already happened.
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

    if !state
        .storage_backends
        .set_default(&name, OffsetDateTime::now_utc())
        .await
        .map_err(describe_failure)?
    {
        return Err(Error::NotFound);
    }
    reload(&state).await?;
    one(&state, &name).await
}

/// Proves the backend opens before it is written down.
fn open_or_refuse(backend: &StorageBackend) -> Result<()> {
    crate::storage::build(&crate::storage::Target {
        url: backend.url.clone(),
        options: backend.options.clone(),
    })
    .map(|_| ())
    .map_err(|error| {
        Error::new(ErrorCode::BackendUnreachable)
            .with_message(format!("that backend will not open: {error}"))
    })
}

/// Asks a backend just written whether it answers, so the response says.
///
/// A failure is shown, not refused: the store may simply not be up yet, and
/// the row is what the operator meant either way.
async fn check(state: &AppState, backend: &StorageBackend) {
    let pool = state.backends.load_full();
    let Some(stores) = pool.open.get(&backend.name) else {
        return;
    };
    if let Err(error) = crate::storage::health::check(
        &pool.health,
        &backend.name,
        &stores.store,
        !backend.read_only,
    )
    .await
    {
        tracing::warn!(
            backend = backend.name,
            error,
            "a storage backend just written is not answering"
        );
    }
}

/// Rebuilds the pool so a change is live for the next request.
///
/// Why these are stored and not configured: an operator
/// adding a bucket should not have to restart the instance to use it.
async fn reload(state: &AppState) -> Result<()> {
    let configured = state
        .storage_backends
        .all()
        .await
        .map_err(describe_failure)?;
    let health = state.backends.load().health.clone();
    let open = crate::storage::build_all(&configured, &health);
    state.replace_backends(crate::storage::Pool {
        open,
        configured,
        health,
    });
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
/// shape they just changed, not a second description that could drift
/// from the one the listing gives.
async fn describe_all(state: &AppState) -> Result<Json<Vec<BackendResponse>>> {
    let used = state.shares.bytes_per_backend().await?;
    let pool = state.backends.load();
    let taken = |name: &str| used.get(name).copied().unwrap_or(0);
    let seen = |name: &str| pool.health.get(name);

    let listed = pool.configured.iter().map(|backend| BackendResponse {
        name: backend.name.clone(),
        url: location::redacted(&backend.url),
        access_key: location::masked_access_key(&backend.url, &backend.options),
        config: StoreConfig::of(&backend.options),
        is_default: backend.is_default,
        capacity_bytes: backend.capacity_bytes.filter(|bytes| *bytes > 0),
        used_bytes: taken(&backend.name),
        read_only: backend.read_only,
        reachable: pool.open.contains_key(&backend.name)
            && seen(&backend.name).is_none_or(|seen| seen.error.is_none()),
        last_checked_at: seen(&backend.name).map(|seen| seen.at),
        last_error: seen(&backend.name).and_then(|seen| seen.error),
        direct_access: backend.direct_access,
    });
    Ok(Json(listed.collect()))
}

/// Turns a repository failure into the answer the caller should get.
///
/// The two that are not internal errors are the two an operator can act on: a
/// stored credential the key cannot open, and a backend that still holds
/// files.
fn describe_failure(error: StorageBackendError) -> Error {
    match error {
        StorageBackendError::Secret(inner) => {
            Error::new(ErrorCode::BackendCredentialUnreadable).with_message(inner.to_string())
        }
        StorageBackendError::NameTaken { .. } => {
            Error::new(ErrorCode::BackendNameTaken).with_message(error.to_string())
        }
        StorageBackendError::StillInUse { .. } => {
            Error::new(ErrorCode::BackendInUse).with_message(error.to_string())
        }
        // A malformed row or a database failure is ours, not the caller's.
        other => {
            tracing::error!(error = %other, "a storage backend could not be read or written");
            Error::Unavailable
        }
    }
}
