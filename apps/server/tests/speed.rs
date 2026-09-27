//! The speed test, and the three things that keep it from being free bandwidth:
//! off by default, then accounts only, and every test capped and counted.

mod common;

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
};
use common::{Auth, TestApp, body_json, expect_status, session_cookie};

const PASSWORD: &str = "a perfectly adequate passphrase";

/// An instance offering the test, with `anonymous` as given and a small cap.
async fn testing(anonymous: bool) -> TestApp {
    TestApp::with_config(move |config| {
        config.speed.enabled = true;
        config.speed.anonymous = anonymous;
        config.speed.max_bytes = 4096;
    })
    .await
}

async fn an_account(app: &TestApp) -> String {
    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": "member",
                "email": "member@example.com",
                "password": PASSWORD,
            }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    session_cookie(&response)
}

async fn download(app: &TestApp, query: &str, auth: Auth<'_>) -> axum::response::Response {
    app.empty(Method::GET, &format!("/api/v1/speed/download{query}"), auth)
        .await
}

async fn upload(app: &TestApp, bytes: usize, auth: Auth<'_>) -> axum::response::Response {
    let builder = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/speed/upload");
    let builder = match auth {
        Auth::Cookie(cookie) => builder.header(axum::http::header::COOKIE, cookie),
        Auth::Bearer(token) => {
            builder.header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
        }
        Auth::None => builder,
    };
    app.send(builder, Body::from(vec![7_u8; bytes])).await
}

#[tokio::test]
async fn a_test_moves_the_bytes_it_was_asked_for() {
    let app = testing(true).await;

    let sent = expect_status(
        download(&app, "?bytes=1000", Auth::None).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        sent.headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store, no-transform")
    );
    let body = to_bytes(sent.into_body(), usize::MAX).await.expect("body");
    assert_eq!(body.len(), 1000);
    // Not all zeroes: a proxy compressing those would be timing itself.
    assert!(body.iter().any(|byte| *byte != 0));

    let read =
        body_json(expect_status(upload(&app, 1000, Auth::None).await, StatusCode::OK).await).await;
    assert_eq!(read["received_bytes"], 1000);

    // Omitting the size requests the full allowance.
    let whole = expect_status(download(&app, "", Auth::None).await, StatusCode::OK).await;
    assert_eq!(
        to_bytes(whole.into_body(), usize::MAX)
            .await
            .expect("body")
            .len(),
        4096
    );
}

/// Off means off, indistinguishable from an instance without the feature.
#[tokio::test]
async fn an_instance_that_has_not_turned_it_on_has_no_speed_test() {
    let app = TestApp::ready().await;
    let capabilities =
        body_json(expect_status(app.get("/api/v1/capabilities").await, StatusCode::OK).await).await;
    assert_eq!(capabilities["speed_test"], false);

    expect_status(
        download(&app, "?bytes=100", Auth::None).await,
        StatusCode::NOT_FOUND,
    )
    .await;
    expect_status(upload(&app, 100, Auth::None).await, StatusCode::NOT_FOUND).await;
}

/// Accounts only: a visitor gets the same capability and endpoint answers as
/// on an instance without the test.
#[tokio::test]
async fn a_visitor_sees_nothing_when_the_test_is_for_accounts() {
    let app = testing(false).await;
    let cookie = an_account(&app).await;

    let seen = async |app: &TestApp, auth: Auth<'_>| {
        body_json(
            expect_status(
                app.empty(Method::GET, "/api/v1/capabilities", auth).await,
                StatusCode::OK,
            )
            .await,
        )
        .await["speed_test"]
            .as_bool()
            .expect("a flag")
    };
    assert!(!seen(&app, Auth::None).await);
    assert!(seen(&app, Auth::Cookie(&cookie)).await);

    expect_status(
        download(&app, "?bytes=100", Auth::None).await,
        StatusCode::NOT_FOUND,
    )
    .await;
    expect_status(upload(&app, 100, Auth::None).await, StatusCode::NOT_FOUND).await;
    expect_status(
        download(&app, "?bytes=100", Auth::Cookie(&cookie)).await,
        StatusCode::OK,
    )
    .await;
}

#[tokio::test]
async fn a_test_larger_than_the_instance_allows_is_refused() {
    let app = testing(true).await;
    expect_status(
        download(&app, "?bytes=4097", Auth::None).await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    expect_status(
        download(&app, "?bytes=0", Auth::None).await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    // The upload is bounded as it arrives, not by what it claims.
    expect_status(
        upload(&app, 5000, Auth::None).await,
        StatusCode::PAYLOAD_TOO_LARGE,
    )
    .await;
}

/// The bound that matters for an open instance: so many tests per address.
#[tokio::test]
async fn one_source_gets_so_many_tests_an_hour() {
    let app = TestApp::with_config(|config| {
        config.speed.enabled = true;
        config.speed.anonymous = true;
        config.speed.max_bytes = 1024;
        config.auth.throttle.speed.max_attempts = 2;
    })
    .await;

    for _ in 0..2 {
        expect_status(
            download(&app, "?bytes=10", Auth::None).await,
            StatusCode::OK,
        )
        .await;
    }
    let refused = expect_status(
        download(&app, "?bytes=10", Auth::None).await,
        StatusCode::TOO_MANY_REQUESTS,
    )
    .await;
    assert!(refused.headers().contains_key("retry-after"));
    // Both directions draw on the same allowance.
    expect_status(
        upload(&app, 10, Auth::None).await,
        StatusCode::TOO_MANY_REQUESTS,
    )
    .await;
}
