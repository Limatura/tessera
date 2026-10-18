//! Uploading with `curl -T`, over HTTP.
//!
//! The helpers below sit outside a `#[test]` body, where `clippy.toml`'s test
//! exemptions do not reach. A fixture that cannot read what the server sent
//! has nothing to report but a failure.
#![expect(clippy::expect_used, reason = "test fixtures; see module docs")]

mod common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use common::{Auth, TestApp, body_json, body_string, expect_status, session_cookie};

/// `curl -T <name>` with the given extra headers.
async fn put(
    app: &TestApp,
    path: &str,
    body: &'static [u8],
    headers: &[(&str, &str)],
) -> axum::response::Response {
    let mut request = Request::builder()
        .method(Method::PUT)
        .uri(path)
        .header(header::HOST, "share.example");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    app.send(request, Body::from(body)).await
}

async fn describe(app: &TestApp, code: &str, password: Option<&str>) -> serde_json::Value {
    let mut request = Request::builder().uri(format!("/api/v1/shares/{code}"));
    if let Some(password) = password {
        request = request.header("x-share-password", password);
    }
    body_json(expect_status(app.send(request, Body::empty()).await, StatusCode::OK).await).await
}

/// The code from a plain-text answer, checking it is the whole link on a line.
fn code_of(link: &str) -> String {
    let code = link
        .strip_prefix("http://share.example/s/")
        .and_then(|rest| rest.strip_suffix('\n'))
        .expect("the link, alone on its line");
    code.to_owned()
}

#[tokio::test]
async fn a_file_comes_back_as_a_link_on_one_line() {
    let app = TestApp::ready().await;
    let response = expect_status(
        put(&app, "/report.pdf", b"quarterly numbers", &[]).await,
        StatusCode::CREATED,
    )
    .await;
    assert!(
        response.headers()[header::CONTENT_TYPE]
            .to_str()
            .expect("ascii")
            .starts_with("text/plain")
    );
    let header_code = response.headers()["x-share-code"]
        .to_str()
        .expect("ascii")
        .to_owned();
    assert!(response.headers().contains_key("x-share-management-token"));
    let code = code_of(&body_string(response).await);
    assert_eq!(code, header_code);

    let share = describe(&app, &code, None).await;
    assert_eq!(share["kind"], "file");
    assert_eq!(share["files"][0]["filename"], "report.pdf");
}

#[tokio::test]
async fn standard_input_makes_a_note() {
    let app = TestApp::ready().await;
    let response = expect_status(put(&app, "/-", b"hello", &[]).await, StatusCode::CREATED).await;
    let code = code_of(&body_string(response).await);
    assert_eq!(describe(&app, &code, None).await["kind"], "text");
}

#[tokio::test]
async fn options_ride_in_headers() {
    let app = TestApp::ready().await;
    let response = expect_status(
        put(
            &app,
            "/notes.txt",
            b"secret plans",
            &[
                ("x-share-expires", "1d"),
                ("x-share-max-downloads", "2"),
                ("x-share-password", "correct horse"),
            ],
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let code = code_of(&body_string(response).await);
    let share = describe(&app, &code, Some("correct horse")).await;
    assert_eq!(share["has_password"], true);
    assert_eq!(share["uses_remaining"], 2);
    let expires = time::OffsetDateTime::parse(
        share["expires_at"].as_str().expect("an expiry"),
        &time::format_description::well_known::Rfc3339,
    )
    .expect("RFC 3339");
    let left = expires - time::OffsetDateTime::now_utc();
    assert!(left > time::Duration::hours(23) && left <= time::Duration::hours(24));
}

#[tokio::test]
async fn json_on_request() {
    let app = TestApp::ready().await;
    let share = body_json(
        expect_status(
            put(&app, "/a.txt", b"data", &[("accept", "application/json")]).await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    assert!(share["code"].is_string());
    assert!(share["path"].as_str().expect("a path").starts_with("/s/"));
}

#[tokio::test]
async fn a_mistake_reads_as_one_line_of_text() {
    let app = TestApp::ready().await;
    let response = expect_status(
        put(&app, "/a.txt", b"data", &[("x-share-expires", "soon")]).await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    let text = body_string(response).await;
    assert!(
        text.starts_with("error: ") && text.ends_with('\n'),
        "{text}"
    );
    assert!(text.contains("X-Share-Expires"), "{text}");
}

#[tokio::test]
async fn an_api_token_uploads_as_its_account() {
    let app = TestApp::ready().await;
    let registered = expect_status(
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
    .await;
    let cookie = session_cookie(&registered);
    let token = body_json(
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/tokens",
                Auth::Cookie(&cookie),
                &serde_json::json!({ "name": "curl" }),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await["secret"]
        .as_str()
        .expect("a secret")
        .to_owned();

    let bearer = format!("Bearer {token}");
    let response = expect_status(
        put(
            &app,
            "/mine.txt",
            b"data",
            &[("authorization", bearer.as_str())],
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    // A signed-in upload needs no token to manage it later.
    assert!(!response.headers().contains_key("x-share-management-token"));
    let code = code_of(&body_string(response).await);
    let mine = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/shares", Auth::Cookie(&cookie))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(mine["data"][0]["code"], code.as_str());
}

#[tokio::test]
async fn other_methods_on_those_paths_still_reach_the_site() {
    let app = TestApp::ready().await;
    for path in ["/", "/login"] {
        let response = app.get(path).await;
        assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED, "{path}");
    }
}
