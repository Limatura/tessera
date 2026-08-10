//! Creating, listing and deleting shares.
//!
//! # How content arrives
//!
//! The body *is* the content, and the metadata comes from query parameters. One
//! rule, and it works for everything that has to post here:
//!
//! ```text
//! curl --data-binary @project.zip 'http://host/api/v1/shares?filename=project.zip'
//! fetch(url, { method: 'POST', body: file })   // the browser streams a File
//! ```
//!
//! `multipart/form-data` is deliberately not accepted yet. A `fetch` with a
//! `File` body streams natively, so the only caller that would need multipart is
//! a plain HTML `<form>` — and supporting one would add a parser, a dependency
//! and a buffering path for a case nothing in this project has.

// `params(CreateShareParams)` makes utoipa emit a fully-qualified path to the
// type. The generated code lands outside the function it decorates, so the
// suppression has to be module-wide; dropping `params(..)` instead would take
// the query parameters out of the contract, which is the point of declaring
// them.
#![expect(
    unused_qualifications,
    reason = "emitted by utoipa's params(..), not written here"
)]

use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use http_body_util::BodyExt as _;
use object_store::{MultipartUpload, ObjectStoreExt as _, PutPayload, path::Path as ObjectPath};
use serde::{Deserialize, Serialize};
use tessera_core::{Password, PasswordPolicy, RetrievalCode, ShareId};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};

use crate::{
    api::SHARES_TAG,
    auth::{
        extract::{CurrentUser, MaybeUser},
        password,
        secret::Secret,
    },
    db::repository::share::{Share, ShareKind, StorageLocation},
    error::{Error, Result},
    state::AppState,
};

/// Bytes accumulated before a part is pushed to the blob store.
///
/// Most object stores require every part but the last to be at least 5 MiB, and
/// some require them all to be the same size, so the block size is fixed rather
/// than following whatever chunk sizes the network happened to deliver.
///
/// **This is the memory an in-flight upload holds** — one block per concurrent
/// upload, not one per connection, because a block is flushed as soon as it
/// fills.
const PART_SIZE: usize = 8 * 1024 * 1024;

/// Attempts to find an unused retrieval code before giving up.
///
/// Collisions are vanishingly rare at the default length, so more than a couple
/// of failures means the code space is genuinely crowded and the operator needs
/// to hear about it rather than have the server keep trying.
const CODE_ATTEMPTS: u8 = 8;

/// How short a share's password may be.
///
/// Lower than an account's minimum on purpose. A share password is a second
/// factor on a link that is already unguessable, and it gets read out over the
/// phone — a four-digit code is a reasonable thing to want. An account password
/// is the only thing between a stranger and the account, which is a different
/// job.
const SHARE_PASSWORD_POLICY: PasswordPolicy = PasswordPolicy {
    min_length: 4,
    max_length: PasswordPolicy::CEILING,
};

/// Metadata for `POST /shares`, supplied as query parameters.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct CreateShareParams {
    /// Original file name. Its presence is what makes this a file share rather
    /// than a note.
    #[param(example = "project.zip")]
    pub filename: Option<String>,
    /// Display name, shown on the share page and in the link preview.
    #[param(example = "Q3 report")]
    pub title: Option<String>,
    /// Syntax-highlighting hint for a text share.
    #[param(example = "typescript")]
    pub language: Option<String>,
    /// How long the share should last. Omit to use the instance's default.
    #[param(example = 24)]
    pub expires_in_hours: Option<u64>,
    /// How many times it may be retrieved. Omit for unlimited; `1` is
    /// burn-after-reading.
    #[param(example = 3)]
    pub max_uses: Option<u32>,
}

