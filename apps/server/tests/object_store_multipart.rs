//! The parts of the object store suite that need a multipart upload.
//!
//! # Why these are their own binary
//!
//! Not every store can hold an upload open across requests. `object_store`'s
//! HTTP backend returns `NotImplemented` for `put_multipart`; fake-gcs-server
//! does not implement the XML multipart API at all. Those are real backends
//! people deploy on, and they should be covered for everything they *can* do
//! instead of left untested for what they cannot.
//!
//! Split by binary, not skipped at runtime, because the alternative is a
//! test that returns early and reports "passed", the one thing the sibling
//! module says it will not do. A CI job either runs this binary or it does not,
//! and which it did is in the log.
//!
//! ```text
//! # Against the same store the sibling suite describes:
//! cargo nextest run --run-ignored all -E 'binary(object_store_multipart)'
//! ```

// The helpers below sit outside a `#[test]` body, and that is as far as
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
    TestApp::with_backend(move |backend| backend.url = url).await
}

/// Downloads a share as a browser would: following a redirect to the store
/// when this server sends one, and reading the body directly otherwise.
async fn fetch(app: &TestApp, code: &str) -> Vec<u8> {
    let response = app
        .empty(Method::GET, &format!("/s/{code}/download"), Auth::None)
        .await;
    if response.status() != StatusCode::TEMPORARY_REDIRECT {
        let response = expect_status(response, StatusCode::OK).await;
        return axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the body")
            .to_vec();
    }
    let location = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .expect("a location")
        .to_owned();
    reqwest::Client::new()
        .get(location)
        .send()
        .await
        .expect("the store answered")
        .bytes()
        .await
        .expect("the body")
        .to_vec()
}

/// The same store, with the operator's word that browsers can reach it.
async fn direct_app() -> TestApp {
    let url = storage_url();
    let options = storage_options();
    TestApp::with_backend(move |backend| {
        backend.url = url;
        backend.options = options;
        backend.direct_access = true;
    })
    .await
}

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

/// A file large enough to become several parts.
///
/// `PART_SIZE` is 8 MiB, so 20 MiB is three parts, enough that a mistake in the
/// last-part handling shows up where two parts would not always catch it.
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
    // Compared by position, not with `assert_eq!` on the whole vector:
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

    // Not aligned to the part size, on purpose: the offset should come back
    // lower than what was sent, and the client re-sends the difference.
    let mut at = 0_usize;
    let mut last = String::new();
    while at < sent.len() {
        // A whole part plus a little, so the offset comes back lower than what
        // was sent and the difference is re-sent: the interrupted-transfer
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
/// that a non-final part is at least 5 MiB, exactly the rule the
/// planner's alignment exists to satisfy. A planner that cut in the wrong place
/// would pass every unit test, pass against `InMemory`, and fail on the store
/// a real deployment pays for.
///
/// It lives in the multipart binary because a split file is written as one
/// multipart upload per segment, the one thing the WebDAV store cannot
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
    let app = TestApp::with_config_and_backend(
        move |config| {
            config.storage.split_across_backends = true;
            config.server.part_size_bytes = part;
            // Otherwise a small file would be stored inline in the row and
            // never reach a bucket. This one is far too large for that, but the
            // threshold decides, and a changed default would change this test.
            config.share.inline_threshold_bytes = 8;
        },
        move |backend| {
            backend.url = storage_url();
            backend.options = storage_options();
            backend.capacity_bytes = Some(ceiling);
        },
    )
    .await;

    // The second bucket, opened as the server opens them, added under its own
    // name with a matching limit.
    app.state.replace_backends(app.pool_with(
        tessera_server::db::repository::storage_backend::StorageBackend {
            name: "cold".to_owned(),
            url: second.clone(),
            options: storage_options(),
            is_default: false,
            capacity_bytes: Some(ceiling),
            read_only: false,
            direct_access: false,
        },
        open_bucket(&second),
    ));

    // A byte pattern, not a constant. A segment written in the wrong
    // order, or one byte out of place, has to show up as a mismatch.
    let content: Vec<u8> = (0..part * 3 / 2)
        .map(|i| u8::try_from(i % 251).unwrap_or(0))
        .collect();
    let (code, _) = upload(&app, "split.bin", content.clone()).await;

    // That it reads back is not on its own proof that it was split: a file
    // stored whole in one bucket reads back too. So look in the other bucket.
    // Listed as a stream, not with a delimiter. At the root a delimited
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

    // Across the seam, where one bucket ends and the other begins.
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
    tessera_server::storage::build_with_multipart(&tessera_server::storage::Target {
        url: url.to_owned(),
        options: storage_options(),
    })
    .expect("open the bucket")
}

