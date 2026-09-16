//! Uploads that survive a dropped connection.
//!
//! # The protocol
//!
//! tus's offset model, the one every resumable-upload client already
//! speaks:
//!
//! ```text
//! POST  /uploads         Upload-Length: N   → 201 + Location
//! HEAD  /uploads/{id}                       → 200 + Upload-Offset
//! PATCH /uploads/{id}    Upload-Offset: k   → 204 + Upload-Offset
//! ```
//!
//! Three rules from the specification are load-bearing, not decorative:
//!
//! - A `PATCH` whose `Upload-Offset` does not match answers **409 and changes nothing**. Without it
//!   two clients writing the same upload interleave, and the file that comes out is neither of
//!   theirs.
//! - `HEAD` must answer `Cache-Control: no-store`. A cached offset is a corrupted upload, and it is
//!   a proxy, not the client, that would cache it.
//! - The last part is the only one allowed to be short. That lives in [`crate::upload::sink`],
//!   because it is arithmetic and belongs with a test.
//!
//! # What a half-finished `PATCH` does
//!
//! A chunk that does not end on a part boundary leaves bytes that cannot become
//! a part yet. tusd parks them in a sibling object and prepends them on resume;
//! this answers with the offset of the last **whole** part instead, and the
//! client re-sends the tail. The protocol already expects that: the offset a
//! server reports is what the server accepted.
//!
//! At most one part is re-sent, and only after an interruption. In exchange
//! there is no second object with its own lifetime to create, find, prepend and
//! clean up. The final chunk is exempt, since there the remainder *is* the last
//! part and may be short.
//!
//! One consequence needs answering: a client that *always* sends less than a
//! part would advance nothing and re-send from the same offset for ever. So a
//! request that was not the last and moved the offset by nothing is refused,
//! naming the size it needed. A tus client left at its default sends the whole
//! file in one `PATCH` and never meets this. Only a multipart store has the
//! floor at all; a local file takes whatever arrives.
//!
//! # Why the destination is chosen at creation
//!
//! `?code=` says "append to that share when this finishes"; leaving it off says
//! "make a new one". Deciding here, not at completion, means the finish
//! step needs nothing the client had to remember.
//!
//! # Who may continue one
//!
//! An account continues its own uploads with the credential it already has. An
//! anonymous uploader is handed a token at creation and presents it after, the
//! same shape as the management token an anonymous *share* gets.
//!
//! tusd, the reference implementation, has no answer here and says so in its
//! documentation: the upload URL is random and that is all. A URL is a poor
//! secret, because URLs end up in access logs, proxy logs and `Referer`, where
//! a header does not. So the identifier stays guessable in principle and the
//! token is what authorises.

// `params(CreateUploadParams)` makes utoipa emit a fully-qualified path to the
// type, and the generated code lands outside the function it decorates, so the
// suppression is module-wide, as in `shares`.
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
use object_store::ObjectStoreExt as _;
use serde::{Deserialize, Serialize};
use tessera_core::{Permission, ShareId, UploadId};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};

use crate::{
    api::{
        extract::{ApiJson, ApiPath, ApiQuery},
        shares::{ShareResponse, object_key},
        tokens::format_timestamp,
    },
    auth::{extract::MaybeUser, secret::Secret},
    db::repository::{
        share::{NewFile, StorageLocation},
        upload::{NewUpload, Upload},
    },
    error::{Error, Result},
    state::AppState,
    upload::sink::{Sink, TakenPart, part_count, split_into_parts},
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
/// How long a URL to write one part directly stays usable, at most.
///
/// Long enough for one part on a slow uplink, and never past the session
/// itself: a URL that outlives its session writes into a multipart upload the
/// sweep has already aborted.
const PART_URL_TTL: std::time::Duration = std::time::Duration::from_hours(1);

/// Where a finished upload should land.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
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
    /// resumed after the page is closed, the situation resuming exists for.
    /// `null`
    /// when the uploader is signed in, since their credential already answers.
    pub token: Option<String>,
    /// Whether the parts may go straight to the store instead of through here.
    ///
    /// When `true`, `GET /uploads/{id}/parts/{number}` hands out a URL to `PUT`
    /// each part to, and `POST /uploads/{id}/complete` finishes with the `ETag`
    /// each `PUT` answered. `PATCH` still works either way.
    ///
    /// Decided by where this upload was placed, which is why it is here and not
    /// in the capabilities: an instance with an S3 bucket and a local directory
    /// answers differently for different uploads.
    pub direct: bool,
}

