//! Uploads that survive a dropped connection.
//!
//! # The protocol
//!
//! tus's offset model, which is the one every resumable-upload client already
//! speaks:
//!
//! ```text
//! POST  /uploads         Upload-Length: N   → 201 + Location
//! HEAD  /uploads/{id}                       → 200 + Upload-Offset
//! PATCH /uploads/{id}    Upload-Offset: k   → 204 + Upload-Offset
//! ```
//!
//! Three rules from the specification are load-bearing rather than decorative:
//!
//! - A `PATCH` whose `Upload-Offset` does not match answers **409 and changes nothing**. Without it
//!   two clients writing the same upload interleave, and the file that comes out is neither of
//!   theirs.
//! - `HEAD` must answer `Cache-Control: no-store`. A cached offset is a corrupted upload, and it is
//!   a proxy rather than the client that would cache it.
//! - The last part is the only one allowed to be short. That lives in [`crate::upload::sink`],
//!   because it is arithmetic and belongs with a test.
//!
//! # What a half-finished `PATCH` does
//!
//! A chunk that does not end on a part boundary leaves bytes that cannot become
//! a part yet. tusd parks them in a sibling object and prepends them on resume;
//! this does not, and answers with the offset of the last **whole** part
//! instead. The client re-sends the tail, which the protocol already expects of
//! it — the offset a server reports is what the server accepted.
//!
//! The cost is bounded: at most one part is re-sent, and only when a transfer
//! was interrupted. What it buys is that there is no second object with its own
//! lifetime to create, find, prepend and clean up. The final chunk is exempt,
//! because there the remainder *is* the last part and is allowed to be short.
//!
//! One consequence has to be answered rather than absorbed: a client that
//! *always* sends less than a part would advance nothing and re-send from the
//! same offset forever. So a request that was not the last and moved the offset
//! by nothing is refused, naming the size it needed. A tus client left at its
//! default sends the whole file in one `PATCH` and never meets this; one
//! configured with a small chunk size does, and gets told so instead of
//! looping. Only a multipart store has the floor at all — a local file takes
//! whatever arrives.
//!
//! # Why the destination is chosen at creation
//!
//! `?code=` says "append to that share when this finishes"; leaving it off says
//! "make a new one". Deciding here rather than at completion means the finish
//! step needs nothing the client has to have remembered — the last `PATCH`
//! already knows what to do.
//!
//! # Who may continue one
//!
//! An account continues its own uploads with the credential it already has. An
//! anonymous uploader is handed a token at creation and presents it after,
//! which is the same shape as the management token an anonymous *share* gets.
//!
//! tusd, the reference implementation, has no answer here at all — its
//! documentation says so: the upload URL is random and that is the whole of it.
//! A URL is not a good secret, though, because URLs end up in access logs, in
//! proxy logs and in `Referer`; a header does not. So the identifier stays
//! guessable-in-principle and the token is what actually authorises.

// `params(CreateUploadParams)` makes utoipa emit a fully-qualified path to the
// type, and the generated code lands outside the function it decorates — so the
// suppression is module-wide, the same as in `shares`.
#![expect(
    unused_qualifications,
    reason = "emitted by utoipa's params(..), not written here"
)]

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tessera_core::{Permission, ShareId, UploadId};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};

use crate::{
    api::{
        extract::{ApiPath, ApiQuery},
        shares::{ShareResponse, object_key},
    },
    auth::{extract::MaybeUser, secret::Secret},
    db::repository::{
        share::{NewFile, StorageLocation},
        upload::{NewUpload, Upload},
    },
    error::{Error, Result},
    state::AppState,
    upload::sink::{Sink, split_into_parts},
};

/// Tag the uploads endpoints appear under.
pub const UPLOADS_TAG: &str = "uploads";

