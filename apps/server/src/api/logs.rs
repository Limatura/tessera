//! `/admin/logs`: the server's log, as the panel reads it. See [`crate::log_files`].
//!
//! All four need `logs:read` and a session. The log names the addresses,
//! accounts and shares behind every request, so it is read by a person at a
//! keyboard, not by a script holding a token.

// utoipa's `params(..)` emits a fully-qualified path to the type outside the
// function it decorates, so the suppression has to be module-wide. Same
// arrangement as `super::admin`.
#![expect(
    unused_qualifications,
    reason = "emitted by utoipa's params(..), not written here"
)]

use std::convert::Infallible;

use axum::{
    Json,
    body::Body,
    extract::State,
    http::{HeaderValue, header},
    response::{IntoResponse as _, Response, sse::Event},
};
use serde::{Deserialize, Serialize};
use tessera_core::Permission;
use time::{Date, OffsetDateTime, format_description::well_known::Rfc3339};
use utoipa::{IntoParams, ToSchema};

use crate::{
    api::{
        ADMIN_TAG,
        admin::{admitted, require_admin_feature},
        extract::{ApiPath, ApiQuery},
    },
    auth::extract::CurrentUser,
    config::LogLevel,
    error::{Error, ErrorCode, Result},
    log_files::{Cursor, Entry, Filter, Query},
    state::AppState,
    topic::RecvError,
};

/// How many lines a page holds when the caller does not say.
const DEFAULT_PAGE: u32 = 200;
/// The most lines one page holds.
const MAX_PAGE: u32 = 1000;

/// One day's file.
#[derive(Debug, Serialize, ToSchema)]
pub struct LogFile {
    /// The day it holds, in UTC.
    #[schema(example = "2026-10-19")]
    pub date: String,
    /// How large it is.
    pub size_bytes: u64,
}

/// Query of `GET /admin/logs`.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct LogQueryParams {
    /// Nothing logged before this, RFC 3339.
    #[serde(default)]
    pub since: Option<String>,
    /// Nothing logged after this, RFC 3339.
    #[serde(default)]
    pub until: Option<String>,
    /// The least severe level wanted: `warn` is warnings and errors.
    #[serde(default)]
    pub level: Option<LogLevel>,
    /// Text the message or a field value contains, ignoring case.
    #[serde(default)]
    pub q: Option<String>,
    /// How many lines, 200 when omitted, at most 1000.
    #[serde(default)]
    pub limit: Option<u32>,
    /// `next_cursor` from the previous page.
    #[serde(default)]
    pub cursor: Option<String>,
}

/// Query of `GET /admin/logs/stream`.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct LogStreamParams {
    /// The least severe level wanted.
    #[serde(default)]
    pub level: Option<LogLevel>,
    /// Text the message or a field value contains, ignoring case.
    #[serde(default)]
    pub q: Option<String>,
}

/// Lines from `GET /admin/logs`, newest first.
#[derive(Debug, Serialize, ToSchema)]
pub struct LogPage {
    /// The lines.
    pub data: Vec<Entry>,
    /// Pass back as `cursor` for older lines. `null` when there are none.
    ///
    /// A page can be short, or even empty, and still have a cursor: a query
    /// stops reading after a fixed amount so a rare match costs several
    /// requests rather than one very long one.
    pub next_cursor: Option<String>,
}

/// The second kind of event on `GET /admin/logs/stream`.
#[derive(Debug, Serialize, ToSchema)]
pub struct LogsLagged {
    /// How many lines this stream missed by falling behind. They are in the
    /// files; `GET /admin/logs` finds them.
    pub skipped: u64,
}

/// Lists the days there are log files for, newest first.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` without `logs:read` or with an API
/// token, `404` when this instance has no administration.
#[utoipa::path(
    get,
    path = "/admin/logs/files",
    operation_id = "list_log_files",
    tag = ADMIN_TAG,
    responses(
        (status = 200, description = "The days kept", body = Vec<LogFile>),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not read the log, or authenticated with an API token", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn list_files(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<LogFile>>> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::LogsRead)?;
    let days = state.logs.days().await?;
    Ok(Json(
        days.into_iter()
            .map(|day| LogFile {
                date: day_name(day.date),
                size_bytes: day.size_bytes,
            })
            .collect(),
    ))
}

