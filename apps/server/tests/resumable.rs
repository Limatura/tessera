//! Uploads that survive a dropped connection.
//!
//! The local-filesystem path, what a default instance runs. The
//! multipart path against a real store is exercised by `object_store`.

// The helpers below sit outside a `#[test]` body, and that is as far as
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

/// The point: a file that arrives in pieces is the same file.
#[tokio::test]
async fn a_file_sent_in_pieces_arrives_whole() {
    let app = TestApp::ready().await;
    let sent: Vec<u8> = (0..3_u32 * 1024 * 1024)
        .map(|i| u8::try_from(i % 251).unwrap_or(0))
        .collect();
    let (id, token, _) = open(&app, "pieces.bin", sent.len()).await;

    // Three requests, and the client could have been restarted between any two
    // of them, since nothing is carried in memory between them.
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
    // proxy, not the client, that would cache it.
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

/// An upload can grow a share that already exists, and that is what multi-file
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
/// nothing behind. Measured with that removed: the file came out 6144 bytes
/// long instead of 4096, holding both writes.
///
/// It does **not** prove the conditional update in `advance`; removing that
/// leaves this passing, because truncation already makes the local path
/// self-consistent. That check earns its keep on the multipart path, where
/// there is nothing to truncate: a part that has been uploaded is at the store
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

/// A session reserves the room it declared, so two cannot be promised the same
/// free space.
///
/// Counting what has landed instead lets both through: at the moment the second
/// asks, the first has sent nothing, so the room looks free twice over and the
/// instance goes past its own limit.
#[tokio::test]
async fn two_sessions_cannot_reserve_the_same_room() {
    let quota = 8192_u64;
    let app = TestApp::with_config(move |config| config.storage.max_total_bytes = quota).await;

    let first = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/uploads?filename=first.bin")
                .header("upload-length", quota.to_string()),
            Body::empty(),
        )
        .await;
    assert_eq!(first.status(), StatusCode::CREATED);

    // Nothing has arrived yet, so the room is only spoken for, not used.
    let second = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/uploads?filename=second.bin")
                .header("upload-length", quota.to_string()),
            Body::empty(),
        )
        .await;
    assert_eq!(
        second.status(),
        StatusCode::INSUFFICIENT_STORAGE,
        "the same free space was promised twice"
    );
}

/// Reserved bytes belong to the scope that opened the session, or an anonymous
/// stranger holding room could refuse a member at their first upload.
///
/// `uploads` carries its own `role_id` for this: until a session completes it
/// has no share to read a role from, so summing every open one is the only
/// other option and it charges everybody for everybody.
#[tokio::test]
async fn an_anonymous_session_does_not_spend_a_members_quota() {
    let app = TestApp::with_config(|config| config.storage.max_total_bytes = 0).await;

    // Both roles get the same figure, so a leak in either direction shows.
    for name in ["guest", "member"] {
        let role = app
            .state
            .roles
            .find_by_name(name)
            .await
            .expect("query")
            .expect("the role");
        app.state
            .roles
            .set_quota(role.id, Some(8192), time::OffsetDateTime::now_utc())
            .await
            .expect("store the quota");
    }

    // An anonymous session holding the entire guest allowance.
    let anonymous = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/uploads?filename=theirs.bin")
                .header("upload-length", "8192"),
            Body::empty(),
        )
        .await;
    assert_eq!(anonymous.status(), StatusCode::CREATED);

    // A second anonymous one is refused: the room really is spoken for.
    let again = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/uploads?filename=theirs-too.bin")
                .header("upload-length", "8192"),
            Body::empty(),
        )
        .await;
    assert_eq!(again.status(), StatusCode::INSUFFICIENT_STORAGE);

    // A member is untouched by it.
    let owner = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": "owner",
                "email": "owner@example.com",
                "password": "a perfectly adequate passphrase",
            }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    drop(owner);
    let alice = common::session_cookie(
        &expect_status(
            app.json(
                Method::POST,
                "/api/v1/auth/register",
                Auth::None,
                &serde_json::json!({
                    "username": "alice",
                    "email": "alice@example.com",
                    "password": "a perfectly adequate passphrase",
                }),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    );
    let mine = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/uploads?filename=mine.bin")
                .header("upload-length", "8192")
                .header(axum::http::header::COOKIE, &alice),
            Body::empty(),
        )
        .await;
    assert_eq!(mine.status(), StatusCode::CREATED);
}

