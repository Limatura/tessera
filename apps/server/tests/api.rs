//! End-to-end tests over the assembled router.
//!
//! These drive the real middleware stack — request ids, compression, body limits,
//! the SPA fallback — rather than calling handlers directly, because most of what
//! this round delivers *is* the stack.

mod common;

use axum::http::{StatusCode, header};
use common::{TestApp, body_json, body_string, expect_status};
use tessera_server::{api, state::ServiceStatus};

#[tokio::test]
async fn health_reports_ready() {
    let app = TestApp::ready().await;
    let response = expect_status(app.get("/health").await, StatusCode::OK).await;
    let body = body_json(response).await;

    assert_eq!(body["status"], "ready");
    assert_eq!(body["version"], tessera_server::VERSION);
    assert_eq!(body["build_rev"], tessera_server::BUILD_REV);
    assert_eq!(body["database"], "sqlite");
    assert!(body["uptime_seconds"].is_u64());
}

#[tokio::test]
async fn health_is_503_before_the_service_is_ready() {
    let app = TestApp::build(ServiceStatus::Starting).await;
    let response = expect_status(app.get("/health").await, StatusCode::SERVICE_UNAVAILABLE).await;
    assert_eq!(body_json(response).await["code"], "unavailable");
}

#[tokio::test]
async fn health_is_503_while_draining() {
    let app = TestApp::ready().await;
    app.state.set_status(ServiceStatus::Draining);
    expect_status(app.get("/health").await, StatusCode::SERVICE_UNAVAILABLE).await;
}

#[tokio::test]
async fn every_response_carries_a_request_id() {
    let app = TestApp::ready().await;
    let response = app.get("/health").await;
    let id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("x-request-id header");
    // ULIDs are 26 characters of Crockford base32.
    assert_eq!(id.len(), 26, "expected a ULID, got `{id}`");
}

#[tokio::test]
async fn request_ids_are_unique_per_request() {
    let app = TestApp::ready().await;
    let first = app.get("/health").await;
    let second = app.get("/health").await;
    assert_ne!(
        first.headers().get("x-request-id"),
        second.headers().get("x-request-id")
    );
}

#[tokio::test]
async fn capabilities_report_what_this_instance_exposes() {
    let app = TestApp::ready().await;
    let response = expect_status(app.get("/api/v1/capabilities").await, StatusCode::OK).await;
    let body = body_json(response).await;

    // Defaults: everything on, registration open.
    assert_eq!(body["accounts"], true);
    assert_eq!(body["admin"], true);
    assert_eq!(body["anonymous_upload"], true);
    assert_eq!(body["registration"], "open");
    assert_eq!(body["version"], tessera_server::VERSION);
    assert!(body["password_min_length"].is_u64());
    assert!(body["max_upload_bytes"].is_u64());
}

/// The endpoint has to be reachable before a client knows whether logging in is
/// possible, so it cannot itself require a session.
#[tokio::test]
async fn capabilities_need_no_credentials() {
    let app = TestApp::ready().await;
    let response = app.get("/api/v1/capabilities").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(!response.headers().contains_key(header::WWW_AUTHENTICATE));
}

#[tokio::test]
async fn contract_is_served_as_json() {
    let app = TestApp::ready().await;
    let response = expect_status(app.get(api::OPENAPI_PATH).await, StatusCode::OK).await;
    let document = body_json(response).await;

    assert_eq!(document["info"]["title"], "Tessera API");
    assert!(document["paths"]["/health"]["get"].is_object());
    // Nesting must rewrite the path in the document, not just in the router —
    // otherwise a generated client would call `/capabilities` and get the SPA.
    assert!(
        document["paths"]["/api/v1/capabilities"]["get"].is_object(),
        "versioned paths must appear with their prefix"
    );
}

#[tokio::test]
async fn swagger_ui_is_served_when_docs_are_enabled() {
    let app = TestApp::ready().await;
    // The UI redirects `/docs` to `/docs/` before serving the page.
    let response = app.get("/docs/").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(body_string(response).await.contains("swagger"));
}

/// Regression test. The Swagger UI answers `/docs` with a redirect to `/docs/`;
/// adding trailing-slash normalisation in front of it turns that into an
/// infinite loop, which is exactly what happened the first time this stack ran
/// in a container.
#[tokio::test]
async fn docs_redirect_terminates() {
    let app = TestApp::ready().await;

    let mut path = "/docs".to_owned();
    for hop in 0..5 {
        let response = app.get(&path).await;
        let Some(location) = response.headers().get(header::LOCATION) else {
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "settled on a non-200 at `{path}`"
            );
            return;
        };
        let next = location.to_str().expect("location header").to_owned();
        assert_ne!(next, path, "`{path}` redirects to itself (hop {hop})");
        path = next;
    }
    panic!("`/docs` never stopped redirecting");
}

#[tokio::test]
async fn root_serves_the_embedded_spa() {
    let app = TestApp::ready().await;
    let response = expect_status(app.get("/").await, StatusCode::OK).await;
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("text/html")
    );
    // index.html is the mutable pointer at hashed assets and must not be cached.
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok()),
        Some("no-cache")
    );
    assert!(response.headers().contains_key(header::ETAG));
    assert!(body_string(response).await.contains("<!doctype html>"));
}

#[tokio::test]
async fn unknown_client_route_falls_back_to_the_spa() {
    let app = TestApp::ready().await;
    let response = expect_status(app.get("/some/client/route").await, StatusCode::OK).await;
    assert!(body_string(response).await.contains("<!doctype html>"));
}

#[tokio::test]
async fn missing_asset_is_a_404_rather_than_the_spa() {
    let app = TestApp::ready().await;
    // A missing `.js` means a broken build; answering with HTML would turn that
    // into a confusing syntax error in the browser console.
    expect_status(
        app.get("/assets/does-not-exist.js").await,
        StatusCode::NOT_FOUND,
    )
    .await;
}

#[tokio::test]
async fn path_traversal_is_refused() {
    let app = TestApp::ready().await;
    expect_status(app.get("/../../etc/passwd").await, StatusCode::NOT_FOUND).await;
}

#[tokio::test]
async fn no_cors_headers_are_issued_for_the_same_origin_default() {
    let app = TestApp::ready().await;
    let response = app.get("/health").await;
    assert!(
        !response
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );
}