/// A URL to write one part of an upload to the store directly.
#[derive(Debug, Serialize, ToSchema)]
pub struct PartUrlResponse {
    /// Where to `PUT` the part's bytes. The response carries an `ETag` header,
    /// which is what `complete` needs back.
    pub url: String,
    /// When the URL stops working, RFC 3339.
    pub expires_at: String,
}

/// Body of `POST /uploads/{id}/complete`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CompleteUploadRequest {
    /// The `ETag` each part's `PUT` answered with, in part order from one.
    ///
    /// Exactly as many as the upload has parts, as sent by the store: quotes
    /// included, if it included them.
    #[schema(example = json!(["\"3858f62230ac3c915f300c664312c11f\"", "\"9b2cf535f27731c974343645a3985328\""]))]
    pub etags: Vec<String>,
}

/// Opens a session.
///
/// # Errors
///
/// `400` when `Upload-Length` is missing or unusable, `403` when the caller may
/// not upload, `404` when `code` names no share, `507` when the declared length
/// does not fit in what is left. The body says whose ceiling ran out.
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
        (status = 507, description = "No room for it — the body says whether that is the account's quota, its role's, or the instance's", body = crate::error::ErrorBody),
    ),
    security((), ("session" = []), ("bearer" = [])),
)]
pub async fn create(
    State(state): State<AppState>,
    MaybeUser(current): MaybeUser,
    ApiQuery(params): ApiQuery<CreateUploadParams>,
    headers: HeaderMap,
) -> Result<Response> {
    if current.is_none() && !state.config().features.anonymous_upload {
        return Err(Error::Unauthorized);
    }
    if let Some(user) = current.as_ref() {
        user.require(Permission::SharesCreate)?;
    }

    let declared = declared_length(&headers)?;
    // Their own limit, not the instance's: an operator who raised
    // `server.max_body_bytes` for themselves has not raised it for strangers.
    // Refused here, not at the first `PATCH`, so the client learns before it
    // starts sending.
    if declared
        > crate::api::shares::max_file_bytes(&state, current.as_ref().map(|current| &current.user))
            .await?
    {
        return Err(Error::PayloadTooLarge);
    }
    // Checked before a byte is accepted, not as they arrive. An upload that
    // cannot possibly fit should be refused while the client can still act
    // on it, not after it has spent an hour sending.
    let room =
        crate::api::shares::remaining_room(&state, current.as_ref().map(|current| &current.user))
            .await?;
    if declared > room.bytes {
        return Err(Error::InsufficientStorage(room.tier));
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

    // Placed on the declared length, which a resumable upload always has,
    // unlike a direct one whose body may be chunked. So this path can always
    // choose well.
    let backend = crate::api::shares::place_resumable(&state, declared).await?;
    let store = state.backend(&backend).ok_or(Error::Unavailable)?;
    let sink = Sink::open(
        &store,
        state
            .backend_multipart(&backend)
            .as_ref()
            .map(|handle| &handle.0),
        // The local directory only exists for the backend built from
        // `storage.url`. Anywhere else is a remote store, and one that cannot
        // hold a multipart open cannot hold a resumable upload either; see
        // `Sink::open`.
        (backend == crate::storage::DEFAULT_BACKEND)
            .then(|| local_root(&state))
            .flatten()
            .as_ref(),
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
                role_id: crate::api::shares::charged_role_of(
                    current.as_ref().map(|current| &current.user),
                ),
                token_hash: secret.digest(),
                filename: crate::api::shares::checked_filename(params.filename)?,
                content_type: None,
                declared_bytes: declared,
                object_key: key,
                backend,
                multipart_id,
            },
            now,
            now + state.config().share.upload_session_ttl(),
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, format!("/api/v1/uploads/{}", upload.id))],
        axum::Json(UploadResponse {
            id: upload.id.to_string(),
            offset: 0,
            length: declared,
            part_size: u64::try_from(state.config().server.part_size_bytes).unwrap_or(u64::MAX),
            token: current.is_none().then(|| secret.expose().to_owned()),
            direct: direct_signer(&state, &upload).is_some(),
        }),
    )
        .into_response())
}