/// A resumable upload goes to a backend with room for it, not always the
/// default.
///
/// Direct uploads were placed but resumable ones were not, so an instance whose
/// default was full refused every resumable upload while other backends sat
/// empty.
#[tokio::test]
async fn a_resumable_upload_is_placed_like_any_other() {
    use tessera_server::db::repository::storage_backend::StorageBackend;

    let app = TestApp::with_backend(|backend| backend.capacity_bytes = Some(1024)).await;
    // A multipart view too, the way a real S3-shaped backend has one. Without
    // it this backend could not hold a resumable upload and the placer would be
    // right to pass it over. One store behind both views, as a real backend
    // has.
    let memory = std::sync::Arc::new(object_store::memory::InMemory::new());
    let second: std::sync::Arc<dyn object_store::ObjectStore> =
        std::sync::Arc::<object_store::memory::InMemory>::clone(&memory);
    let second_multipart = tessera_server::storage::Multipart(memory);
    app.state.replace_backends(app.pool_with(
        StorageBackend {
            name: "cold".to_owned(),
            url: "memory://".to_owned(),
            options: std::collections::BTreeMap::new(),
            is_default: false,
            capacity_bytes: None,
            read_only: false,
            direct_access: false,
        },
        tessera_server::storage::Stores {
            store: second,
            multipart: Some(second_multipart),
            put_multipart: true,
            #[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
            signer: None,
        },
    ));

    // Bigger than the default's 1 KiB, so it has to land on `cold`.
    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/uploads?filename=big.bin")
                    .header("upload-length", "8192"),
                Body::empty(),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;

    let id = tessera_core::UploadId::parse(created["id"].as_str().expect("an id")).expect("parses");
    let session = app
        .state
        .uploads
        .find(id)
        .await
        .expect("query")
        .expect("the session");
    assert_eq!(
        session.backend, "cold",
        "opened the session on a backend with no room for it"
    );
}

/// A session records where it is writing, so a later request must not send the
/// next part to whichever backend is current.
///
/// The session is opened on a *non-default* backend by design: with a local
/// default, "used the recorded name" and "used whatever was in hand" both end
/// at the same appending sink, and the test proves nothing. Here the recorded
/// backend is a remote one, so the two answers differ.
#[tokio::test]
async fn a_resumed_session_keeps_writing_where_it_started() {
    use tessera_server::db::repository::storage_backend::StorageBackend;

    // The default has room for nothing, so the session is placed on `cold`.
    let app = TestApp::with_backend(|backend| backend.capacity_bytes = Some(1)).await;
    // One store behind both views, the way a real backend has one.
    let memory = std::sync::Arc::new(object_store::memory::InMemory::new());
    let cold: std::sync::Arc<dyn object_store::ObjectStore> =
        std::sync::Arc::<object_store::memory::InMemory>::clone(&memory);
    let cold_multipart = tessera_server::storage::Multipart(memory);
    let seeded = app.state.backends.load_full();
    app.state.replace_backends(app.pool_with(
        StorageBackend {
            name: "cold".to_owned(),
            url: "memory://".to_owned(),
            options: std::collections::BTreeMap::new(),
            is_default: false,
            capacity_bytes: None,
            read_only: false,
            direct_access: false,
        },
        tessera_server::storage::Stores {
            store: cold,
            multipart: Some(cold_multipart),
            put_multipart: true,
            #[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
            signer: None,
        },
    ));

    let (id, token, _) = open(&app, "kept.bin", 4096).await;
    let session = app
        .state
        .uploads
        .find(tessera_core::UploadId::parse(&id).expect("parses"))
        .await
        .expect("query")
        .expect("the session");
    assert_eq!(
        session.backend, "cold",
        "the fixture did not place it remotely"
    );

    // `cold` is dropped from the pool. The session still names it, so the write
    // has to fail instead of landing in whatever is current.
    app.state.replace_backends((*seeded).clone());

    let patched = app
        .send(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/uploads/{id}"))
                .header("upload-offset", "0")
                .header("content-type", "application/offset+octet-stream")
                .header("x-upload-token", &token),
            Body::from(vec![1_u8; 4096]),
        )
        .await;
    assert_eq!(
        patched.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "wrote a part into a store this session never opened"
    );
}

