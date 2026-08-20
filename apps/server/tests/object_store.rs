//! Exercises a real object store, the way `backends` exercises a real database.
//!
//! Until this file existed, `storage-s3` only ever proved that it *compiled*.
//! Nothing had put a byte into a bucket and read it back, so "we support S3"
//! rested on the `object_store` crate behaving and on our options being spelled
//! right — neither of which anything checked.
//!
//! Every test is `#[ignore]`d, because it needs a store that a plain
//! `cargo nextest run` has no reason to provide. Ignoring rather than skipping
//! on a missing variable is deliberate: a test that returns early reports
//! "passed" and teaches you to trust coverage that never ran.
//!
//! ```text
//! cat > /tmp/garage.toml <<'EOF'
//! metadata_dir = "/tmp/meta"
//! data_dir = "/tmp/data"
//! db_engine = "sqlite"
//! replication_factor = 1
//! rpc_bind_addr = "[::]:3901"
//! rpc_public_addr = "127.0.0.1:3901"
//!
//! [s3_api]
//! s3_region = "garage"
//! api_bind_addr = "[::]:3900"
//! EOF
//!
//! docker run -d --name garage -p 3900:3900 \
//!   -v /tmp/garage.toml:/etc/garage.toml:ro \
//!   -e GARAGE_RPC_SECRET=$(openssl rand -hex 32) \
//!   -e GARAGE_DEFAULT_ACCESS_KEY=tesseratest \
//!   -e GARAGE_DEFAULT_SECRET_KEY=tesseratestsecret \
//!   -e GARAGE_DEFAULT_BUCKET=tessera \
//!   dxflrs/garage:v2.3.0 /garage server --single-node --default-bucket
//!
//! AWS_ACCESS_KEY_ID=tesseratest AWS_SECRET_ACCESS_KEY=tesseratestsecret \
//! AWS_REGION=garage AWS_ENDPOINT=http://127.0.0.1:3900 \
//! AWS_ALLOW_HTTP=true AWS_VIRTUAL_HOSTED_STYLE_REQUEST=false \
//! TESSERA_TEST_STORAGE_URL=s3://tessera \
//!   cargo nextest run --run-ignored all -E 'binary(object_store)'
//! ```
//!
//! # What this cannot catch
//!
//! Garage accepts multipart parts below S3's 5 MiB floor, and does not require
//! every part but the last to be the same size — both measured against
//! `dxflrs/garage:v2.3.0`. AWS S3 requires the first and Cloudflare R2 requires
//! both. So a part-size mistake passes here and fails on the store somebody
//! actually pays for; the rules have to be pinned by a unit test, not by this
//! one. See #148.
#![cfg(feature = "storage-s3")]
// The helpers below sit outside a `#[test]` body, which is as far as
// clippy.toml's test exemptions reach. A fixture that cannot resolve its target
// has nothing to report but a failure.
#![expect(clippy::expect_used, reason = "test fixture; see module docs")]

mod common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use common::{Auth, TestApp, body_json, expect_status};
use object_store::ObjectStoreExt as _;

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
            .unwrap_or_else(|| panic!("no {key} in {created}"))
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
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn a_small_file_survives_the_round_trip() {
    let app = app().await;
    let sent: Vec<u8> = (0..4096_u32)
        .map(|i| u8::try_from(i % 251).unwrap_or(0))
        .collect();

    let (code, _) = upload(&app, "small.bin", sent.clone()).await;
    assert_eq!(download(&app, &code).await, sent);
}

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

/// Range requests come from the store, not from a buffer we filled.
///
/// The resume path reads a window out of the middle of an object. Against the
/// local filesystem that is a seek; against S3 it is a `Range` header the store
/// has to honour, which is a different thing to get wrong.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn a_range_request_reads_from_the_middle_of_an_object() {
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

/// Deleting a share removes the object, not just the row.
///
/// A store that kept the bytes would report a quota that shrinks while the bill
/// does not.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn deleting_a_share_removes_its_object() {
    let app = app().await;
    let (code, token) = upload(&app, "doomed.bin", b"gone shortly".to_vec()).await;

    let share = app
        .state
        .shares
        .find_by_code(&tessera_core::RetrievalCode::parse(&code).expect("a code"))
        .await
        .expect("query")
        .expect("the share");
    let keys = app
        .state
        .shares
        .object_keys_of(share.id)
        .await
        .expect("the keys");
    assert_eq!(keys.len(), 1, "expected one object");

    for key in &keys {
        app.state
            .storage
            .head(&object_store::path::Path::from(key.as_str()))
            .await
            .expect("the object should be there before the delete");
    }

    // The uploader was anonymous, so the management token is the only proof
    // that this is theirs to remove.
    expect_status(
        app.send(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/shares/{code}"))
                .header("x-share-management-token", &token),
            Body::empty(),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;

    for key in &keys {
        assert!(
            app.state
                .storage
                .head(&object_store::path::Path::from(key.as_str()))
                .await
                .is_err(),
            "the object outlived its share"
        );
    }
}