/// A share as the API describes it. Never carries the content.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ShareResponse {
    /// Stable identifier.
    #[schema(example = "01K1ZQK9V8QF5S3T4M2N6P7R8S")]
    pub id: String,
    /// The retrieval code, as a person reads it.
    #[schema(example = "0ZPQ4M7K")]
    pub code: String,
    /// Where the share lives, relative to this instance's origin.
    ///
    /// Relative on purpose: the server does not know what host it is reached
    /// through, and guessing wrong behind a reverse proxy produces links that
    /// do not work. Whoever calls this already knows the origin they called.
    #[schema(example = "/s/0ZPQ4M7K")]
    pub path: String,
    /// `text` or `file`.
    #[schema(example = "file")]
    pub kind: String,
    /// Display name.
    pub title: Option<String>,
    /// Original file name.
    pub filename: Option<String>,
    /// Syntax-highlighting hint.
    pub language: Option<String>,
    /// Size of the content in bytes.
    #[schema(example = 1_048_576_u64)]
    pub size_bytes: u64,
    /// Whether a password must be presented to retrieve it.
    pub has_password: bool,
    /// How many retrievals remain, or `null` for unlimited.
    pub uses_remaining: Option<u32>,
    /// How many times it has been retrieved.
    pub use_count: u32,
    /// When it stops being served, RFC 3339, or `null` to live until deleted.
    pub expires_at: Option<String>,
    /// When it was created, RFC 3339.
    pub created_at: String,

    /// Token that lets an anonymous uploader delete this share.
    ///
    /// **Shown once, at creation, and only when nobody was signed in.** Only its
    /// digest is stored. Without it a mistaken anonymous upload could never be
    /// taken back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub management_token: Option<String>,
}

impl ShareResponse {
    fn from_share(share: &Share, management_token: Option<String>) -> Self {
        Self {
            id: share.id.to_string(),
            code: share.code.as_str().to_owned(),
            path: format!("/s/{}", share.code),
            kind: share.kind.as_str().to_owned(),
            title: share.title.clone(),
            filename: share.filename.clone(),
            language: share.language.clone(),
            size_bytes: share.size_bytes,
            has_password: share.needs_password(),
            uses_remaining: share
                .use_limit
                .map(|limit| limit.saturating_sub(share.use_count)),
            use_count: share.use_count,
            expires_at: share.expires_at.map(format_timestamp),
            created_at: format_timestamp(share.created_at),
            management_token,
        }
    }
}

fn format_timestamp(value: OffsetDateTime) -> String {
    value
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| value.unix_timestamp().to_string())
}

/// Creates a share from the request body.
///
/// # Errors
///
/// `401` when the instance requires an account to upload, `400` when a parameter
/// is unusable, `413` when the body exceeds the configured limit, `409` when no
/// unused code could be found.
#[utoipa::path(
    post,
    path = "/shares",
    tag = SHARES_TAG,
    params(CreateShareParams),
    request_body(
        content = Vec<u8>,
        description = "The content itself. Send the file as the raw body — no wrapper, no encoding.",
        content_type = "application/octet-stream",
    ),
    responses(
        (status = 201, description = "Created", body = ShareResponse),
        (status = 400, description = "A parameter is unusable", body = crate::error::ErrorBody),
        (status = 401, description = "This instance requires an account to upload", body = crate::error::ErrorBody),
        (status = 413, description = "Body exceeds the configured limit", body = crate::error::ErrorBody),
    ),
    security((), ("session" = []), ("bearer" = [])),
)]
pub async fn create(
    State(state): State<AppState>,
    MaybeUser(current): MaybeUser,
    Query(params): Query<CreateShareParams>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response> {
    if current.is_none() && !state.config.features.anonymous_upload {
        return Err(Error::Unauthorized);
    }

    let now = OffsetDateTime::now_utc();
    let expires_at = resolve_expiry(&state, params.expires_in_hours, now)?;
    let password_hash = match share_password(&headers)? {
        Some(password) => Some(password::hash(password).await?),
        None => None,
    };

    let id = ShareId::generate();
    let kind = if params.filename.is_some() {
        ShareKind::File
    } else {
        ShareKind::Text
    };

    let Ingested {
        storage,
        size_bytes,
        inline,
    } = ingest(&state, id, now, body).await?;

    // Anonymous uploads get a token so their author can take one back. A signed-in
    // uploader already has one — their account.
    let management_secret = current.is_none().then(Secret::generate_session);

    let share = Share {
        id,
        // Replaced on each attempt below; the id and the object key do not
        // change, so a retry costs one insert rather than a re-upload.
        code: RetrievalCode::parse("0000").unwrap_or_else(|_| unreachable!("four zeroes parse")),
        user_id: current.as_ref().map(|user| user.user.id),
        kind,
        password_hash,
        use_limit: params.max_uses,
        use_count: 0,
        expires_at,
        disabled_at: None,
        title: params.title,
        filename: params.filename,
        content_type: None,
        language: params.language,
        size_bytes,
        storage,
        og_config: None,
        management_token_hash: management_secret.as_ref().map(Secret::digest),
        view_count: 0,
        last_accessed_at: None,
        created_at: now,
        updated_at: now,
    };

    let share = insert_with_fresh_code(&state, share, inline).await?;
    let response = ShareResponse::from_share(
        &share,
        management_secret.map(|secret| secret.expose().to_owned()),
    );
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

/// Lists the caller's shares, newest first.
///
/// # Errors
///
/// `401` when nothing was signed in.
#[utoipa::path(
    get,
    path = "/shares",
    tag = SHARES_TAG,
    responses(
        (status = 200, description = "The caller's shares", body = Vec<ShareResponse>),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
    ),
    security(("session" = []), ("bearer" = [])),
)]
pub async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<ShareResponse>>> {
    let shares = state.shares.list_for_user(current.user.id).await?;
    Ok(Json(
        shares
            .iter()
            .map(|share| ShareResponse::from_share(share, None))
            .collect(),
    ))
}