/// Reads log lines, newest first, a page at a time.
///
/// # Errors
///
/// `400` for a time, cursor or level that does not parse, `401` when nothing
/// was signed in, `403` without `logs:read` or with an API token.
#[utoipa::path(
    get,
    path = "/admin/logs",
    operation_id = "read_logs",
    tag = ADMIN_TAG,
    params(LogQueryParams),
    responses(
        (status = 200, description = "A page of lines", body = LogPage),
        (status = 400, description = "A parameter does not parse", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not read the log, or authenticated with an API token", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn read(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiQuery(params): ApiQuery<LogQueryParams>,
) -> Result<Json<LogPage>> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::LogsRead)?;

    let query = Query {
        since: instant("since", params.since.as_deref())?,
        until: instant("until", params.until.as_deref())?,
        filter: Filter::new(params.level, params.q.as_deref()),
        limit: usize::try_from(params.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE))
            .unwrap_or(1),
        cursor: params
            .cursor
            .as_deref()
            .map(|cursor| {
                cursor
                    .parse::<Cursor>()
                    .map_err(|()| invalid("`cursor` is not one this endpoint handed out"))
            })
            .transpose()?,
    };
    let page = state.logs.page(query).await?;
    Ok(Json(LogPage {
        data: page.entries,
        next_cursor: page.next.map(|next| next.to_string()),
    }))
}

/// Streams log lines as they are written.
///
/// `entry` events carry one line each, with the line's `id` as the event id.
/// A `lagged` event says the stream fell behind and missed some; read them
/// with `GET /admin/logs`. Lines from before the stream opened are not sent.
///
/// # Errors
///
/// `400` for a level that does not parse, `401` when nothing was signed in,
/// `403` without `logs:read` or with an API token.
#[utoipa::path(
    get,
    path = "/admin/logs/stream",
    operation_id = "stream_logs",
    tag = ADMIN_TAG,
    params(LogStreamParams),
    responses(
        (status = 200, description = "`entry` events, each a line; `lagged` events, each a count", content_type = "text/event-stream", body = Entry),
        (status = 400, description = "A parameter does not parse", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not read the log, or authenticated with an API token", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn stream(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiQuery(params): ApiQuery<LogStreamParams>,
) -> Result<Response> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::LogsRead)?;

    let filter = Filter::new(params.level, params.q.as_deref());
    let mut lines = state.logs.subscribe();
    let events = async_stream::stream! {
        loop {
            match lines.recv().await {
                Ok(entry) => {
                    if !filter.matches(&entry) {
                        continue;
                    }
                    if let Ok(data) = serde_json::to_string(&*entry) {
                        yield Ok::<_, Infallible>(
                            Event::default().event("entry").id(entry.id.as_str()).data(data),
                        );
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    if let Ok(data) = serde_json::to_string(&LogsLagged { skipped }) {
                        yield Ok(Event::default().event("lagged").data(data));
                    }
                }
                Err(RecvError::Closed) => break,
            }
        }
    };
    Ok(super::sse::respond(&state, events))
}

/// Downloads one day's file.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` without `logs:read` or with an API
/// token, `404` when there is no file for that day or it is not a date.
#[utoipa::path(
    get,
    path = "/admin/logs/files/{date}",
    operation_id = "download_log_file",
    tag = ADMIN_TAG,
    params(("date" = String, Path, description = "The day, `YYYY-MM-DD` in UTC")),
    responses(
        (status = 200, description = "The day's lines, one JSON object each", content_type = "application/x-ndjson", body = String),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not read the log, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "No file for that day", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn download(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(date): ApiPath<String>,
) -> Result<Response> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::LogsRead)?;

    // Parsed into a date first: the path is built from that, never from the
    // text the request sent.
    let date = Date::parse(&date, crate::log_files::DAY).map_err(|_| Error::NotFound)?;
    let file = match tokio::fs::File::open(state.logs.file_for(date)).await {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Err(Error::NotFound),
        Err(error) => return Err(error.into()),
    };
    let disposition = format!("attachment; filename=\"tessera-{}.jsonl\"", day_name(date));
    let mut response = Body::from_stream(tokio_util::io::ReaderStream::new(file)).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-ndjson"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    Ok(response)
}

fn day_name(date: Date) -> String {
    date.format(crate::log_files::DAY).unwrap_or_default()
}

fn instant(name: &str, value: Option<&str>) -> Result<Option<OffsetDateTime>> {
    value
        .map(|value| {
            OffsetDateTime::parse(value, &Rfc3339)
                .map_err(|_| invalid(&format!("`{name}` is not an RFC 3339 time")))
        })
        .transpose()
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorCode::InvalidQuery).with_message(message.to_owned())
}
