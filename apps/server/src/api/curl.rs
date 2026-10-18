//! Uploading with nothing but `curl`.
//!
//! ```sh
//! curl -T report.pdf https://share.example.com/
//! echo hello | curl -T - https://share.example.com/
//! ```
//!
//! `curl -T` sends a `PUT` to the URL with the file's name added when the URL
//! ends in `/`, so this answers `PUT /{name}` at the site root. The share is
//! made by the same code as `POST /api/v1/shares`, under the same limits and
//! permissions; an API token goes in `Authorization` as anywhere else.
//!
//! The answer is the link as one line of plain text, ready for `$(...)`, or
//! the share as JSON with `Accept: application/json`. Options travel in
//! headers rather than the query, so the URL stays as typed and a password
//! never reaches an access log:
//!
//! - `X-Share-Expires: 30m`, `12h`, `1d`, `2w`
//! - `X-Share-Max-Downloads: 3`
//! - `X-Share-Password: ...`, the header `PATCH /shares/{code}` already takes
//!
//! A name of `-`, which is what `curl -T -` sends from standard input, or no
//! name at all, makes a note instead of a file.

use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

use crate::{
    api::shares::{CreateShareParams, ShareResponse, create_share},
    auth::extract::MaybeUser,
    error::{Error, Result},
    state::AppState,
};

/// `PUT /`: a note, since there is no name to give a file.
pub async fn upload_unnamed(
    State(state): State<AppState>,
    MaybeUser(current): MaybeUser,
    headers: HeaderMap,
    body: Body,
) -> Response {
    upload(&state, current, None, &headers, body).await
}

/// `PUT /{name}`: a file under that name, or a note for `-`.
pub async fn upload_named(
    State(state): State<AppState>,
    MaybeUser(current): MaybeUser,
    Path(name): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let name = Some(name).filter(|name| name != "-");
    upload(&state, current, name, &headers, body).await
}

async fn upload(
    state: &AppState,
    current: Option<crate::auth::extract::CurrentUser>,
    name: Option<String>,
    headers: &HeaderMap,
    body: Body,
) -> Response {
    let json = wants_json(headers);
    let made = async {
        let params = params(name, headers)?;
        create_share(state, current, params, headers, body).await
    }
    .await;
    match made {
        Ok(share) if json => (StatusCode::CREATED, axum::Json(share)).into_response(),
        Ok(share) => as_text(state, headers, &share),
        Err(error) if json => error.into_response(),
        Err(error) => error_as_text(error),
    }
}

/// The share's options, from the request's headers.
fn params(name: Option<String>, headers: &HeaderMap) -> Result<CreateShareParams> {
    let mut params = CreateShareParams {
        filename: name,
        ..CreateShareParams::default()
    };
    if let Some(expires) = header_text(headers, "x-share-expires")? {
        let minutes = lifetime_minutes(expires).ok_or_else(|| {
            Error::invalid("X-Share-Expires", "duration").with_message(
                "`X-Share-Expires` must be a number and a unit: `30m`, `12h`, `1d` or `2w`",
            )
        })?;
        if minutes % 60 == 0 {
            params.expires_in_hours = Some(minutes / 60);
        } else {
            params.expires_in_minutes = Some(minutes);
        }
    }
    if let Some(uses) = header_text(headers, "x-share-max-downloads")? {
        params.max_uses = Some(uses.trim().parse().map_err(|_| {
            Error::invalid("X-Share-Max-Downloads", "integer")
                .with_message("`X-Share-Max-Downloads` must be a whole number, at least 1")
        })?);
    }
    Ok(params)
}

fn header_text<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>> {
    headers
        .get(name)
        .map(|value| value.to_str().map_err(|_| Error::invalid(name, "ascii")))
        .transpose()
}

/// `30m`, `12h`, `1d`, `2w` as minutes. `None` for anything else, zero
/// included: a share that is gone before it is made is a mistake.
fn lifetime_minutes(text: &str) -> Option<u64> {
    let text = text.trim();
    let split = text.find(|c: char| !c.is_ascii_digit())?;
    let (number, unit) = text.split_at(split);
    let number: u64 = number.parse().ok().filter(|number| *number > 0)?;
    let per = match unit.trim() {
        "m" | "min" => 1,
        "h" => 60,
        "d" => 60 * 24,
        "w" => 60 * 24 * 7,
        _ => return None,
    };
    number.checked_mul(per)
}

/// Whether the caller asked for JSON rather than a line of text.
fn wants_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|accept| accept.contains("application/json"))
}

/// The link, alone on its line. The code and an anonymous upload's management
/// token ride in headers, for a script that wants them without parsing.
fn as_text(state: &AppState, headers: &HeaderMap, share: &ShareResponse) -> Response {
    let origin = crate::api::preview::origin(&state.config(), headers).unwrap_or_default();
    let mut response = (
        StatusCode::CREATED,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        format!("{origin}{}\n", share.path),
    )
        .into_response();
    let out = response.headers_mut();
    if let Ok(code) = HeaderValue::from_str(&share.code) {
        out.insert("x-share-code", code);
    }
    if let Some(token) = share
        .management_token
        .as_deref()
        .and_then(|token| HeaderValue::from_str(token).ok())
    {
        out.insert("x-share-management-token", token);
    }
    response
}

/// The error as `error: <message>` on one line, keeping its status and any
/// header it carries, such as `Retry-After`.
fn error_as_text(error: Error) -> Response {
    let message = error.public_message();
    let mut response = error.into_response();
    *response.body_mut() = Body::from(format!("error: {message}\n"));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response.headers_mut().remove(header::CONTENT_LENGTH);
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifetimes_read_the_way_people_write_them() {
        assert_eq!(lifetime_minutes("30m"), Some(30));
        assert_eq!(lifetime_minutes("12h"), Some(720));
        assert_eq!(lifetime_minutes("1d"), Some(1440));
        assert_eq!(lifetime_minutes(" 2w "), Some(20160));
        assert_eq!(lifetime_minutes("0d"), None);
        assert_eq!(lifetime_minutes("1y"), None);
        assert_eq!(lifetime_minutes("12"), None);
        assert_eq!(lifetime_minutes("h"), None);
    }
}
