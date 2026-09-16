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
    error::{Error, Result},
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
}

/// Routes that are not part of the versioned API.
pub fn routes() -> Router<AppState> {
    let router = Router::new()
        .route("/{code}/raw", get(raw))
        .route("/{code}/download", get(download));
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
    let mut preview = Preview::of(&share, &content, OffsetDateTime::now_utc());
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

/// The headers a card carries, sent with the picture and with a `304` alike.
///
/// Five minutes is long enough that an unfurler asked twice in a minute does
/// not come back, and short enough that the countdown on the card is not stale
/// by a day. What does the work for a crawler returning later is the tag, not
/// the age.
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
/// Worked out from what would be drawn rather than from the drawing, which is
/// the whole point: answering a returning unfurler then costs a lookup and two
/// digests instead of a render.
///
/// Everything the drawing reads goes in, and there are three of those. The
/// preview is the words. The layout is the operator's, who can change it, and
/// every card on the instance becomes a different picture when they do. And
/// the code is drawn on the card itself, so two shares alike in every other
/// way still draw two different pictures; leaving it out would let a crawler
/// holding one be told the other had not changed.
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
        // Writing into a String is infallible; the result is discarded rather
        // than unwrapped so this stays panic-free.
        let _ = write!(etag, "{byte:02x}");
    }
    etag.push('"');
    etag
}

/// Whether the caller already holds this exact card.
///
/// `If-None-Match` is a list, and `*` matches anything. A weak tag compares
/// equal to its strong twin here, which is the right comparison: nothing on
/// this route serves byte ranges, and a card is only ever handed over whole.
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

/// Draws the card, off the async runtime and behind a ceiling.
///
/// # Why not simply call the renderer
///
/// It is one synchronous call that takes milliseconds or seconds depending on
/// a layout the operator wrote, and calling it here would park a runtime
/// worker for that long. A handful at once would then be enough to stop this
/// process answering anything at all, which is a great deal worse than a slow
/// picture.
///
/// # Why the ceiling refuses rather than queues
///
/// There is no cancelling a render in progress: takumi hands back an image or
/// it does not, so a caller who gave up leaves the work running anyway.
/// Queueing would trade a refusal now for the same CPU spent plus a caller who
/// waited for nothing. An unfurler told to come back does; a page that would
/// have shown a picture shows none.
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
    if share.kind != ShareKind::Text {
        return Err(Error::bad_request(
            "this share is a file; use /download instead",
        ));
    }
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
    let content = state.shares.content_of(&share).await?;
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
    let payload = permit(
        &state,
        &share,
        &headers,
        query.t.as_deref(),
        &method,
        &source,
    )
    .await?;
    // Never the media type the uploader claimed. Serving that back is the same
    // stored-XSS hole as `/raw`, reached from the other direction.
    let content = state.shares.content_of(&share).await?;
    // After `permit`, so the password, the expiry and the access log have all
    // had their say before anybody is sent anywhere.
    if matches!(payload, Payload::Content)
        && let Some(redirect) = redirect_to_store(&state, &share, &content, &headers).await
    {
        return Ok(redirect);
    }
    let disposition = HeaderValue::from_str(&content_disposition(&content))
        .unwrap_or_else(|_| HeaderValue::from_static("attachment"));
    serve(
        &state,
        &share,
        &content,
        &headers,
        payload,
        Labels {
            content_type: HeaderValue::from_static("application/octet-stream"),
            disposition,
            format: query.format,
        },
    )
    .await
}

