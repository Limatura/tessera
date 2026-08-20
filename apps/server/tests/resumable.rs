//! Uploads that survive a dropped connection.
//!
//! The local-filesystem path, which is what a default instance runs. The
//! multipart path against a real store is exercised by `object_store`.

// The helpers below sit outside a `#[test]` body, which is as far as
// clippy.toml's test exemptions reach. A fixture that cannot read what it was
// just handed has nothing to report but a failure.
#![expect(clippy::expect_used, reason = "test fixture")]

mod common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use common::{Auth, TestApp, body_json, body_string, expect_status};

/// Opens a session and returns its id, token and the part size it wants.
async fn open(app: &TestApp, name: &str, length: usize) -> (String, String, usize) {
    let created = body_json(
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
    .await;
    let text = |key: &str| {
        created
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let part = created
        .get("part_size")
        .and_then(serde_json::Value::as_u64)
        .expect("a part size");
    (
        text("id"),
        text("token"),
        usize::try_from(part).unwrap_or(0),
    )
}

/// Sends `bytes` at `offset` and gives back the status and the new offset.
async fn patch(
    app: &TestApp,
    id: &str,
    token: &str,
    offset: u64,
    bytes: Vec<u8>,
) -> (StatusCode, Option<u64>, String) {
    let response = app
        .send(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/uploads/{id}"))
                .header("content-type", "application/offset+octet-stream")
                .header("upload-offset", offset.to_string())
                .header("x-upload-token", token),
            Body::from(bytes),
        )
        .await;
    let status = response.status();
    let at = response
        .headers()
        .get("upload-offset")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok());
    (status, at, body_string(response).await)
}

async fn head(app: &TestApp, id: &str, token: &str) -> (StatusCode, Option<u64>, Option<String>) {
    let response = app
        .send(
            Request::builder()
                .method(Method::HEAD)
                .uri(format!("/api/v1/uploads/{id}"))
                .header("x-upload-token", token),
            Body::empty(),
        )
        .await;
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    (
        response.status(),
        header("upload-offset").and_then(|value| value.parse().ok()),
        header("cache-control"),
    )
}

/// The whole point: a file that arrives in pieces is the same file.
#[tokio::test]
async fn a_file_sent_in_pieces_arrives_whole() {
    let app = TestApp::ready().await;
    let sent: Vec<u8> = (0..3_u32 * 1024 * 1024)
        .map(|i| u8::try_from(i % 251).unwrap_or(0))
        .collect();
    let (id, token, _) = open(&app, "pieces.bin", sent.len()).await;

    // Three requests, and the client could have been restarted between any two
    // of them — nothing is carried in memory between them.
    let third = sent.len() / 3;
    let mut at = 0_u64;
    let mut last = String::new();
    for chunk in sent.chunks(third) {
        let (status, offset, body) = patch(&app, &id, &token, at, chunk.to_vec()).await;
        assert!(
            status == StatusCode::NO_CONTENT || status == StatusCode::CREATED,
            "{status}: {body}"
        );
        at = offset.unwrap_or_else(|| at + u64::try_from(chunk.len()).unwrap_or(0));
        last = body;
    }

    // The last chunk answers with the share the upload became.
    let share: serde_json::Value = serde_json::from_str(&last).expect("a share");
    let code = share
        .get("code")
        .and_then(serde_json::Value::as_str)
        .expect("a code");
    assert_eq!(
        share.get("size_bytes").and_then(serde_json::Value::as_u64),
        Some(u64::try_from(sent.len()).unwrap_or(0))
    );

    let downloaded = expect_status(
        app.empty(Method::GET, &format!("/s/{code}/download"), Auth::None)
            .await,
        StatusCode::OK,
    )
    .await;
    let got = axum::body::to_bytes(downloaded.into_body(), usize::MAX)
        .await
        .expect("the body");
    assert_eq!(got.as_ref(), sent.as_slice());
}

/// `HEAD` is how a client that lost its place finds it again.
#[tokio::test]
async fn head_reports_the_offset_and_forbids_caching() {
    let app = TestApp::ready().await;
    let (id, token, _) = open(&app, "half.bin", 2048).await;

    let (status, at, cache) = head(&app, &id, &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(at, Some(0));
    // A cached offset sends the next chunk to the wrong place, and it is a
    // proxy rather than the client that would cache it.
    assert_eq!(cache.as_deref(), Some("no-store"));

    patch(&app, &id, &token, 0, vec![9; 1024]).await;
    assert_eq!(head(&app, &id, &token).await.1, Some(1024));
}

/// The rule that keeps two clients from interleaving into one file.
#[tokio::test]
async fn a_chunk_at_the_wrong_offset_is_refused_and_changes_nothing() {
    let app = TestApp::ready().await;
    let (id, token, _) = open(&app, "conflict.bin", 4096).await;
    patch(&app, &id, &token, 0, vec![1; 1024]).await;

    let (status, _, _) = patch(&app, &id, &token, 0, vec![2; 1024]).await;
    assert_eq!(status, StatusCode::CONFLICT, "an old offset was accepted");

    let (status, _, _) = patch(&app, &id, &token, 4000, vec![3; 96]).await;
    assert_eq!(status, StatusCode::CONFLICT, "a future offset was accepted");

    // Neither attempt moved it.
    assert_eq!(head(&app, &id, &token).await.1, Some(1024));
}

/// Without the token, a stranger who guessed the identifier gets nowhere.
#[tokio::test]
async fn continuing_someone_elses_upload_needs_their_token() {
    let app = TestApp::ready().await;
    let (id, token, _) = open(&app, "mine.bin", 1024).await;

    let (status, _, _) = patch(&app, &id, "not-the-token", 0, vec![0; 512]).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "a wrong token was accepted");
    assert_eq!(head(&app, &id, &token).await.1, Some(0));
}

/// The specification's content type is required, because a client that has not
/// set it has usually not set anything else either.
#[tokio::test]
async fn a_patch_without_the_offset_content_type_is_refused() {
    let app = TestApp::ready().await;
    let (id, token, _) = open(&app, "typed.bin", 16).await;

    let status = app
        .send(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/uploads/{id}"))
                .header("content-type", "application/octet-stream")
                .header("upload-offset", "0")
                .header("x-upload-token", &token),
            Body::from(vec![0; 16]),
        )
        .await
        .status();
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

/// More than was declared is refused, so a session cannot be used to write
/// past the room the quota already accounted for.
#[tokio::test]
async fn sending_more_than_declared_is_refused() {
    let app = TestApp::ready().await;
    let (id, token, _) = open(&app, "greedy.bin", 100).await;

    let (status, _, _) = patch(&app, &id, &token, 0, vec![0; 200]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// An upload can grow a share that already exists, which is what multi-file
/// shares need from it.
#[tokio::test]
async fn an_upload_can_append_to_an_existing_share() {
    let app = TestApp::ready().await;
    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/shares?filename=first.bin")
                    .header("content-type", "application/octet-stream"),
                Body::from("the first one".as_bytes().to_vec()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let code = created["code"].as_str().expect("a code").to_owned();
    let management = created["management_token"]
        .as_str()
        .expect("a token")
        .to_owned();

    let opened = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/uploads?filename=second.bin&code={code}"))
                    .header("upload-length", "12")
                    .header("x-share-management-token", &management),
                Body::empty(),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let id = opened["id"].as_str().expect("an id").to_owned();
    let token = opened["token"].as_str().expect("a token").to_owned();

    let (status, _, body) = patch(&app, &id, &token, 0, b"the second..".to_vec()).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let share: serde_json::Value = serde_json::from_str(&body).expect("a share");
    assert_eq!(
        share
            .get("files")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len),
        Some(2),
        "the upload did not join the share"
    );
}

/// Two requests writing the same upload must not interleave.
///
/// What this proves is the truncation: the local sink cuts back to the recorded
/// offset before adding to it, so the request that loses the race leaves
/// nothing behind. Measured with that removed — the file came out 6144 bytes
/// long instead of 4096, holding both writes.
///
/// It does **not** prove the conditional update in `advance`; removing that
/// leaves this passing, because truncation already makes the local path
/// self-consistent. That check earns its keep on the multipart path, where
/// there is nothing to truncate — a part that has been uploaded is at the store
/// whatever happens next, and only the recorded list decides what `complete`
/// assembles.
#[tokio::test]
async fn two_requests_at_the_same_offset_do_not_interleave() {
    let app = TestApp::ready().await;
    let total = 4096_usize;
    let (id, token, _) = open(&app, "raced.bin", total).await;

    let first = vec![1_u8; 2048];
    let second = vec![2_u8; 2048];
    let (a, b) = tokio::join!(
        patch(&app, &id, &token, 0, first.clone()),
        patch(&app, &id, &token, 0, second.clone()),
    );

    // Exactly one of them was accepted.
    let accepted = [&a, &b]
        .iter()
        .filter(|(status, ..)| *status == StatusCode::NO_CONTENT)
        .count();
    let refused = [&a, &b]
        .iter()
        .filter(|(status, ..)| *status == StatusCode::CONFLICT)
        .count();
    assert_eq!((accepted, refused), (1, 1), "a={a:?} b={b:?}");

    // And the file holds one of them whole, not a mixture.
    assert_eq!(head(&app, &id, &token).await.1, Some(2048));
    let rest = vec![3_u8; 2048];
    let (status, _, body) = patch(&app, &id, &token, 2048, rest.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let share: serde_json::Value = serde_json::from_str(&body).expect("a share");
    let code = share["code"].as_str().expect("a code");
    let downloaded = expect_status(
        app.empty(Method::GET, &format!("/s/{code}/download"), Auth::None)
            .await,
        StatusCode::OK,
    )
    .await;
    let got = axum::body::to_bytes(downloaded.into_body(), usize::MAX)
        .await
        .expect("the body");

    assert_eq!(got.len(), total, "the loser's bytes were kept as well");
    let winner = got[0];
    assert!(
        got[..2048].iter().all(|byte| *byte == winner),
        "the first half is a mixture of both requests"
    );
    assert!(got[2048..].iter().all(|byte| *byte == 3));
}
