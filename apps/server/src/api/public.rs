//! The public retrieval routes: `/s/{code}/raw` and `/s/{code}/download`.
//!
//! These sit outside `/api/v1` and outside the generated contract, because
//! they are not an API but where a link points. A person pastes one into a
//! chat, prints it, or hands it to `curl`, and it has to keep working for as
//! long as the link exists. **Their shape can never change**, and that is the
//! price of a clean, versionless URL.
//!
//! `/s/{code}` itself is not here: it is the single-page app, served by the
//! fallback, which then reads `GET /api/v1/shares/{code}` to decide what to
//! render.

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use bytes::Bytes;
use object_store::{
    GetOptions, GetRange, ObjectStore, ObjectStoreExt as _, path::Path as ObjectPath,
};
use serde::Deserialize;
use time::OffsetDateTime;
use tokio_stream::StreamExt as _;

use crate::{
    api::{
        archive::{self, Format},
        extract::{ApiPath, ApiQuery},
        gate,
        meter::Served,
    },
    auth::{
        extract::ClientAddress,
        proxy::{Origin, UNPLACEABLE},
    },
    db::repository::share::{Content, FilePart, Share, ShareKind},
    error::{Error, ErrorCode, Result},
    state::AppState,
};

/// Prefix the public routes live under.
pub const SHARE_PREFIX: &str = "/s";

/// Query parameters both routes accept.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalQuery {
    /// A download authorisation, for callers that cannot set a header.
    ///
    /// A download is an `<a href>`, and a browser navigation carries no custom
    /// headers, so the ticket has to be able to ride in the URL. The header is
    /// preferred where available.
    pub t: Option<String>,
    /// Which container to pack several files into: `zip`, `tar` or `7z`.
    ///
    /// Ignored for a share with one file; that is served as the file itself.
    /// Defaulted, not required, since the person following the link did not
    /// choose it. A client offering the choice sends one, and a bare link
    /// still works.
    #[serde(default)]
    pub format: Format,
    /// One file of a multi-file share, by zero-based position.
    ///
    /// Served alone, resumable, without an archive around it. Encrypted shares
    /// must be fetched this way, since no archiver can open an archive of
    /// sealed files.
    pub file: Option<usize>,
}

/// Which file of a share a request is about. See [`RetrievalQuery::file`].
///
/// Not `deny_unknown_fields`, unlike [`RetrievalQuery`]: these routes used to
/// ignore the query entirely, and clients that send one must keep working.
#[derive(Debug, Default, serde::Deserialize)]
pub struct FileQuery {
    /// Its position, from zero.
    pub file: Option<usize>,
}

/// The content a request is about: all of it, or the one file it named.
///
/// # Errors
///
/// `404` for a position the share does not have.
fn narrowed(content: Content, file: Option<usize>) -> Result<Content> {
    let Some(position) = file else {
        return Ok(content);
    };
    content
        .files()
        .get(position)
        .map(|part| Content::Files(vec![part.clone()]))
        .ok_or(Error::NotFound)
}

/// Routes that are not part of the versioned API.
pub fn routes() -> Router<AppState> {
    let router = Router::new()
        .route("/{code}/raw", get(raw))
        .route("/{code}/download", get(download))
        .route("/{code}/segments", get(segments))
        .route("/{code}/segments/{index}", get(segment));
    #[cfg(feature = "og-images")]
    let router = router.route("/{code}/og.png", get(og_image));
    router
}

/// The picture a chat client shows when this link is pasted.
///
/// Reads the share through [`preview::lookup`](crate::api::preview::lookup)
/// and not the gate, for the two reasons that module gives. An unfurler follows
/// the link before any person clicks, so counting it would burn a
/// one-time share on the way to its reader; and a crawler must not be able to
/// spend the lookup allowance meant for people guessing codes.
///
/// What the picture may say is decided by
/// [`Preview`](crate::api::preview::Preview), not here. See [`crate::og`].
#[cfg(feature = "og-images")]
async fn og_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiPath(code): ApiPath<String>,
) -> Result<Response> {
    use crate::api::preview::Preview;

    let cards = state.cards.as_ref().ok_or(Error::NotFound)?;
    let (share, content) = crate::api::preview::lookup(&state, &code)
        .await
        .ok_or(Error::NotFound)?;
    let mut preview = Preview::of(
        &share,
        &content,
        OffsetDateTime::now_utc(),
        state.config().site.language,
    );
    // Read only for a note, and thrown away if the share turns out to be
    // locked. `with_snippet` decides that, so the read here is unconditional
    // and the discretion is not duplicated.
    if share.kind == ShareKind::Text
        && let Ok(Some(text)) = state.shares.text_of(share.id).await
    {
        // Lossy instead of refusing. The column is bytes, a paste is whatever
        // the uploader sent, and a card with a replacement character in it is
        // better than no card.
        let body = String::from_utf8_lossy(&text.content);
        preview = preview.with_snippet(&body, text.language.as_deref());
    }

    let layout = state.config().og.layout.clone();
    let etag = card_etag(&preview, &code, &layout);
    if already_held(&headers, &etag) {
        return Ok((StatusCode::NOT_MODIFIED, card_headers(&etag)).into_response());
    }

    // The tag identifies the picture, so a card held under it is this card.
    let png = if let Some(held) = state.drawn_cards.get(&etag) {
        held
    } else {
        let drawn = draw(&state, Arc::clone(cards), preview, code, layout).await?;
        state.drawn_cards.insert(&etag, drawn.clone());
        drawn
    };
    Ok((card_headers(&etag), png).into_response())
}

/// The headers a card carries, on the image and on a `304` alike.
///
/// Five minutes keeps an unfurler asking twice in a minute from coming back,
/// without leaving the card's countdown a day stale. Returning crawlers are
/// handled by the tag, not the max-age.
#[cfg(feature = "og-images")]
fn card_headers(etag: &str) -> [(header::HeaderName, String); 3] {
    [
        (header::CONTENT_TYPE, "image/png".to_owned()),
        (header::CACHE_CONTROL, "public, max-age=300".to_owned()),
        (header::ETAG, etag.to_owned()),
    ]
}

