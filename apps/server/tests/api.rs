//! End-to-end tests over the assembled router.
//!
//! These drive the real middleware stack — request ids, compression, body limits,
//! the SPA fallback — rather than calling handlers directly, because most of what
//! this round delivers *is* the stack.

mod common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use common::{Auth, TestApp, body_json, body_string, expect_status, session_cookie};
use tessera_server::{api, state::ServiceStatus};

/// The bootstrap administrator's password, for the tests that need to be one.
const ADMIN_PASSWORD: &str = "correct horse battery staple";

/// Bootstraps the administrator and signs in as them, returning the session
/// cookie ready to send back.
///
/// See `common/mod.rs`: the test-code exemption only reaches code inside a
/// `#[test]` body, so a helper has to restate it.
#[expect(
    clippy::expect_used,
    reason = "test helper; a fixture that cannot build has nothing to report but a failure"
)]
async fn admin_session(app: &TestApp) -> String {
    tessera_server::auth::bootstrap::ensure_admin(&app.state.users, &app.state.config.auth)
        .await
        .expect("bootstrap");
    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/login",
            Auth::None,
            &serde_json::json!({ "identifier": "admin", "password": ADMIN_PASSWORD }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    session_cookie(&response)
}

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
    assert!(body["account_password_min_length"].is_u64());
    assert!(body["max_upload_bytes"].is_u64());

    // Everything the upload form needs to draw itself without guessing.
    assert!(
        body["share_password_min_length"].as_u64() < body["account_password_min_length"].as_u64(),
        "a share password is deliberately allowed to be shorter than an account's"
    );
    assert!(body["default_expiry_hours"].is_u64());
    assert!(body["max_expiry_hours"].is_u64());

    // The one field that is not everybody's business.
    assert!(
        body["storage_remaining_bytes"].is_null(),
        "an anonymous caller must not learn how full the disk is"
    );
}

/// How much room is left is the operator's capacity planning, not the visitor's
/// business — telling every anonymous uploader how close the disk is to full
/// invites somebody to close the gap.
#[tokio::test]
async fn only_an_administrator_is_told_how_much_room_is_left() {
    let quota = 50 * 1024 * 1024;
    let app = TestApp::with_config(move |config| {
        config.storage.max_total_bytes = quota;
        config.auth.admin_password = ADMIN_PASSWORD.to_owned();
    })
    .await;

    let anonymous =
        body_json(expect_status(app.get("/api/v1/capabilities").await, StatusCode::OK).await).await;
    assert!(anonymous["storage_remaining_bytes"].is_null());

    let cookie = admin_session(&app).await;
    let admin = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/capabilities", Auth::Cookie(&cookie))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(
        admin["storage_remaining_bytes"].as_u64(),
        Some(quota),
        "nothing stored yet, so the whole quota is free"
    );
}

/// An instance with no quota has no number to report, and saying `0` would read
/// as "full" to a client that cannot tell the two apart.
#[tokio::test]
async fn an_instance_without_a_quota_reports_no_remaining_room() {
    let app = TestApp::with_config(|config| {
        config.auth.admin_password = ADMIN_PASSWORD.to_owned();
    })
    .await;
    let cookie = admin_session(&app).await;

    let body = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/capabilities", Auth::Cookie(&cookie))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert!(body["storage_remaining_bytes"].is_null());
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
async fn the_api_reference_is_off_by_default() {
    // A development convenience, so the shipped defaults do not serve it. An
    // instance that has not opted in answers `/docs` with the SPA, like any
    // other unknown client-side route.
    let app = TestApp::ready().await;
    let response = expect_status(app.get("/docs").await, StatusCode::OK).await;
    let body = body_string(response).await;
    assert!(
        body.contains("<!doctype html>"),
        "should be the SPA fallback"
    );
    assert!(
        !body.contains("scalar"),
        "the reference should not be mounted"
    );
}

#[tokio::test]
async fn the_api_reference_is_served_when_enabled() {
    let app = TestApp::with_config(|config| config.features.docs = true).await;
    let response = expect_status(app.get("/docs").await, StatusCode::OK).await;
    assert!(body_string(response).await.contains("scalar"));
}

/// Scalar serves the page directly, with no redirect. The previous UI answered
/// `/docs` with a redirect to `/docs/`, which became an infinite loop the first
/// time this stack ran behind trailing-slash normalisation — so the absence of a
/// redirect is worth asserting rather than assuming.
#[tokio::test]
async fn the_api_reference_does_not_redirect() {
    let app = TestApp::with_config(|config| config.features.docs = true).await;
    let response = app.get("/docs").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(!response.headers().contains_key(header::LOCATION));
}

/// Turning the UI off must not hide the contract: client generators and the CI
/// drift check both read it from a running instance.
#[tokio::test]
async fn the_contract_is_served_whether_or_not_the_reference_is() {
    for docs in [false, true] {
        let app = TestApp::with_config(|config| config.features.docs = docs).await;
        let response = app.get(api::OPENAPI_PATH).await;
        assert_eq!(response.status(), StatusCode::OK, "docs = {docs}");
    }
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

/// A front-end on its own dev server has to be able to hold the session cookie.
/// Without `Access-Control-Allow-Credentials` a browser drops it, and the app
/// logs in and is immediately logged out again.
#[tokio::test]
async fn a_named_origin_may_carry_credentials() {
    let app = TestApp::with_config(|config| {
        config.server.cors_allowed_origins = vec!["http://localhost:3001".to_owned()];
    })
    .await;

    let response = app
        .send(
            Request::builder()
                .uri("/api/v1/capabilities")
                .header(header::ORIGIN, "http://localhost:3001"),
            Body::empty(),
        )
        .await;

    assert_eq!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:3001")
    );
    assert_eq!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .and_then(|value| value.to_str().ok()),
        Some("true")
    );
}

