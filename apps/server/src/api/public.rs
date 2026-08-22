//! The public retrieval routes: `/s/{code}/raw` and `/s/{code}/download`.
//!
//! These sit outside `/api/v1` and outside the generated contract, because they
//! are not an API — they are where a link points. A person pastes one into a
//! chat, prints it, or hands it to `curl`, and it has to keep working for as
//! long as the link exists. **Their shape can never change**, which is the
//! deliberate price of a clean, versionless URL.
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
use object_store::{GetOptions, GetRange, ObjectStoreExt as _, path::Path as ObjectPath};
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
    /// headers — so the ticket has to be able to ride in the URL. The header is
    /// preferred where it is available.
    pub t: Option<String>,
    /// Which container to pack several files into: `zip`, `tar` or `7z`.
    ///
    /// Ignored for a share with one file, which is served as that file.
    /// Defaulted rather than required, because the person following the link
    /// did not choose it — a client offering the choice sends one, and a bare
    /// link still works.
    #[serde(default)]
    pub format: Format,
}

/// Routes that are not part of the versioned API.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{code}/raw", get(raw))
        .route("/{code}/download", get(download))
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

    // `text/plain` whatever the uploader claimed, and `nosniff` so that a
    // browser cannot decide otherwise. Without both, a `<script>` uploaded as a
    // note becomes stored XSS on the origin that holds everyone's session
    // cookie. GitHub solves this by serving raw content from another domain;
    // self-hosted software cannot ask its operators for a second one, so the
    // headers have to carry it.
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
    // `permit` rather than here.
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