/// Header carrying how much has arrived, in both directions.
const UPLOAD_OFFSET: &str = "upload-offset";
/// Header carrying the total, on creation.
const UPLOAD_LENGTH: &str = "upload-length";
/// Header an anonymous uploader presents to continue.
const UPLOAD_TOKEN: &str = "x-upload-token";
/// The only body type a `PATCH` may carry, per the specification.
const OFFSET_CONTENT_TYPE: &str = "application/offset+octet-stream";

/// Where a finished upload should land.
#[derive(Debug, Deserialize, IntoParams)]
pub struct CreateUploadParams {
    /// What to call the file.
    #[param(example = "holiday.mp4")]
    pub filename: Option<String>,
    /// Retrieval code of the share to append to.
    ///
    /// Omit to make a new share when the upload finishes.
    #[param(example = "4GSVV6QT")]
    pub code: Option<String>,
}

/// A session that has just been opened.
#[derive(Debug, Serialize, ToSchema)]
pub struct UploadResponse {
    /// Identifier, and the last segment of the URL to continue at.
    pub id: String,
    /// How much has arrived. Zero, here.
    pub offset: u64,
    /// How much was declared.
    pub length: u64,
    /// How much to send per chunk, in bytes.
    ///
    /// A chunk that is not the last is accepted only up to a multiple of this;
    /// the rest comes back as a lower offset and has to be re-sent. Sending
    /// exact multiples avoids that entirely.
    pub part_size: u64,
    /// Proof that a later request is the same uploader, for an anonymous one.
    ///
    /// **Store this.** It is shown once, and without it the upload cannot be
    /// resumed after the page is closed — which is the situation resuming
    /// exists for. `null` when the uploader is signed in, because their own
    /// credential already answers the question.
    pub token: Option<String>,
}

/// Opens a session.
///
/// # Errors
///
/// `400` when `Upload-Length` is missing or unusable, `403` when the caller may
/// not upload, `404` when `code` names no share, `507` when the declared length
/// does not fit in what is left of the instance.
#[utoipa::path(
    post,
    path = "/uploads",
    operation_id = "create_upload",
    tag = UPLOADS_TAG,
    params(CreateUploadParams),
    responses(
        (status = 201, description = "The session", body = UploadResponse),
        (status = 400, description = "`Upload-Length` missing or unusable", body = crate::error::ErrorBody),
        (status = 403, description = "Not allowed to upload", body = crate::error::ErrorBody),
        (status = 404, description = "No share has that code", body = crate::error::ErrorBody),
        (status = 507, description = "This instance has no room for it", body = crate::error::ErrorBody),
    ),
    security((), ("session" = []), ("bearer" = [])),
)]
pub async fn create(
    State(state): State<AppState>,
    MaybeUser(current): MaybeUser,
    ApiQuery(params): ApiQuery<CreateUploadParams>,
    headers: HeaderMap,
) -> Result<Response> {
    if current.is_none() && !state.config.features.anonymous_upload {
        return Err(Error::Unauthorized);
    }
    if let Some(user) = current.as_ref() {
        user.require(Permission::SharesCreate)?;
    }

    let declared = declared_length(&headers)?;
    // Checked before a byte is accepted rather than as they arrive: an upload
    // that cannot possibly fit should be refused while the client can still act
    // on it, not after it has spent an hour sending.
    if declared > crate::api::shares::remaining_room(&state).await? {
        return Err(Error::InsufficientStorage);
    }

    let share_id = match params.code.as_deref() {
        Some(code) => Some(resolve_target(&state, code, current.as_ref(), &headers).await?),
        None => None,
    };

    let now = OffsetDateTime::now_utc();
    // The key is decided here and never recomputed. Deriving it per request
    // from the current date is `vastsa/FileCodeBox#362`: an upload resumed the
    // next day wrote somewhere else, and the merge could not find yesterday.
    let key = object_key(ShareId::generate(), now);

    let sink = Sink::open(
        &state.storage,
        state.multipart.as_ref().map(|handle| &handle.0),
        local_root(&state).as_ref(),
        &key,
    )
    .await?;
    let multipart_id = match &sink {
        Sink::Multipart { id, .. } => Some(id.clone()),
        Sink::Appending { .. } => None,
    };

    let secret = Secret::generate_api_token();
    let upload = state
        .uploads
        .create(
            &NewUpload {
                share_id,
                user_id: current.as_ref().map(|user| user.user.id),
                token_hash: secret.digest(),
                filename: crate::api::shares::checked_filename(params.filename)?,
                content_type: None,
                declared_bytes: declared,
                object_key: key,
                multipart_id,
            },
            now,
            now + state.config.share.upload_session_ttl(),
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, format!("/api/v1/uploads/{}", upload.id))],
        axum::Json(UploadResponse {
            id: upload.id.to_string(),
            offset: 0,
            length: declared,
            part_size: u64::try_from(state.config.server.part_size_bytes).unwrap_or(u64::MAX),
            token: current.is_none().then(|| secret.expose().to_owned()),
        }),
    )
        .into_response())
}

