//! Measuring the line between a visitor and this instance.
//!
//! Two endpoints that move throwaway bytes: one sends them, one reads and
//! discards them. Megabits and milliseconds are computed in the browser from
//! how long the transfer took; nothing here times anything, since the only
//! clock that matters is at the other end.
//!
//! The upload side reads its entire body inside the handler, so it is exempt
//! from the wall-clock budget in [`crate::api::timeout`]: a budget there would
//! cap the size a slow line can test, not how long a stalled request may hold
//! a slot.
//!
//! # Keeping it from becoming free bandwidth
//!
//! Off unless the operator enables it, then accounts only unless they open it
//! up, and every test capped at `speed.max_bytes`. Each test also counts
//! against `auth.throttle.speed`, keyed on the source, so an open instance
//! still bounds what one address can use per hour. See
//! [`SpeedConfig`](crate::config::SpeedConfig).

// `params(DownloadParams)` makes utoipa emit a fully-qualified path, and the
// generated code lands outside the function, so the suppression is module-wide
// as in `shares`.
#![expect(
    unused_qualifications,
    reason = "emitted by utoipa's params(..), not written here"
)]

use axum::{
    body::Body,
    extract::State,
    http::{HeaderValue, header},
    response::{IntoResponse as _, Response},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    api::extract::ApiQuery,
    auth::{
        extract::{ClientAddress, MaybeUser},
        proxy::Origin,
        throttle::Decision,
    },
    error::{Error, ErrorCode, Result},
    state::AppState,
};

/// Tag the speed endpoints appear under.
pub const SPEED_TAG: &str = "speed";

/// How much is written at a time.
const CHUNK: usize = 64 * 1024;

/// How much of a test to send.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct DownloadParams {
    /// Bytes to send. Omit for the instance maximum. Larger values are refused,
    /// not silently shortened, so a client never reports a transfer it did not
    /// make.
    #[param(example = 26_214_400_u64)]
    pub bytes: Option<u64>,
}

/// What an upload test read.
#[derive(Debug, Serialize, ToSchema)]
pub struct SpeedUploadResponse {
    /// How many bytes arrived. The client times its own request and divides.
    #[schema(example = 26_214_400_u64)]
    pub received_bytes: u64,
}

/// Sends `bytes` bytes of filler.
///
/// # Errors
///
/// `404` when the instance does not offer the test, or does not offer it to
/// this caller, `400` for more than `speed.max_bytes`, `429` when this source
/// has run its tests for the hour.
#[utoipa::path(
    get,
    path = "/speed/download",
    operation_id = "speed_download",
    tag = SPEED_TAG,
    params(DownloadParams),
    responses(
        (status = 200, description = "The bytes", content_type = "application/octet-stream"),
        (status = 400, description = "More than this instance allows", body = crate::error::ErrorBody),
        (status = 404, description = "Not offered here", body = crate::error::ErrorBody),
        (status = 429, description = "Too many tests from this source", body = crate::error::ErrorBody),
    ),
    security((), ("session" = []), ("bearer" = [])),
)]
pub async fn download(
    State(state): State<AppState>,
    MaybeUser(current): MaybeUser,
    ApiQuery(params): ApiQuery<DownloadParams>,
    ClientAddress(Origin { source, .. }): ClientAddress,
) -> Result<Response> {
    let limit = offered(&state, current.is_some())?;
    let bytes = asked_for(params.bytes, limit)?;
    spend(&state, &source)?;

    let mut left = bytes;
    let body = Body::from_stream(async_stream::stream! {
        let mut filler = Filler::new();
        while left > 0 {
            let take = usize::try_from(left.min(to_u64(CHUNK))).unwrap_or(CHUNK);
            left -= to_u64(take);
            yield Ok::<_, std::io::Error>(filler.next(take));
        }
    });

    let mut response = body.into_response();
    let out = response.headers_mut();
    out.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    // Timing a cached response measures nothing, and a compressing proxy would
    // be timing its own compressor.
    out.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-transform"),
    );
    if let Ok(value) = HeaderValue::from_str(&bytes.to_string()) {
        out.insert(header::CONTENT_LENGTH, value);
    }
    Ok(response)
}