/// Answers how much has arrived.
///
/// # The answer must not be cached
///
/// It carries `Cache-Control: no-store`. A client that caches it anyway will
/// corrupt the upload: a stale offset sends the next chunk to the wrong place.
/// This is the one header on this endpoint that is not politeness.
///
/// # Errors
///
/// `404` when there is no such session, or the caller may not continue it. The
/// same answer, so a stranger cannot tell one from the other.
#[utoipa::path(
    head,
    path = "/uploads/{id}",
    operation_id = "upload_offset",
    tag = UPLOADS_TAG,
    params(("id" = String, Path, description = "The session")),
    // No bodies declared: a `HEAD` response never has one, so promising an
    // error body here would put something in the contract the protocol
    // forbids, and a contract test is right to fail on it.
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
/// # Every chunk but the last must be a whole part
///
/// `part_size` from the `POST` is a floor, not a suggestion: a chunk that is
/// not the last and carries fewer bytes is refused with `400`. The object
/// stores this streams to will not take a part below their own minimum, five
/// mebibytes on S3, so a short chunk cannot be written anywhere and the offset
/// does not move. A client that kept sending one would loop for ever.
///
/// The last chunk is exempt, because there is nothing after it to make it whole.
///
/// A client that sends `declared_bytes` in one request never meets this at all.
///
/// # Errors
///
/// `400` when a chunk that is not the last is shorter than `part_size`, `409`
/// when the offset does not match, `404` when there is no such session, `415`
/// when the content type is not the one the specification names.
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

    // The specification is explicit that a mismatch changes nothing, and that
    // is
    // what keeps two clients from interleaving into a file that is neither of
    // theirs.
    if header_number(&headers, UPLOAD_OFFSET) != Some(upload.offset_bytes) {
        return Err(Error::Conflict(format!(
            "this upload is at {}",
            upload.offset_bytes
        )));
    }

    let part_size = state.config().server.part_size_bytes;
    let stall = state.config().server.upload_stall();
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

        // Flushed as it arrives, not at the end, so the memory an upload holds
        // is one part and not one file.
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
            let written = sink.write(&upload.object_key, taken, offset, parts).await?;
            let was = offset;
            offset = offset.saturating_add(written.accepted);
            taken += written.parts.len();
            // Conditional on the offset we believed. Another request moving it
            // first means this one is writing into a file it no longer
            // understands, and carrying on would interleave the two.
            if !state
                .uploads
                .advance(
                    upload.id,
                    was,
                    offset,
                    &written.parts,
                    OffsetDateTime::now_utc(),
                )
                .await?
            {
                return Err(Error::Conflict(
                    "another request moved this upload while it was being written".to_owned(),
                ));
            }
        }
        pending = rest;
    }

    // A chunk too short to fill a part advances nothing, so a client sending
    // the same short chunk would re-send from the same offset for ever with
    // nothing to say why. A request that was not the last and moved nothing is
    // refused with the number it needed, which the `POST` already told it.
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

