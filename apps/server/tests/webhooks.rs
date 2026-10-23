//! Webhooks: an account's and the instance's, what they are told, and how it
//! reaches them.

mod common;

use std::sync::Arc;

use axum::{
    body::Body,
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use common::{Auth, TestApp, body_json, expect_status};
use serde_json::{Value, json};
use time::OffsetDateTime;

const PASSWORD: &str = "a perfectly adequate passphrase";

/// Registers an account and returns its cookie. The first one administers
/// the instance.
async fn register(app: &TestApp, username: &str) -> String {
    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &json!({
                "username": username,
                "email": format!("{username}@example.com"),
                "password": PASSWORD,
            }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    common::session_cookie(&response)
}

/// Adds a webhook under `base`, `/api/v1/webhooks` or `/api/v1/admin/webhooks`.
async fn add(app: &TestApp, base: &str, cookie: &str, url: &str, events: &[&str]) -> Value {
    body_json(
        expect_status(
            app.json(
                Method::POST,
                base,
                Auth::Cookie(cookie),
                &json!({ "url": url, "events": events }),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await
}

fn text(value: &Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

async fn upload(app: &TestApp, cookie: Option<&str>, query: &str) -> Value {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/shares?filename=notes.txt{query}"));
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    body_json(
        expect_status(
            app.send(request, Body::from("hello")).await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await
}

async fn download(app: &TestApp, code: &str) {
    expect_status(
        app.send(
            Request::builder()
                .uri(format!("/s/{code}/download"))
                .header(header::USER_AGENT, "Tessera-Test/1.0"),
            Body::empty(),
        )
        .await,
        StatusCode::OK,
    )
    .await;
}

async fn deliveries(app: &TestApp, base: &str, cookie: &str, hook: &Value) -> Vec<Value> {
    let id = text(hook, "/id");
    body_json(
        expect_status(
            app.empty(
                Method::GET,
                &format!("{base}/{id}/deliveries"),
                Auth::Cookie(cookie),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await
    .as_array()
    .cloned()
    .unwrap_or_default()
}

/// What a receiver was sent.
type Received = Arc<parking_lot::Mutex<Vec<(HeaderMap, String)>>>;

/// A receiver on this machine answering every request with `status`.
#[expect(
    clippy::expect_used,
    reason = "test fixture; a panic is how it reports"
)]
async fn receiver(status: StatusCode) -> (String, Received) {
    let received: Received = Arc::default();
    let kept = Arc::clone(&received);
    let router = axum::Router::new().fallback(move |headers: HeaderMap, body: String| {
        let kept = Arc::clone(&kept);
        async move {
            kept.lock().push((headers, body));
            status
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a port to listen on");
    let address = listener.local_addr().expect("the address it listens on");
    tokio::spawn(async move { axum::serve(listener, router).await });
    (format!("http://{address}/hook"), received)
}

fn bodies(received: &Received) -> Vec<(HeaderMap, Value)> {
    received
        .lock()
        .iter()
        .map(|(headers, body)| {
            (
                headers.clone(),
                serde_json::from_str(body).unwrap_or_default(),
            )
        })
        .collect()
}

fn header_text(headers: &HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}

#[tokio::test]
async fn an_account_hears_about_its_own_shares_and_the_instance_about_all() {
    let app = TestApp::ready().await;
    let admin = register(&app, "first").await;
    let alice = register(&app, "alice").await;
    let bob = register(&app, "bob").await;

    let mine = add(
        &app,
        "/api/v1/webhooks",
        &alice,
        "https://example.com/hook",
        &["share.created"],
    )
    .await;
    let everyone = add(
        &app,
        "/api/v1/admin/webhooks",
        &admin,
        "http://127.0.0.1:9/hook",
        &["share.created"],
    )
    .await;
    assert!(text(&mine, "/secret").starts_with("whsec_"));

    upload(&app, Some(&alice), "").await;
    upload(&app, Some(&bob), "").await;
    upload(&app, None, "").await;

    let told = deliveries(&app, "/api/v1/webhooks", &alice, &mine).await;
    assert_eq!(told.len(), 1, "only alice's own share");
    assert_eq!(text(&told[0], "/event"), "share.created");
    assert_eq!(text(&told[0], "/state"), "pending");
    let told = deliveries(&app, "/api/v1/admin/webhooks", &admin, &everyone).await;
    assert_eq!(told.len(), 3, "every share, anonymous ones too");

    // Neither sees the other's.
    let listed = body_json(
        app.empty(Method::GET, "/api/v1/webhooks", Auth::Cookie(&bob))
            .await,
    )
    .await;
    assert_eq!(listed.as_array().map(Vec::len), Some(0));
    let id = text(&mine, "/id");
    expect_status(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/webhooks/{id}"),
            Auth::Cookie(&bob),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    expect_status(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/webhooks/{id}"),
            Auth::Cookie(&alice),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
}

#[tokio::test]
async fn an_account_may_not_point_one_at_this_network() {
    let app = TestApp::ready().await;
    let admin = register(&app, "first").await;
    let alice = register(&app, "alice").await;

    for url in [
        "http://127.0.0.1:8080/hook",
        "http://localhost/hook",
        "http://169.254.169.254/latest",
        "http://[::1]/hook",
    ] {
        let response = app
            .json(
                Method::POST,
                "/api/v1/webhooks",
                Auth::Cookie(&alice),
                &json!({ "url": url, "events": ["share.created"] }),
            )
            .await;
        let body = body_json(expect_status(response, StatusCode::BAD_REQUEST).await).await;
        assert_eq!(text(&body, "/details/rule"), "not_public", "{url}");
    }
    // An administrator's may: the instance's own network is theirs.
    add(
        &app,
        "/api/v1/admin/webhooks",
        &admin,
        "http://127.0.0.1:8080/hook",
        &["share.created"],
    )
    .await;
    // And an ordinary account may not add one for the instance.
    expect_status(
        app.json(
            Method::POST,
            "/api/v1/admin/webhooks",
            Auth::Cookie(&alice),
            &json!({ "url": "https://example.com/hook", "events": ["share.created"] }),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
}

#[tokio::test]
async fn events_arrive_signed_and_a_download_says_who() {
    let app = TestApp::with_config(|config| {
        config.share.access_log.enabled = true;
        config.server.public_url = "https://files.example.com/".to_owned();
    })
    .await;
    let admin = register(&app, "first").await;
    let (url, received) = receiver(StatusCode::NO_CONTENT).await;
    let hook = add(
        &app,
        "/api/v1/admin/webhooks",
        &admin,
        &url,
        &["share.created", "share.downloaded", "share.expired"],
    )
    .await;
    let secret = text(&hook, "/secret");

    let share = upload(&app, None, "&max_uses=1").await;
    let code = text(&share, "/code");
    download(&app, &code).await;
    tessera_server::webhook::worker::run_once(&app.state).await;
    // The one download used it up, which the next pass notices.
    tessera_server::webhook::worker::run_once(&app.state).await;

    let got = bodies(&received);
    let types: Vec<String> = got.iter().map(|(_, body)| text(body, "/type")).collect();
    assert_eq!(
        types,
        ["share.created", "share.downloaded", "share.expired"]
    );
    let (headers, body) = &got[1];
    assert_eq!(text(body, "/data/code"), code);
    assert_eq!(
        text(body, "/data/url"),
        format!("https://files.example.com/s/{code}")
    );
    assert_eq!(text(body, "/data/files/0/name"), "notes.txt");
    assert_eq!(body.pointer("/data/use_count"), Some(&json!(1)));
    assert_eq!(text(body, "/data/download/user_agent"), "Tessera-Test/1.0");

    let raw = received.lock()[1].1.clone();
    let id = header_text(headers, "webhook-id");
    let timestamp: i64 = header_text(headers, "webhook-timestamp")
        .parse()
        .unwrap_or_default();
    assert_eq!(
        header_text(headers, "webhook-signature"),
        tessera_server::webhook::worker::sign(&secret, &id, timestamp, &raw)
    );

    let told = deliveries(&app, "/api/v1/admin/webhooks", &admin, &hook).await;
    assert!(
        told.iter()
            .all(|delivery| text(delivery, "/state") == "delivered")
    );

    // Announced once, not on every pass.
    tessera_server::webhook::worker::run_once(&app.state).await;
    assert_eq!(bodies(&received).len(), 3);
}

#[tokio::test]
async fn an_account_is_not_told_who_downloaded() {
    let app = TestApp::with_config(|config| {
        config.share.access_log.enabled = true;
    })
    .await;
    register(&app, "first").await;
    let alice = register(&app, "alice").await;
    add(
        &app,
        "/api/v1/webhooks",
        &alice,
        "https://example.com/hook",
        &["share.downloaded"],
    )
    .await;
    let share = upload(&app, Some(&alice), "").await;
    download(&app, &text(&share, "/code")).await;

    let queued = app
        .state
        .webhooks
        .due(OffsetDateTime::now_utc() + time::Duration::hours(1), 10)
        .await
        .unwrap_or_default();
    assert_eq!(queued.len(), 1);
    let body: Value = serde_json::from_str(&queued[0].body).unwrap_or_default();
    assert_eq!(text(&body, "/type"), "share.downloaded");
    assert_eq!(body.pointer("/data/download"), None);
}

#[tokio::test]
async fn a_failed_event_is_tried_again_later() {
    let app = TestApp::ready().await;
    let admin = register(&app, "first").await;
    let (url, received) = receiver(StatusCode::INTERNAL_SERVER_ERROR).await;
    let hook = add(
        &app,
        "/api/v1/admin/webhooks",
        &admin,
        &url,
        &["share.created"],
    )
    .await;
    upload(&app, None, "").await;
    tessera_server::webhook::worker::run_once(&app.state).await;
    // Not due again yet.
    tessera_server::webhook::worker::run_once(&app.state).await;

    assert_eq!(bodies(&received).len(), 1);
    let told = deliveries(&app, "/api/v1/admin/webhooks", &admin, &hook).await;
    assert_eq!(text(&told[0], "/state"), "pending");
    assert_eq!(told[0].pointer("/attempts"), Some(&json!(1)));
    assert_eq!(told[0].pointer("/last_status"), Some(&json!(500)));
    assert!(
        told[0]
            .pointer("/next_attempt_at")
            .is_some_and(Value::is_string)
    );
}

#[tokio::test]
async fn a_discord_webhook_gets_a_message() {
    let app = TestApp::ready().await;
    let admin = register(&app, "first").await;
    let (url, received) = receiver(StatusCode::NO_CONTENT).await;
    expect_status(
        app.json(
            Method::POST,
            "/api/v1/admin/webhooks",
            Auth::Cookie(&admin),
            &json!({ "url": url, "events": ["share.created"], "format": "discord" }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let share = upload(&app, None, "").await;
    tessera_server::webhook::worker::run_once(&app.state).await;

    let got = bodies(&received);
    let (_, body) = &got[0];
    assert_eq!(text(body, "/embeds/0/title"), "Share created");
    assert!(text(body, "/embeds/0/description").contains(&text(&share, "/code")));
}
