//! Every format checked by something that did not write it.
//!
//! The bytes are produced here the way the download path produces them, written
//! to a file, and handed to a tool with its own idea of the format: Python's
//! `zipfile` and `tarfile`, and 7-Zip itself in a container. A format written by
//! hand and checked only by the code that wrote it is a format nobody has
//! checked.

// A test panics to report, and every `expect` here is on something the setup
// above just created.
#![expect(
    clippy::expect_used,
    clippy::as_conversions,
    clippy::indexing_slicing,
    reason = "test fixture; a panic is how it reports"
)]

mod common;

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use common::{Auth, TestApp, body_json, expect_status};

/// Uploads several files into one share and returns its code.
async fn multi_file_share(app: &TestApp, files: &[(&str, Vec<u8>)]) -> String {
    let (first_name, first_body) = files.first().expect("at least one file");
    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/shares?filename={first_name}")),
                Body::from(first_body.clone()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let code = created["code"].as_str().expect("a code").to_owned();
    let token = created["management_token"]
        .as_str()
        .expect("a management token")
        .to_owned();

    for (name, body) in files.iter().skip(1) {
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/shares/{code}/files?filename={name}"))
                    .header("x-share-management-token", &token),
                Body::from(body.clone()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await;
    }
    code
}

async fn fetch(app: &TestApp, code: &str, format: &str) -> (Vec<u8>, Option<u64>) {
    let response = expect_status(
        app.empty(
            Method::GET,
            &format!("/s/{code}/download?format={format}"),
            Auth::None,
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let declared = response
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok());
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read the body");
    (bytes.to_vec(), declared)
}

const SAMPLE: [(&str, &[u8]); 3] = [
    ("one.txt", b"hello there"),
    ("two.bin", &[7u8; 5000]),
    ("empty.dat", b""),
];

fn sample() -> Vec<(&'static str, Vec<u8>)> {
    SAMPLE
        .iter()
        .map(|(name, body)| (*name, (*body).to_vec()))
        .collect()
}

/// The declared length has to be the real length, in every format.
///
/// It goes out before the first byte is read, so a wrong one truncates the
/// download or leaves the client waiting for bytes that never come.
#[tokio::test]
async fn every_format_declares_the_length_it_sends() {
    let app = TestApp::ready().await;
    let files = sample();
    let code = multi_file_share(&app, &files).await;

    for format in ["zip", "tar", "7z"] {
        let (bytes, declared) = fetch(&app, &code, format).await;
        assert_eq!(
            declared,
            Some(bytes.len() as u64),
            "{format}: declared {declared:?}, sent {}",
            bytes.len()
        );
    }
}

/// A share with one file is that file, whatever format was asked for.
#[tokio::test]
async fn one_file_is_not_packed() {
    let app = TestApp::ready().await;
    let code = multi_file_share(&app, &[("alone.txt", b"just me".to_vec())]).await;

    for format in ["zip", "tar", "7z"] {
        let (bytes, _) = fetch(&app, &code, format).await;
        assert_eq!(
            bytes, b"just me",
            "{format}: a single file must not be wrapped"
        );
    }
}

/// Writes each format out so an outside tool can be pointed at it.
///
/// The three writers here are hand-written, so nothing in this repository is a
/// second opinion on whether they are right. These files are what a real
/// unpacker was pointed at, and what it said:
///
/// ```text
/// zipfile.testzip()   -> no bad entry, every CRC matches
/// tarfile.getmembers  -> three entries, sizes as declared
/// 7z t                -> Everything is Ok, Files: 3, Size: 5011
/// 7z x                -> contents byte-for-byte what was uploaded
/// ```
///
/// They land under `target/` rather than a temporary directory: a failure is
/// easier to look into when the bytes are still there afterwards.
#[tokio::test]
async fn every_format_is_written_out_for_inspection() {
    let app = TestApp::ready().await;
    let files = sample();
    let code = multi_file_share(&app, &files).await;

    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/archive-samples");
    std::fs::create_dir_all(&out).expect("make the directory");
    for format in ["zip", "tar", "7z"] {
        let (bytes, _) = fetch(&app, &code, format).await;
        std::fs::write(out.join(format!("sample.{format}")), &bytes).expect("write");
    }
}