/// Sends a download to the store itself, when that is safe to do.
///
/// # When it is
///
/// Only for a share that promises nothing about who gets the bytes or how often:
/// no password and no use limit. A URL to the store is a bearer credential for
/// its lifetime, so on a password-protected share it would be a copy of the
/// file that needs no password, and on a one-time share a second collection
/// the count never sees. Neither can be repaired at the store: a presigned URL
/// cannot be limited to one use or one client.
///
/// Only for one file in one piece, on a backend whose operator has said a
/// browser can reach it and which can sign a URL at all. An archive does not
/// exist until it is sent, and a split file is several objects.
///
/// # How long the URL lives
///
/// Fifteen seconds by the store's clock, which is measured first; see
/// [`crate::storage::clock`]. Long enough to start a transfer, which the store
/// then finishes whatever the time, and short enough that a URL copied out of
/// the browser is not much of a gift. A resumed download comes back here for a
/// fresh one.
///
/// # What is counted
///
/// What the request asked for, in `bytes_redirected`: the file, or the range.
/// Not `bytes_served`, which counts what left this process as it left, and none
/// did.
///
/// Anything that does not work out answers `None`, and the download is served
/// from here as it always was. Being sent somewhere is an optimisation; failing
/// to be is not an error.
#[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
async fn redirect_to_store(
    state: &AppState,
    share: &Share,
    content: &Content,
    headers: &HeaderMap,
) -> Option<Response> {
    let (backend, key, size) = direct_location(state, share, content)?;
    let signer = state.backend_signer(backend)?;
    let skew = state.clocks.skew(backend, &signer).await?;
    let ttl = crate::storage::clock::download_ttl(skew)?;

    // A range this cannot read is left for `serve`, which answers `416`.
    let requested = match headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
    {
        None => size,
        Some(value) => {
            let (start, end) = parse_range(value, size).ok()?;
            end.saturating_sub(start).saturating_add(1)
        }
    };

    // The same name and the same opaque type this server would send, signed
    // into the URL so the store answers with them. The hardening headers
    // `serve` adds cannot travel this way; an attachment of an opaque type is
    // what keeps the browser from rendering it instead.
    let options = object_store::signer::SignedUrlOptions::new().with_query([
        ("response-content-disposition", content_disposition(content)),
        (
            "response-content-type",
            "application/octet-stream".to_owned(),
        ),
    ]);
    let url = match signer
        .signed_url_opts(Method::GET, &ObjectPath::from(key), ttl, &options)
        .await
    {
        Ok(url) => url,
        Err(error) => {
            tracing::warn!(%error, backend, "could not sign a download; serving it from here");
            return None;
        }
    };
    let location = HeaderValue::from_str(url.as_str()).ok()?;

    // Counted once the redirect is certain, and a failure to count does not
    // stop it, for the reason `record_access` gives.
    if let Err(error) = state.shares.add_bytes_redirected(share.id, requested).await {
        tracing::warn!(?error, share = %share.id, "could not record a redirected download");
    }

    Some(
        (
            StatusCode::TEMPORARY_REDIRECT,
            [
                (header::LOCATION, location),
                // A cached redirect replays a URL that has expired, and skips
                // everything `permit` checks on the way to it.
                (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            ],
        )
            .into_response(),
    )
}

/// Where a download may be sent, if anywhere: the backend, the key, and the
/// file's size.
///
/// The half of [`redirect_to_store`] that needs nothing from the network. No
/// password, no use limit, one file in one piece, and a backend its operator has
/// said a browser can reach.
#[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
fn direct_location<'a>(
    state: &AppState,
    share: &Share,
    content: &'a Content,
) -> Option<(&'a str, &'a str, u64)> {
    if share.needs_password() || share.use_limit.is_some() {
        return None;
    }
    let file = content.single_file()?;
    if file.segments.len() > 1 {
        return None;
    }
    let backend = file.storage.backend()?;
    let key = file.storage.object_key()?;
    state
        .direct_access(backend)
        .then_some((backend, key, file.size_bytes))
}

/// The build with no store that can be sent a browser.
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
    // The single file's own name, or the archive's. A share with one file does
    // not become an archive just because archives exist.
    //
    // The last component only, after either separator. A stored name is what
    // the uploader sent, checked only for length and control characters, so it
    // can carry the folder it came from or be nothing but separators. A header
    // naming a path says where on the recipient's disk to put the file, which
    // RFC 6266 tells us not to try.
    let name = content
        .single_file()
        .and_then(|file| file.filename.as_deref())
        .and_then(|filename| filename.rsplit(['/', '\\']).next())
        .filter(|last| !last.is_empty())
        .unwrap_or("download");
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
async fn stream_one(state: &AppState, file: &FilePart, range: Option<(u64, u64)>) -> Result<Body> {
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
                size_bytes: 5,
                storage: crate::db::repository::share::StorageLocation::Inline,
            },
            FilePart {
                segments: Vec::new(),
                filename: Some("two.txt".to_owned()),
                content_type: None,
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
