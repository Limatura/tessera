//! Handing uploads to a malware scanner, against scanners that behave like the
//! real ones.
//!
//! The clamd here speaks `INSTREAM` over a real socket and flags the EICAR test
//! string as ClamAV does. The HTTP one implements the contract in
//! `scan::http`. Neither mocks this crate: both are the far end of a wire, so
//! the tests cover what goes over it.

#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test fixture; a panic is how it reports"
)]

mod common;

use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    body::{Body, to_bytes},
    http::{HeaderMap, Method, Request, StatusCode},
};
use common::{Auth, TestApp, body_json, expect_status};
use http_body_util::BodyExt as _;
use tessera_server::{
    db::repository::{
        scan::{Scan, Skip},
        share::Content,
    },
    scan::worker::run_once,
};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpListener,
};

/// The industry's harmless test file. Every scanner flags it.
const EICAR: &[u8] = br"X5O!P%@AP[4\PZX54(P^)7CC)7}$EICAR-STANDARD-ANTIVIRUS-TEST-FILE!$H+H*";

/// A clamd that refuses anything longer than `limit` bytes, as
/// `StreamMaxLength` makes the real one do.
async fn clamd(limit: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().expect("an address");
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut command = [0; 10];
                if socket.read_exact(&mut command).await.is_err() || &command != b"zINSTREAM\0" {
                    return;
                }
                let mut received = Vec::new();
                loop {
                    let mut length = [0; 4];
                    if socket.read_exact(&mut length).await.is_err() {
                        return;
                    }
                    let length = usize::try_from(u32::from_be_bytes(length)).unwrap_or(0);
                    if length == 0 {
                        break;
                    }
                    let mut chunk = vec![0; length];
                    if socket.read_exact(&mut chunk).await.is_err() {
                        return;
                    }
                    received.extend(chunk);
                    if received.len() > limit {
                        // As clamd does: answers, and hangs up mid-stream.
                        socket
                            .write_all(b"INSTREAM size limit exceeded. ERROR\0")
                            .await
                            .ok();
                        return;
                    }
                }
                let answer: &[u8] = if contains(&received, EICAR) {
                    b"stream: Eicar-Test-Signature FOUND\0"
                } else {
                    b"stream: OK\0"
                };
                socket.write_all(answer).await.ok();
            });
        }
    });
    format!("clamd://{address}")
}

/// An address where nothing listens.
async fn nothing_there() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().expect("an address");
    drop(listener);
    format!("clamd://{address}")
}