/// Whether the store under test can sign part uploads, going by the scheme the
/// suite was pointed at. See `storage::Stores::signer`.
fn signs_part_uploads() -> bool {
    let url = storage_url();
    url.starts_with("s3://") || url.starts_with("gs://")
}

/// Opens a session and returns what the `POST` said.
async fn open_session(app: &TestApp, name: &str, length: usize) -> serde_json::Value {
    body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/uploads?filename={name}"))
                    .header("upload-length", length.to_string()),
                Body::empty(),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await
}

/// Writes one part to the URL this server signed, and returns the store's
/// `ETag`.
async fn put_part(app: &TestApp, id: &str, token: &str, number: usize, bytes: Vec<u8>) -> String {
    let signed = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/v1/uploads/{id}/parts/{number}"))
                    .header("x-upload-token", token),
                Body::empty(),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    let url = signed
        .get("url")
        .and_then(serde_json::Value::as_str)
        .expect("a url");

    // A real HTTP client, not the router: the bytes must go to the store
    // without this server in the path.
    let response = reqwest::Client::new()
        .put(url)
        .body(bytes)
        .send()
        .await
        .expect("the store answered");
    let status = response.status();
    let etag = response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    assert!(
        status.is_success(),
        "the store refused part {number}: {status} {}",
        response.text().await.unwrap_or_default()
    );
    etag.expect("a part upload answers with an ETag")
}

async fn complete(
    app: &TestApp,
    id: &str,
    token: &str,
    etags: &[String],
) -> axum::response::Response {
    app.send(
        Request::builder()
            .method(Method::POST)
            .uri(format!("/api/v1/uploads/{id}/complete"))
            .header("x-upload-token", token)
            .header("content-type", "application/json"),
        Body::from(serde_json::json!({ "etags": etags }).to_string()),
    )
    .await
}

/// The main path: parts written straight to the store, finished here, and the
/// file read back matches the file sent.
///
/// Two full parts and a short tail, so the `ETag` order matters: a store that
/// numbered them differently would join the file out of order, and the byte
/// comparison would show where.
///
/// Against a store with nothing to sign, the session must report that it
/// cannot take parts directly, which is asserted instead.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn parts_written_straight_to_the_store_come_back_as_one_file() {
    let app = direct_app().await;
    let part = app.state.config().server.part_size_bytes;
    let sent: Vec<u8> = (0..u32::try_from(part * 2 + 4096).unwrap_or(u32::MAX))
        .map(|i| u8::try_from(i % 251).unwrap_or(0))
        .collect();

    let created = open_session(&app, "direct.bin", sent.len()).await;
    if !signs_part_uploads() {
        assert_eq!(created["direct"], false, "{created}");
        return;
    }
    assert_eq!(created["direct"], true, "{created}");
    let id = created["id"].as_str().expect("an id").to_owned();
    let token = created["token"].as_str().expect("a token").to_owned();

    let mut etags = Vec::new();
    for (index, chunk) in sent.chunks(part).enumerate() {
        etags.push(put_part(&app, &id, &token, index + 1, chunk.to_vec()).await);
    }

    let finished = complete(&app, &id, &token, &etags).await;
    let share = body_json(expect_status(finished, StatusCode::CREATED).await).await;
    let code = share["code"].as_str().expect("a code");

    let got = fetch(&app, code).await;
    assert_eq!(got.len(), sent.len(), "wrong length");
    assert_eq!(
        got.iter().zip(&sent).position(|(a, b)| a != b),
        None,
        "first differing byte"
    );
}