/// A session spread over two backends has both of its pieces cleaned up.
///
/// The sweep used to reopen the session's *first* segment and stop. A file
/// written across two backends is two multipart uploads, and S3 bills for an
/// abandoned one's parts until it is aborted, so the second would sit
/// there costing money with nothing left that knew its identifier. Nothing
/// splits yet, so this builds the two-segment session by hand; the point is
/// that the sweep is ready before the write path can produce one.
#[tokio::test]
async fn every_segment_of_an_abandoned_upload_is_cleaned_up() {
    use sea_orm::{ActiveValue::Set, EntityTrait as _};

    let app = TestApp::ready().await;
    let (id, _token, _) = open(&app, "two-parts.bin", 64).await;

    // The partial file the appending sink writes for the segment that exists.
    let root = app
        .state
        .backends
        .load()
        .local_root(tessera_server::storage::DEFAULT_BACKEND)
        .expect("the seeded backend is a directory");
    let first = app
        .state
        .uploads
        .find(id.parse().expect("an id"))
        .await
        .expect("the session")
        .expect("it exists");
    let second_key = format!("{}-second", first.object_key);

    tessera_server::db::entity::upload_segment::Entity::insert(
        tessera_server::db::entity::upload_segment::ActiveModel {
            upload_id: Set(id.clone()),
            seq: Set(1),
            backend: Set(tessera_server::storage::DEFAULT_BACKEND.to_owned()),
            object_key: Set(second_key.clone()),
            offset_bytes: Set(32),
            length_bytes: Set(32),
            multipart_id: Set(None),
            state: Set("open".to_owned()),
            created_at: Set(time::OffsetDateTime::now_utc()),
            updated_at: Set(time::OffsetDateTime::now_utc()),
        },
    )
    .exec(&app.state.db)
    .await
    .expect("the second segment");

    // A partial file for each, as the appending sink leaves behind.
    for key in [&first.object_key, &second_key] {
        let path = root.join(format!("{key}.part"));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("the directory");
        }
        std::fs::write(&path, b"partial").expect("a partial file");
    }

    // Long past its expiry, so the sweep takes it.
    tessera_server::maintenance::sweep_once(
        &app.state,
        time::OffsetDateTime::now_utc() + time::Duration::days(365),
    )
    .await;

    for key in [&first.object_key, &second_key] {
        let path = root.join(format!("{key}.part"));
        assert!(
            !path.exists(),
            "a segment's partial file survived the sweep: {}",
            path.display()
        );
    }
}

/// A local directory has nothing to sign, so an upload placed on one reports
/// that when opened, and the two direct endpoints refuse instead of handing
/// out something that cannot work.
///
/// `409`, not `404`: the session exists and belongs to the caller; only this
/// way of finishing it does not apply.
#[tokio::test]
async fn an_upload_to_a_local_directory_cannot_take_parts_directly() {
    let app = TestApp::ready().await;
    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/uploads?filename=local.bin")
                    .header("upload-length", "4096"),
                Body::empty(),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    assert_eq!(created["direct"], false, "{created}");
    let id = created["id"].as_str().expect("an id");
    let token = created["token"].as_str().expect("a token");

    expect_status(
        app.send(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/uploads/{id}/parts/1"))
                .header("x-upload-token", token),
            Body::empty(),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
    expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/uploads/{id}/complete"))
                .header("x-upload-token", token)
                .header("content-type", "application/json"),
            Body::from(r#"{"etags":["\"abc\""]}"#),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
}

/// The direct endpoints require the same proof as `PATCH`. Without the token a
/// stranger learns nothing, not even that the session exists.
#[tokio::test]
async fn the_direct_endpoints_need_the_uploaders_token() {
    let app = TestApp::ready().await;
    let (id, _token, _) = open(&app, "private.bin", 4096).await;

    expect_status(
        app.send(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/uploads/{id}/parts/1")),
            Body::empty(),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/uploads/{id}/complete"))
                .header("content-type", "application/json"),
            Body::from(r#"{"etags":["\"abc\""]}"#),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
}