/// Deletes a share, and whatever it put in the blob store.
///
/// Accepts either the owning account or, for an anonymous share, the management
/// token from creation — sent as `X-Share-Management-Token`.
///
/// # Errors
///
/// `404` when no share of the caller's has that code. Deliberately the same
/// answer as "somebody else's": distinguishing them would confirm which codes
/// exist.
#[utoipa::path(
    delete,
    path = "/shares/{code}",
    tag = SHARES_TAG,
    params(("code" = String, Path, description = "The retrieval code")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "No share of yours has that code", body = crate::error::ErrorBody),
    ),
    security((), ("session" = []), ("bearer" = [])),
)]
pub async fn delete(
    State(state): State<AppState>,
    MaybeUser(current): MaybeUser,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode> {
    let code = RetrievalCode::parse(&code).map_err(|_| Error::NotFound)?;
    let share = state
        .shares
        .find_by_code(&code)
        .await?
        .ok_or(Error::NotFound)?;

    let owns = current
        .as_ref()
        .is_some_and(|user| share.user_id == Some(user.user.id));
    let holds_token = share.management_token_hash.as_ref().is_some_and(|stored| {
        management_token(&headers).is_some_and(|presented| presented.digest() == *stored)
    });
    if !owns && !holds_token {
        return Err(Error::NotFound);
    }

    // The object first. A row without its object is a broken share; an object
    // without its row is an orphan the sweep can still find by listing, so if
    // only one of the two can happen, this is the better order.
    if let StorageLocation::Object { key } = &share.storage {
        state
            .storage
            .delete(&ObjectPath::from(key.as_str()))
            .await?;
    }
    state.shares.delete(share.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Where a share's bytes ended up, and how many there were.
struct Ingested {
    storage: StorageLocation,
    size_bytes: u64,
    inline: Option<Vec<u8>>,
}

/// Reads the body, keeping it in the database if it is small and streaming it to
/// the blob store if it is not.
///
/// # Why it is not `bytes().await`
///
/// Reading the whole body first is one line and costs the body's size in memory.
/// At the default limit that is 100 MiB per upload in flight, which on the small
/// machines this software is aimed at is the difference between working and
/// being killed by the OOM reaper. So the body is consumed a frame at a time:
/// buffered while it might still be small enough to inline, and pushed to the
/// store in fixed blocks once it is not.
///
/// # Why the size limit is enforced here
///
/// `DefaultBodyLimit` is not a limit on the connection — it is a value that
/// *extractors* consult, so `Bytes`, `String` and `Json` respect it and a
/// handler that takes the `Body` and reads frames itself does not. Reading the
/// body by hand means owning the limit by hand; without this check the layer in
/// `api::build` silently does nothing for this route.
async fn ingest(
    state: &AppState,
    id: ShareId,
    now: OffsetDateTime,
    body: Body,
) -> Result<Ingested> {
    let mut upload: Option<Box<dyn MultipartUpload>> = None;
    let result = ingest_into(state, id, now, body, &mut upload).await;

    if let Some(upload) = upload.as_mut() {
        // Only reachable when something failed: the success path takes the
        // upload out of the slot after completing it. Abandoning a multipart
        // upload leaves parts behind that a store will bill for and that nothing
        // else will ever look for, so the failure path has to say so explicitly.
        if let Err(error) = upload.abort().await {
            tracing::warn!(?error, %id, "could not abort an incomplete upload");
        }
    }
    result
}

async fn ingest_into(
    state: &AppState,
    id: ShareId,
    now: OffsetDateTime,
    body: Body,
    upload: &mut Option<Box<dyn MultipartUpload>>,
) -> Result<Ingested> {
    let threshold = state.config.share.inline_threshold_bytes;
    let limit = u64::try_from(state.config.server.max_body_bytes).unwrap_or(u64::MAX);
    let mut body = body;
    let mut buffered: Vec<u8> = Vec::new();
    let mut size_bytes: u64 = 0;
    let mut key: Option<String> = None;

    while let Some(frame) = body.frame().await {
        let frame =
            frame.map_err(|error| Error::bad_request(format!("body ended early: {error}")))?;
        let Ok(chunk) = frame.into_data() else {
            // Trailers carry no content; a body that is only trailers is empty.
            continue;
        };
        size_bytes = size_bytes.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
        if size_bytes > limit {
            return Err(Error::PayloadTooLarge);
        }
        buffered.extend_from_slice(&chunk);

        if upload.is_none() && buffered.len() <= threshold {
            continue;
        }
        // Past the threshold: start the upload the first time, then keep the
        // buffer drained in whole blocks.
        if upload.is_none() {
            let object_key = object_key(id, now);
            let started = state
                .storage
                .put_multipart(&ObjectPath::from(object_key.as_str()))
                .await?;
            key = Some(object_key);
            *upload = Some(started);
        }
        if let Some(upload) = upload.as_mut() {
            flush_full_parts(upload.as_mut(), &mut buffered).await?;
        }
    }

    match key {
        Some(key) => {
            let mut started = upload.take().ok_or_else(|| {
                Error::bad_request("upload state was lost part-way through the body")
            })?;
            if !buffered.is_empty() {
                started
                    .put_part(PutPayload::from(std::mem::take(&mut buffered)))
                    .await?;
            }
            started.complete().await?;
            Ok(Ingested {
                storage: StorageLocation::Object { key },
                size_bytes,
                inline: None,
            })
        }
        None => Ok(Ingested {
            storage: StorageLocation::Inline,
            size_bytes,
            inline: Some(buffered),
        }),
    }
}

/// Pushes as many whole blocks as the buffer holds, leaving the remainder.
async fn flush_full_parts(upload: &mut dyn MultipartUpload, buffered: &mut Vec<u8>) -> Result<()> {
    while buffered.len() >= PART_SIZE {
        let rest = buffered.split_off(PART_SIZE);
        let part = std::mem::replace(buffered, rest);
        upload.put_part(PutPayload::from(Bytes::from(part))).await?;
    }
    Ok(())
}

/// `{yyyy}/{mm}/{id}`.
///
/// Sharded by date rather than flat: a personal instance running for years would
/// otherwise put every object in one directory, and a local filesystem handles
/// that badly. Sharding on the identifier's leading characters would not help —
/// a ULID starts with a timestamp, so those characters are almost the same for
/// everything written in the same era. Dates are also what an operator wants to
/// see when deciding what to back up or what is old.
fn object_key(id: ShareId, created_at: OffsetDateTime) -> String {
    format!(
        "shares/{:04}/{:02}/{id}",
        created_at.year(),
        u8::from(created_at.month())
    )
}

/// Inserts the share, trying another code each time one is already taken.
async fn insert_with_fresh_code(
    state: &AppState,
    mut share: Share,
    inline: Option<Vec<u8>>,
) -> Result<Share> {
    for _ in 0..CODE_ATTEMPTS {
        share.code = generate_code(state.config.share.code_length)?;
        match state.shares.insert(&share, inline.clone()).await {
            Ok(()) => return Ok(share),
            Err(error) => {
                // Distinguish "that code is taken" from a database that is
                // unwell by looking, rather than assuming every failure is a
                // collision — that would retry through an outage and then
                // report it as a conflict.
                if state.shares.find_by_code(&share.code).await?.is_none() {
                    return Err(error.into());
                }
            }
        }
    }
    Err(Error::Conflict(
        "could not find an unused retrieval code; consider raising share.code_length".to_owned(),
    ))
}

fn generate_code(length: usize) -> Result<RetrievalCode> {
    let mut entropy = vec![0_u8; length];
    crate::auth::secret::fill_random(&mut entropy);
    Ok(RetrievalCode::from_entropy(&entropy)?)
}

/// Reads the optional share password from `X-Share-Password`.
///
/// A header rather than a query parameter: query strings end up in access logs,
/// in shell history and in the `Referer` sent to whatever the page links to
/// next.
fn share_password(headers: &HeaderMap) -> Result<Option<Password>> {
    let Some(raw) = headers
        .get("x-share-password")
        .and_then(|value| value.to_str().ok())
    else {
        return Ok(None);
    };
    Ok(Some(Password::parse(raw, SHARE_PASSWORD_POLICY)?))
}

fn management_token(headers: &HeaderMap) -> Option<Secret> {
    headers
        .get("x-share-management-token")
        .and_then(|value| value.to_str().ok())
        .map(Secret::from_client)
}

/// Turns a requested lifetime into an instant, refusing one beyond the ceiling.
fn resolve_expiry(
    state: &AppState,
    requested_hours: Option<u64>,
    now: OffsetDateTime,
) -> Result<Option<OffsetDateTime>> {
    let ceiling = state.config.share.max_expiry();
    let Some(hours) = requested_hours else {
        // No request: the ceiling doubles as the default, so an instance that
        // sets one never accumulates shares that outlive it.
        return Ok(ceiling.map(|limit| now.saturating_add(duration_of(limit))));
    };

    if hours == 0 {
        return Err(Error::bad_request(
            "`expires_in_hours` must be at least 1; omit it for the instance default",
        ));
    }
    let requested = std::time::Duration::from_secs(hours.saturating_mul(3600));
    if ceiling.is_some_and(|limit| requested > limit) {
        return Err(Error::bad_request(format!(
            "`expires_in_hours` must be at most {}",
            state.config.share.max_expiry_hours
        )));
    }
    Ok(Some(now.saturating_add(duration_of(requested))))
}

/// `std::time::Duration` is unsigned and `time::Duration` is not, so the
/// conversion only fails for a span beyond any calendar. Clamping keeps an
/// absurd configuration from breaking uploads.
fn duration_of(value: std::time::Duration) -> time::Duration {
    time::Duration::try_from(value).unwrap_or(time::Duration::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_keys_are_sharded_by_month() {
        let id = ShareId::generate();
        let at = OffsetDateTime::from_unix_timestamp(1_767_225_600).expect("representable");
        assert_eq!(object_key(id, at), format!("shares/2026/01/{id}"));
    }

    /// A share password is a second factor on an already-unguessable link, so it
    /// is allowed to be the sort of thing people read out over the phone.
    #[test]
    fn a_short_share_password_is_accepted() {
        assert!(Password::parse("1234", SHARE_PASSWORD_POLICY).is_ok());
        assert!(Password::parse("123", SHARE_PASSWORD_POLICY).is_err());
    }

    #[test]
    fn generated_codes_have_the_configured_length() {
        for length in [4, 8, 12] {
            let code = generate_code(length).expect("valid length");
            assert_eq!(code.as_str().chars().count(), length);
        }
    }
}
