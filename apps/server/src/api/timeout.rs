//! How long a request is given, and why uploads are asked a different question.
//!
//! # The two questions
//!
//! Every route except the ones that receive a file gets a wall-clock budget:
//! from the moment the request arrives to the moment a response is produced.
//! That is the right shape for work whose duration does not depend on the
//! caller's line: a query, a login, a listing.
//!
//! An upload's duration depends on almost nothing else. A wall-clock budget
//! large enough for the biggest file an operator allows, sent over the slowest
//! line they expect, is also how long a client that connects and then sends
//! nothing gets to hold a slot. Measured on the default settings before this
//! existed: a 4 KiB body dribbled out over two seconds was answered `408` at
//! exactly the one-second budget it was given. Size had nothing to do with it.
//!
//! So uploads are asked whether the other end is still sending, not how long
//! they have been going. That question lives in the body loop, next to the only
//! thing that can answer it. See `shares::ingest_into`.
//!
//! # Why a middleware and not `TimeoutLayer`
//!
//! `TimeoutLayer` is built once, from the configured value, and holds it. This
//! reads the setting per request, so an operator can change it
//! without a restart, and is also the only place that can see which
//! route a request is for before deciding whether to apply a budget at all.

use axum::{
    extract::{Request, State},
    http::Method,
    middleware::Next,
    response::{IntoResponse as _, Response},
};

use crate::{error::Error, state::AppState};

/// Applies the wall-clock budget, except to the routes that receive a file.
pub async fn enforce(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if carries_an_upload(request.method(), request.uri().path()) {
        return next.run(request).await;
    }
    let budget = state.config().server.request_timeout();
    (tokio::time::timeout(budget, next.run(request)).await)
        .unwrap_or_else(|_| Error::Timeout.into_response())
}

/// Whether this request's body is bytes being sent to us over the caller's
/// line.
///
/// Matched on the path, not on `MatchedPath`, which axum fills in during
/// routing and is not there yet: this layer wraps the router. The routes are
/// named exactly, because `POST /shares/{code}/unlock` and
/// `POST /shares/{code}/events` sit under the same prefix and are ordinary
/// requests that should keep their budget.
///
/// The speed test's upload is here for the same reason as a file: the handler
/// reads the whole body, so the budget would be a cap on the *size* a slow
/// line can test rather than on how long a stalled request may hold a slot.
fn carries_an_upload(method: &Method, path: &str) -> bool {
    if method != Method::POST {
        return false;
    }
    if path == "/api/v1/speed/upload" {
        return true;
    }
    let Some(rest) = path.strip_prefix("/api/v1/shares") else {
        return false;
    };
    // `POST /shares` creates a share from its first file; `POST
    // /shares/{code}/files` adds the rest.
    rest.is_empty() || rest == "/" || rest.strip_suffix("/files").is_some_and(|c| !c.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_routes_that_receive_bytes_are_exempt() {
        assert!(carries_an_upload(&Method::POST, "/api/v1/shares"));
        assert!(carries_an_upload(
            &Method::POST,
            "/api/v1/shares/abc123/files"
        ));
        // A speed test on a slow line is a long request by design. Left in the
        // budget, the test would stop at whatever fits in thirty seconds.
        assert!(carries_an_upload(&Method::POST, "/api/v1/speed/upload"));
        assert!(!carries_an_upload(&Method::GET, "/api/v1/speed/upload"));
    }

    /// These share the prefix and are ordinary requests. Exempting them would
    /// hand an anonymous caller a socket that is never reclaimed.
    #[test]
    fn the_other_routes_under_the_same_prefix_keep_their_budget() {
        for path in [
            "/api/v1/shares/abc123/unlock",
            "/api/v1/shares/abc123/events",
            "/api/v1/shares/abc123",
            "/api/v1/shares/files",
        ] {
            assert!(!carries_an_upload(&Method::POST, path), "{path}");
        }
    }

    /// Listing shares is not uploading one, whatever the path says.
    #[test]
    fn only_post_is_an_upload() {
        for method in [Method::GET, Method::DELETE, Method::PUT, Method::HEAD] {
            assert!(!carries_an_upload(&method, "/api/v1/shares"), "{method}");
        }
    }
}