/// A part URL cannot be limited to a length, so a client can write more than
/// it declared. `complete` catches that: the object and the session are
/// deleted, and no file is recorded.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn a_direct_upload_that_comes_out_the_wrong_size_is_refused() {
    if !signs_part_uploads() {
        return;
    }
    let app = direct_app().await;
    // One part, so it is also the last and may be any size.
    let created = open_session(&app, "oversized.bin", 4096).await;
    let id = created["id"].as_str().expect("an id").to_owned();
    let token = created["token"].as_str().expect("a token").to_owned();

    let etag = put_part(&app, &id, &token, 1, vec![7_u8; 5000]).await;
    let refused = complete(&app, &id, &token, &[etag]).await;
    let body = body_string(expect_status(refused, StatusCode::BAD_REQUEST).await).await;
    assert!(
        body.contains("5000"),
        "the refusal should say what arrived: {body}"
    );

    // The session was deleted with the object, so nothing is left to finish.
    expect_status(
        app.send(
            Request::builder()
                .method(Method::HEAD)
                .uri(format!("/api/v1/uploads/{id}"))
                .header("x-upload-token", &token),
            Body::empty(),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
}

/// The count is checked before the store is asked anything, and a refusal
/// leaves the session open to be finished properly.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn the_wrong_number_of_etags_is_refused_and_the_upload_can_still_finish() {
    if !signs_part_uploads() {
        return;
    }
    let app = direct_app().await;
    let sent = vec![9_u8; 4096];
    let created = open_session(&app, "counted.bin", sent.len()).await;
    let id = created["id"].as_str().expect("an id").to_owned();
    let token = created["token"].as_str().expect("a token").to_owned();

    let etag = put_part(&app, &id, &token, 1, sent.clone()).await;
    expect_status(
        complete(&app, &id, &token, &[etag.clone(), etag.clone()]).await,
        StatusCode::BAD_REQUEST,
    )
    .await;

    let share = body_json(
        expect_status(
            complete(&app, &id, &token, &[etag]).await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    assert_eq!(
        fetch(&app, share["code"].as_str().expect("a code")).await,
        sent
    );
}

/// An `ETag` the store does not recognise means the client's list is wrong, not
/// that the server failed: `400`, and the upload stays open for the right list.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn a_wrong_etag_is_refused_as_the_clients_mistake() {
    if !signs_part_uploads() {
        return;
    }
    let app = direct_app().await;
    let sent = vec![7_u8; 4096];
    let created = open_session(&app, "wrong-etag.bin", sent.len()).await;
    let id = created["id"].as_str().expect("an id").to_owned();
    let token = created["token"].as_str().expect("a token").to_owned();

    let etag = put_part(&app, &id, &token, 1, sent.clone()).await;
    let refused = body_json(
        expect_status(
            complete(
                &app,
                &id,
                &token,
                &["\"0123456789abcdef0123456789abcdef\"".to_owned()],
            )
            .await,
            StatusCode::BAD_REQUEST,
        )
        .await,
    )
    .await;
    assert_eq!(refused["code"], "upload_parts_mismatch", "{refused}");

    let share = body_json(
        expect_status(
            complete(&app, &id, &token, &[etag]).await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    assert_eq!(
        fetch(&app, share["code"].as_str().expect("a code")).await,
        sent
    );
}

/// A signer means a URL can be made, not that browsers can use it. Without the
/// operator's word, an upload to an S3 bucket still goes through this server.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn without_the_switch_an_upload_is_not_direct() {
    let app = app().await;
    let created = open_session(&app, "switched-off.bin", 4096).await;
    assert_eq!(created["direct"], false, "{created}");
}

/// Uploads `bytes` as a share and returns its code.
async fn share(
    app: &TestApp,
    name: &str,
    bytes: Vec<u8>,
    query: &str,
    headers: &[(&str, &str)],
) -> String {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/shares?filename={name}{query}"));
    for (key, value) in headers {
        request = request.header(*key, *value);
    }
    let created = body_json(
        expect_status(
            app.send(request, Body::from(bytes)).await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    created
        .get("code")
        .and_then(serde_json::Value::as_str)
        .expect("a code")
        .to_owned()
}

fn sample(len: u32) -> Vec<u8> {
    (0..len)
        .map(|i| u8::try_from(i % 251).unwrap_or(0))
        .collect()
}

async fn stored(app: &TestApp, code: &str) -> tessera_server::db::repository::share::Share {
    app.state
        .shares
        .find_by_code(&tessera_core::RetrievalCode::parse(code).expect("a code"))
        .await
        .expect("a query")
        .expect("the share")
}

/// The point of the switch: a share with no password or use limit is handed to
/// the store, the file read back matches the one sent, under its uploaded
/// name, and the requested bytes are counted as redirected, not served.
///
/// Against a store with nothing to sign, the same download is served from here.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn a_download_is_sent_to_the_store_when_the_backend_allows_it() {
    let app = direct_app().await;
    let sent = sample(300_000);
    let code = share(&app, "sent-to-the-store.bin", sent.clone(), "", &[]).await;

    let response = app
        .empty(Method::GET, &format!("/s/{code}/download"), Auth::None)
        .await;
    if !signs_part_uploads() {
        assert_eq!(response.status(), StatusCode::OK);
        return;
    }
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store"),
        "a cached redirect replays an expired URL"
    );
    let location = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .expect("a location")
        .to_owned();

    let fetched = reqwest::Client::new()
        .get(&location)
        .send()
        .await
        .expect("the store answered");
    assert!(fetched.status().is_success(), "{}", fetched.status());
    let disposition = fetched
        .headers()
        .get("content-disposition")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    assert!(
        disposition.contains("sent-to-the-store.bin"),
        "the store did not answer with the uploaded name: {disposition:?}"
    );
    let got = fetched.bytes().await.expect("the body");
    assert_eq!(got.as_ref(), sent.as_slice());

    let row = stored(&app, &code).await;
    assert_eq!(row.bytes_redirected, 300_000);
    assert_eq!(row.bytes_served, 0, "nothing left this process");
}

/// A range goes to the store too, and what is counted is the range, not the file.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn a_range_is_sent_to_the_store_and_counted_as_asked() {
    if !signs_part_uploads() {
        return;
    }
    let app = direct_app().await;
    let sent = sample(300_000);
    let code = share(&app, "ranged.bin", sent.clone(), "", &[]).await;

    let response = app
        .send(
            Request::builder()
                .uri(format!("/s/{code}/download"))
                .header("range", "bytes=1000-1099"),
            Body::empty(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    let location = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .expect("a location")
        .to_owned();

    let fetched = reqwest::Client::new()
        .get(&location)
        .header("range", "bytes=1000-1099")
        .send()
        .await
        .expect("the store answered");
    assert_eq!(fetched.status().as_u16(), 206);
    let got = fetched.bytes().await.expect("the body");
    assert_eq!(got.as_ref(), sent.get(1000..1100).expect("in range"));
    assert_eq!(stored(&app, &code).await.bytes_redirected, 100);
}

/// A password and a use limit are promises about who gets the bytes and how
/// often, and a store URL keeps neither. Those downloads stay here, and so
/// does a `HEAD`, which is not a download at all.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn a_password_or_a_use_limit_keeps_the_download_on_this_server() {
    let app = direct_app().await;

    let locked = share(
        &app,
        "locked.bin",
        sample(300_000),
        "",
        &[("x-share-password", "a perfectly adequate passphrase")],
    )
    .await;
    expect_status(
        app.send(
            Request::builder()
                .uri(format!("/s/{locked}/download"))
                .header("x-share-password", "a perfectly adequate passphrase"),
            Body::empty(),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    let counted = share(&app, "counted.bin", sample(300_000), "&max_uses=5", &[]).await;
    expect_status(
        app.empty(Method::GET, &format!("/s/{counted}/download"), Auth::None)
            .await,
        StatusCode::OK,
    )
    .await;

    let open = share(&app, "open.bin", sample(300_000), "", &[]).await;
    expect_status(
        app.empty(Method::HEAD, &format!("/s/{open}/download"), Auth::None)
            .await,
        StatusCode::OK,
    )
    .await;
}

/// Without the operator's word, nothing is redirected.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn without_the_switch_downloads_stay_on_this_server() {
    let app = app().await;
    let code = share(&app, "switched-off.bin", sample(300_000), "", &[]).await;
    expect_status(
        app.empty(Method::GET, &format!("/s/{code}/download"), Auth::None)
            .await,
        StatusCode::OK,
    )
    .await;
}

/// Reads the listing, or `None` when this share has none.
async fn listing(app: &TestApp, code: &str) -> Option<serde_json::Value> {
    let response = app
        .empty(Method::GET, &format!("/s/{code}/segments"), Auth::None)
        .await;
    if response.status() == StatusCode::NOT_FOUND {
        return None;
    }
    Some(body_json(expect_status(response, StatusCode::OK).await).await)
}

/// Fetches one piece as a browser joining them would.
async fn piece(app: &TestApp, code: &str, index: usize) -> Vec<u8> {
    let response = app
        .empty(
            Method::GET,
            &format!("/s/{code}/segments/{index}"),
            Auth::None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    let location = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .expect("a location")
        .to_owned();
    reqwest::Client::new()
        .get(location)
        .send()
        .await
        .expect("the store answered")
        .bytes()
        .await
        .expect("the body")
        .to_vec()
}

/// An unsplit file still has one piece, so a browser can join every share the
/// same way without asking which kind it is.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn an_unsplit_file_is_one_piece() {
    let app = direct_app().await;
    let sent = sample(300_000);
    let code = share(&app, "one-piece.bin", sent.clone(), "", &[]).await;

    let Some(listed) = listing(&app, &code).await else {
        assert!(
            !signs_part_uploads(),
            "a signing store should have a listing"
        );
        return;
    };
    assert_eq!(listed["size_bytes"], 300_000, "{listed}");
    let segments = listed["segments"].as_array().expect("segments");
    assert_eq!(segments.len(), 1, "{listed}");
    assert_eq!(segments[0]["offset"], 0);
    assert_eq!(segments[0]["length"], 300_000);

    assert_eq!(piece(&app, &code, 0).await, sent);
    assert_eq!(stored(&app, &code).await.bytes_redirected, 300_000);
}

/// A share that promises anything about who or how often has no listing, for
/// the same reason it gets no redirect: a store URL keeps neither promise.
///
/// The two are refused differently, and both correctly. A locked share asks
/// for its password first, as `/download` does, so the answer reveals nothing
/// about what is behind it. A counted one has nothing to hide and just has no
/// listing.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn a_password_or_a_use_limit_has_no_segment_listing() {
    if !signs_part_uploads() {
        return;
    }
    let app = direct_app().await;

    let locked = share(
        &app,
        "locked.bin",
        sample(300_000),
        "",
        &[("x-share-password", "a perfectly adequate passphrase")],
    )
    .await;
    expect_status(
        app.empty(Method::GET, &format!("/s/{locked}/segments"), Auth::None)
            .await,
        StatusCode::UNAUTHORIZED,
    )
    .await;

    let counted = share(&app, "counted.bin", sample(300_000), "&max_uses=5", &[]).await;
    assert!(
        listing(&app, &counted).await.is_none(),
        "a counted share was listed"
    );
}

/// The reason this exists: a file split across two buckets, listed as its
/// pieces and joined from them, with no byte passing through this server.
///
/// Offsets are checked as they are used, so a listing that ordered the pieces
/// wrong would join into a file that does not match.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn a_split_file_is_listed_and_joined_piece_by_piece() {
    if !signs_part_uploads() {
        return;
    }
    let Some(second) = second_url() else {
        panic!("TESSERA_TEST_STORAGE_URL_SECOND is not set; see the module docs");
    };

    // Same shape as the round-trip test above: one part per bucket, and a file
    // of one and a half parts, so it must be split, and only on the part
    // boundary.
    let part = 5 * 1024 * 1024_usize;
    let ceiling = u64::try_from(part).expect("a part size");
    let app = TestApp::with_config_and_backend(
        move |config| {
            config.storage.split_across_backends = true;
            config.server.part_size_bytes = part;
            config.share.inline_threshold_bytes = 8;
        },
        move |backend| {
            backend.url = storage_url();
            backend.options = storage_options();
            backend.capacity_bytes = Some(ceiling);
            backend.direct_access = true;
        },
    )
    .await;

    app.state.replace_backends(app.pool_with(
        tessera_server::db::repository::storage_backend::StorageBackend {
            name: "cold".to_owned(),
            url: second.clone(),
            options: storage_options(),
            is_default: false,
            capacity_bytes: Some(ceiling),
            read_only: false,
            direct_access: true,
        },
        open_bucket(&second),
    ));

    let content = sample(u32::try_from(part * 3 / 2).expect("a size"));
    let (code, _) = upload(&app, "split.bin", content.clone()).await;

    let listed = listing(&app, &code).await.expect("a split file is listed");
    assert_eq!(listed["size_bytes"], content.len(), "{listed}");
    let segments = listed["segments"].as_array().expect("segments").clone();
    assert!(segments.len() >= 2, "the file was not split: {listed}");

    let mut joined: Vec<u8> = Vec::new();
    for (index, described) in segments.iter().enumerate() {
        assert_eq!(
            described["offset"].as_u64(),
            Some(u64::try_from(joined.len()).expect("an offset")),
            "piece {index} does not start where the last one ended"
        );
        let bytes = piece(&app, &code, index).await;
        assert_eq!(
            u64::try_from(bytes.len()).ok(),
            described["length"].as_u64(),
            "piece {index} is not the length it was listed as"
        );
        joined.extend_from_slice(&bytes);
    }
    assert_eq!(joined.len(), content.len(), "joined to a different length");
    assert_eq!(
        joined.iter().zip(&content).position(|(a, b)| a != b),
        None,
        "first differing byte"
    );
    assert_eq!(
        stored(&app, &code).await.bytes_redirected,
        u64::try_from(content.len()).expect("a size")
    );
}