/// An entity tag for the card this share would draw right now.
///
/// Computed from the inputs to the drawing, not the drawing itself, so a
/// returning unfurler costs a lookup and two digests instead of a render.
///
/// All three inputs go in. The preview is the text. The layout belongs to the
/// operator, and changing it changes every card on the instance. The code is
/// drawn on the card, so two otherwise identical shares still produce different
/// images; leaving it out would let a crawler holding one be told the other had
/// not changed.
#[cfg(feature = "og-images")]
fn card_etag(preview: &crate::api::preview::Preview, code: &str, layout: &str) -> String {
    use std::fmt::Write as _;

    use sha2::{Digest as _, Sha256};

    let mut digest = Sha256::new();
    digest.update(preview.fingerprint());
    // Lengths as well as contents, so a code that ends where a layout begins
    // cannot be the same stream of bytes as some other pair.
    digest.update(code.len().to_le_bytes());
    digest.update(code.as_bytes());
    digest.update(layout.len().to_le_bytes());
    digest.update(layout.as_bytes());

    let mut etag = String::with_capacity(66);
    etag.push('"');
    for byte in digest.finalize() {
        // Writing into a String cannot fail; the result is discarded instead of
        // unwrapped so this stays panic-free.
        let _ = write!(etag, "{byte:02x}");
    }
    etag.push('"');
    etag
}

/// Whether the caller already holds this exact card.
///
/// `If-None-Match` is a list, and `*` matches anything. A weak tag compares
/// equal to its strong twin here, which is correct: this route never serves
/// byte ranges, only whole cards.
#[cfg(feature = "og-images")]
fn already_held(headers: &HeaderMap, etag: &str) -> bool {
    let Some(held) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    held.split(',').any(|candidate| {
        let candidate = candidate.trim();
        candidate == "*" || candidate.trim_start_matches("W/") == etag
    })
}

/// Draws the card off the async runtime, under a concurrency ceiling.
///
/// # Why not call the renderer directly
///
/// It is one synchronous call taking milliseconds to seconds depending on the
/// operator's layout, and would park a runtime worker that long. A handful at
/// once could stop the process answering anything, which is far worse than a
/// slow image.
///
/// # Why the ceiling refuses instead of queueing
///
/// A render cannot be cancelled: takumi returns an image or it does not, so a
/// caller who gave up leaves the work running anyway. Queueing would trade a
/// refusal now for the same CPU spent plus a caller waiting for nothing. An
/// unfurler told to retry does; a page that would have shown an image shows
/// none.
#[cfg(feature = "og-images")]
async fn draw(
    state: &AppState,
    cards: Arc<crate::og::Cards>,
    preview: crate::api::preview::Preview,
    code: String,
    layout: String,
) -> Result<Bytes> {
    let permits = Arc::clone(&state.card_renders);
    let Ok(permit) = permits.try_acquire_owned() else {
        tracing::warn!("every card-drawing permit is taken; refusing to draw another");
        return Err(Error::Unavailable);
    };

    let drawn = tokio::task::spawn_blocking(move || {
        let png = cards.draw_with(&preview, &code, &layout);
        drop(permit);
        png
    })
    .await
    .map_err(|error| {
        tracing::error!(%error, "the task drawing a link-preview card did not finish");
        Error::Unavailable
    })?
    .map_err(|error| {
        tracing::error!(%error, "could not draw a link-preview card");
        Error::Unavailable
    })?;

    Ok(Bytes::from(drawn))
}

/// Serves a share's content as plain text.
async fn raw(
    State(state): State<AppState>,
    method: Method,
    ApiPath(code): ApiPath<String>,
    ApiQuery(query): ApiQuery<RetrievalQuery>,
    ClientAddress(Origin { source, .. }): ClientAddress,
    headers: HeaderMap,
) -> Result<Response> {
    // `find` and not `open`: whether this share may be handed out depends on
    // whether the caller holds a ticket for it, and `permit` is where that is
    // known. See `gate`'s module docs.
    let share = gate::find(&state, &code, &source).await?;
    // Before authorising, not after: answering "you wanted the other route" is
    // the caller's mistake, and a mistake must not cost the share one of its
    // retrievals. Anything that can be refused without looking at the content
    // belongs on this side of the gate.
    // A link's address reads as text too, for a client that wants to show
    // where it goes rather than go there.
    if !matches!(share.kind, ShareKind::Text | ShareKind::Link) {
        return Err(Error::new(ErrorCode::ShareIsFile));
    }
    // Sealed bytes labelled as UTF-8 text are useless to a reader; only the page
    // holding the key can turn them into the note.
    if share.encryption.is_some() {
        return Err(Error::new(ErrorCode::ShareEncrypted));
    }
    // Before `permit`: a share held for scanning must not use up a retrieval
    // on a request that is then turned away.
    let content = state.shares.content_of(&share).await?;
    crate::scan::release(&state, &content)?;
    let payload = permit(
        &state,
        &share,
        &headers,
        query.t.as_deref(),
        &method,
        &source,
    )
    .await?;

    // `text/plain` whatever the uploader claimed, plus `nosniff` so a browser
    // cannot decide otherwise. Without both, a `<script>` uploaded as a note
    // becomes stored XSS on the origin holding everyone's session cookie.
    // GitHub serves raw content from another domain; self-hosted software
    // cannot ask its operators for a second one, so the headers carry it.
    serve(
        &state,
        &share,
        &content,
        &headers,
        payload,
        Labels {
            content_type: HeaderValue::from_static("text/plain; charset=utf-8"),
            disposition: HeaderValue::from_static("inline"),
            format: query.format,
        },
    )
    .await
}

/// Serves a share's content as a download, honouring `Range`.
async fn download(
    State(state): State<AppState>,
    method: Method,
    ApiPath(code): ApiPath<String>,
    ApiQuery(query): ApiQuery<RetrievalQuery>,
    ClientAddress(Origin { source, .. }): ClientAddress,
    headers: HeaderMap,
) -> Result<Response> {
    // See `raw`: the admission decision needs the ticket, so it happens in
    // `permit`, not here.
    let share = gate::find(&state, &code, &source).await?;
    // No archiver can open an archive of sealed files, so these are fetched one
    // file at a time. Applied to every encrypted file share, not only
    // multi-file ones, so the rule depends only on public share fields and is
    // settled before a retrieval is spent or a locked share's file count leaks.
    if share.encryption.is_some() && share.kind == ShareKind::File && query.file.is_none() {
        return Err(Error::new(ErrorCode::ValueEmpty).with_detail("field", "file").with_message(
            "an end-to-end encrypted share is fetched one file at a time; add `?file=` with the \
             file's position, from zero",
        ));
    }
    // Never the uploader's claimed media type: serving it back is the same
    // stored-XSS hole as `/raw`, from the other direction.
    //
    // Read before `permit`, for the reason `raw` gives.
    let content = narrowed(state.shares.content_of(&share).await?, query.file)?;
    crate::scan::release(&state, &content)?;
    let payload = permit(
        &state,
        &share,
        &headers,
        query.t.as_deref(),
        &method,
        &source,
    )
    .await?;
    if share.kind == ShareKind::Link {
        return follow(&state, &share).await;
    }
    // After `permit`, so the password, expiry and access log are all checked
    // before the client is redirected anywhere.
    if matches!(payload, Payload::Content)
        && let Some(redirect) = redirect_to_store(&state, &share, &content, &headers).await
    {
        return Ok(redirect);
    }
    // Plain text leaves as a text file, named so it opens in an editor. An
    // encrypted note is ciphertext until the browser opens it, so it stays
    // opaque, and its title and language are sealed anyway.
    let (content_type, disposition) = match &content {
        Content::Text(text) if share.encryption.is_none() => (
            "text/plain; charset=utf-8",
            attachment(&text_name(
                share.title.as_deref(),
                &share.code.to_string(),
                text.language.as_deref(),
            )),
        ),
        _ => ("application/octet-stream", content_disposition(&content)),
    };
    let disposition = HeaderValue::from_str(&disposition)
        .unwrap_or_else(|_| HeaderValue::from_static("attachment"));
    serve(
        &state,
        &share,
        &content,
        &headers,
        payload,
        Labels {
            content_type: HeaderValue::from_static(content_type),
            disposition,
            format: query.format,
        },
    )
    .await
}

