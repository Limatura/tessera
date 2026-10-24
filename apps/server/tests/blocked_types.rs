//! `share.blocked_extensions`: file types an instance will not share.

mod common;

use axum::{
    body::Body,
    http::{Method, Request, Response, StatusCode},
};
use common::{TestApp, body_json, expect_status};

/// The start of a Windows program.
const PROGRAM: &[u8] = b"MZ\x90\0\x03\0\0\0\x04\0\0\0\xff\xff\0\0";

async fn blocking(entries: &[&str]) -> TestApp {
    let entries: Vec<String> = entries.iter().map(|&entry| entry.to_owned()).collect();
    TestApp::with_config(|config| {
        config.share.blocked_extensions = entries;
        config.features.encryption = true;
    })
    .await
}

async fn post(app: &TestApp, query: &str, body: &[u8]) -> Response<Body> {
    app.send(
        Request::builder()
            .method(Method::POST)
            .uri(format!("/api/v1/shares{query}")),
        Body::from(body.to_vec()),
    )
    .await
}

async fn refused(response: Response<Body>, extension: &str) {
    let body = body_json(expect_status(response, StatusCode::FORBIDDEN).await).await;
    assert_eq!(
        body.get("code").and_then(serde_json::Value::as_str),
        Some("file_type_blocked")
    );
    assert_eq!(
        body.pointer("/details/extension")
            .and_then(serde_json::Value::as_str),
        Some(extension)
    );
}

#[tokio::test]
async fn a_listed_extension_is_refused_before_anything_is_sent() {
    let app = blocking(&[".EXE", "apk"]).await;
    refused(post(&app, "?filename=Setup.exe", b"hello").await, "EXE").await;
    refused(post(&app, "?filename=game.APK", b"hello").await, "apk").await;

    let opened = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/uploads?filename=setup.exe")
                .header("upload-length", "5"),
            Body::empty(),
        )
        .await;
    refused(opened, "EXE").await;
}

#[tokio::test]
async fn a_renamed_program_is_refused_by_its_first_bytes() {
    let app = blocking(&["exe"]).await;
    refused(post(&app, "?filename=cat.jpg", PROGRAM).await, "exe").await;

    // And through a resumable upload, whose bytes arrive separately.
    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/uploads?filename=cat.jpg")
                    .header("upload-length", PROGRAM.len().to_string()),
                Body::empty(),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let id = created["id"].as_str().unwrap_or_default();
    let token = created["token"].as_str().unwrap_or_default();
    let patched = app
        .send(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/uploads/{id}"))
                .header("content-type", "application/offset+octet-stream")
                .header("upload-offset", "0")
                .header("x-upload-token", token),
            Body::from(PROGRAM.to_vec()),
        )
        .await;
    refused(patched, "exe").await;
}

#[tokio::test]
async fn what_is_not_listed_goes_through() {
    let app = blocking(&["apk"]).await;
    expect_status(
        post(&app, "?filename=tool.bin", PROGRAM).await,
        StatusCode::CREATED,
    )
    .await;
    expect_status(
        post(&app, "?filename=notes.txt", b"hello").await,
        StatusCode::CREATED,
    )
    .await;
}

/// The server can read neither the name nor the bytes, and serves ciphertext.
#[tokio::test]
async fn an_encrypted_file_goes_through() {
    let app = blocking(&["exe"]).await;
    expect_status(
        post(&app, "?encryption=link&sealed_name=AAAA", PROGRAM).await,
        StatusCode::CREATED,
    )
    .await;
}

#[tokio::test]
async fn a_rule_added_later_stops_what_was_shared_before_it() {
    let app = blocking(&[]).await;
    let renamed = body_json(
        expect_status(
            post(&app, "?filename=cat.jpg", PROGRAM).await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let named = body_json(
        expect_status(
            post(&app, "?filename=setup.exe", b"hello").await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let plain = body_json(
        expect_status(
            post(&app, "?filename=notes.txt", b"hello").await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;

    let mut config = tessera_server::Config::clone(&app.state.config());
    config.share.blocked_extensions = vec!["exe".to_owned()];
    app.state.replace_config(std::sync::Arc::new(config));

    for share in [&renamed, &named] {
        let code = share["code"].as_str().unwrap_or_default();
        refused(app.get(&format!("/s/{code}/download")).await, "exe").await;
    }
    let code = plain["code"].as_str().unwrap_or_default();
    expect_status(
        app.get(&format!("/s/{code}/download")).await,
        StatusCode::OK,
    )
    .await;
}