/// What a response calls the content, which is the only thing the two routes
/// disagree about.
///
/// Grouped rather than passed loose because they are decided together — a
/// download says `attachment` and an opaque type, a note says `inline` and
/// plain text — and neither route ever mixes them.
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
/// One file is not archived. Two or more are, and then this is the exact number
/// of bytes the response will carry — which is what lets it go out as a
/// `Content-Length` before the first byte is read.
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
/// Both routes reach this: they differ only in what they call the content, not
/// in how a transfer works. Keeping the range handling in one place is not tidy
/// housekeeping — `/raw` had none of it, so a client that asked for a range got
/// the whole note with a `200`, and a downloader that had already written the
/// first half wrote it again.
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

    // Several files leave as one archive, which has its own size, its own type
    // and its own name. One file is still that file — the feature that makes
    // archives exists, and that is not a reason to wrap something that is
    // already a single download.
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

    // `If-Range` is what keeps a resumed transfer from stitching two different
    // files together. A client sends the entity tag it had; if the share is not
    // that one any more we must ignore the `Range` and send the whole thing,
    // because serving a slice of the new content to a client holding the head of
    // the old one produces a corrupt result that nothing reports as an error.
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
    // without it the browser has no progress bar and no estimated time — on
    // exactly the transfers somebody is watching.
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
    // Tells the meter which share to credit. Set here rather than worked out
    // from the path in the middleware: this is the only place that has already
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
/// A ticket means the retrieval was paid for at `unlock` — that is the whole
/// point of it, and why a resumed download does not cost a second one. Without
/// one this is a direct retrieval (`curl` following a link), so the password is
/// checked and a use is spent here and now.
async fn permit(
    state: &AppState,
    share: &Share,
    headers: &HeaderMap,
    ticket: Option<&str>,
    method: &Method,
    source: &str,
) -> Result<Payload> {
    // First, because the answer decides whether an exhausted share is still
    // collectable — the caller who emptied it is the one now coming for the
    // bytes. `admit` is what turns anyone else away.
    let held = gate::holds_ticket(state, share, headers, ticket).await?;
    gate::admit(share, held)?;

    // `HEAD` asks what a `GET` would answer, and answering that is not a
    // retrieval. axum routes `HEAD` to the same handler as `GET`, so without
    // this a link checker, a prefetch or a chat client probing the URL spends
    // somebody's one-time share — the exact failure the ticket model exists to
    // prevent, reached by a route nobody thinks about.
    //
    // Without a ticket the password is still required: `HEAD` reveals the size,
    // and a locked share should not describe itself. With one it is not, for
    // the same reason a `GET` skips it — the ticket is the proof.
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
/// handed over. A `HEAD` does not reach it, on purpose and for the same reason
/// it does not spend a use: a link checker, a prefetch or a chat client
/// unfurling the URL is not a person collecting the file, and a log full of
/// those answers nobody's question.
///
/// # Why a failure here is not the caller's problem
///
/// The share has already been authorised and paid for by this point. Refusing
/// to serve it because a bookkeeping row would not insert would turn an
/// operator's optional record-keeping into an outage on the one path that has
/// to work. It is logged and the download continues.
async fn record_access(state: &AppState, share: &Share, headers: &HeaderMap, source: &str) {
    if !state.config.share.access_log.enabled {
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
/// Derived from the identifier and the size rather than from the bytes: hashing
/// the content would mean reading it to answer a `HEAD`, and a share's bytes
/// never change once written — only deletion and re-upload produce different
/// content, and that produces a different identifier.
fn etag_for(share: &Share, content: &Content) -> String {
    format!("\"{}-{}\"", share.id, content.size_bytes())
}

/// `Content-Disposition`, with the RFC 5987 form so non-ASCII names survive.
///
/// The plain `filename` is kept for old clients and the starred one carries the
/// real value; without the latter a Chinese or Japanese file name arrives
/// mangled in most browsers.
fn content_disposition(content: &Content) -> String {
    // The single file's own name, or the archive's. A share with one file is
    // still that file — it does not become an archive because the feature that
    // makes archives exists.
    //
    // The last component only, and after either separator. A stored name is
    // what the uploader sent, unchanged apart from a length and control-character
    // check — so it can carry the folder it came from, and it can be nothing but
    // separators. A header naming a path is a header saying where on the
    // recipient's disk to put the file, which is not ours to say and which RFC
    // 6266 tells us not to try.
    let name = content
        .single_file()
        .and_then(|file| file.filename.as_deref())
        .and_then(|filename| filename.rsplit(['/', '\\']).next())
        .filter(|last| !last.is_empty())
        .unwrap_or("download");
    // The fallback keeps anything printable ASCII, which includes the space —
    // `"my file.zip"` is a perfectly ordinary name and turning its space into an
    // underscore helps nobody. What cannot survive is a quote or a backslash,
    // since the value sits inside quotes.
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
/// Multi-range requests exist and are answered with the whole object instead:
/// they require a multipart response, no browser download manager sends one, and
/// the ones that do accept a `200` perfectly well.
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
/// Collecting the object first costs its size in memory, twice over if it is
/// then copied. Measured before this was a stream: an 80 MiB download raised
/// peak RSS by about 160 MB, which on the machines this software is aimed at is
/// the difference between serving a file and being killed. Uploads were careful
/// about exactly this; reads were not, which made every claim about large files
/// untrue in the direction that matters most — a share is written once and read
/// many times.
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
        // One file is that file. It keeps its name, its ranges and its ability
        // to be resumed — none of which an archive has, and all of which the
        // share that has always been single-file already had.
        [only] => stream_one(state, only, range).await,
        // Several become an archive, and an archive has no ranges: it is
        // produced as it is sent, so there is no earlier part to seek back to.
        many => stream_archive(state, many, format).await,
    }
}

/// Streams one file, honouring a range.
async fn stream_one(state: &AppState, file: &FilePart, range: Option<(u64, u64)>) -> Result<Body> {
    let Some(key) = file.storage.object_key() else {
        return Err(Error::NotFound);
    };
    let options = GetOptions {
        // HTTP ranges are inclusive at both ends; `GetRange::Bounded` is not.
        range: range.map(|(start, end)| GetRange::Bounded(start..end.saturating_add(1))),
        ..GetOptions::default()
    };
    let result = state
        .storage
        .get_opts(&ObjectPath::from(key), options)
        .await?;
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
/// entries go out one at a time, exactly as they are read from the store, and
/// the parts this adds — headers, descriptors, the directory — are a few dozen
/// bytes each.
///
/// # Why there is no range
///
/// The archive does not exist until it is sent, so there is no earlier part to
/// seek to. `serve` does not offer ranges for one, and a client asking for one
/// gets the whole thing.
async fn stream_archive(state: &AppState, files: &[FilePart], format: Format) -> Result<Body> {
    let storage = Arc::clone(&state.storage);
    // The same names `archived` predicted the size from. Two files can share a
    // name, and an archive with two identical entries unpacks to one file — so
    // the names are decided for the set, not one at a time, and both places have
    // to decide them the same way.
    let raw: Vec<Option<&str>> = files.iter().map(|f| f.filename.as_deref()).collect();
    let planned: Vec<(String, u64, Option<String>)> = archive::safe_names(&raw)
        .into_iter()
        .zip(files.iter())
        .map(|(name, file)| {
            (
                name,
                file.size_bytes,
                file.storage.object_key().map(str::to_owned),
            )
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
                    if let Some(key) = key {
                        let mut chunks = storage.get(&ObjectPath::from(key.as_str())).await?.into_stream();
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
                    if let Some(key) = key {
                        let mut chunks = storage.get(&ObjectPath::from(key.as_str())).await?.into_stream();
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
                // is built before anything is read — which is only possible
                // because it carries no per-file checksums. See the module.
                let listed: Vec<(String, u64)> = planned
                    .iter()
                    .map(|(name, size, _)| (name.clone(), *size))
                    .collect();
                let header = archive::sevenz::header(&listed);
                let packed: u64 = listed.iter().map(|(_, size)| *size).sum();
                let signature: Bytes = archive::sevenz::signature_header(packed, &header);
                yield signature;

                for (_, _, key) in &planned {
                    if let Some(key) = key {
                        let mut chunks = storage.get(&ObjectPath::from(key.as_str())).await?.into_stream();
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
        // Past the end is clamped, which is what RFC 9110 asks for.
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
            filename: filename.map(str::to_owned),
            content_type: None,
            size_bytes: 0,
            storage: crate::db::repository::share::StorageLocation::Inline,
        }])
    }

    /// Two files, which is what stops being one file's download.
    #[test]
    fn a_share_with_several_files_is_offered_as_an_archive() {
        let two = Content::Files(vec![
            FilePart {
                filename: Some("one.txt".to_owned()),
                content_type: None,
                size_bytes: 5,
                storage: crate::db::repository::share::StorageLocation::Inline,
            },
            FilePart {
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
