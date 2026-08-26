//! The parts of the object store suite that need a multipart upload.
//!
//! # Why these are their own binary
//!
//! Not every store can hold an upload open across requests. `object_store`'s
//! HTTP backend returns `NotImplemented` for `put_multipart`; fake-gcs-server
//! does not implement the XML multipart API at all. Those are real backends
//! somebody deploys on, and they should be covered for everything they *can*
//! do rather than left untested because of what they cannot.
//!
//! Split by binary rather than skipped at runtime, because the alternative is a
//! test that returns early and reports "passed" — which is the one thing the
//! sibling module says it will not do. A CI job either runs this binary or it
//! does not, and which it did is in the log.
//!
//! ```text
//! # Against the same store the sibling suite describes:
//! cargo nextest run --run-ignored all -E 'binary(object_store_multipart)'
//! ```

// The helpers below sit outside a `#[test]` body, which is as far as
// clippy.toml's test exemptions reach.
#![expect(clippy::expect_used, reason = "test fixture; see module docs")]

mod common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use common::{Auth, TestApp, body_json, body_string, expect_status};

/// Overridable so the suite can point at an existing bucket instead of the
/// throwaway container the docs describe.
const DEFAULT_URL: &str = "s3://tessera";

fn storage_url() -> String {
    std::env::var("TESSERA_TEST_STORAGE_URL").unwrap_or_else(|_| DEFAULT_URL.to_owned())
}

async fn app() -> TestApp {
    let url = storage_url();
    TestApp::with_config(move |config| config.storage.url = url).await
}

/// Uploads `bytes` and returns the retrieval code and its management token.
async fn upload(app: &TestApp, name: &str, bytes: Vec<u8>) -> (String, String) {
    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/shares?filename={name}"))
                    .header("content-type", "application/octet-stream"),
                Body::from(bytes),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let field = |key: &str| {
        created
            .get(key)
            .and_then(serde_json::Value::as_str)
            .expect("the field")
            .to_owned()
    };
    (field("code"), field("management_token"))
}

async fn download(app: &TestApp, code: &str) -> Vec<u8> {
    let response = expect_status(
        app.empty(Method::GET, &format!("/s/{code}/download"), Auth::None)
            .await,
        StatusCode::OK,
    )
    .await;
    axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body")
        .to_vec()
}

/// A file small enough that the multipart upload never starts.
///
/// This is the `put` path, which is a different call from the one below and has
/// its own way of being misconfigured.
/// A file large enough to become several parts.
///
/// `PART_SIZE` is 8 MiB, so 20 MiB is three parts — enough that a mistake in
/// the last-part handling shows up, which two parts would not always catch.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn a_multipart_upload_survives_the_round_trip() {
    let app = app().await;
    let sent: Vec<u8> = (0..20 * 1024 * 1024_u32)
        .map(|i| u8::try_from(i % 251).unwrap_or(0))
        .collect();

    let (code, _) = upload(&app, "big.bin", sent.clone()).await;
    let got = download(&app, &code).await;

    assert_eq!(got.len(), sent.len(), "wrong length");
    // Compared by position rather than with `assert_eq!` on the whole vector:
    // a mismatch in twenty megabytes should say where, not print both.
    let first = got.iter().zip(&sent).position(|(a, b)| a != b);
    assert_eq!(first, None, "first differing byte");
}

