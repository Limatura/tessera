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
//! # A second bucket, for the test that a file is read from the backend it
//! # names — see `a_file_is_read_from_the_bucket_it_names`. Garage's CLI is in
//! # the image, so this needs no S3 client:
//! #
//! #   docker exec garage /garage bucket create tessera-cold
//! #   docker exec garage /garage bucket allow --read --write --owner \
//! #     tessera-cold --key tesseratest
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
//! TESSERA_TEST_STORAGE_URL_SECOND=s3://tessera-cold \
//!   cargo nextest run --run-ignored all -E 'binary(object_store)'
//! ```
//!
//! Azure goes through Azurite, whose account and key are fixed and published.
//! Its containers have to exist first, and creating one is a Shared Key
//! signature rather than a plain request — `.github/workflows/rust.yml` signs it
//! with `openssl` so no Azure client has to be installed. One trap worth
//! knowing: `date` obeys the locale, and a localised day name inside the signed
//! string produces a `403` with nothing in the body saying why, so that command
//! runs under `LC_ALL=C`.
//!
//! ```text
//! docker run -d --name azurite -p 31000:10000 \
//!   mcr.microsoft.com/azure-storage/azurite:3.36.0 \
//!   azurite-blob --blobHost 0.0.0.0 --skipApiVersionCheck
//!
//! AZURE_STORAGE_ACCOUNT_NAME=devstoreaccount1 \
//! AZURE_STORAGE_ACCOUNT_KEY=Eby8vdM0...soGMGw== \
//! AZURE_STORAGE_ENDPOINT=http://127.0.0.1:31000/devstoreaccount1 \
//! AZURE_ALLOW_HTTP=true \
//! TESSERA_TEST_STORAGE_URL=azure://tessera \
//! TESSERA_TEST_STORAGE_URL_SECOND=azure://tessera-cold \
//!   cargo nextest run --run-ignored all -E 'binary(object_store)'
//! ```
//!
//! WebDAV runs without credentials, because a URL cannot carry them: `reqwest`
//! rejects `http://user:pass@host` outright with `BadScheme`. Basic auth would
//! have to arrive as options, which nothing here builds yet.
//!
//! Two things are easy to lose an afternoon to. Plain HTTP needs
//! `allow_http=true`, the same thing `AWS_ALLOW_HTTP` says for S3, and it cannot
//! come from the environment: `is_backend_option` asks the S3, GCS and Azure key
//! sets whether a name is a real option, and the HTTP store's keys are in none
//! of them — so a WebDAV backend is configurable through the config file and the
//! admin API and nowhere else. And WebDAV does not create parent collections on
//! `PUT`, so the collections the suite writes into have to exist first or every
//! upload comes back `409`.
//!
//! For the same reason this suite runs `--test-threads 1` against WebDAV.
//! `object_store` answers a `409` by walking up with MKCOL and retrying the
//! write once; concurrent first writes into a fresh backend race to create
//! `shares/YYYY/MM`, and an upload whose single retry lands while a sibling's
//! MKCOL is still in flight has nothing left to try. That is a property of the
//! backend rather than of these tests — a real WebDAV store can return `500` on
//! its first burst of uploads for exactly this reason.
//!
//! ```text
//! docker run -d --name webdav -p 38080:8080 \
//!   rclone/rclone:1.75.0 serve webdav /data --addr :8080
//! curl -X MKCOL http://127.0.0.1:38080/tessera
//! curl -X MKCOL http://127.0.0.1:38080/tessera-cold
//!
//! TESSERA_TEST_STORAGE_URL=http://127.0.0.1:38080/tessera \
//! TESSERA_TEST_STORAGE_URL_SECOND=http://127.0.0.1:38080/tessera-cold \
//! TESSERA_TEST_STORAGE_OPTIONS=allow_http=true \
//!   cargo nextest run --run-ignored all -E 'binary(object_store)'
//! ```
//!
//! # Which stores this runs against
//!
//! Garage, `RustFS`, Azurite and a WebDAV server in CI. Two S3 implementations,
//! because Garage is deliberately permissive — see below — and a single
//! implementation is a single opinion about what S3 means. `RustFS` is stricter,
//! and it is what the self-hosting audience moved to after MinIO's repository
//! was archived. Azurite is there because Azure is a different wire format, not
//! a stricter reading of the same one. WebDAV is not object storage at all: it
//! has no multipart, so it runs this binary only, which is what
//! `object_store_multipart` being separate is for.
//!
//! One was tried and is not here:
//!
//! - **fake-gcs-server** does not implement the GCS *XML* API for writes, which is the one
//!   `object_store` uses — its `put` goes to `{base}/{bucket}/{path}` with the object's name in the
//!   path (see the `xml-api/put-object-upload` link in `gcp/client.rs`). Without `-public-host`
//!   that path has no handler at all and answers `404`; with it, the request is routed into the
//!   *JSON* upload handler, which wants `?uploadType=media&name=…` and rejects the bare `PUT` with
//!   `name is required for simple uploads`. The two disagree about where an object's name goes, so
//!   nothing configurable brings them together. `fsouza/fake-gcs-server#852` records the multipart
//!   half of the same gap.
//!
//! # Real providers
//!
//! Cloudflare R2, Backblaze B2 and AWS S3 have no emulator, and they are the
//! only things that can confirm the rules this repository encodes about them —
//! `config.rs` records R2's equal-part requirement, and nothing here enforces
//! it. So the job carries a step gated on `vars.STORAGE_PROVIDER_URL`: it costs
//! nothing until somebody sets credentials, and it does not block a merge when
//! a provider has an outage, because that is their failure and not ours.
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

