//! `bandwidth.*`: transfers held to the configured rate.
//!
//! Timed against the real clock with small numbers, so the margins are wide:
//! the point is "slowed down to about this", not a benchmark.

mod common;

use std::time::{Duration, Instant};

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
};
use common::{Auth, TestApp, expect_status};

/// 32 KiB at 16 KiB/s: about two seconds, less the 100 ms burst and the last
/// 4 KiB piece, which goes when it starts rather than when it ends.
const BYTES: usize = 32 * 1024;
const RATE: u64 = 16 * 1024;

async fn speed_test_with(set: impl FnOnce(&mut tessera_server::config::Config)) -> TestApp {
    TestApp::with_config(move |config| {
        config.speed.enabled = true;
        config.speed.anonymous = true;
        config.speed.max_bytes = 1 << 20;
        set(config);
    })
    .await
}

#[expect(
    clippy::expect_used,
    reason = "a test helper; a missing body is a failed test"
)]
async fn timed_download(app: &TestApp) -> Duration {
    let started = Instant::now();
    let response = expect_status(
        app.empty(
            Method::GET,
            &format!("/api/v1/speed/download?bytes={BYTES}"),
            Auth::None,
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body arrives");
    assert_eq!(body.len(), BYTES);
    started.elapsed()
}

async fn timed_upload(app: &TestApp) -> Duration {
    let started = Instant::now();
    let builder = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/speed/upload");
    expect_status(
        app.send(builder, Body::from(vec![7_u8; BYTES])).await,
        StatusCode::OK,
    )
    .await;
    started.elapsed()
}

#[tokio::test]
async fn unlimited_is_the_default() {
    let app = speed_test_with(|_| {}).await;
    assert!(timed_download(&app).await < Duration::from_secs(1));
    assert!(timed_upload(&app).await < Duration::from_secs(1));
}

#[tokio::test]
async fn the_server_wide_download_cap_slows_a_download() {
    let app = speed_test_with(|config| config.bandwidth.download = RATE).await;
    let took = timed_download(&app).await;
    assert!(took >= Duration::from_millis(1500), "took {took:?}");
    // The other direction is not touched.
    assert!(timed_upload(&app).await < Duration::from_secs(1));
}

#[tokio::test]
async fn the_per_transfer_upload_cap_slows_an_upload() {
    let app = speed_test_with(|config| config.bandwidth.upload_per_transfer = RATE).await;
    let took = timed_upload(&app).await;
    assert!(took >= Duration::from_millis(1500), "took {took:?}");
    assert!(timed_download(&app).await < Duration::from_secs(1));
}
