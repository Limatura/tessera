//! What a request is given, and what it is not.
//!
//! The bug these were written for: the wall-clock budget covered the time spent
//! receiving the body, so an upload's real ceiling was that budget times the
//! caller's line speed rather than `server.max_body_bytes`. On the defaults, a
//! 100 MiB file needed 28 Mbit/s of sustained upload to arrive at all, and the
//! `408` said nothing about why.

mod common;

use std::time::{Duration, Instant};

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use common::TestApp;

/// A body sent in `pieces`, one every `gap`.
fn dribble(pieces: usize, gap: Duration) -> Body {
    Body::from_stream(async_stream::stream! {
        for _ in 0..pieces {
            tokio::time::sleep(gap).await;
            yield Ok::<_, std::io::Error>(vec![b'x'; 1024]);
        }
    })
}

async fn post(app: &TestApp, uri: &str, content_type: &str, body: Body) -> StatusCode {
    app.send(
        Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header("content-type", content_type),
        body,
    )
    .await
    .status()
}

/// The bug. A file arriving slower than the request budget is not a failure —
/// it is a file arriving over a slow line, which is the ordinary case for
/// somebody uploading from home.
#[tokio::test]
async fn an_upload_slower_than_the_request_budget_still_finishes() {
    let app = TestApp::with_config(|config| {
        config.server.request_timeout_secs = 1;
        config.server.upload_stall_secs = 30;
    })
    .await;

    // Four seconds of sending against a one-second request budget.
    let status = post(
        &app,
        "/api/v1/shares?filename=slow.bin",
        "application/octet-stream",
        dribble(8, Duration::from_millis(500)),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "a slow upload was cut off");
}

/// The other half: a caller that connects and stops sending must not hold the
/// slot the previous test just opened up.
#[tokio::test]
async fn an_upload_that_stops_sending_is_dropped() {
    let app = TestApp::with_config(|config| {
        config.server.request_timeout_secs = 30;
        config.server.upload_stall_secs = 1;
    })
    .await;

    let stalls = Body::from_stream(async_stream::stream! {
        yield Ok::<_, std::io::Error>(vec![b'x'; 1024]);
        // Then nothing, for much longer than the stall allows.
        tokio::time::sleep(Duration::from_secs(10)).await;
        yield Ok::<_, std::io::Error>(vec![b'x'; 1024]);
    });

    let started = Instant::now();
    let status = post(
        &app,
        "/api/v1/shares?filename=stalled.bin",
        "application/octet-stream",
        stalls,
    )
    .await;

    assert_eq!(status, StatusCode::REQUEST_TIMEOUT);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "it waited {:?}, so it was the ten-second body that ended it rather than the stall limit",
        started.elapsed()
    );
}

/// The exemption is for the two routes that receive a file, not for everything
/// that shares their prefix. `POST /shares/{code}/events` is an ordinary request
/// and keeps its budget — exempting it would hand an anonymous caller a socket
/// nothing reclaims.
#[tokio::test]
async fn a_neighbouring_route_still_has_a_wall_clock_budget() {
    let app = TestApp::with_config(|config| {
        config.server.request_timeout_secs = 1;
        config.server.upload_stall_secs = 30;
    })
    .await;

    let status = post(
        &app,
        "/api/v1/shares/abcdefgh/events",
        // JSON, so the extractor reads the body rather than refusing the type
        // before it gets there.
        "application/json",
        dribble(8, Duration::from_millis(500)),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::REQUEST_TIMEOUT,
        "a slow body on a non-upload route was allowed to run past the budget"
    );
}