/// An HTTP scanner that wants `Authorization: Bearer secret`, answers `413`
/// past `limit` bytes, and `status` when told to fail.
async fn http_scanner(limit: usize, status: Option<StatusCode>) -> String {
    let app = axum::Router::new().route(
        "/scan",
        axum::routing::post(
            move |headers: HeaderMap, body: axum::body::Bytes| async move {
                use axum::response::IntoResponse as _;
                if let Some(status) = status {
                    return status.into_response();
                }
                if headers.get("authorization").and_then(|v| v.to_str().ok())
                    != Some("Bearer secret")
                {
                    return StatusCode::UNAUTHORIZED.into_response();
                }
                if body.len() > limit {
                    return StatusCode::PAYLOAD_TOO_LARGE.into_response();
                }
                if contains(&body, EICAR) {
                    axum::Json(
                        serde_json::json!({ "clean": false, "threat": "Eicar-Test-Signature" }),
                    )
                    .into_response()
                } else {
                    axum::Json(serde_json::json!({ "clean": true })).into_response()
                }
            },
        ),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address: SocketAddr = listener.local_addr().expect("an address");
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{address}/scan")
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

async fn scanning_with(url: String) -> TestApp {
    TestApp::with_config(move |config| {
        config.scan.url = url;
        "Bearer secret".clone_into(&mut config.scan.authorization);
        config.features.encryption = true;
    })
    .await
}

/// Uploads `body` and returns the share's code and whether the response says
/// it is being scanned.
async fn upload(app: &TestApp, query: &str, body: &[u8]) -> (String, bool) {
    let share = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/shares{query}")),
                Body::from(body.to_vec()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    (
        share["code"].as_str().expect("a code").to_owned(),
        share["scanning"]
            .as_bool()
            .expect("says whether it is scanning"),
    )
}

async fn download(app: &TestApp, code: &str) -> axum::response::Response {
    app.get(&format!("/s/{code}/download")).await
}

/// The stored verdict for the first file or the note.
///
/// Asserting on this, not only on whether a download works, distinguishes two
/// outcomes that both serve the file: skipped as too large, and deferred
/// because the scanner was unreachable.
async fn verdict(app: &TestApp, code: &str) -> Option<Scan> {
    let code = tessera_core::RetrievalCode::parse(code).expect("a code");
    let share = app
        .state
        .shares
        .find_by_code(&code)
        .await
        .expect("query")
        .expect("the share");
    match app.state.shares.content_of(&share).await.expect("query") {
        Content::Files(files) => files.first().and_then(|file| file.scan.clone()),
        Content::Text(text) => text.scan,
        Content::Gone => None,
    }
}

fn switch_scanner(app: &TestApp, url: String) {
    let mut config = tessera_server::Config::clone(&app.state.config());
    config.scan.url = url;
    app.state.replace_config(Arc::new(config));
}

#[tokio::test]
async fn a_clean_file_is_held_until_the_scanner_says_so() {
    let app = scanning_with(clamd(usize::MAX).await).await;
    let (code, scanning) =
        upload(&app, "?filename=report.pdf&max_uses=1", b"an ordinary file").await;
    assert!(scanning, "the upload's own answer says it is being scanned");

    let held = expect_status(download(&app, &code).await, StatusCode::LOCKED).await;
    assert!(held.headers().contains_key("retry-after"));
    expect_status(
        app.empty(
            Method::POST,
            &format!("/api/v1/shares/{code}/unlock"),
            Auth::None,
        )
        .await,
        StatusCode::LOCKED,
    )
    .await;

    run_once(&app.state).await;

    // Still downloadable: being turned away did not use up the one-time share.
    let served = expect_status(download(&app, &code).await, StatusCode::OK).await;
    let bytes = to_bytes(served.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(&bytes[..], b"an ordinary file");
}

/// Disabled the way an operator disables a share, so it answers the same way:
/// as if it never existed.
#[tokio::test]
async fn an_infected_file_switches_its_share_off() {
    let app = scanning_with(clamd(usize::MAX).await).await;
    let (code, _) = upload(&app, "?filename=eicar.com", EICAR).await;
    run_once(&app.state).await;

    expect_status(download(&app, &code).await, StatusCode::NOT_FOUND).await;
    expect_status(
        app.empty(Method::GET, &format!("/api/v1/shares/{code}"), Auth::None)
            .await,
        StatusCode::NOT_FOUND,
    )
    .await;
}

/// The uploader, unlike everyone else, is told why: the management route
/// answers for a disabled share and carries the verdict.
#[tokio::test]
async fn its_uploader_is_told_a_share_was_switched_off_for_a_threat() {
    let app = scanning_with(clamd(usize::MAX).await).await;
    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/shares?filename=eicar.com"),
                Body::from(EICAR.to_vec()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let code = created["code"].as_str().expect("a code");
    let token = created["management_token"].as_str().expect("a token");
    assert_eq!(created["scan_state"], "pending");
    assert_eq!(created["disabled"], false);

    let manage = |token: &str| {
        app.send(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/shares/{code}/manage"))
                .header("x-share-management-token", token),
            Body::empty(),
        )
    };
    expect_status(manage("not-the-token").await, StatusCode::NOT_FOUND).await;

    run_once(&app.state).await;

    let share = body_json(expect_status(manage(token).await, StatusCode::OK).await).await;
    assert_eq!(share["scanning"], false);
    assert_eq!(share["scan_state"], "infected");
    assert_eq!(share["scan_detail"], "Eicar-Test-Signature");
    assert_eq!(share["disabled"], true);
}

/// A disabled share looks like any inactive one unless the listing says why.
#[tokio::test]
async fn the_admin_listing_says_a_share_was_switched_off_for_a_threat() {
    let app = scanning_with(clamd(usize::MAX).await).await;
    let registered = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": "first",
                "email": "first@example.com",
                "password": "correct horse battery staple",
            }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let admin = common::session_cookie(&registered);
    upload(&app, "?filename=eicar.com", EICAR).await;
    upload(&app, "?filename=fine.txt", b"an ordinary file").await;
    run_once(&app.state).await;

    let page = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/admin/shares", Auth::Cookie(&admin))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    let row = |name: &str| {
        page["data"]
            .as_array()
            .expect("rows")
            .iter()
            .find(|row| row["filename"] == name)
            .cloned()
            .expect("the share")
    };
    let infected = row("eicar.com");
    assert_eq!(infected["active"], false);
    assert_eq!(infected["status"], "disabled");
    assert_eq!(infected["scan_state"], "infected");
    assert_eq!(infected["scan_detail"], "Eicar-Test-Signature");
    let fine = row("fine.txt");
    assert_eq!(fine["status"], "available");
    assert_eq!(fine["scan_state"], "clean");
    assert!(fine["scan_detail"].is_null());
}

#[tokio::test]
async fn an_infected_note_is_switched_off_too() {
    let app = scanning_with(clamd(usize::MAX).await).await;
    let (code, scanning) = upload(&app, "", EICAR).await;
    assert!(scanning);
    expect_status(app.get(&format!("/s/{code}/raw")).await, StatusCode::LOCKED).await;
    run_once(&app.state).await;
    expect_status(
        app.get(&format!("/s/{code}/raw")).await,
        StatusCode::NOT_FOUND,
    )
    .await;
}

/// The agreed behaviour during an outage: serve without a verdict, and scan
/// once the scanner is back. An infected file is caught late, not never.
#[tokio::test]
async fn while_the_scanner_is_down_files_are_served_and_scanned_later() {
    let app = scanning_with(nothing_there().await).await;
    let (first, _) = upload(&app, "?filename=a.bin", EICAR).await;
    let (second, _) = upload(&app, "?filename=b.bin", b"fine").await;

    run_once(&app.state).await;
    expect_status(download(&app, &first).await, StatusCode::OK).await;
    expect_status(download(&app, &second).await, StatusCode::OK).await;
    assert_eq!(verdict(&app, &first).await, Some(Scan::Deferred));

    switch_scanner(&app, clamd(usize::MAX).await);
    run_once(&app.state).await;
    expect_status(download(&app, &first).await, StatusCode::NOT_FOUND).await;
    expect_status(download(&app, &second).await, StatusCode::OK).await;
    assert_eq!(verdict(&app, &second).await, Some(Scan::Clean));
}

#[tokio::test]
async fn a_file_too_large_to_scan_is_served_unscanned() {
    let app = scanning_with(clamd(100).await).await;
    let (code, _) = upload(&app, "?filename=big.bin", &[b'x'; 10_000]).await;
    run_once(&app.state).await;
    expect_status(download(&app, &code).await, StatusCode::OK).await;
    assert_eq!(
        verdict(&app, &code).await,
        Some(Scan::Skipped(Skip::TooLarge))
    );
}

/// Nothing for a scanner to read, so nothing to wait for.
#[tokio::test]
async fn an_encrypted_file_is_not_held() {
    let app = scanning_with(clamd(usize::MAX).await).await;
    let (code, scanning) = upload(&app, "?encryption=link&sealed_name=bmFtZQ", EICAR).await;
    assert!(!scanning);
    assert_eq!(
        verdict(&app, &code).await,
        Some(Scan::Skipped(Skip::Encrypted))
    );
    expect_status(
        app.get(&format!("/s/{code}/download?file=0")).await,
        StatusCode::OK,
    )
    .await;
}

#[tokio::test]
async fn a_resumable_upload_is_held_the_same_way() {
    let app = scanning_with(clamd(usize::MAX).await).await;
    let session = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/uploads?filename=eicar.txt")
                    .header("upload-length", EICAR.len().to_string()),
                Body::empty(),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let finished = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::PATCH)
                    .uri(format!(
                        "/api/v1/uploads/{}",
                        session["id"].as_str().expect("an id")
                    ))
                    .header("content-type", "application/offset+octet-stream")
                    .header("upload-offset", "0")
                    .header(
                        "x-upload-token",
                        session["token"].as_str().expect("a token"),
                    ),
                Body::from(EICAR.to_vec()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    assert_eq!(finished["scanning"], true);
    let code = finished["code"].as_str().expect("a code");
    expect_status(download(&app, code).await, StatusCode::LOCKED).await;
    run_once(&app.state).await;
    expect_status(download(&app, code).await, StatusCode::NOT_FOUND).await;
}

#[tokio::test]
async fn an_http_scanner_is_asked_the_same_questions() {
    let app = scanning_with(http_scanner(1000, None).await).await;
    let (clean, _) = upload(&app, "?filename=a.txt", b"fine").await;
    let (infected, _) = upload(&app, "?filename=b.txt", EICAR).await;
    let (large, _) = upload(&app, "?filename=c.bin", &[b'x'; 5000]).await;
    run_once(&app.state).await;
    expect_status(download(&app, &clean).await, StatusCode::OK).await;
    expect_status(download(&app, &infected).await, StatusCode::NOT_FOUND).await;
    expect_status(download(&app, &large).await, StatusCode::OK).await;
    assert_eq!(verdict(&app, &clean).await, Some(Scan::Clean));
    assert_eq!(
        verdict(&app, &large).await,
        Some(Scan::Skipped(Skip::TooLarge))
    );
    assert_eq!(verdict(&app, &clean).await, Some(Scan::Clean));
    assert!(matches!(
        verdict(&app, &infected).await,
        Some(Scan::Infected(_))
    ));
    assert_eq!(
        verdict(&app, &large).await,
        Some(Scan::Skipped(Skip::TooLarge))
    );
}

/// A scanner answering with an error has given no verdict, and a share must
/// not wait on it.
#[tokio::test]
async fn an_http_scanner_that_errors_counts_as_down() {
    let app =
        scanning_with(http_scanner(1000, Some(StatusCode::INTERNAL_SERVER_ERROR)).await).await;
    let (code, _) = upload(&app, "?filename=a.txt", EICAR).await;
    run_once(&app.state).await;
    expect_status(download(&app, &code).await, StatusCode::OK).await;
    assert_eq!(verdict(&app, &code).await, Some(Scan::Deferred));
}

/// Pending shares are not stuck if the operator turns scanning off before
/// the verdict: nothing is left to deliver one.
#[tokio::test]
async fn switching_scanning_off_releases_what_it_was_holding() {
    let app = scanning_with(clamd(usize::MAX).await).await;
    let (code, _) = upload(&app, "?filename=a.txt", b"fine").await;
    expect_status(download(&app, &code).await, StatusCode::LOCKED).await;
    switch_scanner(&app, String::new());
    expect_status(download(&app, &code).await, StatusCode::OK).await;
}

/// The same checks against a real ClamAV. Needs one listening at
/// `TESSERA_TEST_CLAMD`, such as `clamd://127.0.0.1:3310` from the
/// `clamav/clamav` image, so it is `#[ignore]`d like the database suites.
#[tokio::test]
#[ignore = "needs a running clamd; see the doc comment"]
async fn a_real_clamd_agrees() {
    let url = std::env::var("TESSERA_TEST_CLAMD").expect("TESSERA_TEST_CLAMD");
    let app = TestApp::with_config(move |config| {
        config.scan.url = url;
        // Room for the file that is too large for clamd.
        config.server.max_body_bytes = 200 * 1024 * 1024;
    })
    .await;
    let (clean, _) = upload(&app, "?filename=a.txt", b"nothing to see").await;
    let (infected, _) = upload(&app, "?filename=eicar.com", EICAR).await;
    // Past the image's default `StreamMaxLength` of 100 MiB.
    let (large, _) = upload(&app, "?filename=big.bin", &vec![0_u8; 101 * 1024 * 1024]).await;
    run_once(&app.state).await;
    expect_status(download(&app, &clean).await, StatusCode::OK).await;
    expect_status(download(&app, &infected).await, StatusCode::NOT_FOUND).await;
    expect_status(download(&app, &large).await, StatusCode::OK).await;
    assert_eq!(verdict(&app, &clean).await, Some(Scan::Clean));
    assert!(matches!(
        verdict(&app, &infected).await,
        Some(Scan::Infected(_))
    ));
    assert_eq!(
        verdict(&app, &large).await,
        Some(Scan::Skipped(Skip::TooLarge))
    );
}

/// More queued than the scanner is sent at once. Once it stops answering, the
/// unsent ones are released too, instead of each waiting a full interval for
/// its turn to fail.
#[tokio::test]
async fn once_the_scanner_is_down_nothing_else_waits_on_it() {
    let app = scanning_with(nothing_there().await).await;
    let mut codes = Vec::new();
    for n in 0..10 {
        codes.push(
            upload(&app, &format!("?filename=f{n}.txt"), b"fine")
                .await
                .0,
        );
    }
    run_once(&app.state).await;
    for code in &codes {
        expect_status(download(&app, code).await, StatusCode::OK).await;
    }
}

/// Uploads `body` and returns the share's code and management token.
async fn upload_anonymously(app: &TestApp, body: &[u8]) -> (String, String) {
    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/shares?filename=a.txt"),
                Body::from(body.to_vec()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    (
        created["code"].as_str().expect("a code").to_owned(),
        created["management_token"]
            .as_str()
            .expect("a token")
            .to_owned(),
    )
}

/// Opens `GET /shares/{code}/manage/events` with the management token.
async fn watch(app: &TestApp, code: &str, token: &str) -> axum::response::Response {
    app.send(
        Request::builder()
            .method(Method::GET)
            .uri(format!("/api/v1/shares/{code}/manage/events"))
            .header("x-share-management-token", token),
        Body::empty(),
    )
    .await
}

/// An event stream, read one event at a time.
struct Events {
    body: Body,
    buffer: String,
}

impl Events {
    fn of(response: axum::response::Response) -> Self {
        assert_eq!(
            response.headers()["content-type"],
            "text/event-stream",
            "an event stream"
        );
        Self {
            body: response.into_body(),
            buffer: String::new(),
        }
    }

    /// The next `share` event's body, or `None` once the stream has ended.
    async fn next(&mut self) -> Option<serde_json::Value> {
        loop {
            if let Some(end) = self.buffer.find("\n\n") {
                let event: String = self.buffer.drain(..end + 2).collect();
                assert!(
                    event.lines().any(|line| line == "event: share"),
                    "a share event: {event}"
                );
                let data = event
                    .lines()
                    .find_map(|line| line.strip_prefix("data: "))
                    .expect("a data line");
                return Some(serde_json::from_str(data).expect("JSON"));
            }
            let frame = tokio::time::timeout(Duration::from_secs(5), self.body.frame())
                .await
                .expect("the stream moved on in time")?
                .expect("a readable stream");
            if let Ok(data) = frame.into_data() {
                self.buffer
                    .push_str(std::str::from_utf8(&data).expect("UTF-8"));
            }
        }
    }
}

/// The uploader's page is told the verdict when it lands, and the stream ends
/// with it, instead of polling `/manage` until it changes.
#[tokio::test]
async fn the_verdict_is_pushed_to_its_uploader() {
    let app = scanning_with(clamd(usize::MAX).await).await;
    let (code, token) = upload_anonymously(&app, b"fine").await;

    let mut events =
        Events::of(expect_status(watch(&app, &code, &token).await, StatusCode::OK).await);
    let first = events.next().await.expect("the state right away");
    assert_eq!(first["scan_state"], "pending");
    assert_eq!(first["scanning"], true);

    run_once(&app.state).await;

    let last = events.next().await.expect("the verdict");
    assert_eq!(last["scan_state"], "clean");
    assert_eq!(last["scanning"], false);
    assert_eq!(events.next().await, None, "nothing left to wait for");
}

#[tokio::test]
async fn a_threat_is_pushed_with_the_share_switched_off() {
    let app = scanning_with(clamd(usize::MAX).await).await;
    let (code, token) = upload_anonymously(&app, EICAR).await;

    let mut events =
        Events::of(expect_status(watch(&app, &code, &token).await, StatusCode::OK).await);
    events.next().await.expect("the state right away");
    run_once(&app.state).await;

    let last = events.next().await.expect("the verdict");
    assert_eq!(last["scan_state"], "infected");
    assert_eq!(last["disabled"], true);
    assert_eq!(events.next().await, None);
}

/// Deferring everything at once announces no particular share; each stream
/// still reads its own again.
#[tokio::test]
async fn a_scanner_going_down_is_pushed_too() {
    let app = scanning_with(nothing_there().await).await;
    let (code, token) = upload_anonymously(&app, b"fine").await;

    let mut events =
        Events::of(expect_status(watch(&app, &code, &token).await, StatusCode::OK).await);
    events.next().await.expect("the state right away");
    run_once(&app.state).await;

    let last = events.next().await.expect("the new state");
    assert_eq!(last["scan_state"], "deferred");
    assert_eq!(events.next().await, None);
}

/// Nothing to wait for: one event, then the end.
#[tokio::test]
async fn a_share_that_was_never_held_ends_at_once() {
    let app = TestApp::ready().await;
    let (code, token) = upload_anonymously(&app, b"fine").await;

    let mut events =
        Events::of(expect_status(watch(&app, &code, &token).await, StatusCode::OK).await);
    let only = events.next().await.expect("the state");
    assert_eq!(only["scanning"], false);
    assert_eq!(events.next().await, None);
}

#[tokio::test]
async fn only_its_manager_may_watch_a_share() {
    let app = scanning_with(clamd(usize::MAX).await).await;
    let (code, _) = upload_anonymously(&app, b"fine").await;
    expect_status(
        watch(&app, &code, "not-the-token").await,
        StatusCode::NOT_FOUND,
    )
    .await;
}

/// Graceful shutdown waits for open responses, so a stream must not hold it.
#[tokio::test]
async fn draining_ends_the_stream() {
    let app = scanning_with(clamd(usize::MAX).await).await;
    let (code, token) = upload_anonymously(&app, b"fine").await;

    let mut events =
        Events::of(expect_status(watch(&app, &code, &token).await, StatusCode::OK).await);
    events.next().await.expect("the state right away");
    app.state
        .set_status(tessera_server::state::ServiceStatus::Draining);
    assert_eq!(events.next().await, None);
}

/// Registers the first account, an administrator, and returns its cookie.
async fn sign_in_admin(app: &TestApp) -> String {
    let registered = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": "first",
                "email": "first@example.com",
                "password": "correct horse battery staple",
            }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    common::session_cookie(&registered)
}

async fn scanner_status(app: &TestApp, admin: &str) -> serde_json::Value {
    body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/admin/scanner", Auth::Cookie(admin))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await
}

/// Files are served unscanned while the scanner is down, which nothing else
/// shows, so the operator has to be able to see it here, and see it end.
#[tokio::test]
async fn the_operator_sees_the_scanner_go_down_and_come_back() {
    let app = scanning_with(nothing_there().await).await;
    let admin = sign_in_admin(&app).await;
    upload(&app, "?filename=a.bin", b"fine").await;
    run_once(&app.state).await;

    let down = scanner_status(&app, &admin).await;
    assert_eq!(down["enabled"], true, "{down}");
    assert_eq!(down["reachable"], false, "{down}");
    assert!(down["last_error"].is_string(), "{down}");
    assert_eq!(down["deferred"], 1, "{down}");
    assert_eq!(down["pending"], 0, "{down}");

    switch_scanner(&app, clamd(usize::MAX).await);
    run_once(&app.state).await;

    let back = scanner_status(&app, &admin).await;
    assert_eq!(back["reachable"], true, "{back}");
    assert_eq!(back["last_error"], serde_json::Value::Null, "{back}");
    assert_eq!(back["deferred"], 0, "{back}");
}

/// With nothing uploaded, nothing would ever call the scanner, so its state
/// would stay unknown. It is asked directly instead.
#[tokio::test]
async fn an_idle_scanner_is_asked_directly() {
    let app = scanning_with(clamd(usize::MAX).await).await;
    let admin = sign_in_admin(&app).await;
    run_once(&app.state).await;

    let status = scanner_status(&app, &admin).await;
    assert_eq!(status["reachable"], true, "{status}");
    assert!(status["last_checked_at"].is_string(), "{status}");
}

#[tokio::test]
async fn an_idle_http_scanner_that_is_down_shows_as_down() {
    let app = scanning_with(http_scanner(usize::MAX, Some(StatusCode::BAD_GATEWAY)).await).await;
    let admin = sign_in_admin(&app).await;
    run_once(&app.state).await;

    let status = scanner_status(&app, &admin).await;
    assert_eq!(status["reachable"], false, "{status}");
    assert!(status["last_error"].is_string(), "{status}");
}

#[tokio::test]
async fn with_no_scanner_there_is_nothing_to_report() {
    let app = TestApp::ready().await;
    let admin = sign_in_admin(&app).await;
    run_once(&app.state).await;

    let status = scanner_status(&app, &admin).await;
    assert_eq!(status["enabled"], false, "{status}");
    assert_eq!(
        status["last_checked_at"],
        serde_json::Value::Null,
        "{status}"
    );
}
