//! Any scanner that speaks this contract over HTTP.
//!
//! The file is the `POST` body, streamed. `200` with `{"clean": true}` or
//! `{"clean": false, "threat": "..."}` is a verdict; `413` means too large to
//! scan, and is treated like clamd's size limit; anything else, including no
//! answer, means the scanner is unavailable and the file is asked about again
//! later. `Authorization` carries `scan.authorization` when that is set.

use bytes::Bytes;
use serde::Deserialize;
use tokio_stream::Stream;

use super::{Unreachable, Verdict};

#[derive(Deserialize)]
struct Answer {
    clean: bool,
    #[serde(default)]
    threat: Option<String>,
}

/// Scans `body`, `length` bytes long, with the scanner at `url`.
///
/// # Errors
///
/// [`Unreachable`] for anything but a `200` carrying a verdict, or a `413`.
pub async fn scan<S, E>(
    url: url::Url,
    authorization: &str,
    body: S,
    length: u64,
) -> Result<Verdict, Unreachable>
where
    S: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: std::fmt::Display + Send + 'static,
{
    let client = reqwest::Client::builder()
        .connect_timeout(super::worker::CONNECT_TIMEOUT)
        .build()?;
    let body = tokio_stream::StreamExt::map(body, |piece| {
        piece.map_err(|error| std::io::Error::other(error.to_string()))
    });
    let mut request = client
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .header(reqwest::header::CONTENT_LENGTH, length)
        .body(reqwest::Body::wrap_stream(body));
    if !authorization.is_empty() {
        request = request.header(reqwest::header::AUTHORIZATION, authorization);
    }
    let response = request.send().await?;
    match response.status() {
        reqwest::StatusCode::OK => {
            let answer: Answer = response
                .json()
                .await
                .map_err(|error| Unreachable::Answer(error.to_string()))?;
            Ok(if answer.clean {
                Verdict::Clean
            } else {
                Verdict::Infected(answer.threat.unwrap_or_else(|| "unnamed".to_owned()))
            })
        }
        reqwest::StatusCode::PAYLOAD_TOO_LARGE => Ok(Verdict::TooLarge),
        status => Err(Unreachable::Answer(format!("answered {status}"))),
    }
}
