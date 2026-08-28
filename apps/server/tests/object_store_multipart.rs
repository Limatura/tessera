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

/// The second bucket, which the split test needs and nothing else here does.
fn second_url() -> Option<String> {
    std::env::var("TESSERA_TEST_STORAGE_URL_SECOND").ok()
}

/// Options both buckets are opened with, from the environment.
///
/// Empty for the S3 and Azure stores, which read their credentials from the
/// environment themselves. The WebDAV store needs `allow_http=true` and does not
/// reach this binary at all, but a store added later might.
fn storage_options() -> std::collections::BTreeMap<String, String> {
    let Ok(raw) = std::env::var("TESSERA_TEST_STORAGE_OPTIONS") else {
        return std::collections::BTreeMap::new();
    };
    raw.split(',')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair
                .split_once('=')
                .expect("TESSERA_TEST_STORAGE_OPTIONS wants key=value pairs");
            (key.to_owned(), value.to_owned())
        })
        .collect()
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

/// A file split across two real buckets, written and read back.
///
/// The split path had only ever run against `InMemory`, which accepts any part
/// layout at all. `RustFS`, `MinIO` and the rest do not: they enforce S3's rule
/// that a non-final part is at least 5 MiB, which is exactly the rule the
/// planner's alignment exists to satisfy. A planner that cut in the wrong place
/// would pass every unit test, pass against `InMemory`, and fail on the store
/// somebody actually pays for.
///
/// It lives in the multipart binary because a split file is written as one
/// multipart upload per segment, which is the one thing the WebDAV store cannot
/// do.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn a_file_split_across_two_buckets_survives_the_round_trip() {
    let Some(second) = second_url() else {
        panic!("TESSERA_TEST_STORAGE_URL_SECOND is not set; see the module docs");
    };

    // Five-mebibyte parts and a ceiling of one part per bucket, so a file of one
    // and a half parts has to be cut, and cannot be cut anywhere except on the
    // part boundary.
    let part = 5 * 1024 * 1024_usize;
    let ceiling = u64::try_from(part).expect("a part size");
    let app = TestApp::with_config(move |config| {
        config.storage.url = storage_url();
        config.storage.options = storage_options();
        config.storage.split_across_backends = true;
        // The backend built from `storage.url` has no row, so its ceiling is
        // configured where its URL is.
        config.storage.capacity_bytes = ceiling;
        config.server.part_size_bytes = part;
        // Otherwise a small file would be stored in the row and never reach a
        // bucket. This one is far too large for that, but the threshold is what
        // decides it and a default that changed would change this test.
        config.share.inline_threshold_bytes = 8;
    })
    .await;

    // Both buckets, opened the way the server opens them, with the second added
    // under a name of its own and a ceiling to match.
    let open = tessera_server::storage::Backends::from([
        (
            tessera_server::storage::DEFAULT_BACKEND.to_owned(),
            open_bucket(&storage_url()),
        ),
        ("cold".to_owned(), open_bucket(&second)),
    ]);
    app.state.replace_backends(tessera_server::storage::Pool {
        open,
        configured: vec![
            tessera_server::db::repository::storage_backend::StorageBackend {
                name: "cold".to_owned(),
                url: second.clone(),
                options: storage_options(),
                is_default: false,
                capacity_bytes: Some(ceiling),
                read_only: false,
            },
        ],
    });

    // A byte pattern rather than a constant: a segment written in the wrong
    // order, or one byte out of place, has to show up as a mismatch.
    let content: Vec<u8> = (0..part * 3 / 2)
        .map(|i| u8::try_from(i % 251).unwrap_or(0))
        .collect();
    let (code, _) = upload(&app, "split.bin", content.clone()).await;

    // That it reads back is not on its own proof that it was split: a file
    // stored whole in one bucket reads back too. So look in the other bucket.
    // Listed as a stream rather than with a delimiter: at the root a delimited
    // listing reports `shares/` as a common prefix and no objects at all, which
    // reads as "nothing was written" for a bucket that is not empty.
    let cold = open_bucket(&second);
    let in_cold: Vec<_> = futures_util::StreamExt::collect::<Vec<_>>(
        object_store::ObjectStore::list(cold.store.as_ref(), None),
    )
    .await;
    assert!(
        !in_cold.is_empty(),
        "nothing was written to the second bucket, so the file was not split"
    );

    assert_eq!(
        download(&app, &code).await,
        content,
        "the file did not come back as it went in"
    );

    // Across the seam, which is where one bucket ends and the other begins.
    let from = part - 100;
    let to = part + 99;
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
        .expect("read the body");
    assert_eq!(
        got.as_ref(),
        &content[from..=to],
        "the range across the seam came back wrong"
    );
}

/// Opens one bucket with the suite's options, as a store that can do multipart.
fn open_bucket(url: &str) -> tessera_server::storage::Stores {
    tessera_server::storage::build_with_multipart(&tessera_server::config::StorageConfig {
        url: url.to_owned(),
        options: storage_options(),
        ..tessera_server::config::StorageConfig::default()
    })
    .expect("open the bucket")
}