/// Answers how much has arrived.
///
/// # Errors
///
/// `404` when there is no such session, or the caller may not continue it —
/// the same answer, so that a stranger cannot tell one from the other.
#[utoipa::path(
    head,
    path = "/uploads/{id}",
    operation_id = "upload_offset",
    tag = UPLOADS_TAG,
    params(("id" = String, Path, description = "The session")),
    // No bodies declared: a `HEAD` response never has one, so promising an
    // error body here would be the contract saying something the protocol
    // forbids — and a contract test is right to call that a lie.
    responses(
        (status = 200, description = "`Upload-Offset` says how much has arrived"),
        (status = 404, description = "No such session"),
        (status = 410, description = "That session was abandoned"),
    ),
    security((), ("session" = []), ("bearer" = [])),
)]
pub async fn offset(
    State(state): State<AppState>,
    MaybeUser(current): MaybeUser,
    ApiPath(id): ApiPath<String>,
    headers: HeaderMap,
) -> Result<Response> {
    let upload = continuable(&state, &id, current.as_ref(), &headers).await?;
    Ok(offset_headers(&upload).into_response())
}

/// Applies bytes at the given offset.
///
/// # Errors
///
/// `409` when the offset does not match, `404` when there is no such session,
/// `415` when the content type is not the one the specification names.
#[utoipa::path(
    patch,
    path = "/uploads/{id}",
    operation_id = "append_to_upload",
    tag = UPLOADS_TAG,
    params(("id" = String, Path, description = "The session")),
    request_body(content = Vec<u8>, description = "The next bytes", content_type = "application/offset+octet-stream"),
    responses(
        (status = 204, description = "Accepted; `Upload-Offset` says the new total"),
        (status = 201, description = "That was the last of it", body = ShareResponse),
        (status = 404, description = "No such session", body = crate::error::ErrorBody),
        (status = 409, description = "The offset does not match", body = crate::error::ErrorBody),
        (status = 415, description = "Wrong content type", body = crate::error::ErrorBody),
    ),
    security((), ("session" = []), ("bearer" = [])),
)]
pub async fn append(
    State(state): State<AppState>,
    MaybeUser(current): MaybeUser,
    ApiPath(id): ApiPath<String>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response> {
    if content_type(&headers).as_deref() != Some(OFFSET_CONTENT_TYPE) {
        return Err(Error::UnsupportedMediaType);
    }
    let upload = continuable(&state, &id, current.as_ref(), &headers).await?;

    // The specification is explicit that a mismatch changes nothing, which is
    // what keeps two clients from interleaving into a file that is neither of
    // theirs.
    if header_number(&headers, UPLOAD_OFFSET) != Some(upload.offset_bytes) {
        return Err(Error::Conflict(format!(
            "this upload is at {}",
            upload.offset_bytes
        )));
    }

    let part_size = state.config.server.part_size_bytes;
    let stall = state.config.server.upload_stall();
    let sink = sink_for(&state, &upload)?;

    let mut pending: Vec<u8> = Vec::new();
    let mut offset = upload.offset_bytes;
    let mut taken = state.uploads.parts_of(upload.id).await?.len();
    let mut body = body;

    while let Some(chunk) = crate::api::shares::next_chunk(&mut body, stall).await? {
        if offset
            .saturating_add(u64::try_from(pending.len()).unwrap_or(u64::MAX))
            .saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX))
            > upload.declared_bytes
        {
            return Err(Error::bad_request("that is more than this upload declared"));
        }
        pending.extend_from_slice(&chunk);

        // Flushed as it arrives rather than at the end, so the memory an
        // upload holds is one part and not one file.
        let is_final = offset.saturating_add(u64::try_from(pending.len()).unwrap_or(u64::MAX))
            == upload.declared_bytes;
        // A local file takes whatever arrives; only a multipart store needs
        // whole blocks. See `Sink::buffers`.
        let (parts, rest) = if sink.buffers() {
            split_into_parts(&pending, part_size, is_final)
        } else if pending.is_empty() {
            (Vec::new(), Vec::new())
        } else {
            (
                vec![bytes::Bytes::from(std::mem::take(&mut pending))],
                Vec::new(),
            )
        };
        if !parts.is_empty() {
            let written = sink.write(&upload.object_key, taken, parts).await?;
            offset = offset.saturating_add(written.accepted);
            taken += written.parts.len();
            state
                .uploads
                .advance(upload.id, offset, &written.parts, OffsetDateTime::now_utc())
                .await?;
        }
        pending = rest;
    }

    // A chunk too short to fill a part advances nothing, and a client that
    // keeps sending the same short chunk would never finish — it would re-send
    // from the same offset forever, and nothing would say why. So a request
    // that was not the last and moved nothing is refused with the number it
    // needed, which is also what the `POST` already told it.
    if sink.buffers() && offset == upload.offset_bytes && offset < upload.declared_bytes {
        return Err(Error::bad_request(format!(
            "a chunk that is not the last must carry at least {part_size} bytes"
        )));
    }

    // Whatever did not fill a part is not recorded, so the client re-sends it.
    // See the module docs for why that beats parking it somewhere.
    if offset < upload.declared_bytes {
        let mut refreshed = upload;
        refreshed.offset_bytes = offset;
        return Ok((StatusCode::NO_CONTENT, offset_headers(&refreshed)).into_response());
    }

    finish(&state, upload, offset).await
}

