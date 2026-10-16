//! A fake malware scanner, speaking the HTTP contract in `docs/scanning.md`.
//!
//! Flags the EICAR test string, as every real scanner does, and passes
//! everything else. Answers `413` past a size small enough to hit in testing,
//! so the "too large to scan" path can be tried too.

use axum::{
    Json, Router,
    body::Bytes,
    http::StatusCode,
    response::{IntoResponse as _, Response},
    routing::post,
};

/// The harmless file every scanner is built to catch.
const EICAR: &[u8] = br"X5O!P%@AP[4\PZX54(P^)7CC)7}$EICAR-STANDARD-ANTIVIRUS-TEST-FILE!$H+H*";

/// Past this, the answer is "too large to scan".
const LIMIT: usize = 50 * 1024 * 1024;

pub(crate) fn router() -> Router {
    Router::new()
        .route("/scan", post(scan))
        // The server's own body limit is tighter than a scanner's would be;
        // this one only has to hold what the limit above lets through.
        .layer(axum::extract::DefaultBodyLimit::max(LIMIT + 1))
}

async fn scan(body: Bytes) -> Response {
    if body.len() > LIMIT {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    if body.windows(EICAR.len()).any(|window| window == EICAR) {
        return Json(serde_json::json!({ "clean": false, "threat": "Eicar-Test-Signature" }))
            .into_response();
    }
    Json(serde_json::json!({ "clean": true })).into_response()
}
