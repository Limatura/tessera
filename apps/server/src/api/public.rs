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

use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use object_store::{GetOptions, GetRange, path::Path as ObjectPath};
use serde::Deserialize;

use crate::{
    api::{
        extract::{ApiPath, ApiQuery},
        gate,
    },
    auth::extract::ClientAddress,
    db::repository::share::{Share, ShareKind},
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
    ClientAddress(source): ClientAddress,
    headers: HeaderMap,
) -> Result<Response> {
    let share = gate::open(&state, &code, &source).await?;
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

    let body = match payload {
        Payload::Headers => Body::empty(),
        Payload::Content => read_range(&state, &share, None).await?,
    };
    let mut response = (StatusCode::OK, body).into_response();
    let out = response.headers_mut();
    // `text/plain` whatever the uploader claimed, and `nosniff` so that a
    // browser cannot decide otherwise. Without both, a `<script>` uploaded as a
    // note becomes stored XSS on the origin that holds everyone's session
    // cookie. GitHub solves this by serving raw content from another domain;
    // self-hosted software cannot ask its operators for a second one, so the
    // headers have to carry it.
    out.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    out.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("inline"),
    );
    harden(out);
    Ok(response)
}

/// Serves a share's content as a download, honouring `Range`.
async fn download(
    State(state): State<AppState>,
    method: Method,
    ApiPath(code): ApiPath<String>,
    ApiQuery(query): ApiQuery<RetrievalQuery>,
    ClientAddress(source): ClientAddress,
    headers: HeaderMap,
) -> Result<Response> {
    let share = gate::open(&state, &code, &source).await?;
    let payload = permit(
        &state,
        &share,
        &headers,
        query.t.as_deref(),
        &method,
        &source,
    )
    .await?;
    let etag = etag_for(&share);

    // `If-Range` is what keeps a resumed download from stitching two different
    // files together. A browser sends the entity tag it had; if the share is not
    // that one any more we must ignore the `Range` and send the whole thing,
    // because serving a slice of the new file to a client holding the head of
    // the old one produces a corrupt result that nothing reports as an error.
    let honour_range = headers
        .get(header::IF_RANGE)
        .is_none_or(|value| value.as_bytes() == etag.as_bytes());
    let requested = honour_range
        .then(|| headers.get(header::RANGE))
        .flatten()
        .and_then(|value| value.to_str().ok())
        .map(|value| parse_range(value, share.size_bytes))
        .transpose()?;

    let (status, range) = match requested {
        Some(range) => (StatusCode::PARTIAL_CONTENT, Some(range)),
        None => (StatusCode::OK, None),
    };

    let length = range.map_or(share.size_bytes, |(start, end)| {
        end.saturating_sub(start).saturating_add(1)
    });
    let body = match payload {
        Payload::Headers => Body::empty(),
        Payload::Content => read_range(&state, &share, range).await?,
    };
    let mut response = (status, body).into_response();
    let out = response.headers_mut();

    // Never the media type the uploader claimed. Serving that back is the same
    // stored-XSS hole as `/raw`, reached from the other direction.
    out.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
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
    if let Ok(value) = HeaderValue::from_str(&content_disposition(&share)) {
        out.insert(header::CONTENT_DISPOSITION, value);
    }
    if let Some((start, end)) = range
        && let Ok(value) =
            HeaderValue::from_str(&format!("bytes {start}-{end}/{}", share.size_bytes))
    {
        out.insert(header::CONTENT_RANGE, value);
    }
    harden(out);
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
    // `HEAD` asks what a `GET` would answer, and answering that is not a
    // retrieval. axum routes `HEAD` to the same handler as `GET`, so without
    // this a link checker, a prefetch or a chat client probing the URL spends
    // somebody's one-time share — the exact failure the ticket model exists to
    // prevent, reached by a route nobody thinks about.
    //
    // The password is still required: `HEAD` reveals the size, and a locked
    // share should not describe itself.
    if method == Method::HEAD {
        gate::unlock(state, share, headers, source).await?;
        return Ok(Payload::Headers);
    }

    if gate::holds_ticket(state, share, headers, ticket).await? {
        return Ok(Payload::Content);
    }
    gate::unlock(state, share, headers, source).await?;
    gate::spend(state, share).await?;
    Ok(Payload::Content)
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
fn etag_for(share: &Share) -> String {
    format!("\"{}-{}\"", share.id, share.size_bytes)
}

/// `Content-Disposition`, with the RFC 5987 form so non-ASCII names survive.
///
/// The plain `filename` is kept for old clients and the starred one carries the
/// real value; without the latter a Chinese or Japanese file name arrives
/// mangled in most browsers.
fn content_disposition(share: &Share) -> String {
    let name = share.filename.as_deref().unwrap_or("download");
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
async fn read_range(state: &AppState, share: &Share, range: Option<(u64, u64)>) -> Result<Body> {
    let Some(key) = share.storage.object_key() else {
        let content = state
            .shares
            .inline_content(share.id)
            .await?
            .ok_or(Error::NotFound)?;
        return Ok(Body::from(slice(&content, range)));
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
        let share = test_share(Some("專案 v2.zip"));
        let value = content_disposition(&share);
        assert!(value.starts_with("attachment; "), "{value}");
        assert!(value.contains(r#"filename="__ v2.zip""#), "{value}");
        assert!(
            value.contains("filename*=UTF-8''%E5%B0%88%E6%A1%88%20v2.zip"),
            "{value}"
        );
    }

    #[test]
    fn a_share_with_no_filename_still_downloads() {
        let value = content_disposition(&test_share(None));
        assert!(value.contains(r#"filename="download""#), "{value}");
    }

    fn test_share(filename: Option<&str>) -> Share {
        use tessera_core::{RetrievalCode, ShareId};

        use crate::db::repository::share::StorageLocation;

        Share {
            id: ShareId::generate(),
            code: RetrievalCode::parse("ABCD1234").expect("a valid code"),
            user_id: None,
            kind: ShareKind::File,
            password_hash: None,
            use_limit: None,
            use_count: 0,
            expires_at: None,
            disabled_at: None,
            title: None,
            filename: filename.map(str::to_owned),
            content_type: None,
            language: None,
            size_bytes: 10,
            storage: StorageLocation::Inline,
            og_config: None,
            management_token_hash: None,
            view_count: 0,
            last_accessed_at: None,
            created_at: time::OffsetDateTime::now_utc(),
            updated_at: time::OffsetDateTime::now_utc(),
        }
    }
}