/// Turns a finished upload into a file, and answers with the share.
///
/// The session row goes in the same breath: it has nothing left to describe,
/// and leaving it would make the sweep abort a multipart upload that has
/// already completed.
async fn finish(state: &AppState, upload: Upload, size_bytes: u64) -> Result<Response> {
    let sink = sink_for(state, &upload)?;
    let parts = state.uploads.parts_of(upload.id).await?;
    sink.complete(&upload.object_key, parts).await?;

    let mut finished = upload.clone();
    finished.offset_bytes = size_bytes;
    let file = finished_file(&finished);
    let now = OffsetDateTime::now_utc();

    let response = match upload.share_id {
        Some(share_id) => {
            state.shares.append_file(share_id, &file, now).await?;
            crate::api::shares::describe_for_owner(state, share_id).await?
        }
        None => crate::api::shares::create_from_file(state, upload.user_id, file, now).await?,
    };

    state.uploads.delete(upload.id).await?;
    Ok((StatusCode::CREATED, axum::Json(response)).into_response())
}

/// Reads `Upload-Length`, which is how much the client says it will send.
fn declared_length(headers: &HeaderMap) -> Result<u64> {
    header_number(headers, UPLOAD_LENGTH)
        .ok_or_else(|| Error::bad_request("`Upload-Length` must be a number of bytes"))
}

fn header_number(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse().ok())
}

fn content_type(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(';')
                .next()
                .unwrap_or(value)
                .trim()
                .to_ascii_lowercase()
        })
}