/// The resumable path against a store that really does multipart.
///
/// The local-filesystem side is covered by `resumable`; this is the half where
/// the offset only advances on whole parts, because S3 refuses short ones.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn a_resumable_upload_survives_being_sent_in_pieces() {
    let app = app().await;
    let part = app.state.config().server.part_size_bytes;
    // Two whole parts and a short tail, so both rules are exercised: the tail
    // may only go out as the last part.
    let sent: Vec<u8> = (0..u32::try_from(part * 2 + 4096).unwrap_or(u32::MAX))
        .map(|i| u8::try_from(i % 251).unwrap_or(0))
        .collect();

    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/uploads?filename=resumed.bin")
                    .header("upload-length", sent.len().to_string()),
                Body::empty(),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let id = created["id"].as_str().expect("an id").to_owned();
    let token = created["token"].as_str().expect("a token").to_owned();

    // Deliberately not aligned to the part size: the offset should come back
    // lower than what was sent, and the client re-sends the difference.
    let mut at = 0_usize;
    let mut last = String::new();
    while at < sent.len() {
        // A whole part plus a little, so the offset comes back lower than what
        // was sent and the difference is re-sent — the interrupted-transfer
        // path, exercised on purpose.
        let end = (at + part + 4096).min(sent.len());
        let response = app
            .send(
                Request::builder()
                    .method(Method::PATCH)
                    .uri(format!("/api/v1/uploads/{id}"))
                    .header("content-type", "application/offset+octet-stream")
                    .header("upload-offset", at.to_string())
                    .header("x-upload-token", &token),
                Body::from(sent[at..end].to_vec()),
            )
            .await;
        let status = response.status();
        let reported = response
            .headers()
            .get("upload-offset")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok());
        last = body_string(response).await;
        assert!(
            status == StatusCode::NO_CONTENT || status == StatusCode::CREATED,
            "{status}: {last}"
        );
        let next = reported.unwrap_or(end);
        assert!(
            next > at || status == StatusCode::CREATED,
            "the offset stopped moving at {at}"
        );
        at = next;
    }

    let share: serde_json::Value = serde_json::from_str(&last).expect("a share");
    let code = share["code"].as_str().expect("a code");
    let got = download(&app, code).await;
    assert_eq!(got.len(), sent.len(), "wrong length");
    assert_eq!(
        got.iter().zip(&sent).position(|(a, b)| a != b),
        None,
        "first differing byte"
    );
}

/// A chunk too short to fill a part is refused, not silently ignored.
///
/// It advances nothing, so a client that kept sending it would re-send from the
/// same offset forever and nothing would say why. The refusal names the size it
/// needed, which the `POST` response already carried.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn a_chunk_below_the_part_size_is_refused_rather_than_ignored() {
    let app = app().await;
    let part = app.state.config().server.part_size_bytes;

    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/uploads?filename=short.bin")
                    .header("upload-length", (part * 2).to_string()),
                Body::empty(),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    assert_eq!(
        created["part_size"].as_u64(),
        Some(u64::try_from(part).unwrap_or(0)),
        "the client is not told what it needs to send"
    );

    let response = app
        .send(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!(
                    "/api/v1/uploads/{}",
                    created["id"].as_str().expect("an id")
                ))
                .header("content-type", "application/offset+octet-stream")
                .header("upload-offset", "0")
                .header(
                    "x-upload-token",
                    created["token"].as_str().expect("a token"),
                ),
            Body::from(vec![0_u8; 4096]),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        body_string(response).await.contains(&part.to_string()),
        "the refusal does not say how much was needed"
    );
}

/// A range out of the middle of an object that was written in parts.
///
/// Moved here from the other binary, where it did not belong: 20 MiB takes the
/// multipart path, so a store without multipart fails this on the upload and
/// never reaches the range at all. The small-object half of this check stays
/// over there, where every backend can run it.
#[tokio::test]
#[ignore = "needs an object store that can hold a multipart upload open; see the module docs"]
async fn a_range_request_reads_across_a_part_boundary() {
    let app = app().await;
    let sent: Vec<u8> = (0..20 * 1024 * 1024_u32)
        .map(|i| u8::try_from(i % 251).unwrap_or(0))
        .collect();
    let (code, _) = upload(&app, "ranged.bin", sent.clone()).await;

    // Well past the first part boundary, so this cannot be served by whatever
    // the first `put_part` happened to leave behind.
    let (from, to) = (9_000_000_usize, 9_000_255_usize);
    let response = expect_status(
        app.send(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/s/{code}/download"))
                .header("range", format!("bytes={from}-{to}")),
            Body::empty(),
        )
        .await,
        StatusCode::PARTIAL_CONTENT,
    )
    .await;

    let got = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body");
    assert_eq!(got.as_ref(), &sent[from..=to]);
}