/// Sends the client to where a link share points.
///
/// After `permit`, like everything else a download hands out: the password,
/// the expiry, the use limit and the access log all apply to a link as they do
/// to a file.
///
/// `302`, and never cached. A cached redirect would skip every one of those
/// checks on the next visit, and a permanent one would outlive the share.
/// Without a referrer, so the page it lands on is not told this share's code.
async fn follow(state: &AppState, share: &Share) -> Result<Response> {
    let text = state
        .shares
        .text_of(share.id)
        .await?
        .ok_or(Error::NotFound)?;
    // Checked when it was stored, so this only fails on a row that was
    // changed behind the server's back.
    let location = std::str::from_utf8(&text.content)
        .ok()
        .and_then(|address| HeaderValue::from_str(address).ok())
        .ok_or(Error::NotFound)?;
    Ok((
        StatusCode::FOUND,
        [
            (header::LOCATION, location),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (
                header::REFERRER_POLICY,
                HeaderValue::from_static("no-referrer"),
            ),
        ],
    )
        .into_response())
}

/// Redirects a download to the store itself, when that is safe.
///
/// # When
///
/// Only for shares that promise nothing about who gets the bytes or how often:
/// no password and no use limit. A store URL is a bearer credential while it
/// lives, so on a password-protected share it would be a passwordless copy, and
/// on a one-time share an uncounted second download. The store cannot enforce
/// either, since a presigned URL cannot be limited to one use or one client.
///
/// Only for a single unsplit file, on a backend the operator has marked as
/// browser-reachable and that can sign URLs. An archive does not exist until
/// it is streamed, and a split file is several objects.
///
/// # URL lifetime
///
/// Fifteen seconds by the store's clock, which is measured first; see
/// [`crate::storage::clock`]. Enough to start a transfer (the store finishes
/// it regardless), and short enough that a URL copied out of the browser is
/// worth little. A resumed download comes back here for a fresh one.
///
/// # Accounting
///
/// The requested bytes (the file or the range) go into `bytes_redirected`, not
/// `bytes_served`, which counts bytes leaving this process, and none did.
///
/// Anything that does not work out returns `None`, and the download is served
/// from here as usual. The redirect is an optimisation; not getting one is not
/// an error.
#[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
async fn redirect_to_store(
    state: &AppState,
    share: &Share,
    content: &Content,
    headers: &HeaderMap,
) -> Option<Response> {
    let (file, pieces) = direct_file(state, share, content)?;
    // One URL names one object. A split file is served piece by piece instead;
    // see [`segments`].
    let [piece] = pieces.as_slice() else {
        return None;
    };

    // A range this cannot read is left for `serve`, which answers `416`.
    let requested = requested_bytes(headers, file.size_bytes)?;

    // Same filename and opaque content type this server would send, signed into
    // the URL so the store returns them. The hardening headers `serve` adds
    // cannot be carried this way; an opaque-typed attachment is what stops the
    // browser rendering it.
    let options = object_store::signer::SignedUrlOptions::new().with_query([
        ("response-content-disposition", content_disposition(content)),
        (
            "response-content-type",
            "application/octet-stream".to_owned(),
        ),
    ]);
    let location = signed_get(state, piece, &options).await?;
    count_redirected(state, share, requested).await;
    Some(hand_over(location))
}

/// How many bytes a request asked for: the full size, or its range.
#[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
fn requested_bytes(headers: &HeaderMap, size: u64) -> Option<u64> {
    match headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
    {
        None => Some(size),
        Some(value) => {
            let (start, end) = parse_range(value, size).ok()?;
            Some(end.saturating_sub(start).saturating_add(1))
        }
    }
}

/// Records what was handed to a store. A failure here does not stop the
/// download, for the reason `record_access` gives.
#[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
async fn count_redirected(state: &AppState, share: &Share, bytes: u64) {
    if let Err(error) = state.shares.add_bytes_redirected(share.id, bytes).await {
        tracing::warn!(?error, share = %share.id, "could not record a redirected download");
    }
}

/// How a file is stored, for a browser that fetches the pieces itself.
///
/// A file split across backends is several objects, and the only way to save
/// this server's bandwidth is for the browser to fetch and join them, which
/// needs their count and lengths.
#[derive(Debug, serde::Serialize)]
pub struct SegmentsResponse {
    /// The full file size, so the browser can check what it joined.
    pub size_bytes: u64,
    /// Its pieces, in order. A file that was never split has one.
    pub segments: Vec<SegmentPart>,
}

/// One piece, as the listing describes it.
///
/// No URL: each lives fifteen seconds and joining a large file takes minutes,
/// so a list of them would have expired by the end. Each piece is requested
/// when needed, from `/s/{code}/segments/{index}`.
#[derive(Debug, serde::Serialize)]
pub struct SegmentPart {
    /// Where it starts within the file.
    pub offset: u64,
    /// How many bytes it holds.
    pub length: u64,
}

/// How this share is stored, for a browser that joins the pieces itself.
///
/// `404` unless every piece can be fetched from its store directly under the
/// rules in [`direct_file`]: there is no point describing pieces whose bytes
/// would come through here anyway, and on a `404` the client falls back to
/// `/download`.
///
/// Costs no more of a retrieval than a `HEAD`, since describing the layout
/// hands nothing over.
#[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
async fn segments(
    State(state): State<AppState>,
    ApiPath(code): ApiPath<String>,
    ApiQuery(query): ApiQuery<FileQuery>,
    ClientAddress(Origin { source, .. }): ClientAddress,
    headers: HeaderMap,
) -> Result<axum::Json<SegmentsResponse>> {
    let share = gate::find(&state, &code, &source).await?;
    permit(&state, &share, &headers, None, &Method::HEAD, &source).await?;
    let content = narrowed(state.shares.content_of(&share).await?, query.file)?;
    crate::scan::release(&state, &content)?;
    let (file, pieces) = direct_file(&state, &share, &content).ok_or(Error::NotFound)?;
    Ok(axum::Json(SegmentsResponse {
        size_bytes: file.size_bytes,
        segments: pieces
            .iter()
            .map(|piece| SegmentPart {
                offset: piece.offset,
                length: piece.length,
            })
            .collect(),
    }))
}