/// Options for the store under test, as `key=value` pairs separated by commas.
///
/// Plain HTTP needs `allow_http=true` — the same thing `AWS_ALLOW_HTTP` says for
/// S3 — and no environment variable can carry it, because `is_backend_option`
/// asks only the S3, GCS and Azure key sets whether a name is a real option. So
/// a WebDAV backend is configurable through the config file and the admin API
/// and nowhere else, and this is how the suite reaches the same place.
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
    let options = storage_options();
    TestApp::with_config(move |config| {
        config.storage.url = url;
        config.storage.options = options;
    })
    .await
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

/// Range requests come from the store, not from a buffer we filled.
///
/// Small on purpose: under `share.inline_threshold_bytes` the object goes up in
/// one `put`, so this stays inside what every backend can do — a store without
/// multipart still has to honour a `Range`. The multipart binary carries the
/// same check across a part boundary, which is the harder half and needs a
/// store that can write one.
#[tokio::test]
#[ignore = "needs an object store; see the module docs"]
async fn a_range_request_reads_from_the_middle_of_an_object() {
    let app = app().await;
    let sent: Vec<u8> = (0..32 * 1024_u32)
        .map(|i| u8::try_from(i % 251).unwrap_or(0))
        .collect();
    let (code, _) = upload(&app, "ranged.bin", sent.clone()).await;

    let (from, to) = (20_000_usize, 20_255_usize);
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

/// A second bucket, when the suite is pointed at a store that has one.
///
/// The multi-backend work is the reason this matters: a file records which
/// backend holds it, and nothing but a real store proves that the recorded name
/// is what the reader actually opens. Against one bucket the wrong answer and
/// the right answer look identical.
fn second_url() -> Option<String> {
    std::env::var("TESSERA_TEST_STORAGE_URL_SECOND").ok()
}

/// The name a file records decides which bucket its bytes come from.
///
/// Written against two real buckets because the failure this guards is
/// "the reader ignored the name and used whichever store it had": with one
/// bucket that bug passes every test, and it is exactly what shipped before
/// backends were recorded.
#[tokio::test]
#[ignore = "needs a live object store; see the module docs"]
async fn a_file_is_read_from_the_bucket_it_names() {
    let Some(second) = second_url() else {
        panic!("TESSERA_TEST_STORAGE_URL_SECOND is not set; see the module docs");
    };

    let app = app().await;
    let (code, _) = upload(&app, "kept.bin", vec![4_u8; 32 * 1024]).await;
    expect_status(
        app.get(&format!("/s/{code}/download")).await,
        StatusCode::OK,
    )
    .await;

    // The name every file in this test recorded, pointed at the *other*
    // bucket. Nothing was copied there, so the bytes cannot be found — unless
    // the reader is ignoring the name.
    let elsewhere = tessera_server::storage::build(&tessera_server::config::StorageConfig {
        url: second,
        // The same options as the first, for the same reason: a store that
        // cannot be opened would fail this test's assertion by accident.
        options: storage_options(),
        ..tessera_server::config::StorageConfig::default()
    })
    .expect("open the second bucket");
    app.state.replace_backends(tessera_server::storage::Pool {
        open: tessera_server::storage::Backends::from([(
            tessera_server::storage::DEFAULT_BACKEND.to_owned(),
            (elsewhere, None),
        )]),
        configured: Vec::new(),
    });

    assert_ne!(
        app.get(&format!("/s/{code}/download")).await.status(),
        StatusCode::OK,
        "served bytes from a bucket this file does not name"
    );
}