/// The Fetch standard refuses credentials alongside a wildcard origin, and a
/// browser drops the entire response rather than explaining why.
#[tokio::test]
async fn a_wildcard_origin_does_not_claim_to_carry_credentials() {
    let app = TestApp::with_config(|config| {
        config.server.cors_allowed_origins = vec!["*".to_owned()];
    })
    .await;

    let response = app
        .send(
            Request::builder()
                .uri("/api/v1/capabilities")
                .header(header::ORIGIN, "http://anywhere.example"),
            Body::empty(),
        )
        .await;

    assert_eq!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("*")
    );
    assert!(
        !response
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
    );
}

/// Compressing a download would drop `Content-Length` in favour of chunked
/// encoding, so the browser loses the progress bar and the estimated time — on
/// exactly the transfers where somebody is watching them. And a ZIP does not get
/// smaller anyway.
///
/// Asserted on the predicate directly. The alternative is inferring the policy
/// from response headers, which tests the plumbing rather than the decision.
#[test]
fn the_compression_predicate_skips_opaque_media_types() {
    use axum::http::Response;
    use tower_http::compression::predicate::Predicate as _;

    let predicate = api::compression_predicate();
    let judge = |content_type: &str| {
        let response = Response::builder()
            .header(header::CONTENT_TYPE, content_type)
            // A real body: the size half of the default predicate reads the
            // body's size hint, not the `Content-Length` header.
            .body(Body::from(vec![0_u8; 100_000]))
            .expect("response");
        predicate.should_compress(&response)
    };

    // The download route's type is fixed at `application/octet-stream` whatever
    // was uploaded, so that one entry covers every download.
    for opaque in [
        "application/octet-stream",
        "application/zip",
        "video/mp4",
        "audio/mpeg",
        "font/woff2",
    ] {
        assert!(!judge(opaque), "{opaque} should not be compressed");
    }

    // `/s/{code}/raw` is text, and text is where compression pays.
    for worthwhile in ["text/plain; charset=utf-8", "application/json"] {
        assert!(judge(worthwhile), "{worthwhile} should be compressed");
    }
}

/// An unfurler fetches the URL and reads the HTML; it does not run JavaScript,
/// so tags the app adds at runtime are invisible to it. They have to be in the
/// bytes the server sends.
#[tokio::test]
async fn a_share_page_carries_its_link_preview() {
    let app = TestApp::ready().await;
    let created = expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=holiday.jpg&max_uses=3"),
            Body::from(vec![0_u8; 2048]),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let share = body_json(created).await;
    let code = share["code"].as_str().expect("a code").to_owned();

    let page =
        body_string(expect_status(app.get(&format!("/s/{code}")).await, StatusCode::OK).await)
            .await;
    assert!(
        page.contains(r#"<meta property="og:title" content="holiday.jpg">"#),
        "{page}"
    );
    assert!(page.contains("2.0 KiB"), "{page}");

    // Fetching the preview must not spend a retrieval: an unfurler follows the
    // link the moment it is pasted, before any person clicks.
    for _ in 0..5 {
        expect_status(app.get(&format!("/s/{code}")).await, StatusCode::OK).await;
    }
    let after = body_json(
        expect_status(
            app.get(&format!("/api/v1/shares/{code}")).await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(after["uses_remaining"], 3, "a preview is not a retrieval");
}

/// A preview is shown to everyone in the channel. A password-protected share
/// announcing its file name there would make the password decorative.
#[tokio::test]
async fn a_locked_share_gives_nothing_away_in_its_preview() {
    let app = TestApp::ready().await;
    let created = expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=payroll.xlsx&title=Salaries")
                .header("x-share-password", "1234"),
            Body::from("secret"),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let code = body_json(created).await["code"]
        .as_str()
        .expect("a code")
        .to_owned();

    let page =
        body_string(expect_status(app.get(&format!("/s/{code}")).await, StatusCode::OK).await)
            .await;
    assert!(!page.contains("payroll"), "{page}");
    assert!(!page.contains("Salaries"), "{page}");
    assert!(page.contains("Password-protected share"), "{page}");
}

/// The title is attacker-supplied and goes into an attribute on a page served
/// from this origin.
#[tokio::test]
async fn a_title_cannot_inject_script_into_the_page() {
    let app = TestApp::ready().await;
    let created = expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?title=%22%3E%3Cscript%3Ealert(1)%3C%2Fscript%3E"),
            Body::from("x"),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let code = body_json(created).await["code"]
        .as_str()
        .expect("a code")
        .to_owned();

    let page =
        body_string(expect_status(app.get(&format!("/s/{code}")).await, StatusCode::OK).await)
            .await;
    assert!(!page.contains("<script>alert(1)"), "{page}");
    assert!(page.contains("&quot;&gt;&lt;script&gt;"), "{page}");
}