/// The session, if this caller may continue it.
///
/// Answers `404` rather than `403` when they may not: a stranger probing
/// identifiers learns nothing from "that exists but is not yours".
async fn continuable(
    state: &AppState,
    id: &str,
    current: Option<&crate::auth::extract::CurrentUser>,
    headers: &HeaderMap,
) -> Result<Upload> {
    let id = UploadId::parse(id).map_err(|_| Error::NotFound)?;
    let upload = state.uploads.find(id).await?.ok_or(Error::NotFound)?;

    if upload.expires_at <= OffsetDateTime::now_utc() {
        return Err(Error::Gone("that upload was abandoned".to_owned()));
    }

    let owns = match (upload.user_id, current) {
        (Some(owner), Some(user)) => owner == user.user.id,
        _ => false,
    };
    let holds_token = headers
        .get(UPLOAD_TOKEN)
        .and_then(|value| value.to_str().ok())
        .map(Secret::from_client)
        .is_some_and(|presented| presented.digest() == upload.token_hash);

    if owns || holds_token {
        Ok(upload)
    } else {
        Err(Error::NotFound)
    }
}

/// The `HEAD` answer, which is also what a `PATCH` returns.
///
/// `no-store` is not politeness: a cached offset sends the next chunk to the
/// wrong place, and it is a proxy rather than the client that would cache it.
fn offset_headers(upload: &Upload) -> impl IntoResponse {
    (
        StatusCode::OK,
        [
            (
                header::HeaderName::from_static(UPLOAD_OFFSET),
                HeaderValue::from(upload.offset_bytes),
            ),
            (
                header::HeaderName::from_static(UPLOAD_LENGTH),
                HeaderValue::from(upload.declared_bytes),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
    )
}

/// The share `code` names, if this caller may add to it.
///
/// The same two proofs `shares::add_file` accepts, and the same `404` when
/// neither holds — otherwise this endpoint answers which codes exist.
async fn resolve_target(
    state: &AppState,
    code: &str,
    current: Option<&crate::auth::extract::CurrentUser>,
    headers: &HeaderMap,
) -> Result<ShareId> {
    let code = tessera_core::RetrievalCode::parse(code).map_err(|_| Error::NotFound)?;
    let share = state
        .shares
        .find_by_code(&code)
        .await?
        .ok_or(Error::NotFound)?;

    let owns = current.is_some_and(|user| {
        share.user_id == Some(user.user.id) && user.permissions.allows(Permission::SharesCreate)
    });
    let holds_token = share.management_token_hash.as_ref().is_some_and(|stored| {
        headers
            .get("x-share-management-token")
            .and_then(|value| value.to_str().ok())
            .map(Secret::from_client)
            .is_some_and(|presented| presented.digest() == *stored)
    });
    if !owns && !holds_token {
        return Err(Error::NotFound);
    }
    if share.kind == crate::db::repository::share::ShareKind::Text {
        return Err(Error::Conflict(
            "that share holds text; a text share holds one thing".to_owned(),
        ));
    }
    Ok(share.id)
}

/// The sink for a session, or the error a caller should answer with.
fn sink_for(state: &AppState, upload: &Upload) -> Result<Sink> {
    crate::upload::reopen(state, upload)
        .ok_or_else(|| Error::bad_request("this instance's store cannot continue that upload"))
}

/// Where partial files live when the store is a local filesystem.
///
/// `None` when it is not one, in which case there is a multipart handle to use
/// instead. The two are exclusive by construction — see `storage`.
fn local_root(state: &AppState) -> Option<std::path::PathBuf> {
    crate::storage::local_path(&state.config.storage.url).map(std::path::PathBuf::from)
}

/// A `NewFile` for a finished upload.
pub(crate) fn finished_file(upload: &Upload) -> NewFile {
    NewFile {
        filename: upload.filename.clone(),
        content_type: upload.content_type.clone(),
        size_bytes: upload.offset_bytes,
        storage: StorageLocation::Object {
            key: upload.object_key.clone(),
        },
    }
}