/// Reads a body and discards it.
///
/// # Errors
///
/// As [`download`], and `413` when the body runs past `speed.max_bytes`.
#[utoipa::path(
    post,
    path = "/speed/upload",
    operation_id = "speed_upload",
    tag = SPEED_TAG,
    request_body(content = Vec<u8>, description = "Anything; it is counted and dropped", content_type = "application/octet-stream"),
    responses(
        (status = 200, description = "How much arrived", body = SpeedUploadResponse),
        (status = 404, description = "Not offered here", body = crate::error::ErrorBody),
        (status = 413, description = "More than this instance allows", body = crate::error::ErrorBody),
        (status = 429, description = "Too many tests from this source", body = crate::error::ErrorBody),
    ),
    security((), ("session" = []), ("bearer" = [])),
)]
pub async fn upload(
    State(state): State<AppState>,
    MaybeUser(current): MaybeUser,
    ClientAddress(Origin { source, .. }): ClientAddress,
    body: Body,
) -> Result<axum::Json<SpeedUploadResponse>> {
    let limit = offered(&state, current.is_some())?;
    spend(&state, &source)?;

    // Read to the end, counting. Nothing is kept: a speed test that stored
    // anything would be an upload endpoint with no quota.
    let mut received = 0_u64;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = tokio_stream::StreamExt::next(&mut stream).await {
        let chunk = chunk.map_err(|error| {
            Error::new(ErrorCode::BodyIncomplete).with_message(error.to_string())
        })?;
        received = received.saturating_add(to_u64(chunk.len()));
        if received > limit {
            return Err(Error::PayloadTooLarge);
        }
    }
    Ok(axum::Json(SpeedUploadResponse {
        received_bytes: received,
    }))
}

/// The cap for this caller, or `404` when the test is not offered to them.
///
/// Not `403`: to a visitor, an instance that does not offer this should look
/// like one without the feature, which it effectively is.
fn offered(state: &AppState, signed_in: bool) -> Result<u64> {
    let speed = state.config().speed;
    if !speed.enabled || (!speed.anonymous && !signed_in) {
        return Err(Error::NotFound);
    }
    Ok(speed.max_bytes)
}

/// How much to send: the requested amount, or the full allowance.
fn asked_for(bytes: Option<u64>, limit: u64) -> Result<u64> {
    match bytes {
        None => Ok(limit),
        Some(0) => Err(Error::out_of_range("bytes", Some(1), Some(limit))
            .with_message("`bytes` must be at least one")),
        Some(asked) if asked <= limit => Ok(asked),
        Some(_) => Err(
            Error::out_of_range("bytes", Some(1), Some(limit)).with_message(format!(
                "this instance tests at most {limit} bytes at a time"
            )),
        ),
    }
}

/// Counts one test against the source, or refuses it.
fn spend(state: &AppState, source: &str) -> Result<()> {
    let counter = &state.throttles.speed;
    if let Decision::Deny { retry_after } = counter.check(source) {
        return Err(Error::TooManyRequests {
            retry_after_secs: retry_after.as_secs(),
        });
    }
    counter.record_failure(source);
    Ok(())
}

const fn to_u64(value: usize) -> u64 {
    #[expect(
        clippy::as_conversions,
        reason = "const context; `u64::try_from` is not const"
    )]
    let widened = value as u64;
    widened
}

/// Cheap incompressible bytes.
///
/// Zeroes would measure a proxy's compressor, not the line, and reading
/// megabytes from the system random source only to discard them wastes it. An
/// xorshift keeps the stream incompressible at a gigabyte a second.
struct Filler {
    state: u64,
}

impl Filler {
    fn new() -> Self {
        // Seeded per transfer so two tests do not send identical bytes that a
        // cache could notice. Nothing here is secret, so the clock and a
        // counter are enough.
        static RUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| u64::try_from(since.as_nanos()).unwrap_or(0));
        let run = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self {
            state: (nanos ^ run.rotate_left(32)) | 1,
        }
    }

    fn next(&mut self, len: usize) -> bytes::Bytes {
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            self.state ^= self.state << 13;
            self.state ^= self.state >> 7;
            self.state ^= self.state << 17;
            let word = self.state.to_le_bytes();
            let take = (len - out.len()).min(word.len());
            out.extend_from_slice(word.get(..take).unwrap_or(&word));
        }
        bytes::Bytes::from(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_is_as_large_as_it_asked_or_as_the_instance_allows() {
        assert_eq!(asked_for(None, 100).unwrap(), 100);
        assert_eq!(asked_for(Some(40), 100).unwrap(), 40);
        assert!(asked_for(Some(0), 100).is_err());
        let refusal = asked_for(Some(101), 100).unwrap_err();
        assert_eq!(refusal.details()["max"], 100);
        assert!(refusal.public_message().contains("100"));
    }

    /// The bytes must be worth timing: a proxy compressing a stream of zeroes
    /// would report an impossibly fast line.
    #[test]
    fn the_filler_does_not_repeat_itself() {
        let mut filler = Filler::new();
        let first = filler.next(4096);
        let second = filler.next(4096);
        assert_eq!(first.len(), 4096);
        assert_ne!(first, second);
        assert!(
            first.windows(64).all(|window| window != &first[..64]) || first[..64] != first[64..128]
        );
    }
}