/// Hands over one piece by redirecting the browser to the store holding it.
///
/// The index is the position in [`SegmentsResponse::segments`], from zero.
///
/// # Errors
///
/// `404` when there is no such share, no such piece, or the share is not one
/// whose pieces may be fetched directly.
#[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
async fn segment(
    State(state): State<AppState>,
    ApiPath((code, index)): ApiPath<(String, usize)>,
    ApiQuery(query): ApiQuery<FileQuery>,
    ClientAddress(Origin { source, .. }): ClientAddress,
    headers: HeaderMap,
) -> Result<Response> {
    let share = gate::find(&state, &code, &source).await?;
    permit(&state, &share, &headers, None, &Method::HEAD, &source).await?;
    let content = narrowed(state.shares.content_of(&share).await?, query.file)?;
    crate::scan::release(&state, &content)?;
    let (_, pieces) = direct_file(&state, &share, &content).ok_or(Error::NotFound)?;
    let piece = pieces.get(index).ok_or(Error::NotFound)?;

    let requested = requested_bytes(&headers, piece.length)
        .ok_or(Error::RangeNotSatisfiable { size: piece.length })?;
    let options = object_store::signer::SignedUrlOptions::new().with_query([(
        "response-content-type",
        "application/octet-stream".to_owned(),
    )]);
    let location = signed_get(&state, piece, &options)
        .await
        .ok_or(Error::Unavailable)?;
    count_redirected(&state, &share, requested).await;
    Ok(hand_over(location))
}

/// A build with no directly fetchable store has nothing to describe.
#[cfg(not(any(feature = "storage-s3", feature = "storage-gcs")))]
#[expect(
    clippy::unused_async,
    reason = "the same signature as the build that answers"
)]
async fn segments(
    State(_): State<AppState>,
    ApiPath(_): ApiPath<String>,
    ApiQuery(_): ApiQuery<FileQuery>,
    ClientAddress(_): ClientAddress,
    _: HeaderMap,
) -> Result<axum::Json<SegmentsResponse>> {
    Err(Error::NotFound)
}

/// As [`segments`].
#[cfg(not(any(feature = "storage-s3", feature = "storage-gcs")))]
#[expect(
    clippy::unused_async,
    reason = "the same signature as the build that answers"
)]
async fn segment(
    State(_): State<AppState>,
    ApiPath(_): ApiPath<(String, usize)>,
    ApiQuery(_): ApiQuery<FileQuery>,
    ClientAddress(_): ClientAddress,
    _: HeaderMap,
) -> Result<Response> {
    Err(Error::NotFound)
}

/// One piece of a file, wherever it lives.
///
/// An unsplit file is one piece; a split one is its segments, in order. Giving
/// both the same shape lets the download and stitching paths share rules.
#[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
struct Piece<'a> {
    backend: &'a str,
    key: &'a str,
    offset: u64,
    length: u64,
}

/// The pieces a file is stored as, or `None` when its bytes are gone.
#[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
fn pieces(file: &FilePart) -> Option<Vec<Piece<'_>>> {
    if file.segments.is_empty() {
        return Some(vec![Piece {
            backend: file.storage.backend()?,
            key: file.storage.object_key()?,
            offset: 0,
            length: file.size_bytes,
        }]);
    }
    Some(
        file.segments
            .iter()
            .map(|segment| Piece {
                backend: &segment.backend,
                key: &segment.object_key,
                offset: segment.offset_bytes,
                length: segment.length_bytes,
            })
            .collect(),
    )
}

/// The file a browser may fetch from the store itself, and where its pieces are.
///
/// No password and no use limit, for the reasons [`redirect_to_store`] gives.
/// One file, because an archive does not exist until it is streamed. And
/// **every** piece on a backend the operator has opened *and* that can sign
/// URLs: with only half the pieces directly fetchable, the browser would start
/// joining and fail to finish. The switch alone is not enough, since a local
/// directory can have it on with nothing to sign with.
#[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
fn direct_file<'a>(
    state: &AppState,
    share: &Share,
    content: &'a Content,
) -> Option<(&'a FilePart, Vec<Piece<'a>>)> {
    if share.needs_password() || share.use_limit.is_some() {
        return None;
    }
    let file = content.single_file()?;
    let pieces = pieces(file)?;
    pieces
        .iter()
        .all(|piece| {
            state.direct_access(piece.backend) && state.backend_signer(piece.backend).is_some()
        })
        .then_some((file, pieces))
}

/// Signs a `GET` for one object, once the store's clock has been checked.
#[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
async fn signed_get(
    state: &AppState,
    piece: &Piece<'_>,
    options: &object_store::signer::SignedUrlOptions,
) -> Option<HeaderValue> {
    let signer = state.backend_signer(piece.backend)?;
    let skew = state.clocks.skew(piece.backend, &signer).await?;
    let ttl = crate::storage::clock::download_ttl(skew)?;
    let url = match signer
        .signed_url_opts(Method::GET, &ObjectPath::from(piece.key), ttl, options)
        .await
    {
        Ok(url) => url,
        Err(error) => {
            tracing::warn!(%error, backend = piece.backend, "could not sign a download; serving it from here");
            return None;
        }
    };
    HeaderValue::from_str(url.as_str()).ok()
}

/// The `307` a signed URL is handed over as.
#[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
fn hand_over(location: HeaderValue) -> Response {
    (
        StatusCode::TEMPORARY_REDIRECT,
        [
            (header::LOCATION, location),
            // A cached redirect would replay an expired URL and skip everything
            // `permit` checks.
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
    )
        .into_response()
}

/// The build with no store a browser can be sent to.
#[cfg(not(any(feature = "storage-s3", feature = "storage-gcs")))]
#[expect(
    clippy::unused_async,
    reason = "the same signature as the build that redirects"
)]
async fn redirect_to_store(
    _: &AppState,
    _: &Share,
    _: &Content,
    _: &HeaderMap,
) -> Option<Response> {
    None
}