/// Hands out a URL to write one part straight to the store.
///
/// # Why this exists
///
/// Every byte of a `PATCH` crosses this server twice: in from the uploader,
/// out to the bucket. On a self-hosted instance those are often the same
/// uplink. A part written to the store directly crosses it zero times.
///
/// # What it does not check
///
/// The part's size. The URL carries the part number and the upload it belongs
/// to, both signed, but a length cannot be: `object_store` refuses to sign
/// `content-length`. So a client may write a part of any size, and nothing
/// finds out until `complete` asks the store how large the object came out.
/// See [`complete`].
///
/// # Errors
///
/// `404` when there is no such session or no such part, `409` when this upload
/// cannot take parts directly, `410` when the session was abandoned.
#[utoipa::path(
    get,
    path = "/uploads/{id}/parts/{number}",
    operation_id = "upload_part_url",
    tag = UPLOADS_TAG,
    params(
        ("id" = String, Path, description = "The session"),
        ("number" = u64, Path, description = "Which part, from one"),
    ),
    responses(
        (status = 200, description = "Where to `PUT` that part", body = PartUrlResponse),
        (status = 400, description = "`number` is not a part number", body = crate::error::ErrorBody),
        (status = 404, description = "No such session, or no such part", body = crate::error::ErrorBody),
        (status = 409, description = "This upload cannot take parts directly", body = crate::error::ErrorBody),
        (status = 410, description = "That session was abandoned", body = crate::error::ErrorBody),
    ),
    security((), ("session" = []), ("bearer" = [])),
)]
pub async fn part_url(
    State(state): State<AppState>,
    MaybeUser(current): MaybeUser,
    ApiPath((id, number)): ApiPath<(String, u64)>,
    headers: HeaderMap,
) -> Result<axum::Json<PartUrlResponse>> {
    let upload = continuable(&state, &id, current.as_ref(), &headers).await?;
    let signer = direct_signer(&state, &upload).ok_or_else(not_direct)?;
    let multipart_id = upload.multipart_id.clone().ok_or_else(not_direct)?;

    let parts = part_count(upload.declared_bytes, part_size(&state));
    if number == 0 || number > parts {
        return Err(Error::NotFound);
    }

    let now = OffsetDateTime::now_utc();
    let remaining = std::time::Duration::try_from(upload.expires_at - now).unwrap_or_default();
    let ttl = PART_URL_TTL.min(remaining);

    let url = sign_part(&signer, &upload.object_key, number, multipart_id, ttl).await?;

    Ok(axum::Json(PartUrlResponse {
        url: url.to_string(),
        expires_at: format_timestamp(now + ttl),
    }))
}

