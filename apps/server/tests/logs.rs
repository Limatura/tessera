//! `/admin/logs`: who may read the log, and the four ways it is read.
//!
//! The helpers below sit outside a `#[test]` body, where `clippy.toml`'s test
//! exemptions do not reach. A fixture that cannot write its files has nothing
//! to report but a failure.
#![expect(clippy::expect_used, reason = "test fixtures; see module docs")]

mod common;

use std::time::Duration;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use common::{Auth, TestApp, body_json, body_string, expect_status};
use http_body_util::BodyExt as _;
use tessera_server::log_files::FileLayer;
use tracing_subscriber::layer::SubscriberExt as _;

const PASSWORD: &str = "a perfectly adequate passphrase";

/// The first account is the owner, any later one a plain account.
async fn register(app: &TestApp, username: &str) -> String {
    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
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

async fn get(app: &TestApp, uri: &str, auth: Auth<'_>) -> axum::response::Response {
    app.empty(Method::GET, uri, auth).await
}

/// Two days of lines, written as the writer would.
fn write_days(app: &TestApp) {
    let dir = app.state.logs.dir();
    std::fs::create_dir_all(dir).expect("log directory");
    for (day, lines) in [
        (
            "2026-10-18",
            &[
                ("2026-10-18T22:00:00Z", "info", "a"),
                ("2026-10-18T23:00:00Z", "warn", "b"),
            ][..],
        ),
        (
            "2026-10-19",
            &[
                ("2026-10-19T01:00:00Z", "info", "c"),
                ("2026-10-19T02:00:00Z", "error", "d"),
                ("2026-10-19T03:00:00Z", "info", "e"),
            ][..],
        ),
    ] {
        let mut contents = String::new();
        for (time, level, message) in lines {
            let line = serde_json::json!({
                "id": format!("{day}:{}", contents.len()),
                "time": time,
                "level": level,
                "target": "tessera_server",
                "message": message,
                "fields": { "backend": "clamd" },
            });
            contents.push_str(&line.to_string());
            contents.push('\n');
        }
        std::fs::write(dir.join(format!("{day}.jsonl")), contents).expect("write day");
    }
}

fn messages(page: &serde_json::Value) -> Vec<String> {
    page["data"]
        .as_array()
        .expect("data")
        .iter()
        .map(|entry| entry["message"].as_str().expect("message").to_owned())
        .collect()
}

#[tokio::test]
async fn only_an_account_holding_logs_read_may_read_the_log() {
    let app = TestApp::ready().await;
    let owner = register(&app, "first").await;
    let member = register(&app, "second").await;

    for uri in [
        "/api/v1/admin/logs",
        "/api/v1/admin/logs/files",
        "/api/v1/admin/logs/stream",
        "/api/v1/admin/logs/files/2026-10-19",
    ] {
        assert_eq!(
            get(&app, uri, Auth::None).await.status(),
            StatusCode::UNAUTHORIZED,
            "{uri}"
        );
        assert_eq!(
            get(&app, uri, Auth::Cookie(&member)).await.status(),
            StatusCode::FORBIDDEN,
            "{uri}"
        );
    }
    expect_status(
        get(&app, "/api/v1/admin/logs/files", Auth::Cookie(&owner)).await,
        StatusCode::OK,
    )
    .await;

    // The migration grants it to the admin role too, not only the owner's.
    let member_id = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("second").expect("a name"))
        .await
        .expect("query")
        .expect("the account")
        .id;
    expect_status(
        app.json(
            Method::PUT,
            &format!("/api/v1/admin/users/{member_id}/roles"),
            Auth::Cookie(&owner),
            &serde_json::json!({ "role": tessera_core::RoleId::ADMIN }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    expect_status(
        get(&app, "/api/v1/admin/logs", Auth::Cookie(&member)).await,
        StatusCode::OK,
    )
    .await;
}

#[tokio::test]
async fn the_files_are_listed_paged_through_and_filtered() {
    let app = TestApp::ready().await;
    let owner = register(&app, "first").await;
    write_days(&app);

    let files = body_json(
        expect_status(
            get(&app, "/api/v1/admin/logs/files", Auth::Cookie(&owner)).await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    let dates: Vec<&str> = files
        .as_array()
        .expect("a list")
        .iter()
        .map(|file| file["date"].as_str().expect("date"))
        .collect();
    assert_eq!(dates, ["2026-10-19", "2026-10-18"]);

    let mut seen = Vec::new();
    let mut uri = "/api/v1/admin/logs?limit=2".to_owned();
    loop {
        let page = body_json(
            expect_status(get(&app, &uri, Auth::Cookie(&owner)).await, StatusCode::OK).await,
        )
        .await;
        seen.extend(messages(&page));
        match page["next_cursor"].as_str() {
            Some(cursor) => uri = format!("/api/v1/admin/logs?limit=2&cursor={cursor}"),
            None => break,
        }
    }
    assert_eq!(seen, ["e", "d", "c", "b", "a"]);

    let warnings =
        body_json(get(&app, "/api/v1/admin/logs?level=warn", Auth::Cookie(&owner)).await).await;
    assert_eq!(messages(&warnings), ["d", "b"]);

    let window = body_json(
        get(
            &app,
            "/api/v1/admin/logs?since=2026-10-18T23:00:00Z&until=2026-10-19T02:00:00Z&q=CLAMD",
            Auth::Cookie(&owner),
        )
        .await,
    )
    .await;
    assert_eq!(messages(&window), ["d", "c", "b"]);
}

#[tokio::test]
async fn parameters_that_do_not_parse_are_refused() {
    let app = TestApp::ready().await;
    let owner = register(&app, "first").await;

    for query in ["cursor=nonsense", "since=yesterday", "level=loud", "page=2"] {
        assert_eq!(
            get(
                &app,
                &format!("/api/v1/admin/logs?{query}"),
                Auth::Cookie(&owner)
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST,
            "{query}"
        );
    }
}

#[tokio::test]
async fn a_day_is_downloaded_by_its_date_and_nothing_else() {
    let app = TestApp::ready().await;
    let owner = register(&app, "first").await;
    write_days(&app);

    let response = expect_status(
        get(
            &app,
            "/api/v1/admin/logs/files/2026-10-18",
            Auth::Cookie(&owner),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/x-ndjson"
    );
    assert_eq!(
        response.headers()[header::CONTENT_DISPOSITION],
        "attachment; filename=\"tessera-2026-10-18.jsonl\""
    );
    let expected =
        std::fs::read_to_string(app.state.logs.dir().join("2026-10-18.jsonl")).expect("the file");
    assert_eq!(body_string(response).await, expected);

    for date in [
        "2026-10-17",
        "2026-10-18.jsonl",
        "..%2Fsecret.key",
        "latest",
    ] {
        assert_eq!(
            get(
                &app,
                &format!("/api/v1/admin/logs/files/{date}"),
                Auth::Cookie(&owner)
            )
            .await
            .status(),
            StatusCode::NOT_FOUND,
            "{date}"
        );
    }
}

/// Lines logged while the stream is open arrive on it, filtered.
#[tokio::test]
async fn the_stream_carries_lines_as_they_are_written() {
    let app = TestApp::ready().await;
    let owner = register(&app, "first").await;
    let layer = FileLayer::new();
    layer
        .attach(&app.state.logs, app.state.config_handle())
        .expect("attach");

    let response = expect_status(
        app.send(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/admin/logs/stream?level=warn")
                .header(header::COOKIE, &owner),
            Body::empty(),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    let mut body = response.into_body();

    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("not asked for");
        tracing::warn!(backend = "clamd", "scanner unreachable");
    });

    let mut buffer = String::new();
    let event = loop {
        if let Some(end) = buffer.find("\n\n") {
            let event: String = buffer.drain(..end + 2).collect();
            if event.lines().any(|line| line == "event: entry") {
                break event;
            }
            continue;
        }
        let frame = tokio::time::timeout(Duration::from_secs(5), body.frame())
            .await
            .expect("a line in time")
            .expect("the stream is open")
            .expect("a readable stream");
        if let Ok(data) = frame.into_data() {
            buffer.push_str(std::str::from_utf8(&data).expect("UTF-8"));
        }
    };
    let data = event
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .expect("a data line");
    let entry: serde_json::Value = serde_json::from_str(data).expect("JSON");
    assert_eq!(entry["message"], "scanner unreachable");
    assert_eq!(entry["level"], "warn");
    assert_eq!(entry["fields"]["backend"], "clamd");
    let id = event
        .lines()
        .find_map(|line| line.strip_prefix("id: "))
        .expect("an id line");
    assert_eq!(entry["id"], id);
}

/// Three days kept means today and the two before; the sweep reads the
/// setting each pass.
#[tokio::test]
async fn the_sweep_keeps_only_the_days_asked_for() {
    let app = TestApp::with_config(|config| config.logs.retention_days = 3).await;
    let dir = app.state.logs.dir();
    std::fs::create_dir_all(dir).expect("log directory");
    for day in ["2026-10-16", "2026-10-17", "2026-10-18", "2026-10-19"] {
        std::fs::write(dir.join(format!("{day}.jsonl")), "").expect("write day");
    }
    std::fs::write(dir.join("notes.txt"), "kept").expect("write stray");

    tessera_server::maintenance::sweep_once(
        &app.state,
        time::macros::datetime!(2026-10-19 12:00 UTC),
    )
    .await;

    let mut left: Vec<String> = std::fs::read_dir(dir)
        .expect("directory")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .into_string()
                .expect("UTF-8")
        })
        .collect();
    left.sort();
    assert_eq!(
        left,
        [
            "2026-10-17.jsonl",
            "2026-10-18.jsonl",
            "2026-10-19.jsonl",
            "notes.txt"
        ]
    );
}