/// What a response calls the content, the only thing the two routes disagree
/// about.
///
/// Grouped, not passed loose, because they are decided together. A download
/// says `attachment` and an opaque type, a note says `inline` and
/// plain text, and neither route mixes them.
struct Labels {
    /// The `Content-Type` to send, never the one the uploader claimed.
    content_type: HeaderValue,
    /// The `Content-Disposition` to send.
    disposition: HeaderValue,
    /// Which container several files are packed into.
    ///
    /// Grouped with the labels because it decides two of them: an archive has
    /// its own media type and its own name.
    format: Format,
}

/// The size of the archive this share becomes, or `None` when it is one file.
///
/// One file is not archived. Two or more are, and then this is the exact byte
/// count the response will carry, so it can go out as a `Content-Length` before
/// the first byte is read.
fn archived(content: &Content, format: Format) -> Option<u64> {
    let files = content.files();
    if files.len() < 2 {
        return None;
    }
    let raw: Vec<Option<&str>> = files.iter().map(|f| f.filename.as_deref()).collect();
    let listed: Vec<(String, u64)> = archive::safe_names(&raw)
        .into_iter()
        .zip(files.iter().map(|file| file.size_bytes))
        .collect();
    Some(format.archive_size(&listed))
}

/// Answers with the slice the request asked for, and the headers that say so.
///
/// Both routes reach this and differ only in what they call the content, not
/// in how a transfer works. Keeping range handling in one place matters:
/// `/raw` had none of it, so a client asking for a range got the whole note
/// with a `200`, and a downloader that had written the first half wrote it
/// again.
async fn serve(
    state: &AppState,
    share: &Share,
    content: &Content,
    headers: &HeaderMap,
    payload: Payload,
    labels: Labels,
) -> Result<Response> {
    let Labels {
        mut content_type,
        mut disposition,
        format,
    } = labels;

    // Several files leave as one archive, with its own size, type and name.
    // One file is still that file: archives existing is not a reason to wrap
    // something that is already a single download.
    let packed = archived(content, format);
    if let Some(size) = packed {
        archive::check_size(format, size)?;
        content_type = HeaderValue::from_static("application/octet-stream");
        if let Ok(value) = HeaderValue::from_str(format.media_type()) {
            content_type = value;
        }
        if let Ok(value) = HeaderValue::from_str(&format!(
            "attachment; filename=\"{}.{}\"",
            share.code,
            format.extension()
        )) {
            disposition = value;
        }
    }
    let total = packed.unwrap_or_else(|| content.size_bytes());
    let etag = etag_for(share, content);

    // `If-Range` keeps a resumed transfer from stitching two different files
    // together. A client sends the entity tag it had; if the share is no longer
    // that one, the `Range` is ignored and the whole thing sent. Serving a
    // slice of new content to a client holding the head of the old produces a
    // corrupt result nothing reports as an error.
    let honour_range = headers
        .get(header::IF_RANGE)
        .is_none_or(|value| value.as_bytes() == etag.as_bytes());
    let requested = honour_range
        .then(|| headers.get(header::RANGE))
        .flatten()
        .and_then(|value| value.to_str().ok())
        .map(|value| parse_range(value, total))
        .transpose()?;

    let (status, range) = match requested {
        Some(range) => (StatusCode::PARTIAL_CONTENT, Some(range)),
        None => (StatusCode::OK, None),
    };

    let length = range.map_or(total, |(start, end)| {
        end.saturating_sub(start).saturating_add(1)
    });
    let body = match payload {
        Payload::Headers => Body::empty(),
        Payload::Content => read_range(state, share, content, range, format).await?,
    };
    let mut response = (status, body).into_response();
    let out = response.headers_mut();

    out.insert(header::CONTENT_TYPE, content_type);
    out.insert(header::CONTENT_DISPOSITION, disposition);
    out.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    // Set by hand because a streamed body has no length axum can infer, and
    // without it the browser shows no progress bar and no estimated time, on
    // exactly the transfers being watched.
    if let Ok(value) = HeaderValue::from_str(&length.to_string()) {
        out.insert(header::CONTENT_LENGTH, value);
    }
    if let Ok(value) = HeaderValue::from_str(&etag) {
        out.insert(header::ETAG, value);
    }
    if let Some((start, end)) = range
        && let Ok(value) = HeaderValue::from_str(&format!("bytes {start}-{end}/{total}"))
    {
        out.insert(header::CONTENT_RANGE, value);
    }
    harden(out);
    // Tells the meter which share to credit. Set here instead of worked out
    // from the path in the middleware, because this is the only place that has
    // resolved the code, and doing it again would be a second lookup on every
    // retrieval.
    response.extensions_mut().insert(Served(share.id));
    Ok(response)
}

/// Headers every retrieval carries, whatever it is serving.
fn harden(headers: &mut HeaderMap) {
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    // Nothing here should be able to load anything, run anything, or be framed.
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
}

/// Makes sure the caller may have this share's content, paying for it if they
/// have not already.
///
/// A ticket means the retrieval was paid for at `unlock`, so a resumed download
/// does not cost a second one. Without one this is a direct
/// retrieval (`curl` following a link), so the password is checked and a use is
/// spent here and now.
async fn permit(
    state: &AppState,
    share: &Share,
    headers: &HeaderMap,
    ticket: Option<&str>,
    method: &Method,
    source: &str,
) -> Result<Payload> {
    // First, because the answer decides whether an exhausted share is still
    // collectable: the caller who emptied it is the one now coming for the
    // bytes. `admit` turns anyone else away.
    let held = gate::holds_ticket(state, share, headers, ticket).await?;
    gate::admit(share, held)?;

    // `HEAD` asks what a `GET` would answer, and that is not a retrieval. axum
    // routes `HEAD` to the same handler as `GET`, so without this a link
    // checker, a prefetch or a chat client probing the URL spends a one-time
    // share.
    //
    // Without a ticket the password is still required, since `HEAD` reveals the
    // size and a locked share should not describe itself. With one it is not,
    // for the same reason a `GET` skips it: the ticket is the proof.
    if method == Method::HEAD {
        if held == gate::Ticket::Absent {
            gate::unlock(state, share, headers, source).await?;
        }
        return Ok(Payload::Headers);
    }

    if held == gate::Ticket::Held {
        record_access(state, share, headers, source).await;
        return Ok(Payload::Content);
    }
    gate::unlock(state, share, headers, source).await?;
    gate::spend(state, share).await?;
    record_access(state, share, headers, source).await;
    Ok(Payload::Content)
}