/// Finishes an upload whose parts went straight to the store.
///
/// # Where the size is checked
///
/// Here, and only here. A part URL cannot be limited to a length, so this
/// completes the object and then asks the store how large it came out. One
/// that is not the size the session declared, and was charged against the
/// uploader's room for, is deleted and refused.
///
/// Until then the extra bytes sit in the bucket. That is the cost of not
/// having the bytes pass through here, and it is bounded by what a session
/// that already passed the size and room checks can write before this runs.
///
/// # Errors
///
/// `400` when the number of `ETag`s is not the number of parts, or the object
/// did not come out the declared size. `404` when there is no such session,
/// `409` when this upload cannot take parts directly or has already had parts
/// sent through this server, `410` when the session was abandoned.
#[utoipa::path(
    post,
    path = "/uploads/{id}/complete",
    operation_id = "complete_upload",
    tag = UPLOADS_TAG,
    params(("id" = String, Path, description = "The session")),
    request_body = CompleteUploadRequest,
    responses(
        (status = 201, description = "The upload is a file now", body = ShareResponse),
        (status = 400, description = "Wrong number of parts, or not the declared size", body = crate::error::ErrorBody),
        (status = 404, description = "No such session", body = crate::error::ErrorBody),
        (status = 409, description = "This upload cannot be finished this way", body = crate::error::ErrorBody),
        (status = 410, description = "That session was abandoned", body = crate::error::ErrorBody),
    ),
    security((), ("session" = []), ("bearer" = [])),
)]
pub async fn complete(
    State(state): State<AppState>,
    MaybeUser(current): MaybeUser,
    ApiPath(id): ApiPath<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<CompleteUploadRequest>,
) -> Result<Response> {
    let upload = continuable(&state, &id, current.as_ref(), &headers).await?;
    direct_signer(&state, &upload).ok_or_else(not_direct)?;

    // One way or the other, not both. Parts this server wrote through `PATCH`
    // have their own identifiers recorded, and completing with a client's list
    // around them would describe an object neither half fully wrote.
    if !state.uploads.parts_of(upload.id).await?.is_empty() {
        return Err(Error::Conflict(
            "parts of this upload came through this server; finish it with PATCH".to_owned(),
        ));
    }

    let parts = part_count(upload.declared_bytes, part_size(&state));
    if u64::try_from(body.etags.len()).ok() != Some(parts) {
        return Err(Error::bad_request(format!(
            "this upload is {parts} parts, and {} ETags were sent",
            body.etags.len()
        )));
    }
    if body.etags.iter().any(|etag| etag.trim().is_empty()) {
        return Err(Error::bad_request("an ETag cannot be empty"));
    }

    let taken = body
        .etags
        .into_iter()
        .zip(1..)
        .map(|(content_id, number)| TakenPart { number, content_id })
        .collect();
    // A wrong ETag fails here, at the store, and leaves the multipart upload
    // open: the client can send the right list again.
    sink_for(&state, &upload)?
        .complete(&upload.object_key, taken)
        .await?;

    let store = state.backend(&upload.backend).ok_or(Error::Unavailable)?;
    let path = object_store::path::Path::from(upload.object_key.as_str());
    let size = store.head(&path).await?.size;
    if size != upload.declared_bytes {
        if let Err(error) = store.delete(&path).await {
            // The reconciliation pass collects an object no share names, so
            // this is late rather than lost.
            tracing::warn!(%error, upload = %upload.id, "could not delete an upload that came out the wrong size");
        }
        state.uploads.delete(upload.id).await?;
        return Err(Error::bad_request(format!(
            "the parts came to {size} bytes, and this upload declared {}",
            upload.declared_bytes
        )));
    }

    settle(&state, upload, size).await
}

/// The signer to hand out part URLs with, when this upload can take them.
///
/// Three things have to hold: the store it was placed on can sign a part
/// upload, a multipart upload is open there, and there is at least one part to
/// write. An empty upload has none, and a multipart upload cannot be completed
/// with zero parts.
fn direct_signer(state: &AppState, upload: &Upload) -> Option<PartSigner> {
    if upload.multipart_id.is_none() || upload.declared_bytes == 0 {
        return None;
    }
    #[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
    {
        state.backend_signer(&upload.backend)
    }
    #[cfg(not(any(feature = "storage-s3", feature = "storage-gcs")))]
    {
        let _ = state;
        None
    }
}

/// What hands out a part URL, in a build that can have one.
///
/// Only S3 and GCS sign a part upload, and `object_store` compiles its signing
/// only for the cloud backends. A build with neither has nothing that could
/// fill this, so it is a type with no values: every upload there answers
/// `direct: false`, and the endpoints stay in the contract and refuse.
#[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
type PartSigner = std::sync::Arc<dyn object_store::signer::Signer>;
#[cfg(not(any(feature = "storage-s3", feature = "storage-gcs")))]
type PartSigner = std::convert::Infallible;

/// Signs a `PUT` for one part of the multipart upload `multipart_id`.
#[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
async fn sign_part(
    signer: &PartSigner,
    key: &str,
    number: u64,
    multipart_id: String,
    ttl: std::time::Duration,
) -> Result<url::Url> {
    let options = object_store::signer::SignedUrlOptions::new().with_query([
        ("partNumber", number.to_string()),
        ("uploadId", multipart_id),
    ]);
    Ok(signer
        .signed_url_opts(
            axum::http::Method::PUT,
            &object_store::path::Path::from(key),
            ttl,
            &options,
        )
        .await?)
}

