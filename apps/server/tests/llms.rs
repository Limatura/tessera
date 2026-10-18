//! `/llms.txt`, over HTTP.

mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use common::{TestApp, body_string, expect_status};

async fn read(app: &TestApp) -> (String, String) {
    let response = expect_status(
        app.send(
            Request::builder()
                .uri("/llms.txt")
                .header(header::HOST, "share.example"),
            Body::empty(),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let kind = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    (kind, body_string(response).await)
}

#[tokio::test]
async fn it_describes_this_instance_by_its_own_address() {
    let app = TestApp::ready().await;
    let (kind, text) = read(&app).await;
    assert!(kind.starts_with("text/markdown"), "{kind}");
    assert!(text.starts_with("# "), "{text}");
    assert!(
        text.contains("curl -T report.pdf http://share.example/"),
        "{text}"
    );
    assert!(
        text.contains("http://share.example/s/CODE/download"),
        "{text}"
    );
    assert!(text.contains("Uploading needs no account."), "{text}");
    assert!(text.contains("Largest file: "), "{text}");
}

#[tokio::test]
async fn it_says_when_an_account_is_needed() {
    let app = TestApp::with_config(|config| config.features.anonymous_upload = false).await;
    let (_, text) = read(&app).await;
    assert!(text.contains("Uploading needs an account"), "{text}");
    assert!(!text.contains("needs no account"), "{text}");
}