/// Writes down who fetched this, when the operator has asked for that.
///
/// # Why only here
///
/// This is the one place both retrieval routes agree the content is being
/// handed over. A `HEAD` does not reach it, for the same reason it does not
/// spend a use: a link checker or a chat client unfurling the URL is not a
/// person collecting the file, and a log full of those answers no question.
///
/// # Why a failure here is not the caller's problem
///
/// The share has already been authorised and paid for. Refusing to serve it
/// because a bookkeeping row would not insert turns optional record-keeping
/// into an outage on the one path that has to work. It is logged and the
/// download continues.
async fn record_access(state: &AppState, share: &Share, headers: &HeaderMap, source: &str) {
    if !state.config().share.access_log.enabled {
        return;
    }
    // `UNPLACEABLE` is the throttle's stand-in for "there was no address", and
    // storing it would put the word `unknown` in a column a person reads as an
    // address.
    let address = (source != UNPLACEABLE).then_some(source);
    let agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok());
    if let Err(error) = state
        .events
        .record(share.id, OffsetDateTime::now_utc(), address, agent)
        .await
    {
        tracing::warn!(?error, share = %share.id, "could not record an access event");
    }
}

/// Whether this request is owed the bytes or only the headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Payload {
    /// A `HEAD`: describe the response a `GET` would give, and send no body.
    Headers,
    /// A `GET`: send the content.
    Content,
}

/// A tag that changes when the content does.
///
/// Derived from the identifier and the size, not the bytes. Hashing the content
/// would mean reading it to answer a `HEAD`, and a share's bytes never
/// change once written. Only deletion and re-upload produce different content,
/// and that produces a different identifier.
fn etag_for(share: &Share, content: &Content) -> String {
    format!("\"{}-{}\"", share.id, content.size_bytes())
}

/// `Content-Disposition`, with the RFC 5987 form so non-ASCII names survive.
///
/// The plain `filename` is kept for old clients and the starred one carries the
/// real value; without the latter a Chinese or Japanese file name arrives
/// mangled in most browsers.
fn content_disposition(content: &Content) -> String {
    attachment(file_name(content))
}

/// The single file's own name, or the archive's. A share with one file does not
/// become an archive just because archives exist.
fn file_name(content: &Content) -> &str {
    content
        .single_file()
        .and_then(|file| last_component(file.filename.as_deref()))
        .unwrap_or("download")
}

/// The last component only, after either separator.
///
/// A stored name is what the uploader sent, checked only for length and control
/// characters, so it can carry the folder it came from or be nothing but
/// separators. A header naming a path says where on the recipient's disk to put
/// the file, which RFC 6266 tells us not to try.
fn last_component(name: Option<&str>) -> Option<&str> {
    name.and_then(|name| name.rsplit(['/', '\\']).next())
        .filter(|last| !last.is_empty())
}

/// What a note is saved as: its title, or its retrieval code, with the
/// extension its language implies when the title does not carry one.
///
/// Without an extension the file opens in nothing useful, which is the whole
/// complaint about calling it `download`.
fn text_name(title: Option<&str>, code: &str, language: Option<&str>) -> String {
    let stem = last_component(title.map(str::trim)).unwrap_or(code);
    if std::path::Path::new(stem).extension().is_some() {
        return stem.to_owned();
    }
    let extension = match language {
        Some("javascript") => "js",
        Some("typescript") => "ts",
        Some("python") => "py",
        Some("html") => "html",
        Some("css") => "css",
        Some("json") => "json",
        Some("markdown") => "md",
        Some("sql") => "sql",
        Some("java") => "java",
        Some("cpp") => "cpp",
        Some("rust") => "rs",
        Some("go") => "go",
        Some("yaml") => "yaml",
        Some("php") => "php",
        Some("shellscript") => "sh",
        Some("mermaid") => "mmd",
        _ => "txt",
    };
    format!("{stem}.{extension}")
}