/// The build with nothing to sign never reaches here: [`direct_signer`] has no
/// value to give, so the caller has already refused.
#[cfg(not(any(feature = "storage-s3", feature = "storage-gcs")))]
#[expect(
    clippy::unused_async,
    reason = "the same signature as the build that signs"
)]
async fn sign_part(
    signer: &PartSigner,
    _key: &str,
    _number: u64,
    _multipart_id: String,
    _ttl: std::time::Duration,
) -> Result<url::Url> {
    match *signer {}
}

fn not_direct() -> Error {
    Error::Conflict("this upload cannot take parts directly; send them with PATCH".to_owned())
}

fn part_size(state: &AppState) -> u64 {
    u64::try_from(state.config().server.part_size_bytes).unwrap_or(u64::MAX)
}

/// Turns a finished upload into a file, and answers with the share.
///
/// The session row goes with it: it has nothing left to describe, and leaving
/// it would make the sweep abort an already-completed multipart upload.
async fn finish(state: &AppState, upload: Upload, size_bytes: u64) -> Result<Response> {
    let sink = sink_for(state, &upload)?;
    let parts = state.uploads.parts_of(upload.id).await?;
    sink.complete(&upload.object_key, parts).await?;
    settle(state, upload, size_bytes).await
}

/// Records an upload whose object is already whole, and answers with the share.
///
/// The half of finishing that does not care how the parts arrived, shared by a
/// `PATCH` that carried the last bytes and a `complete` after parts went
/// straight to the store.
async fn settle(state: &AppState, upload: Upload, size_bytes: u64) -> Result<Response> {
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

/// Reads `Upload-Length`: how much the client says it will send.
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
/// Answers `404`, not `403`, when they may not. A stranger probing identifiers
/// learns nothing from "that exists but is not yours".
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

/// The `HEAD` answer, and also what a `PATCH` returns.
///
/// `no-store` is not politeness: a cached offset sends the next chunk to the
/// wrong place, and it is a proxy, not the client, that would cache it.
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
/// neither holds, or this endpoint would answer which codes exist.
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
///
/// `503`, not `400`. The session and its parts are probably fine, and
/// what is missing is the backend it named, because an operator changed the
/// configuration under an open upload and can change it back. A `400` would
/// tell the client it had done something wrong.
fn sink_for(state: &AppState, upload: &Upload) -> Result<Sink> {
    // The segment the next bytes belong in. One today; the write path is what
    // will have to choose between several. The sweep already walks them all,
    // because cleaning up is the half that cannot wait.
    let segment = upload.segments.first().ok_or_else(|| {
        tracing::error!(%upload.id, "an open upload has no segment");
        Error::Unavailable
    })?;
    crate::upload::reopen(state, segment).ok_or_else(|| {
        tracing::error!(
            backend = upload.backend,
            "an open upload names a backend that is not configured"
        );
        Error::Unavailable
    })
}

/// Where partial files live when the store is a local filesystem.
///
/// `None` when it is not one, in which case there is a multipart handle to use
/// instead. The two are exclusive by construction; see `storage`.
fn local_root(state: &AppState) -> Option<std::path::PathBuf> {
    crate::storage::local_path(&state.config().storage.url).map(std::path::PathBuf::from)
}

/// A `NewFile` for a finished upload.
pub(crate) fn finished_file(upload: &Upload) -> NewFile {
    NewFile {
        // A resumable session writes one object; the split planner is only asked
        // on the direct path so far.
        segments: Vec::new(),
        filename: upload.filename.clone(),
        content_type: upload.content_type.clone(),
        size_bytes: upload.offset_bytes,
        storage: StorageLocation::Object {
            // Where the session actually wrote. Reading the configuration here
            // instead would record the *current* default for a file that went
            // somewhere else, and the share would stop being deliverable the
            // moment an operator moved the default.
            backend: upload.backend.clone(),
            key: upload.object_key.clone(),
        },
    }
}