/// `Content-Disposition: attachment` for `name`.
fn attachment(name: &str) -> String {
    // The fallback keeps anything printable ASCII, space included: `"my
    // file.zip"` is an ordinary name, and turning its space into an underscore
    // gains nothing. A quote or a backslash cannot survive, since the value
    // sits inside quotes.
    let ascii: String = name
        .chars()
        .map(|c| {
            if c.is_ascii() && !c.is_ascii_control() && c != '"' && c != '\\' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!(
        "attachment; filename=\"{ascii}\"; filename*=UTF-8''{}",
        percent_encode(name)
    )
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

/// Parses one byte range, the only form worth supporting.
///
/// Multi-range requests are answered with the whole object instead: they need
/// a multipart response, no browser download manager sends one, and the clients
/// that do accept a `200`.
fn parse_range(value: &str, size: u64) -> Result<(u64, u64)> {
    let unsatisfiable = || Error::RangeNotSatisfiable { size };
    let spec = value.strip_prefix("bytes=").ok_or_else(unsatisfiable)?;
    if spec.contains(',') {
        return Err(unsatisfiable());
    }
    let (start, end) = spec.split_once('-').ok_or_else(unsatisfiable)?;
    let last = size.saturating_sub(1);

    let (start, end) = match (start.trim(), end.trim()) {
        // `bytes=-500`: the final 500 bytes.
        ("", suffix) => {
            let length: u64 = suffix.parse().map_err(|_| unsatisfiable())?;
            (size.saturating_sub(length.max(1)), last)
        }
        (from, "") => (from.parse().map_err(|_| unsatisfiable())?, last),
        (from, to) => (
            from.parse().map_err(|_| unsatisfiable())?,
            to.parse::<u64>().map_err(|_| unsatisfiable())?.min(last),
        ),
    };

    if size == 0 || start > end || start > last {
        return Err(unsatisfiable());
    }
    Ok((start, end))
}

/// Streams the whole content, or one range of it.
///
/// # Why this is a stream and not a `Vec<u8>`
///
/// Collecting the object first costs its size in memory, twice over if copied.
/// Measured before this was a stream: an 80 MiB download raised peak RSS by
/// about 160 MB, which on a small machine is the difference between serving a
/// file and being killed. Uploads were careful about this and reads were not,
/// the wrong way round: a share is written once and read many times.
///
/// Inline content is already in memory and bounded by
/// `share.inline_threshold_bytes`, so it is handed over as it is.
async fn read_range(
    state: &AppState,
    share: &Share,
    content: &Content,
    range: Option<(u64, u64)>,
    format: Format,
) -> Result<Body> {
    // Text is its own row, so it is already in hand.
    if matches!(content, Content::Text(_)) {
        let text = state
            .shares
            .text_of(share.id)
            .await?
            .ok_or(Error::NotFound)?;
        return Ok(Body::from(slice(&text.content, range)));
    }

    let files = content.files();
    match files {
        [] => Err(Error::NotFound),
        // One file is that file, keeping its name, its ranges and its ability
        // to be resumed. An archive has none of those.
        [only] => stream_one(state, only, range).await,
        // Several become an archive, and an archive has no ranges: it is
        // produced as it is sent, so there is no earlier part to seek back to.
        many => stream_archive(state, many, format).await,
    }
}

/// One archive entry: its name, its size, and where to read it from.
///
/// The location is optional because a file whose bytes are gone still takes a
/// place in the archive, as an empty entry, so the listing matches the share.
type PlannedEntry = (String, u64, Option<(Arc<dyn ObjectStore>, String)>);

/// The store holding this file's bytes.
///
/// # Errors
///
/// [`Error::Unavailable`] when the file names a backend this process has no
/// configuration for. Not "no such share": the row and probably the bytes are
/// fine, and what is missing is credentials an operator can put back.
fn backend_of(state: &AppState, file: &FilePart) -> Result<Arc<dyn ObjectStore>> {
    let name = file.storage.backend().ok_or(Error::NotFound)?;
    state.backend(name).ok_or_else(|| {
        tracing::error!(
            backend = name,
            "a share names a backend that is not configured"
        );
        Error::Unavailable
    })
}

/// Streams a file that was written across several backends.
///
/// The segments are read in order and their bodies laid end to end. Each is
/// asked for the slice of itself the range covers, which for the middle ones is
/// all of it. See [`stitch::reads_for`](crate::storage::stitch::reads_for).
///
/// The stream is built lazily, one segment at a time, so a fifty-gigabyte file
/// costs one segment's chunk of memory instead of fifty gigabytes. That
/// laziness is also why this is not `async`: nothing is fetched until the body
/// is read.
///
/// # Errors
///
/// Returns [`Error`] if a backend the file names is not configured.
fn stream_stitched(state: &AppState, file: &FilePart, range: Option<(u64, u64)>) -> Result<Body> {
    let extent: Vec<(u64, u64)> = file
        .segments
        .iter()
        .map(|piece| (piece.offset_bytes, piece.length_bytes))
        .collect();
    let whole = (0, file.size_bytes.saturating_sub(1));
    let reads = crate::storage::stitch::reads_for(&extent, range.unwrap_or(whole));

    // Resolved before any of it is streamed: a backend that is not configured is
    // a `503`, and answering that after the first segment has already gone out
    // would be a truncated download reported as a success.
    let mut opened = Vec::with_capacity(reads.len());
    for read in reads {
        let piece = file.segments.get(read.segment).ok_or(Error::NotFound)?;
        let store = state.backend(&piece.backend).ok_or_else(|| {
            tracing::error!(
                backend = piece.backend,
                "a share names a backend that is not configured"
            );
            Error::Unavailable
        })?;
        opened.push((store, piece.object_key.clone(), read.start, read.length));
    }

    // Lazily, one segment at a time: the next is not asked for until the one
    // before it has been handed over, so the process holds a chunk and not the
    // whole file.
    //
    // `async_stream` and not a combinator, since `futures-util` is not a
    // dependency of the server, only of its tests.
    let chunks = async_stream::stream! {
        for (store, key, start, length) in opened {
            let options = GetOptions {
                range: Some(GetRange::Bounded(start..start.saturating_add(length))),
                ..GetOptions::default()
            };
            match store.get_opts(&ObjectPath::from(key.as_str()), options).await {
                Ok(result) => {
                    let mut piece = result.into_stream();
                    while let Some(chunk) = piece.next().await {
                        yield chunk;
                    }
                }
                Err(error) => {
                    yield Err(error);
                    return;
                }
            }
        }
    };
    Ok(Body::from_stream(chunks))
}

/// Streams one file, honouring a range.
pub(crate) async fn stream_one(
    state: &AppState,
    file: &FilePart,
    range: Option<(u64, u64)>,
) -> Result<Body> {
    // More than one piece means the file was spread, and reading it is a walk
    // and not a request. One piece is every file written with splitting off,
    // and it takes the path below unchanged.
    if file.segments.len() > 1 {
        return stream_stitched(state, file, range);
    }
    let Some(key) = file.storage.object_key() else {
        return Err(Error::NotFound);
    };
    // The backend this file recorded, not whichever one new uploads go to. A
    // name no longer configured is a share the operator orphaned by changing
    // the configuration, so `503` ("cannot serve it right now") is true and
    // recoverable where `404` would claim it never existed.
    let store = backend_of(state, file)?;
    let options = GetOptions {
        // HTTP ranges are inclusive at both ends; `GetRange::Bounded` is not.
        range: range.map(|(start, end)| GetRange::Bounded(start..end.saturating_add(1))),
        ..GetOptions::default()
    };
    let result = store.get_opts(&ObjectPath::from(key), options).await?;
    // The store hands back a stream of chunks; passing it straight through means
    // the process never holds more than one chunk of the object.
    Ok(Body::from_stream(result.into_stream()))
}

#[expect(
    clippy::unused_async,
    reason = "the caller awaits every arm; a sync signature here would split it"
)]
/// Streams several files as one archive.
///
/// # Why the whole thing is not built first
///
/// It would mean holding every byte of the share in memory to send it. The
/// entries go out one at a time as they are read from the store, and the parts
/// this adds (headers, descriptors, the directory) are a few dozen bytes each.
///
/// # Why there is no range
///
/// The archive does not exist until it is sent, so there is no earlier part to
/// seek to. `serve` does not offer ranges for one, and a client asking for one
/// gets the whole thing.
async fn stream_archive(state: &AppState, files: &[FilePart], format: Format) -> Result<Body> {
    // The same names `archived` predicted the size from. Two files can share a
    // name and an archive with two identical entries unpacks to one file, so
    // the names are decided for the set and not one at a time, and both places
    // have to decide them the same way.
    let raw: Vec<Option<&str>> = files.iter().map(|f| f.filename.as_deref()).collect();
    // Each entry carries its own store, because the files in one archive need
    // not all live in the same backend.
    let planned: Vec<PlannedEntry> = archive::safe_names(&raw)
        .into_iter()
        .zip(files.iter())
        .map(|(name, file)| {
            let located = file
                .storage
                .object_key()
                .and_then(|key| Some((backend_of(state, file).ok()?, key.to_owned())));
            (name, file.size_bytes, located)
        })
        .collect();

    let stream = async_stream::try_stream! {
        match format {
            Format::Zip => {
                let mut entries: Vec<(archive::zip::Entry, u64)> = Vec::with_capacity(planned.len());
                let mut offset: u64 = 0;
                for (name, _, key) in &planned {
                    let start = offset;
                    let header: Bytes = archive::zip::local_header(name);
                    offset += u64::try_from(header.len()).unwrap_or(0);
                    yield header;

                    let mut crc = archive::zip::Crc32::default();
                    let mut size: u64 = 0;
                    if let Some((store, key)) = key {
                        let mut chunks = store.get(&ObjectPath::from(key.as_str())).await?.into_stream();
                        while let Some(chunk) = chunks.next().await {
                            let chunk = chunk?;
                            crc.update(&chunk);
                            size += u64::try_from(chunk.len()).unwrap_or(0);
                            offset += u64::try_from(chunk.len()).unwrap_or(0);
                            yield chunk;
                        }
                    }
                    let crc = crc.finish();
                    let descriptor: Bytes = archive::zip::data_descriptor(crc, size);
                    offset += u64::try_from(descriptor.len()).unwrap_or(0);
                    yield descriptor;
                    entries.push((archive::zip::Entry { name: name.clone(), size, crc }, start));
                }
                let directory: Bytes = archive::zip::central_directory(&entries);
                yield directory;
            }
            Format::Tar => {
                for (name, size, key) in &planned {
                    let header: Bytes = archive::tar::header(name, *size);
                    yield header;
                    if let Some((store, key)) = key {
                        let mut chunks = store.get(&ObjectPath::from(key.as_str())).await?.into_stream();
                        while let Some(chunk) = chunks.next().await {
                            yield chunk?;
                        }
                    }
                    let padding: Bytes = archive::tar::padding(*size);
                    if !padding.is_empty() {
                        yield padding;
                    }
                }
                let trailer: Bytes = archive::tar::trailer();
                yield trailer;
            }
            Format::SevenZ => {
                // The signature goes first and names the header, so the header
                // is built before anything is read. Only possible because it
                // carries no per-file checksums. See the module.
                let listed: Vec<(String, u64)> = planned
                    .iter()
                    .map(|(name, size, _)| (name.clone(), *size))
                    .collect();
                let header = archive::sevenz::header(&listed);
                let packed: u64 = listed.iter().map(|(_, size)| *size).sum();
                let signature: Bytes = archive::sevenz::signature_header(packed, &header);
                yield signature;

                for (_, _, key) in &planned {
                    if let Some((store, key)) = key {
                        let mut chunks = store.get(&ObjectPath::from(key.as_str())).await?.into_stream();
                        while let Some(chunk) = chunks.next().await {
                            yield chunk?;
                        }
                    }
                }
                yield header;
            }
        }
    };
    Ok(Body::from_stream(annotate(stream)))
}

/// Pins the archive stream's error type, which the block cannot infer.
const fn annotate<S>(stream: S) -> S
where
    S: tokio_stream::Stream<Item = std::result::Result<Bytes, object_store::Error>>,
{
    stream
}

fn slice(content: &[u8], range: Option<(u64, u64)>) -> Vec<u8> {
    let Some((start, end)) = range else {
        return content.to_vec();
    };
    let start = usize::try_from(start).unwrap_or(usize::MAX);
    let length = usize::try_from(end.saturating_sub(start_as_u64(start)).saturating_add(1))
        .unwrap_or(usize::MAX);
    content
        .get(start..)
        .map(|tail| tail.get(..length).unwrap_or(tail).to_vec())
        .unwrap_or_default()
}

fn start_as_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_range_forms_a_download_manager_sends() {
        assert_eq!(parse_range("bytes=0-499", 1000).unwrap(), (0, 499));
        assert_eq!(parse_range("bytes=500-", 1000).unwrap(), (500, 999));
        assert_eq!(parse_range("bytes=-500", 1000).unwrap(), (500, 999));
        // Past the end is clamped, as RFC 9110 asks.
        assert_eq!(parse_range("bytes=0-99999", 1000).unwrap(), (0, 999));
    }

    #[test]
    fn refuses_ranges_it_cannot_answer() {
        for spec in ["bytes=1000-", "bytes=abc-", "items=0-1", "bytes=0-1,5-6"] {
            assert!(parse_range(spec, 1000).is_err(), "{spec}");
        }
        assert!(parse_range("bytes=0-0", 0).is_err(), "empty object");
    }

    /// Without the starred form a non-ASCII name arrives mangled, and without
    /// the plain one some older clients ignore the header entirely.
    #[test]
    fn a_non_ascii_filename_survives_content_disposition() {
        let value = content_disposition(&named(Some("專案 v2.zip")));
        assert!(value.starts_with("attachment; "), "{value}");
        assert!(value.contains(r#"filename="__ v2.zip""#), "{value}");
        assert!(
            value.contains("filename*=UTF-8''%E5%B0%88%E6%A1%88%20v2.zip"),
            "{value}"
        );
    }

    #[test]
    fn a_note_is_named_for_its_title_or_code_and_language() {
        assert_eq!(text_name(None, "ab12cd", Some("python")), "ab12cd.py");
        assert_eq!(text_name(None, "ab12cd", None), "ab12cd.txt");
        assert_eq!(text_name(None, "ab12cd", Some("plaintext")), "ab12cd.txt");
        assert_eq!(
            text_name(Some("notes"), "ab12cd", Some("markdown")),
            "notes.md"
        );
        assert_eq!(
            text_name(Some("main.rs"), "ab12cd", Some("rust")),
            "main.rs"
        );
        assert_eq!(
            text_name(Some("a/b/run"), "ab12cd", Some("shellscript")),
            "run.sh"
        );
        assert_eq!(text_name(Some(" / "), "ab12cd", None), "ab12cd.txt");
        assert_eq!(text_name(Some("  "), "ab12cd", None), "ab12cd.txt");
    }

    #[test]
    fn a_share_with_no_filename_still_downloads() {
        let value = content_disposition(&named(None));
        assert!(value.contains(r#"filename="download""#), "{value}");
    }

    /// Only the name matters here, and it now comes from the content.
    fn named(filename: Option<&str>) -> Content {
        Content::Files(vec![FilePart {
            segments: Vec::new(),
            filename: filename.map(str::to_owned),
            content_type: None,
            sealed_name: None,
            scan: None,
            size_bytes: 0,
            storage: crate::db::repository::share::StorageLocation::Inline,
        }])
    }

    /// Two files, so this stops being one file's download.
    #[test]
    fn a_share_with_several_files_is_offered_as_an_archive() {
        let two = Content::Files(vec![
            FilePart {
                segments: Vec::new(),
                filename: Some("one.txt".to_owned()),
                content_type: None,
                sealed_name: None,
                scan: None,
                size_bytes: 5,
                storage: crate::db::repository::share::StorageLocation::Inline,
            },
            FilePart {
                segments: Vec::new(),
                filename: Some("two.txt".to_owned()),
                content_type: None,
                sealed_name: None,
                scan: None,
                size_bytes: 7,
                storage: crate::db::repository::share::StorageLocation::Inline,
            },
        ]);
        assert!(two.single_file().is_none());
        assert_eq!(two.size_bytes(), 12);
        // No single name to offer, so the archive's is used.
        assert!(content_disposition(&two).contains(r#"filename="download""#));
    }
}
