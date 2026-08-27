//! Shares, at the storage layer and over HTTP.

mod common;

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use common::{Auth, TestApp, body_json, body_string, expect_status, share_repository_round_trip};
use futures_util::TryStreamExt as _;
use object_store::{ObjectStore as _, ObjectStoreExt as _};
use sea_orm::ConnectionTrait as _;
use tessera_server::db::repository::share::{Content, StorageLocation};
use time::OffsetDateTime;
use tower::ServiceExt as _;

#[tokio::test]
async fn shares_round_trip_through_the_repository() {
    share_repository_round_trip(&TestApp::ready().await, "SQ").await;
}

/// The first file's object key, for a test that wants to reach into the store.
fn first_key(content: &Content) -> Option<&str> {
    content
        .files()
        .first()
        .and_then(|file| file.storage.object_key())
}

#[expect(
    clippy::expect_used,
    reason = "test fixture; a panic is how it reports"
)]
/// Where a share's content is, as the tables that own it say.
///
/// The columns these used to read came off `shares` — the subtype tables own
/// them now, so a test asking "where did the bytes go" asks the same way the
/// program does.
async fn content_of(app: &TestApp, id: tessera_core::ShareId) -> Content {
    let share = app
        .state
        .shares
        .find_by_id(id)
        .await
        .expect("query")
        .expect("the share");
    app.state
        .shares
        .content_of(&share)
        .await
        .expect("read the content details")
}

/// Posts a body and returns the created share.
async fn upload(app: &TestApp, query: &str, body: &[u8]) -> serde_json::Value {
    let response = expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/shares{query}")),
            Body::from(body.to_vec()),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    body_json(response).await
}

#[tokio::test]
async fn text_is_kept_in_the_database() {
    let app = TestApp::ready().await;
    let content = b"const foo = \"bar\";\n";

    let share = upload(&app, "?language=typescript&title=snippet", content).await;
    assert_eq!(share["kind"], "text");
    assert_eq!(share["size_bytes"], content.len());
    assert_eq!(share["language"], "typescript");
    assert_eq!(
        share["path"],
        format!("/s/{}", share["code"].as_str().expect("a code"))
    );

    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");
    let stored = content_of(&app, id).await;
    let Content::Text(text) = &stored else {
        panic!("text belongs in its own table, got {stored:?}");
    };
    assert_eq!(
        text.size_bytes,
        u64::try_from(content.len()).expect("a size")
    );
    assert_eq!(
        app.state
            .shares
            .text_of(id)
            .await
            .expect("query")
            .expect("text lives in its own table")
            .content,
        content.to_vec()
    );
}

/// The point of the whole ingest path: past the threshold the bytes go to the
/// blob store rather than into a column, and they get there without the body
/// ever being held whole.
#[tokio::test]
async fn a_file_is_streamed_to_the_blob_store() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 1024;
    })
    .await;
    let content = vec![0x5A_u8; 64 * 1024];

    let share = upload(&app, "?filename=blob.bin", &content).await;
    assert_eq!(share["kind"], "file");
    assert_eq!(share["size_bytes"], content.len());

    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");
    let stored = content_of(&app, id).await;
    let key = first_key(&stored)
        .unwrap_or_else(|| panic!("expected object storage, got {stored:?}"))
        .to_owned();
    assert!(
        key.starts_with("shares/") && key.ends_with(&id.to_string()),
        "keys are sharded by date: {key}"
    );
    assert!(
        app.state.shares.text_of(id).await.expect("query").is_none(),
        "a file has no text row"
    );

    let fetched = app
        .state
        .storage
        .get(&object_store::path::Path::from(key.as_str()))
        .await
        .expect("the object should exist")
        .bytes()
        .await
        .expect("bytes");
    assert_eq!(fetched.as_ref(), content.as_slice());
}

/// Without a token, a mistaken anonymous upload could never be taken back.
#[tokio::test]
async fn an_anonymous_upload_can_be_taken_back_with_its_token() {
    let app = TestApp::ready().await;
    let share = upload(&app, "", b"oops").await;
    let token = share["management_token"]
        .as_str()
        .expect("an anonymous upload should carry a management token")
        .to_owned();
    let code = share["code"].as_str().expect("a code").to_owned();

    // Not without it.
    expect_status(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/shares/{code}"),
            Auth::None,
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;

    expect_status(
        app.send(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/shares/{code}"))
                .header("x-share-management-token", &token),
            Body::empty(),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;

    // And it is really gone.
    expect_status(
        app.send(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/shares/{code}"))
                .header("x-share-management-token", &token),
            Body::empty(),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
}

#[tokio::test]
async fn deleting_a_share_removes_its_object_too() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
    })
    .await;
    let share = upload(&app, "?filename=x.bin", &vec![7_u8; 4096]).await;
    let token = share["management_token"]
        .as_str()
        .expect("token")
        .to_owned();
    let code = share["code"].as_str().expect("code").to_owned();

    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("id")).expect("id");
    let stored = content_of(&app, id).await;
    let key = first_key(&stored)
        .unwrap_or_else(|| panic!("expected object storage, got {stored:?}"))
        .to_owned();

    expect_status(
        app.send(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/shares/{code}"))
                .header("x-share-management-token", &token),
            Body::empty(),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;

    assert!(
        app.state
            .storage
            .get(&object_store::path::Path::from(key.as_str()))
            .await
            .is_err(),
        "the object should have gone with the row"
    );
}

#[tokio::test]
async fn an_instance_that_refuses_anonymous_uploads_says_so() {
    let app = TestApp::with_config(|config| {
        config.features.anonymous_upload = false;
    })
    .await;
    let response = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares"),
            Body::from("nope"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_lifetime_beyond_the_ceiling_is_refused() {
    let app = TestApp::with_config(|config| {
        config.share.max_expiry_hours = 24;
    })
    .await;
    let response = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?expires_in_hours=48"),
            Body::from("too long"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(body_string(response).await.contains("at most 24"));
}

/// Omitting a lifetime uses the ceiling, so an instance that sets one never
/// accumulates shares that outlive it.
#[tokio::test]
async fn the_ceiling_doubles_as_the_default() {
    let app = TestApp::with_config(|config| {
        config.share.max_expiry_hours = 24;
    })
    .await;
    let share = upload(&app, "", b"a note").await;
    assert!(
        share["expires_at"].is_string(),
        "expected an expiry, got {share}"
    );
}

#[tokio::test]
async fn a_password_is_recorded_but_never_returned() {
    let app = TestApp::ready().await;
    let response = expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares")
                .header("x-share-password", "1234"),
            Body::from("secret"),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let raw = body_string(response).await;
    assert!(
        !raw.contains("1234"),
        "the password must not be echoed: {raw}"
    );

    let share: serde_json::Value = serde_json::from_str(&raw).expect("json");
    assert_eq!(share["has_password"], true);
}

#[tokio::test]
async fn a_listing_shows_only_the_callers_own_shares() {
    let app = TestApp::ready().await;

    // Anonymous upload: belongs to nobody.
    upload(&app, "?title=anonymous", b"not mine").await;

    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": "uploader",
                "email": "uploader@example.com",
                "password": "a perfectly adequate passphrase",
            }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let cookie = common::session_cookie(&response);

    let mine = expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?title=mine")
                .header(header::COOKIE, &cookie),
            Body::from("mine"),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let mine = body_json(mine).await;
    assert!(
        mine["management_token"].is_null(),
        "a signed-in uploader already has a way back in: their account"
    );

    let listed = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/shares", Auth::Cookie(&cookie))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    let titles: Vec<_> = listed["shares"]
        .as_array()
        .expect("an array")
        .iter()
        .map(|share| share["title"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(titles, vec!["mine"]);
}

/// The listing used to return everything an account had. An account driven by
/// a script is not bounded by what somebody would upload by hand, so this pins
/// that a page is a page and that `total` says how much is behind it.
#[tokio::test]
async fn the_listing_is_paged_and_says_how_much_there_is() {
    let app = TestApp::ready().await;

    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": "prolific",
                "email": "prolific@example.com",
                "password": "a perfectly adequate passphrase",
            }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let cookie = common::session_cookie(&response);

    for n in 0..5 {
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/shares?title=share-{n}"))
                    .header(header::COOKIE, &cookie),
                Body::from(format!("body {n}")),
            )
            .await,
            StatusCode::CREATED,
        )
        .await;
    }

    let page = async |query: &str| {
        body_json(
            expect_status(
                app.empty(
                    Method::GET,
                    &format!("/api/v1/shares{query}"),
                    Auth::Cookie(&cookie),
                )
                .await,
                StatusCode::OK,
            )
            .await,
        )
        .await
    };

    let first = page("?limit=2").await;
    assert_eq!(
        first["shares"].as_array().expect("an array").len(),
        2,
        "the limit is what decides the page size"
    );
    assert_eq!(
        first["total"], 5,
        "`total` counts what exists, not what came back"
    );

    // Newest first, so an offset of two skips the two most recent.
    let second = page("?limit=2&offset=2").await;
    let titles = |value: &serde_json::Value| -> Vec<String> {
        value["shares"]
            .as_array()
            .expect("an array")
            .iter()
            .map(|share| share["title"].as_str().unwrap_or_default().to_owned())
            .collect()
    };
    assert_eq!(titles(&first), vec!["share-4", "share-3"]);
    assert_eq!(titles(&second), vec!["share-2", "share-1"]);
}

/// `DefaultBodyLimit` is a value extractors consult, not a limit on the
/// connection — a handler that reads the body itself gets no protection from it.
/// This is the regression test for discovering that the hard way.
#[tokio::test]
async fn a_body_past_the_limit_is_refused() {
    let app = TestApp::with_config(|config| {
        config.server.max_body_bytes = 4096;
        config.share.inline_threshold_bytes = 512;
    })
    .await;

    let response = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=too-big.bin"),
            Body::from(vec![0_u8; 16 * 1024]),
        )
        .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

    // And nothing was left behind: a refused upload that had already started
    // writing must not leave parts nobody will ever collect.
    let listed = app
        .state
        .storage
        .list(None)
        .try_collect::<Vec<_>>()
        .await
        .expect("list");
    assert!(
        listed.is_empty(),
        "a refused upload left objects behind: {listed:?}"
    );
}

/// The contract promises that every non-2xx response is an `ErrorBody` a client
/// can branch on. axum's own extractors do not keep that promise — they reject
/// with plain text, and with statuses of their own choosing including `422` and
/// `415`, neither of which is in `ErrorCode`. A front-end switching on
/// `error.code` gets `undefined` the first time somebody sends a bad request.
#[tokio::test]
async fn every_rejection_keeps_the_error_envelope() {
    let app = TestApp::ready().await;

    let cases: Vec<(&str, Request<Body>)> = vec![
        (
            "malformed json",
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{not json"))
                .expect("request"),
        ),
        (
            "missing field",
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"identifier":"a"}"#))
                .expect("request"),
        ),
        (
            "wrong content type",
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .body(Body::from("{}"))
                .expect("request"),
        ),
        (
            "unknown query parameter",
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?nope=1")
                .body(Body::from("x"))
                .expect("request"),
        ),
        (
            "unparsable query value",
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?expires_in_hours=soon")
                .body(Body::from("x"))
                .expect("request"),
        ),
    ];

    for (label, request) in cases {
        let response = app.router.clone().oneshot(request).await.expect("response");
        let status = response.status();
        let body = body_string(response).await;
        let parsed: serde_json::Value = serde_json::from_str(&body)
            .unwrap_or_else(|_| panic!("{label}: expected an ErrorBody, got `{body}`"));

        assert_eq!(status, StatusCode::BAD_REQUEST, "{label}");
        assert_eq!(parsed["code"], "bad_request", "{label}");
        assert!(
            parsed["message"].as_str().is_some_and(|m| !m.is_empty()),
            "{label}: the message should say what was wrong: {parsed}"
        );
    }
}

/// These columns are narrower on MySQL than on the others, so an unchecked value
/// is `201` on SQLite and `500` on MySQL — the same request, two answers,
/// decided by which database the operator happened to choose.
#[tokio::test]
async fn oversized_labels_are_refused_rather_than_left_to_the_database() {
    let app = TestApp::ready().await;

    for (label, query) in [
        ("filename", format!("?filename={}", "a".repeat(300))),
        ("title", format!("?title={}", "b".repeat(300))),
        ("language", format!("?language={}", "c".repeat(100))),
    ] {
        let response = app
            .send(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/shares{query}")),
                Body::from("x"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{label}");
    }
}

/// A filename is echoed in `Content-Disposition` when the share is downloaded,
/// so a value carrying CR or LF would end the header and let the caller append
/// one of their own.
#[tokio::test]
async fn a_filename_that_could_forge_a_header_is_refused() {
    let app = TestApp::ready().await;
    let response = app
        .send(
            Request::builder().method(Method::POST).uri(format!(
                "/api/v1/shares?filename={}",
                urlencoding_of("evil\r\nX-Injected: 1")
            )),
            Body::from("x"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

fn urlencoding_of(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

#[tokio::test]
async fn a_share_of_nothing_is_refused() {
    let app = TestApp::ready().await;
    let response = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares"),
            Body::empty(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// `max_uses=0` would create a share that is exhausted before anyone sees it —
/// and `expires_in_hours=0` is already refused, so accepting this one was just
/// inconsistent.
#[tokio::test]
async fn a_zero_use_limit_is_refused() {
    let app = TestApp::ready().await;
    let response = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?max_uses=0"),
            Body::from("x"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_share_can_be_created_authorised_and_retrieved() {
    common::share_retrieval_lifecycle(&TestApp::ready().await).await;
}

/// Answering "you wanted the other route" is the caller's mistake, and a mistake
/// must not cost the share one of its retrievals.
#[tokio::test]
async fn asking_the_wrong_route_does_not_spend_a_retrieval() {
    let app = TestApp::ready().await;
    let share = upload(&app, "?filename=a.bin&max_uses=1", b"binary-ish").await;
    let code = share["code"].as_str().expect("a code").to_owned();

    let refused = app.get(&format!("/s/{code}/raw")).await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);

    // Still retrievable, which it would not be if the refusal had counted.
    let served = app.get(&format!("/s/{code}/download")).await;
    assert_eq!(served.status(), StatusCode::OK);
}

/// A one-time share is gone after one retrieval, and says so distinctly enough
/// that a client can tell "this expired" from "you typed the code wrong".
#[tokio::test]
async fn a_one_time_share_is_gone_after_its_retrieval() {
    let app = TestApp::ready().await;
    let share = upload(&app, "?max_uses=1", b"read me once").await;
    let code = share["code"].as_str().expect("a code").to_owned();

    let first = expect_status(app.get(&format!("/s/{code}/raw")).await, StatusCode::OK).await;
    assert_eq!(body_string(first).await, "read me once");

    let second = app.get(&format!("/s/{code}/raw")).await;
    assert_eq!(second.status(), StatusCode::GONE);
    assert_eq!(body_json(second).await["code"], "gone");

    // And a code nobody ever used is a different answer.
    let missing = app.get("/s/ZZZZZZZZ/raw").await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

/// A password-protected share must not describe itself to somebody who has not
/// presented the password — the same reason its link preview must not.
#[tokio::test]
async fn a_locked_share_reveals_only_that_it_is_locked() {
    let app = TestApp::ready().await;
    let created = expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=payroll.xlsx&title=Q3")
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

    let locked = body_json(
        expect_status(
            app.get(&format!("/api/v1/shares/{code}")).await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(locked["has_password"], true);
    assert_eq!(locked["unlocked"], false);
    for withheld in ["filename", "title", "size_bytes", "created_at"] {
        assert!(
            locked[withheld].is_null(),
            "`{withheld}` describes the content and must stay hidden: {locked}"
        );
    }

    // Retrieval without the password is refused. (`/download`, because this is a
    // file share — `/raw` would answer "wrong route" first, which is a check
    // that deliberately happens before the gate so a mistake costs nothing.)
    assert_eq!(
        app.get(&format!("/s/{code}/download")).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

/// Serving the type the uploader claimed is how a `text/html` upload becomes
/// stored XSS on the origin that holds everyone's session cookie.
#[tokio::test]
async fn retrieval_never_echoes_the_uploaders_media_type() {
    let app = TestApp::ready().await;
    let created = expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=trap.html")
                .header(header::CONTENT_TYPE, "text/html"),
            Body::from("<script>alert(1)</script>"),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let code = body_json(created).await["code"]
        .as_str()
        .expect("a code")
        .to_owned();

    let response = expect_status(
        app.get(&format!("/s/{code}/download")).await,
        StatusCode::OK,
    )
    .await;
    let headers = response.headers();
    assert_eq!(
        headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/octet-stream")
    );
    assert_eq!(
        headers
            .get(header::X_CONTENT_TYPE_OPTIONS)
            .and_then(|v| v.to_str().ok()),
        Some("nosniff")
    );
    assert!(
        headers
            .get(header::CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("attachment;")),
        "a file must never render inline"
    );
}

/// A stale `If-Range` must produce the whole object, not a slice of a different
/// one — otherwise a resumed download stitches two files together and nothing
/// reports an error.
#[tokio::test]
async fn a_stale_if_range_gets_the_whole_object() {
    let app = TestApp::ready().await;
    let share = upload(&app, "?filename=big.bin", &vec![9_u8; 4096]).await;
    let code = share["code"].as_str().expect("a code").to_owned();

    let fresh = app.get(&format!("/s/{code}/download")).await;
    let etag = fresh
        .headers()
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .expect("an entity tag")
        .to_owned();

    let matched = app
        .send(
            Request::builder()
                .uri(format!("/s/{code}/download"))
                .header(header::RANGE, "bytes=1000-")
                .header(header::IF_RANGE, &etag),
            Body::empty(),
        )
        .await;
    assert_eq!(matched.status(), StatusCode::PARTIAL_CONTENT);

    let stale = app
        .send(
            Request::builder()
                .uri(format!("/s/{code}/download"))
                .header(header::RANGE, "bytes=1000-")
                .header(header::IF_RANGE, "\"something-else\""),
            Body::empty(),
        )
        .await;
    assert_eq!(stale.status(), StatusCode::OK);
    assert_eq!(body_string(stale).await.len(), 4096);
}

/// A ticket authorises one share. Presenting somebody else's is the same as
/// presenting none — which on a password-protected share means being asked for
/// the password.
#[tokio::test]
async fn a_ticket_is_bound_to_its_share() {
    let app = TestApp::ready().await;

    let locked = expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares")
                .header("x-share-password", "1234"),
            Body::from("mine"),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let mine = body_json(locked).await["code"]
        .as_str()
        .expect("a code")
        .to_owned();

    let theirs = upload(&app, "", b"theirs").await;
    let theirs = theirs["code"].as_str().expect("a code").to_owned();
    let ticket = body_json(
        expect_status(
            app.empty(
                Method::POST,
                &format!("/api/v1/shares/{theirs}/unlock"),
                Auth::None,
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    let ticket = ticket["ticket"].as_str().expect("a ticket").to_owned();

    // Their ticket buys nothing here: the password is still required.
    let refused = app.get(&format!("/s/{mine}/raw?t={ticket}")).await;
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);

    // And it still opens the share it was issued for.
    let served = app.get(&format!("/s/{theirs}/raw?t={ticket}")).await;
    assert_eq!(served.status(), StatusCode::OK);
    assert_eq!(body_string(served).await, "theirs");
}

/// axum routes `HEAD` to the same handler as `GET`, so without an explicit check
/// a link checker, a prefetch or a chat client probing the URL spends somebody's
/// one-time share — the exact failure the ticket model exists to prevent,
/// reached by a route nobody thinks about.
#[tokio::test]
async fn a_head_request_describes_without_spending() {
    let app = TestApp::ready().await;
    let share = upload(&app, "?filename=probe.bin&max_uses=2", b"hello").await;
    let code = share["code"].as_str().expect("a code").to_owned();

    let head = app
        .send(
            Request::builder()
                .method(Method::HEAD)
                .uri(format!("/s/{code}/download")),
            Body::empty(),
        )
        .await;
    assert_eq!(head.status(), StatusCode::OK);
    assert_eq!(
        head.headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok()),
        Some("5"),
        "a HEAD must still describe the response a GET would give"
    );
    assert!(head.headers().contains_key(header::ETAG));

    let after_head = body_json(
        expect_status(
            app.get(&format!("/api/v1/shares/{code}")).await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(after_head["uses_remaining"], 2, "HEAD must cost nothing");

    expect_status(
        app.get(&format!("/s/{code}/download")).await,
        StatusCode::OK,
    )
    .await;
    let after_get = body_json(
        expect_status(
            app.get(&format!("/api/v1/shares/{code}")).await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(after_get["uses_remaining"], 1, "a GET is the retrieval");
}

/// A `416` without `Content-Range` does not tell the client what range *would*
/// have worked, so it has to guess.
#[tokio::test]
async fn an_impossible_range_says_how_big_the_object_is() {
    let app = TestApp::ready().await;
    let share = upload(&app, "?filename=small.bin", b"0123456789").await;
    let code = share["code"].as_str().expect("a code").to_owned();

    let response = app
        .send(
            Request::builder()
                .uri(format!("/s/{code}/download"))
                .header(header::RANGE, "bytes=9999-"),
            Body::empty(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok()),
        Some("bytes */10")
    );
    assert_eq!(body_json(response).await["code"], "range_not_satisfiable");
}

/// A streamed body has no length axum can infer, so it has to be set by hand —
/// and without it the browser shows no progress bar and no time remaining.
#[tokio::test]
async fn a_download_always_states_its_length() {
    let app = TestApp::with_config(|config| {
        // Small enough that the payload goes to the blob store, which is the
        // path that streams and therefore the one that can lose the length.
        config.share.inline_threshold_bytes = 16;
    })
    .await;
    let share = upload(&app, "?filename=big.bin", &vec![3_u8; 4096]).await;
    let code = share["code"].as_str().expect("a code").to_owned();

    let whole = expect_status(
        app.get(&format!("/s/{code}/download")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        whole
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok()),
        Some("4096")
    );
    assert_eq!(body_string(whole).await.len(), 4096);

    let part = app
        .send(
            Request::builder()
                .uri(format!("/s/{code}/download"))
                .header(header::RANGE, "bytes=1000-1099"),
            Body::empty(),
        )
        .await;
    assert_eq!(part.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        part.headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok()),
        Some("100")
    );
    assert_eq!(body_string(part).await.len(), 100);
}

/// A share password may be four characters, which is the right trade for
/// something read out over the phone and worthless without a limit on guesses.
/// Measured against a running server before this existed: 246 attempts a second,
/// so ten thousand combinations fell in forty-two seconds.
#[tokio::test]
async fn guessing_a_share_password_is_throttled() {
    let app = TestApp::ready().await;
    let created = expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?max_uses=100")
                .header("x-share-password", "4271"),
            Body::from("the secret"),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let code = body_json(created).await["code"]
        .as_str()
        .expect("a code")
        .to_owned();

    let guess = async |password: String| {
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/shares/{code}/unlock"))
                .header("x-share-password", password),
            Body::empty(),
        )
        .await
    };

    let mut refused = None;
    for attempt in 0..20 {
        let response = guess(format!("{:04}", 1000 + attempt)).await;
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            refused = Some(response);
            break;
        }
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let refused = refused.expect("repeated wrong passwords must eventually be refused");
    assert!(
        refused
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|seconds| seconds.parse::<u64>().is_ok_and(|s| s > 0)),
        "a 429 without Retry-After leaves the client guessing"
    );

    // And the right password is refused too while the block holds — otherwise
    // the limit would be trivially sidestepped by trying the real one last.
    assert_eq!(
        guess("4271".to_owned()).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

/// Walking the code space has to get expensive. The default eight-character
/// code was never brute-forceable at any rate, but the reasoning written next to
/// `share.code_length` says a short code depends on this — so it had better
/// exist.
#[tokio::test]
async fn trying_codes_that_do_not_exist_is_throttled() {
    let app = TestApp::ready().await;
    let real = upload(&app, "", b"findable").await;
    let real = real["code"].as_str().expect("a code").to_owned();

    let mut refused = None;
    for attempt in 0..200_u32 {
        // Every guess is a different code, which is what makes counting per code
        // useless and counting per source necessary.
        let guess = format!("{attempt:08}");
        let response = app.get(&format!("/api/v1/shares/{guess}")).await;
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            refused = Some(response);
            break;
        }
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    let refused = refused.expect("guessing codes must eventually be refused");
    assert!(
        refused
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|seconds| seconds.parse::<u64>().is_ok_and(|s| s > 0)),
        "a 429 without Retry-After leaves the client guessing"
    );

    // The block covers every route that resolves a code, not just the one that
    // tripped it — otherwise the limit is one `curl` away from meaningless.
    assert_eq!(
        app.get(&format!("/s/{real}/raw")).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

/// A share that exists but has expired is somebody returning to a link they were
/// given, not somebody guessing — so it must not count towards the limit.
#[tokio::test]
async fn returning_to_a_dead_link_is_not_guessing() {
    let app = TestApp::ready().await;
    let share = upload(&app, "?max_uses=1", b"read once").await;
    let code = share["code"].as_str().expect("a code").to_owned();
    expect_status(app.get(&format!("/s/{code}/raw")).await, StatusCode::OK).await;

    // Far more attempts than the limit allows, all on a code that is real.
    for _ in 0..100 {
        assert_eq!(
            app.get(&format!("/api/v1/shares/{code}")).await.status(),
            StatusCode::GONE
        );
    }

    // And a genuine miss is still counted afterwards, so the counter was not
    // simply switched off.
    assert_eq!(
        app.get("/api/v1/shares/ZZZZZZZZ").await.status(),
        StatusCode::NOT_FOUND
    );
}

/// `/raw` used to ignore `Range` entirely: it answered `200` with the whole
/// note however little was asked for, and advertised nothing to say so. A
/// downloader that had written the first half wrote it again, and believed it
/// had resumed. `/download` had honoured ranges since it existed, which is what
/// made the gap easy to miss — the two routes disagreed about how a transfer
/// works, and only one of them was tested.
#[tokio::test]
async fn raw_honours_a_range_the_way_download_does() {
    let app = TestApp::ready().await;
    let note = "0123456789".repeat(64);
    let share = upload(&app, "", note.as_bytes()).await;
    let code = share["code"].as_str().expect("a code").to_owned();

    let whole = expect_status(app.get(&format!("/s/{code}/raw")).await, StatusCode::OK).await;
    assert_eq!(
        whole
            .headers()
            .get(header::ACCEPT_RANGES)
            .and_then(|v| v.to_str().ok()),
        Some("bytes"),
        "a route that honours ranges has to say so, or nobody asks"
    );
    assert_eq!(
        whole
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok()),
        Some("640")
    );
    assert!(whole.headers().contains_key(header::ETAG));

    let part = app
        .send(
            Request::builder()
                .uri(format!("/s/{code}/raw"))
                .header(header::RANGE, "bytes=10-19"),
            Body::empty(),
        )
        .await;
    assert_eq!(part.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        part.headers()
            .get(header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok()),
        Some("bytes 10-19/640")
    );
    assert_eq!(body_string(part).await, "0123456789");
}

/// The same refusal `/download` gives. A `416` without `Content-Range` leaves
/// the client with no way to work out what it should have asked for.
#[tokio::test]
async fn raw_refuses_a_range_past_the_end() {
    let app = TestApp::ready().await;
    let share = upload(&app, "", b"short note").await;
    let code = share["code"].as_str().expect("a code").to_owned();

    let response = app
        .send(
            Request::builder()
                .uri(format!("/s/{code}/raw"))
                .header(header::RANGE, "bytes=9999-"),
            Body::empty(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok()),
        Some("bytes */10")
    );
}

/// Resuming against content that has been replaced would splice two different
/// notes together, and nothing downstream would report it as an error.
#[tokio::test]
async fn raw_ignores_a_range_whose_if_range_is_stale() {
    let app = TestApp::ready().await;
    let note = "abcdefghij".repeat(10);
    let share = upload(&app, "", note.as_bytes()).await;
    let code = share["code"].as_str().expect("a code").to_owned();

    let etag = expect_status(app.get(&format!("/s/{code}/raw")).await, StatusCode::OK)
        .await
        .headers()
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .expect("an entity tag")
        .to_owned();

    let matching = app
        .send(
            Request::builder()
                .uri(format!("/s/{code}/raw"))
                .header(header::RANGE, "bytes=50-")
                .header(header::IF_RANGE, &etag),
            Body::empty(),
        )
        .await;
    assert_eq!(matching.status(), StatusCode::PARTIAL_CONTENT);

    let stale = app
        .send(
            Request::builder()
                .uri(format!("/s/{code}/raw"))
                .header(header::RANGE, "bytes=50-")
                .header(header::IF_RANGE, "\"something-else\""),
            Body::empty(),
        )
        .await;
    assert_eq!(stale.status(), StatusCode::OK);
    assert_eq!(body_string(stale).await.len(), 100);
}

/// The hardening `/raw` exists for must survive the refactor that gave it
/// ranges: a note is `text/plain` whatever was uploaded, and cannot be sniffed
/// into something executable on the origin holding everyone's session cookie.
#[tokio::test]
async fn a_ranged_raw_response_is_still_hardened() {
    let app = TestApp::ready().await;
    let share = upload(&app, "", b"<script>alert(1)</script> padding padding").await;
    let code = share["code"].as_str().expect("a code").to_owned();

    let part = app
        .send(
            Request::builder()
                .uri(format!("/s/{code}/raw"))
                .header(header::RANGE, "bytes=0-9"),
            Body::empty(),
        )
        .await;
    assert_eq!(part.status(), StatusCode::PARTIAL_CONTENT);
    let headers = part.headers();
    assert_eq!(
        headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("text/plain; charset=utf-8")
    );
    assert_eq!(
        headers
            .get(header::X_CONTENT_TYPE_OPTIONS)
            .and_then(|v| v.to_str().ok()),
        Some("nosniff")
    );
    assert!(headers.contains_key(header::CONTENT_SECURITY_POLICY));
    assert_eq!(
        headers
            .get(header::CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok()),
        Some("inline")
    );
}

/// The lifetime a share gets when nobody asks for one used to be the ceiling
/// itself — so the longest life anyone was *allowed* to request was also what
/// everyone got by default. On the shipped settings that meant holding thirty
/// days of uploads to serve links that are almost all collected within hours.
///
/// Ephemeral storage does not accumulate: what is on disk is roughly the upload
/// rate times how long things are kept, which makes this one number a bigger
/// lever on disk usage than any quota.
#[tokio::test]
async fn an_unspecified_lifetime_uses_the_default_not_the_ceiling() {
    let app = TestApp::with_config(|config| {
        config.share.default_expiry_hours = 24;
        config.share.max_expiry_hours = 24 * 30;
    })
    .await;

    let created = OffsetDateTime::now_utc();
    let share = upload(&app, "", b"no lifetime requested").await;
    let expires = OffsetDateTime::parse(
        share["expires_at"].as_str().expect("an expiry"),
        &time::format_description::well_known::Rfc3339,
    )
    .expect("RFC 3339");

    let lifetime = expires - created;
    assert!(
        lifetime < time::Duration::hours(25) && lifetime > time::Duration::hours(23),
        "expected about a day, got {lifetime:?} — the ceiling is thirty"
    );
}

/// Asking for more than the ceiling is still refused, and asking for something
/// between the default and the ceiling is still granted. Shortening the default
/// must not quietly become a shorter limit.
#[tokio::test]
async fn the_ceiling_still_governs_what_may_be_asked_for() {
    let app = TestApp::with_config(|config| {
        config.share.default_expiry_hours = 24;
        config.share.max_expiry_hours = 24 * 30;
    })
    .await;

    let created = OffsetDateTime::now_utc();
    let share = upload(&app, "?expires_in_hours=200", b"longer than the default").await;
    let expires = OffsetDateTime::parse(
        share["expires_at"].as_str().expect("an expiry"),
        &time::format_description::well_known::Rfc3339,
    )
    .expect("RFC 3339");
    assert!(
        expires - created > time::Duration::hours(199),
        "a request between the default and the ceiling must be honoured"
    );

    let refused = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?expires_in_hours=99999"),
            Body::from("past the ceiling"),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
}

/// A share's row and its content have different reasons to survive. The row is
/// what lets a link say "this expired" instead of "no such code", which is worth
/// keeping for a while. The content is worth nothing the moment the share dies —
/// the gate refuses a dead share before anything reads it — and it is the half
/// that costs money.
///
/// They used to go together, so a burn-after-reading share occupied its full
/// size for a week after the one person who was meant to have it already did.
#[tokio::test]
async fn a_dead_share_loses_its_content_but_keeps_its_row() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.share.retain_after_death_hours = 24 * 7;
    })
    .await;

    let share = upload(&app, "?filename=once.bin&max_uses=1", &vec![7_u8; 4096]).await;
    let code = share["code"].as_str().expect("a code").to_owned();

    // Spend the single retrieval it was allowed.
    expect_status(
        app.get(&format!("/s/{code}/download")).await,
        StatusCode::OK,
    )
    .await;

    let stored_before = app
        .state
        .shares
        .total_stored_bytes()
        .await
        .expect("a total");
    assert_eq!(
        stored_before, 4096,
        "the content is still held before a sweep"
    );

    tessera_server::maintenance::sweep_once(&app.state, OffsetDateTime::now_utc()).await;

    // The row survives, so the link still explains itself.
    let response = app.get(&format!("/api/v1/shares/{code}")).await;
    assert_eq!(
        response.status(),
        StatusCode::GONE,
        "the row has to outlive the content, or the link says `no such code`"
    );

    // The bytes do not.
    assert_eq!(
        app.state
            .shares
            .total_stored_bytes()
            .await
            .expect("a total"),
        0,
        "a dead share's content must not sit in storage until the row expires"
    );
}

/// Without a ceiling, an instance with anonymous upload on is an unlimited drive
/// for anyone who can reach it, and the first sign of trouble is writes failing
/// because the disk is full.
#[tokio::test]
async fn uploads_stop_when_the_instance_is_out_of_room() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.storage.max_total_bytes = 8192;
    })
    .await;

    upload(&app, "?filename=first.bin", &vec![1_u8; 6000]).await;

    // 6000 held, 8192 allowed: this one does not fit, and the refusal has to
    // name the instance rather than the file.
    let refused = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=too-much.bin"),
            Body::from(vec![2_u8; 4000]),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::INSUFFICIENT_STORAGE);

    // Something that does fit is still accepted, so the limit is a limit rather
    // than a switch.
    upload(&app, "?filename=small.bin", &vec![3_u8; 1000]).await;
}

/// A quota that counted rows kept only to say "this expired" would refuse
/// uploads to make room for content that is not there.
#[tokio::test]
async fn reaped_shares_free_their_quota() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.storage.max_total_bytes = 8192;
    })
    .await;

    let share = upload(&app, "?filename=big.bin&max_uses=1", &vec![9_u8; 6000]).await;
    let code = share["code"].as_str().expect("a code").to_owned();
    expect_status(
        app.get(&format!("/s/{code}/download")).await,
        StatusCode::OK,
    )
    .await;
    tessera_server::maintenance::sweep_once(&app.state, OffsetDateTime::now_utc()).await;

    // The row is still there answering 410, but its 6000 bytes are not.
    upload(&app, "?filename=now-it-fits.bin", &vec![4_u8; 6000]).await;
}

/// Small content lives in the database rather than the blob store, and marking
/// a share `gone` takes it out of the quota either way. For an inline share
/// that used to be a lie in the dangerous direction: the bytes stayed in
/// `share_contents` until the row itself went a week later, so the instance
/// believed it had room it did not have, and the drift accumulated for as long
/// as the retention window.
#[tokio::test]
async fn reaping_an_inline_share_actually_frees_the_database() {
    let app = TestApp::with_config(|config| {
        // Comfortably above the payload, so it stays in the database.
        config.share.inline_threshold_bytes = 64 * 1024;
    })
    .await;

    let share = upload(&app, "?max_uses=1", &vec![5_u8; 4096]).await;
    let code = share["code"].as_str().expect("a code").to_owned();
    let id: tessera_core::ShareId = share["id"]
        .as_str()
        .expect("an id")
        .parse()
        .expect("a share id");

    expect_status(app.get(&format!("/s/{code}/raw")).await, StatusCode::OK).await;
    tessera_server::maintenance::sweep_once(&app.state, OffsetDateTime::now_utc()).await;

    // The quota already says these bytes are free…
    assert_eq!(
        app.state
            .shares
            .total_stored_bytes()
            .await
            .expect("a total"),
        0
    );
    // …so they had better actually be.
    assert_eq!(
        app.state
            .shares
            .text_of(id)
            .await
            .expect("a lookup")
            .map(|t| t.content),
        None,
        "the quota released this content, so the database must have too"
    );
}

/// Expiry and exhaustion are one-way: no amount of waiting brings a share back,
/// so destroying its content the moment it dies costs nothing. Disabling is not
/// like that — an operator did it and may undo it — so a disabled share keeps
/// its content until its row goes for good.
///
/// Nothing writes `disabled_at` yet (the admin endpoints are #7), which is
/// exactly why this is worth pinning: the reaper would happily have taken those
/// bytes, and the bug would only have surfaced the first time somebody
/// re-enabled something and got a live link to nothing.
#[tokio::test]
async fn a_disabled_share_keeps_its_content() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
    })
    .await;

    let share = upload(&app, "?filename=paused.bin", &vec![2_u8; 4096]).await;
    let id: tessera_core::ShareId = share["id"]
        .as_str()
        .expect("an id")
        .parse()
        .expect("a share id");

    // No endpoint writes `disabled_at` yet, so the test does it the only way
    // there is. That is the point: the reaper must already be right about a
    // state the API cannot reach, because #7 will make it reachable.
    app.state
        .db
        .execute_unprepared(&format!(
            "UPDATE shares SET disabled_at = '{}' WHERE id = '{id}'",
            OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .expect("RFC 3339")
        ))
        .await
        .expect("could not disable the share");

    tessera_server::maintenance::sweep_once(&app.state, OffsetDateTime::now_utc()).await;

    assert_eq!(
        app.state
            .shares
            .total_stored_bytes()
            .await
            .expect("a total"),
        4096,
        "a disabled share is paused, not dead — re-enabling it must give back \
         something that works"
    );
}
#[tokio::test]
async fn lowering_the_quota_below_current_usage_refuses_rather_than_panics() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.storage.max_total_bytes = 10_000;
    })
    .await;
    upload(&app, "?filename=a.bin", &vec![1_u8; 8000]).await;

    // The operator lowers it below what is already held. saturating_sub keeps
    // this at "no room" rather than wrapping to "almost unlimited".
    let tight = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.storage.max_total_bytes = 1;
    })
    .await;
    let refused = tight
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=b.bin"),
            Body::from(vec![2_u8; 100]),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::INSUFFICIENT_STORAGE);
}

/// The limit is a limit, not a fence one byte inside it. An upload that lands
/// exactly on the figure has to be accepted, or every operator's quota is
/// quietly one byte smaller than they wrote.
#[tokio::test]
async fn an_upload_that_exactly_fills_the_quota_is_accepted() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.storage.max_total_bytes = 4096;
    })
    .await;

    upload(&app, "?filename=exact.bin", &vec![8_u8; 4096]).await;
    assert_eq!(
        app.state
            .shares
            .total_stored_bytes()
            .await
            .expect("a total"),
        4096
    );

    let refused = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=one-more.bin"),
            Body::from(vec![9_u8; 1]),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::INSUFFICIENT_STORAGE);
}

/// The sweep treats a failed object delete as "try again next hour", which is
/// right for a store that is down and wrong for an object that is simply not
/// there. Somebody who tidies their bucket by hand would otherwise leave rows
/// the reaper can never mark — retried every hour forever, and counted against
/// the quota for bytes that do not exist.
#[tokio::test]
async fn a_share_whose_object_already_vanished_is_still_reaped() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
    })
    .await;

    let share = upload(
        &app,
        "?filename=vanishing.bin&max_uses=1",
        &vec![3_u8; 4096],
    )
    .await;
    let code = share["code"].as_str().expect("a code").to_owned();
    expect_status(
        app.get(&format!("/s/{code}/download")).await,
        StatusCode::OK,
    )
    .await;

    // Somebody cleaned the bucket out from under us.
    let id: tessera_core::ShareId = share["id"].as_str().expect("an id").parse().expect("an id");
    let stored = content_of(&app, id).await;
    let key = first_key(&stored).expect("an object key").to_owned();
    app.state
        .storage
        .delete(&object_store::path::Path::from(key))
        .await
        .expect("could not remove the object by hand");

    tessera_server::maintenance::sweep_once(&app.state, OffsetDateTime::now_utc()).await;

    assert_eq!(
        app.state
            .shares
            .total_stored_bytes()
            .await
            .expect("a total"),
        0,
        "a row pointing at an object that is gone must still be reaped, or it \
         is retried forever and holds quota for bytes nobody has"
    );
}

/// A ticket authorises one share. Presenting it against another must not work,
/// or one paid retrieval would open everything the holder can name.
#[tokio::test]
async fn a_ticket_does_not_travel_between_shares() {
    let app = TestApp::ready().await;
    let mine = upload(&app, "", b"mine").await;
    let theirs = upload(&app, "?max_uses=1", b"theirs").await;
    let my_code = mine["code"].as_str().expect("a code").to_owned();
    let their_code = theirs["code"].as_str().expect("a code").to_owned();

    let unlocked = expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/shares/{my_code}/unlock")),
            Body::empty(),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let ticket = body_json(unlocked).await["ticket"]
        .as_str()
        .expect("a ticket")
        .to_owned();

    // Presenting it elsewhere must not be free: it falls through to a normal
    // retrieval, which spends the other share's single use.
    let response = app
        .send(
            Request::builder()
                .uri(format!("/s/{their_code}/raw"))
                .header("x-share-ticket", &ticket),
            Body::empty(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let after = app.get(&format!("/s/{their_code}/raw")).await;
    assert_eq!(
        after.status(),
        StatusCode::GONE,
        "the other share's one use was spent, so a foreign ticket bought nothing"
    );
}

/// Unlocks a share and returns the ticket that authorises the transfer.
///
/// See `common/mod.rs`: the test-code exemption in `clippy.toml` only reaches
/// code inside a `#[test]` body, so a helper has to restate it.
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test helper; a fixture that cannot build has nothing to report but a failure"
)]
async fn unlock(app: &TestApp, code: &str) -> String {
    let response = expect_status(
        app.empty(
            Method::POST,
            &format!("/api/v1/shares/{code}/unlock"),
            Auth::None,
        )
        .await,
        StatusCode::OK,
    )
    .await;
    body_json(response).await["ticket"]
        .as_str()
        .expect("a ticket")
        .to_owned()
}

/// How many retrievals a share has been charged for.
#[expect(clippy::expect_used, reason = "test helper; see `unlock`")]
async fn uses_spent(app: &TestApp, id: &str) -> u32 {
    let id = tessera_core::ShareId::parse(id).expect("a share id");
    app.state
        .shares
        .find_by_id(id)
        .await
        .expect("query")
        .expect("the share")
        .use_count
}

/// The bug this pins: unlocking a one-time share is what empties it, so the
/// share is already exhausted by the time the download that paid for it
/// arrives. Judging availability before looking at the ticket refused every
/// single-use share at the last step — the feature was unusable through its own
/// front door, and only the unticketed path (`curl` straight at `/raw`) worked.
#[tokio::test]
async fn a_one_time_share_survives_the_unlock_that_emptied_it() {
    let app = TestApp::ready().await;
    let share = upload(&app, "?max_uses=1", b"read me once").await;
    let code = share["code"].as_str().expect("a code").to_owned();

    let ticket = unlock(&app, &code).await;
    assert_eq!(
        uses_spent(&app, share["id"].as_str().expect("an id")).await,
        1
    );

    let served = expect_status(
        app.get(&format!("/s/{code}/raw?t={ticket}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(body_string(served).await, "read me once");

    // And it bought nothing for anyone else: the share really is spent.
    let refused = app.get(&format!("/s/{code}/raw")).await;
    assert_eq!(refused.status(), StatusCode::GONE);
}

/// The reason a ticket exists at all. A transfer is many requests — `Range`,
/// a dropped connection, a `HEAD` before resuming — and none of them may cost
/// a second retrieval.
#[tokio::test]
async fn a_resumed_transfer_is_charged_once() {
    let app = TestApp::ready().await;
    // Two halves of forty bytes, named rather than sliced out again later — so
    // the assertion below cannot agree with a mistake in the arithmetic.
    let head = "0123456789".repeat(4);
    let tail = "abcdefghij".repeat(4);
    let note = format!("{head}{tail}");
    let share = upload(&app, "?max_uses=1", note.as_bytes()).await;
    let code = share["code"].as_str().expect("a code").to_owned();
    let id = share["id"].as_str().expect("an id").to_owned();

    let ticket = unlock(&app, &code).await;

    // What a resuming client actually does: ask what is there, then take the
    // rest in pieces.
    let probe = app
        .send(
            Request::builder()
                .method(Method::HEAD)
                .uri(format!("/s/{code}/raw?t={ticket}")),
            Body::empty(),
        )
        .await;
    assert_eq!(probe.status(), StatusCode::OK);

    for (range, expected) in [("bytes=0-39", &head), ("bytes=40-79", &tail)] {
        let part = app
            .send(
                Request::builder()
                    .uri(format!("/s/{code}/raw?t={ticket}"))
                    .header(header::RANGE, range),
                Body::empty(),
            )
            .await;
        assert_eq!(part.status(), StatusCode::PARTIAL_CONTENT, "for {range}");
        assert_eq!(body_string(part).await, *expected, "for {range}");
    }

    assert_eq!(
        uses_spent(&app, &id).await,
        1,
        "four requests, one retrieval"
    );
}

/// A ticket clears exhaustion because that is this reader's own retrieval
/// coming back around. An expiry is the uploader having said "not after this",
/// and no ticket outranks that — a transfer still running when the share
/// expires stops there.
#[tokio::test]
async fn a_ticket_does_not_outlive_the_expiry() {
    let app = TestApp::ready().await;
    let share = upload(&app, "", b"time-limited").await;
    let code = share["code"].as_str().expect("a code").to_owned();
    let id = share["id"].as_str().expect("an id").to_owned();

    let ticket = unlock(&app, &code).await;
    // It works right up until it does not.
    expect_status(
        app.get(&format!("/s/{code}/raw?t={ticket}")).await,
        StatusCode::OK,
    )
    .await;

    backdate(&app, &id, "expires_at", OffsetDateTime::now_utc()).await;

    let refused = app.get(&format!("/s/{code}/raw?t={ticket}")).await;
    assert_eq!(refused.status(), StatusCode::GONE);
    assert_eq!(body_json(refused).await["code"], "gone");
}

/// Nor does it outrank an operator. A disabled share reads as "never existed"
/// to everyone, ticket or not.
#[tokio::test]
async fn a_ticket_does_not_survive_a_disable() {
    let app = TestApp::ready().await;
    let share = upload(&app, "", b"switched off").await;
    let code = share["code"].as_str().expect("a code").to_owned();
    let id = share["id"].as_str().expect("an id").to_owned();

    let ticket = unlock(&app, &code).await;
    backdate(&app, &id, "disabled_at", OffsetDateTime::now_utc()).await;

    let refused = app.get(&format!("/s/{code}/raw?t={ticket}")).await;
    assert_eq!(refused.status(), StatusCode::NOT_FOUND);
}

/// Writes a timestamp column directly.
///
/// No endpoint sets `disabled_at` yet (#7) and `expires_at` is only settable in
/// whole hours at creation, so a test cannot reach either state through the API.
#[expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test helper; see `unlock`"
)]
async fn backdate(app: &TestApp, id: &str, column: &str, at: OffsetDateTime) {
    let at = at
        .format(&time::format_description::well_known::Rfc3339)
        .expect("RFC 3339");
    app.state
        .db
        .execute_unprepared(&format!(
            "UPDATE shares SET {column} = '{at}' WHERE id = '{id}'"
        ))
        .await
        .unwrap_or_else(|error| panic!("could not set {column}: {error}"));
}

// ── the table multi-file will need ──────────────────────────────────

/// A file share writes a row to `file_shares` as well as `shares`. The table has
/// to be true from the first row, because a table the migration filled and
/// nothing maintains is worse than no table.
#[tokio::test]
async fn a_file_share_records_its_file() {
    let app = TestApp::ready().await;
    let created = upload(&app, "?filename=notes.txt", b"hello there").await;
    let id = tessera_core::ShareId::parse(created["id"].as_str().expect("an id")).expect("an id");

    let files = app.state.shares.files_of(id).await.expect("query");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].filename.as_deref(), Some("notes.txt"));
    assert_eq!(files[0].size_bytes, 11);
    assert_eq!(files[0].position, 0);
    assert_eq!(files[0].share_id, id.to_string());
}

/// Text has no file, and gets no row. Its content stays in `share_contents`
/// until the migration that gives text its own table.
#[tokio::test]
async fn a_text_share_records_no_file() {
    let app = TestApp::ready().await;
    let created = upload(&app, "", b"just a note").await;
    let id = tessera_core::ShareId::parse(created["id"].as_str().expect("an id")).expect("an id");

    assert!(
        app.state
            .shares
            .files_of(id)
            .await
            .expect("query")
            .is_empty()
    );
}

/// Deleting a share takes its files with it — the foreign key says so, and
/// nothing in the application has to remember.
#[tokio::test]
async fn deleting_a_share_takes_its_files() {
    let app = TestApp::ready().await;
    let created = upload(&app, "?filename=gone.txt", b"bytes").await;
    let id = tessera_core::ShareId::parse(created["id"].as_str().expect("an id")).expect("an id");

    app.state.shares.delete(id).await.expect("delete");
    assert!(
        app.state
            .shares
            .files_of(id)
            .await
            .expect("query")
            .is_empty()
    );
}

/// A text share writes its content to `text_shares` as well. Same reasoning as
/// `file_shares`: the table has to be true from the first row, and the readers
/// move over separately.
#[tokio::test]
async fn a_text_share_records_its_text() {
    let app = TestApp::ready().await;
    let created = upload(&app, "?language=rust", b"fn main() {}").await;
    let id = tessera_core::ShareId::parse(created["id"].as_str().expect("an id")).expect("an id");

    let text = app
        .state
        .shares
        .text_of(id)
        .await
        .expect("query")
        .expect("the text");
    assert_eq!(text.content, b"fn main() {}");
    assert_eq!(text.language.as_deref(), Some("rust"));
}

/// A file share gets no text row, the same way a text share gets no file row.
#[tokio::test]
async fn a_file_share_records_no_text() {
    let app = TestApp::ready().await;
    let created = upload(&app, "?filename=data.bin", b"\x00\x01\x02").await;
    let id = tessera_core::ShareId::parse(created["id"].as_str().expect("an id")).expect("an id");

    assert!(app.state.shares.text_of(id).await.expect("query").is_none());
}

/// Deleting a share takes its text with it, by the same foreign key.
#[tokio::test]
async fn deleting_a_share_takes_its_text() {
    let app = TestApp::ready().await;
    let created = upload(&app, "", b"a note").await;
    let id = tessera_core::ShareId::parse(created["id"].as_str().expect("an id")).expect("an id");

    app.state.shares.delete(id).await.expect("delete");
    assert!(app.state.shares.text_of(id).await.expect("query").is_none());
}

/// The rule this instance follows is about what the content is, not how much of
/// it there is. Nine bytes with a filename is a file, and a file is an object —
/// so somebody looking for an upload finds it in one place, and a backup covers
/// it by covering the store.
#[tokio::test]
async fn even_a_tiny_file_becomes_an_object() {
    let app = TestApp::ready().await;
    let share = upload(&app, "?filename=tiny.bin", b"nine byte").await;
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");

    let stored = content_of(&app, id).await;
    let key =
        first_key(&stored).unwrap_or_else(|| panic!("expected object storage, got {stored:?}"));

    let bytes = app
        .state
        .storage
        .get(&object_store::path::Path::from(key))
        .await
        .expect("fetch")
        .bytes()
        .await
        .expect("read");
    assert_eq!(bytes.as_ref(), b"nine byte");
}

/// And text never leaves the database, however close to the limit it gets.
#[tokio::test]
async fn text_stays_in_the_database_at_any_size() {
    let app = TestApp::ready().await;
    // Well past the old inline threshold, which no longer decides anything.
    let content = "x".repeat(200 * 1024);
    let share = upload(&app, "", content.as_bytes()).await;
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");

    let stored = content_of(&app, id).await;
    assert!(
        first_key(&stored).is_none(),
        "text must not reach the object store, got {stored:?}"
    );
    assert_eq!(
        app.state
            .shares
            .text_of(id)
            .await
            .expect("query")
            .expect("the text")
            .content
            .len(),
        content.len()
    );
}

/// Text is held whole in memory and stored whole as a row, so there has to be a
/// point where the answer is "that is a file". Refused as it arrives rather than
/// after the whole thing has been read.
#[tokio::test]
async fn text_past_the_limit_is_refused_and_says_what_to_do() {
    let app = TestApp::with_config(|config| {
        config.share.max_text_bytes = 4096;
    })
    .await;

    let response = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares"),
            Body::from("x".repeat(8192)),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_json(response).await;
    assert!(
        body["message"]
            .as_str()
            .expect("a message")
            .contains("as a file"),
        "the message has to say what to do instead, got {}",
        body["message"]
    );
}

/// The same content with a filename is a file, and files have no such limit.
#[tokio::test]
async fn the_same_bytes_as_a_file_are_accepted() {
    let app = TestApp::with_config(|config| {
        config.share.max_text_bytes = 4096;
    })
    .await;

    let share = upload(&app, "?filename=big.txt", "x".repeat(8192).as_bytes()).await;
    assert_eq!(share["kind"], "file");
}

/// A share whose content has been reaped says so.
///
/// The reaper marks `file_shares` as gone once the object is deleted. Leaving
/// it pointing at the object would mean a reader that slipped past the gate
/// asking the store for something that is not there.
#[tokio::test]
async fn a_reaped_share_reads_as_gone_from_the_new_table_as_well() {
    let app = TestApp::ready().await;
    let share = upload(&app, "?filename=doomed.bin", b"bytes").await;
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");

    app.state
        .shares
        .mark_content_removed(id)
        .await
        .expect("mark it removed");

    // The row survives — it is the record of what was removed — but it stops
    // naming an object, which is what stops a reader asking the store for one.
    let content = content_of(&app, id).await;
    let file = content.single_file().expect("the row is still there");
    assert_eq!(
        file.storage,
        StorageLocation::Gone,
        "a reaped file must not still point at its object"
    );
}

/// The two a client can report, and the one it cannot.
#[tokio::test]
async fn a_client_may_report_a_view_but_not_a_retrieval() {
    let app = TestApp::ready().await;
    let share = upload(&app, "?filename=note.txt", b"hello").await;
    let code = share["code"].as_str().expect("a code").to_owned();

    for kind in ["view", "copy", "view"] {
        assert_eq!(
            app.json(
                Method::POST,
                &format!("/api/v1/shares/{code}/events"),
                Auth::None,
                &serde_json::json!({ "kind": kind }),
            )
            .await
            .status(),
            StatusCode::NO_CONTENT,
            "{kind}"
        );
    }

    // A retrieval is a limit, so reporting one is refused rather than ignored —
    // a client that got this wrong should hear about it.
    assert_eq!(
        app.json(
            Method::POST,
            &format!("/api/v1/shares/{code}/events"),
            Auth::None,
            &serde_json::json!({ "kind": "download" }),
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );

    let stored = app
        .state
        .shares
        .find_by_code(&tessera_core::RetrievalCode::parse(&code).expect("a code"))
        .await
        .expect("query")
        .expect("the share");
    assert_eq!(stored.view_count, 2);
    assert_eq!(stored.copy_count, 1);
    assert_eq!(stored.use_count, 0, "none of that was a retrieval");
}

/// An unauthenticated endpoint must not become a way to ask which codes exist.
#[tokio::test]
async fn reporting_against_an_unknown_code_says_nothing() {
    let app = TestApp::ready().await;
    let real = upload(&app, "?filename=note.txt", b"hello").await;
    let real_code = real["code"].as_str().expect("a code").to_owned();

    let answer_for_real = app
        .json(
            Method::POST,
            &format!("/api/v1/shares/{real_code}/events"),
            Auth::None,
            &serde_json::json!({ "kind": "view" }),
        )
        .await
        .status();
    let answer_for_nothing = app
        .json(
            Method::POST,
            "/api/v1/shares/ZZZZZZZZZZ/events",
            Auth::None,
            &serde_json::json!({ "kind": "view" }),
        )
        .await
        .status();
    let answer_for_nonsense = app
        .json(
            Method::POST,
            "/api/v1/shares/not-a-code/events",
            Auth::None,
            &serde_json::json!({ "kind": "view" }),
        )
        .await
        .status();

    assert_eq!(answer_for_real, StatusCode::NO_CONTENT);
    assert_eq!(answer_for_nothing, answer_for_real);
    assert_eq!(answer_for_nonsense, answer_for_real);
}

/// The uploader's own listing carries the numbers, and says which to trust.
#[tokio::test]
async fn the_uploader_sees_the_counts_on_their_own_share() {
    let app = TestApp::ready().await;
    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": "counter",
                "email": "counter@example.com",
                "password": "a perfectly adequate passphrase",
            }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let session = common::session_cookie(&response);
    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/shares?filename=mine.bin")
                    .header(header::COOKIE, &session),
                Body::from(b"content".to_vec()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let code = created["code"].as_str().expect("a code").to_owned();

    expect_status(
        app.json(
            Method::POST,
            &format!("/api/v1/shares/{code}/events"),
            Auth::None,
            &serde_json::json!({ "kind": "view" }),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    expect_status(
        app.empty(Method::GET, &format!("/s/{code}/download"), Auth::None)
            .await,
        StatusCode::OK,
    )
    .await;

    let listed = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/shares", Auth::Cookie(&session))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    let mine = listed["shares"]
        .as_array()
        .expect("a list")
        .first()
        .expect("the share");
    assert_eq!(mine["view_count"], 1);
    assert_eq!(mine["copy_count"], 0);
    assert_eq!(mine["use_count"], 1, "the server counted the retrieval");
    assert!(
        mine["last_accessed_at"].is_string(),
        "has anybody picked it up yet"
    );
}

/// What a share sent is counted as it goes, not assumed from its size.
#[tokio::test]
async fn a_completed_download_counts_what_it_sent() {
    let app = TestApp::ready().await;
    let content = vec![7u8; 40 * 1024];
    let share = upload(&app, "?filename=big.bin", &content).await;
    let code = share["code"].as_str().expect("a code").to_owned();
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");

    let served = || async {
        app.state
            .shares
            .find_by_id(id)
            .await
            .expect("query")
            .expect("the share")
            .bytes_served
    };
    assert_eq!(served().await, 0);

    for _ in 0..2 {
        let response = expect_status(
            app.empty(Method::GET, &format!("/s/{code}/download"), Auth::None)
                .await,
            StatusCode::OK,
        )
        .await;
        // The body has to be consumed for anything to have been sent.
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read");
        assert_eq!(body.len(), content.len());
    }

    // The write is spawned from the body being dropped, so it lands just after.
    for _ in 0..50 {
        if served().await == u64::try_from(content.len() * 2).expect("a size") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(
        served().await,
        u64::try_from(content.len() * 2).expect("a size"),
        "two whole downloads"
    );
}

/// every time somebody clicked and changed their mind.
#[tokio::test]
async fn an_abandoned_download_counts_only_what_left() {
    use http_body::Body as _;

    let app = TestApp::ready().await;
    let content = vec![7u8; 512 * 1024];
    let share = upload(&app, "?filename=big.bin", &content).await;
    let code = share["code"].as_str().expect("a code").to_owned();
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");

    let response = expect_status(
        app.empty(Method::GET, &format!("/s/{code}/download"), Auth::None)
            .await,
        StatusCode::OK,
    )
    .await;
    let mut body = response.into_body();
    let mut taken = 0usize;
    // One frame, then walk away.
    if let Some(Ok(frame)) =
        std::future::poll_fn(|cx| std::pin::Pin::new(&mut body).poll_frame(cx)).await
        && let Some(data) = frame.data_ref()
    {
        taken = data.len();
    }
    drop(body);

    let served = || async {
        app.state
            .shares
            .find_by_id(id)
            .await
            .expect("query")
            .expect("the share")
            .bytes_served
    };
    for _ in 0..50 {
        if served().await > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let counted = served().await;
    assert_eq!(
        counted,
        u64::try_from(taken).expect("a size"),
        "only what actually left"
    );
    assert!(
        counted < u64::try_from(content.len()).expect("a size"),
        "the whole file must not be counted: {counted} of {}",
        content.len()
    );
}

/// The columns are gone, so nothing can disagree with the subtype tables.
///
/// This replaces the pair that used to prove the readers had switched: one made
/// `shares` say the wrong name and checked the response said the right one, the
/// other checked a share with no subtype row still worked. Neither is
/// expressible now — there is no second copy to be wrong, and no fallback to
/// fall back to. What is left worth asserting is that the one remaining source
/// answers.
#[tokio::test]
async fn a_shares_details_come_from_the_table_that_owns_them() {
    let app = TestApp::ready().await;
    let share = upload(&app, "?filename=holiday.jpg", b"0123456789").await;
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");

    let content = content_of(&app, id).await;
    let file = content.single_file().expect("one file");
    assert_eq!(file.filename.as_deref(), Some("holiday.jpg"));
    assert_eq!(file.size_bytes, 10);

    // And the only row carrying that is the one on `file_shares`.
    let files = app.state.shares.files_of(id).await.expect("query");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].filename.as_deref(), Some("holiday.jpg"));
}

/// The point of the endpoint: the link is already out, and the reason for the
/// original settings has changed.
#[tokio::test]
async fn an_anonymous_uploader_can_change_what_they_created() {
    let app = TestApp::ready().await;
    let share = upload(&app, "?title=notes&max_uses=1", b"hello").await;
    let token = share["management_token"]
        .as_str()
        .expect("a token")
        .to_owned();
    let code = share["code"].as_str().expect("a code").to_owned();

    let patched = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::PATCH)
                    .uri(format!("/api/v1/shares/{code}"))
                    .header("content-type", "application/json")
                    .header("x-share-management-token", &token),
                Body::from(r#"{"max_uses": 5, "expires_in_hours": 48}"#),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;

    assert_eq!(patched["uses_remaining"], 5);
    assert!(
        patched["expires_at"].is_string(),
        "the new expiry should come back: {patched}"
    );
}

/// Same answer as somebody else's share, on purpose — telling the two apart
/// would confirm which codes exist.
#[tokio::test]
async fn patching_without_the_token_is_not_found() {
    let app = TestApp::ready().await;
    let share = upload(&app, "", b"hello").await;
    let code = share["code"].as_str().expect("a code").to_owned();

    for token in ["", "not-the-token"] {
        assert_eq!(
            app.send(
                Request::builder()
                    .method(Method::PATCH)
                    .uri(format!("/api/v1/shares/{code}"))
                    .header("content-type", "application/json")
                    .header("x-share-management-token", token),
                Body::from(r#"{"max_uses": 9}"#),
            )
            .await
            .status(),
            StatusCode::NOT_FOUND,
            "`{token}` should not be able to change somebody else's share"
        );
    }
}

/// The whole reason the body uses a three-state field. An omitted key and an
/// explicit null are opposite instructions, and `Option` alone cannot tell
/// them apart — which would silently clear settings the caller never mentioned.
#[tokio::test]
async fn an_omitted_field_is_left_alone_and_null_clears_it() {
    let app = TestApp::ready().await;
    let share = upload(&app, "?max_uses=3", b"hello").await;
    let token = share["management_token"]
        .as_str()
        .expect("a token")
        .to_owned();
    let code = share["code"].as_str().expect("a code").to_owned();

    let patch = async |json: &str| {
        body_json(
            expect_status(
                app.send(
                    Request::builder()
                        .method(Method::PATCH)
                        .uri(format!("/api/v1/shares/{code}"))
                        .header("content-type", "application/json")
                        .header("x-share-management-token", &token),
                    Body::from(json.to_owned()),
                )
                .await,
                StatusCode::OK,
            )
            .await,
        )
        .await
    };

    // Mentions only the expiry. The use limit must survive.
    let after = patch(r#"{"expires_in_hours": 12}"#).await;
    assert_eq!(
        after["uses_remaining"], 3,
        "an omitted field must not be cleared: {after}"
    );

    // Now say so explicitly.
    let after = patch(r#"{"max_uses": null}"#).await;
    assert!(
        after["uses_remaining"].is_null(),
        "an explicit null should clear the limit: {after}"
    );
}

/// Rotating a password is the case this endpoint exists for that a delete and
/// re-upload cannot serve: the code stays the same, so the link already sent
/// keeps working for whoever is meant to have it.
#[tokio::test]
async fn a_password_can_be_added_then_removed() {
    let app = TestApp::ready().await;
    let share = upload(&app, "", b"hello").await;
    let token = share["management_token"]
        .as_str()
        .expect("a token")
        .to_owned();
    let code = share["code"].as_str().expect("a code").to_owned();

    let describe = async || {
        body_json(
            expect_status(
                app.empty(Method::GET, &format!("/api/v1/shares/{code}"), Auth::None)
                    .await,
                StatusCode::OK,
            )
            .await,
        )
        .await
    };
    assert_eq!(describe().await["has_password"], false);

    expect_status(
        app.send(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/shares/{code}"))
                .header("content-type", "application/json")
                .header("x-share-management-token", &token)
                .header("x-share-password", "a perfectly adequate passphrase"),
            Body::from("{}"),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        describe().await["has_password"],
        true,
        "the share should now be behind a password"
    );

    expect_status(
        app.send(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/shares/{code}"))
                .header("content-type", "application/json")
                .header("x-share-management-token", &token),
            Body::from(r#"{"remove_password": true}"#),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(describe().await["has_password"], false);
}

/// Two contradictory instructions in one request. Guessing which one was meant
/// is how a share ends up with a password nobody intended, or without one
/// somebody was relying on.
#[tokio::test]
async fn setting_and_removing_a_password_at_once_is_refused() {
    let app = TestApp::ready().await;
    let share = upload(&app, "", b"hello").await;
    let token = share["management_token"]
        .as_str()
        .expect("a token")
        .to_owned();
    let code = share["code"].as_str().expect("a code").to_owned();

    assert_eq!(
        app.send(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/shares/{code}"))
                .header("content-type", "application/json")
                .header("x-share-management-token", &token)
                .header("x-share-password", "a perfectly adequate passphrase"),
            Body::from(r#"{"remove_password": true}"#),
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
}

/// The instance's ceiling is not something a caller can step outside by asking
/// for "no expiry" after the fact — that would be a way around a limit the
/// operator set, reached through a door that was never meant to open it.
#[tokio::test]
async fn the_expiry_cannot_be_cleared_when_the_instance_caps_it() {
    let app = TestApp::ready().await;
    let share = upload(&app, "", b"hello").await;
    let token = share["management_token"]
        .as_str()
        .expect("a token")
        .to_owned();
    let code = share["code"].as_str().expect("a code").to_owned();

    let refuse = async |json: &str| {
        app.send(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/shares/{code}"))
                .header("content-type", "application/json")
                .header("x-share-management-token", &token),
            Body::from(json.to_owned()),
        )
        .await
        .status()
    };

    assert_eq!(
        refuse(r#"{"expires_in_hours": null}"#).await,
        StatusCode::BAD_REQUEST
    );
    // The same ceiling that applies at creation.
    assert_eq!(
        refuse(r#"{"expires_in_hours": 100000}"#).await,
        StatusCode::BAD_REQUEST
    );
    // And zero uses is a share nobody can fetch, which is a delete spelled badly.
    assert_eq!(refuse(r#"{"max_uses": 0}"#).await, StatusCode::BAD_REQUEST);
}

/// The switch is the whole point. An instance that has not asked for a record
/// of its visitors must not be keeping one — anonymity is a reason people run
/// this, and a default that quietly logs would take that decision from them.
#[tokio::test]
async fn nothing_is_recorded_until_the_operator_asks() {
    let app = TestApp::ready().await;
    let share = upload(&app, "", b"hello").await;
    let code = share["code"].as_str().expect("a code").to_owned();
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");

    expect_status(
        app.get(&format!("/s/{code}/download")).await,
        StatusCode::OK,
    )
    .await;

    assert!(
        app.state
            .events
            .for_share(id, 10)
            .await
            .expect("query")
            .is_empty(),
        "the log is off, so a download should leave no trace of who made it"
    );
}

/// And when it is asked for, it has to actually answer the question the
/// operator turned it on for: who, and when.
#[tokio::test]
async fn a_download_is_recorded_once_the_log_is_on() {
    let app = TestApp::with_config(|config| {
        config.share.access_log.enabled = true;
    })
    .await;
    let share = upload(&app, "", b"hello").await;
    let code = share["code"].as_str().expect("a code").to_owned();
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");

    expect_status(
        app.send(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/s/{code}/download"))
                .header(header::USER_AGENT, "Tessera-Test/1.0"),
            Body::empty(),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    let events = app.state.events.for_share(id, 10).await.expect("query");
    assert_eq!(events.len(), 1, "one download, one row");
    assert_eq!(events[0].user_agent.as_deref(), Some("Tessera-Test/1.0"));
}

/// A `HEAD` is a link checker, a prefetch, or a chat client unfurling the URL.
/// None of those is a person collecting the file, which is why `HEAD` does not
/// spend a use either — the log follows the same line.
#[tokio::test]
async fn a_head_request_is_not_an_access() {
    let app = TestApp::with_config(|config| {
        config.share.access_log.enabled = true;
    })
    .await;
    let share = upload(&app, "", b"hello").await;
    let code = share["code"].as_str().expect("a code").to_owned();
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");

    expect_status(
        app.empty(Method::HEAD, &format!("/s/{code}/download"), Auth::None)
            .await,
        StatusCode::OK,
    )
    .await;

    assert!(
        app.state
            .events
            .for_share(id, 10)
            .await
            .expect("query")
            .is_empty(),
        "a HEAD asks what a GET would answer; it is not a retrieval"
    );
}

/// Switching the log off must not be the one action that keeps somebody's
/// records for ever. The retention sweep runs whatever the setting says.
#[tokio::test]
async fn the_sweep_clears_old_events_even_after_the_log_is_switched_off() {
    let app = TestApp::with_config(|config| {
        config.share.access_log.enabled = true;
        config.share.access_log.retention_days = 7;
    })
    .await;
    let share = upload(&app, "", b"hello").await;
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");

    let long_ago = OffsetDateTime::now_utc() - time::Duration::days(30);
    app.state
        .events
        .record(id, long_ago, Some("203.0.113.9"), Some("old"))
        .await
        .expect("record");
    assert_eq!(
        app.state
            .events
            .for_share(id, 10)
            .await
            .expect("query")
            .len(),
        1
    );

    tessera_server::maintenance::sweep_once(&app.state, OffsetDateTime::now_utc()).await;

    assert!(
        app.state
            .events
            .for_share(id, 10)
            .await
            .expect("query")
            .is_empty(),
        "an event past its retention should have been swept"
    );
}

/// A visitor being recorded should be told, and `/capabilities` is the only
/// place a client can find out that they are. Without this the frontend has no
/// way to know, and the person being written down is the last to hear about it.
#[tokio::test]
async fn capabilities_says_whether_visitors_are_being_recorded() {
    let quiet = TestApp::ready().await;
    let body =
        body_json(expect_status(quiet.get("/api/v1/capabilities").await, StatusCode::OK).await)
            .await;
    assert_eq!(body["access_log"], false, "off by default");

    let recording = TestApp::with_config(|config| {
        config.share.access_log.enabled = true;
    })
    .await;
    let body =
        body_json(expect_status(recording.get("/api/v1/capabilities").await, StatusCode::OK).await)
            .await;
    assert_eq!(body["access_log"], true);
}

/// The promise the settings table makes: what the panel stores takes effect
/// without a restart. This is the mechanism that has to hold for that to be
/// true — swap the configuration, and the next request sees the new value.
#[tokio::test]
async fn replacing_the_configuration_reaches_the_next_request() {
    let app = TestApp::ready().await;

    // Anonymous upload is on, so this succeeds.
    upload(&app, "", b"hello").await;

    let mut next: tessera_server::config::Config = (**app.state.config()).clone();
    next.features.anonymous_upload = false;
    app.state.replace_config(std::sync::Arc::new(next));

    assert_eq!(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares"),
            Body::from(b"again".to_vec()),
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED,
        "the change should apply to the very next request, with no restart"
    );
}

/// Everything built from a setting has to be rebuilt with it. A swap that left
/// the throttles on the old limits would be a change the panel reported as
/// saved and that did nothing — the exact failure the table exists to avoid.
#[tokio::test]
async fn replacing_the_configuration_rebuilds_what_was_derived_from_it() {
    let app = TestApp::ready().await;

    let mut next: tessera_server::config::Config = (**app.state.config()).clone();
    next.auth.throttle.lookup.max_attempts = 1;
    next.server.trusted_proxies = vec!["10.0.0.0/8".to_owned()];
    app.state.replace_config(std::sync::Arc::new(next));

    // The throttle took the new limit without being rebuilt.
    app.state.throttles.lookup.record_failure("1.2.3.4");
    assert!(
        matches!(
            app.state.throttles.lookup.check("1.2.3.4"),
            tessera_server::auth::throttle::Decision::Deny { .. }
        ),
        "the lookup throttle should be using the limit that was just set"
    );

    // And the proxy list was rebuilt from the new value: a request arriving
    // from inside 10.0.0.0/8 is now believed about the address it forwards.
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("x-forwarded-for", "203.0.113.9".parse().expect("a header"));
    let origin = app
        .state
        .proxies()
        .resolve(Some("10.0.0.1".parse().expect("an address")), &headers);
    assert_eq!(
        origin.source, "203.0.113.9",
        "the trusted-proxy list should have followed the swap"
    );
}

/// A ticket is a string, and handing it to somebody is easier than handing them
/// ten gigabytes. A fixed lifetime has to be long enough for the slowest link
/// to finish the largest file — which is exactly long enough for the ticket to
/// be worth passing on. Using it renews it instead.
#[tokio::test]
async fn using_a_ticket_renews_it() {
    let app = TestApp::with_config(|config| {
        config.share.ticket_idle_hours = 2;
        config.share.ticket_ttl_hours = 24;
    })
    .await;

    let share = upload(&app, "?max_uses=1", b"content").await;
    let code = share["code"].as_str().expect("a code").to_owned();
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");

    // Unlock spends the single use and hands back the ticket.
    let secret = unlock(&app, &code).await;
    expect_status(
        app.get(&format!("/s/{code}/download?t={secret}")).await,
        StatusCode::OK,
    )
    .await;

    let ticket = app
        .state
        .tickets
        .for_share(id)
        .await
        .expect("query")
        .into_iter()
        .next()
        .expect("a ticket should have been issued");
    let first = ticket.expires_at;

    // It starts on the idle window, not the ceiling — otherwise renewing it
    // would never have anything to do.
    assert!(
        first < OffsetDateTime::now_utc() + time::Duration::hours(3),
        "a fresh ticket should carry the idle window, not the whole ttl: {first}"
    );

    // Presenting it again pushes the expiry out.
    expect_status(
        app.get(&format!("/s/{code}/download?t={secret}")).await,
        StatusCode::OK,
    )
    .await;
    let renewed = app
        .state
        .tickets
        .for_share(id)
        .await
        .expect("query")
        .into_iter()
        .next()
        .expect("still there")
        .expires_at;
    assert!(
        renewed > first,
        "using it should have renewed it: {first} -> {renewed}"
    );
}

/// Renewal cannot go on for ever, or a ticket kept warm on purpose never dies.
#[tokio::test]
async fn renewal_stops_at_the_ceiling() {
    let app = TestApp::with_config(|config| {
        config.share.ticket_idle_hours = 24;
        // A ceiling below the idle window, so the very first renewal is capped.
        config.share.ticket_ttl_hours = 1;
    })
    .await;

    let share = upload(&app, "?max_uses=1", b"content").await;
    let code = share["code"].as_str().expect("a code").to_owned();
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");

    let secret = unlock(&app, &code).await;
    for _ in 0..2 {
        expect_status(
            app.get(&format!("/s/{code}/download?t={secret}")).await,
            StatusCode::OK,
        )
        .await;
    }

    let ticket = app
        .state
        .tickets
        .for_share(id)
        .await
        .expect("query")
        .into_iter()
        .next()
        .expect("a ticket");
    assert!(
        ticket.expires_at <= ticket.created_at + time::Duration::hours(1),
        "the ceiling is created_at + ticket_ttl_hours, whatever the idle window says"
    );
}

/// `user_id IS NULL` means two things — nobody was signed in, and the account
/// that uploaded it has since been deleted. So it cannot answer "was this
/// anonymous". The role can.
#[tokio::test]
async fn an_anonymous_upload_is_charged_to_guest() {
    let app = TestApp::ready().await;
    let share = upload(&app, "", b"nobody").await;
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");

    let stored = app
        .state
        .shares
        .find_by_id(id)
        .await
        .expect("query")
        .expect("the share");
    assert!(stored.user_id.is_none());
    assert_eq!(
        stored.role_id.map(|role| role.to_string()),
        Some(tessera_core::RoleId::GUEST.to_owned()),
        "an anonymous upload should carry the guest role, not nothing"
    );
}

/// The role is the uploader's, so a signed-in upload counts against theirs.
#[tokio::test]
async fn a_signed_in_upload_is_charged_to_their_role() {
    let app = TestApp::ready().await;

    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": "uploader",
                "email": "uploader@example.com",
                "password": "a perfectly adequate passphrase",
            }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let cookie = common::session_cookie(&response);

    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/shares")
                    .header(header::COOKIE, &cookie),
                Body::from(b"mine".to_vec()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let id = tessera_core::ShareId::parse(created["id"].as_str().expect("an id")).expect("an id");

    let stored = app
        .state
        .shares
        .find_by_id(id)
        .await
        .expect("query")
        .expect("the share");
    // The first account on an instance owns it.
    assert_eq!(
        stored.role_id.map(|role| role.to_string()),
        Some(tessera_core::RoleId::OWNER.to_owned())
    );
}

/// A snapshot, not a pointer. Moving somebody to another role must not rewrite
/// which quota their old shares counted against — one change to one account
/// would otherwise silently redistribute storage already accounted for.
#[tokio::test]
async fn changing_someones_role_does_not_rewrite_their_old_shares() {
    let app = TestApp::ready().await;
    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": "mover",
                "email": "mover@example.com",
                "password": "a perfectly adequate passphrase",
            }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let cookie = common::session_cookie(&response);

    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/shares")
                    .header(header::COOKIE, &cookie),
                Body::from(b"before".to_vec()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let id = tessera_core::ShareId::parse(created["id"].as_str().expect("an id")).expect("an id");

    let user = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("mover").expect("a name"))
        .await
        .expect("query")
        .expect("the account");
    app.state
        .roles
        .set_role_of(
            user.id,
            Some(tessera_core::RoleId::parse(tessera_core::RoleId::MEMBER).expect("a role")),
            OffsetDateTime::now_utc(),
        )
        .await
        .expect("move them");

    let stored = app
        .state
        .shares
        .find_by_id(id)
        .await
        .expect("query")
        .expect("the share");
    assert_eq!(
        stored.role_id.map(|role| role.to_string()),
        Some(tessera_core::RoleId::OWNER.to_owned()),
        "the share should still name the role its uploader held at the time"
    );
}

// ── Tiered quotas ────────────────────────────────────────────────────────
//
// Three ceilings can refuse an upload — the account's own, its role's, and the
// instance's — and which one refused decides who can do anything about it.

/// Registers an account and returns its session cookie and id.
#[expect(
    clippy::expect_used,
    reason = "test fixture; a panic is how it reports"
)]
async fn member(app: &TestApp, name: &str) -> (String, tessera_core::UserId) {
    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": name,
                "email": format!("{name}@example.com"),
                "password": "a perfectly adequate passphrase",
            }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let cookie = common::session_cookie(&response);
    let id = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse(name).expect("a username"))
        .await
        .expect("query")
        .expect("the account")
        .id;
    (cookie, id)
}

/// Posts a body as a signed-in account and returns the response.
async fn upload_as(
    app: &TestApp,
    cookie: &str,
    query: &str,
    body: &[u8],
) -> axum::http::Response<Body> {
    app.send(
        Request::builder()
            .method(Method::POST)
            .uri(format!("/api/v1/shares{query}"))
            .header(header::COOKIE, cookie),
        Body::from(body.to_vec()),
    )
    .await
}

/// Sets a role's quota by name.
#[expect(
    clippy::expect_used,
    reason = "test fixture; a panic is how it reports"
)]
async fn set_role_quota(app: &TestApp, name: &str, bytes: Option<u64>) {
    let role = app
        .state
        .roles
        .find_by_name(name)
        .await
        .expect("query")
        .expect("the role");
    app.state
        .roles
        .set_quota(role.id, bytes, OffsetDateTime::now_utc())
        .await
        .expect("store the quota");
}

/// The `message` of a refusal, which is what a client shows.
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test fixture; a panic is how it reports"
)]
async fn refusal_message(response: axum::http::Response<Body>) -> String {
    body_json(response).await["message"]
        .as_str()
        .expect("a message")
        .to_owned()
}

/// The whole point of the role layer: an instance can hold a hundred gigabytes
/// and still tell one member they have had their share of it.
#[tokio::test]
async fn a_role_quota_refuses_before_the_instance_does() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.storage.max_total_bytes = 1_000_000;
    })
    .await;
    // The first account is the owner, so a second one is the member this is
    // about.
    let (_, _) = member(&app, "owner").await;
    set_role_quota(&app, "member", Some(4096)).await;
    let (cookie, _) = member(&app, "alice").await;

    expect_status(
        upload_as(&app, &cookie, "?filename=first.bin", &vec![1_u8; 3000]).await,
        StatusCode::CREATED,
    )
    .await;

    let refused = upload_as(&app, &cookie, "?filename=second.bin", &vec![2_u8; 2000]).await;
    assert_eq!(refused.status(), StatusCode::INSUFFICIENT_STORAGE);
    // Naming the tier is the point: "the instance is full" would send them to
    // the operator, and the operator has 996 kB free.
    assert_eq!(
        refusal_message(refused).await,
        "the storage quota for your role is full"
    );
}

/// The account layer exists so that raising one person does not mean inventing
/// a role for them.
#[tokio::test]
async fn an_account_quota_beats_its_role() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.storage.max_total_bytes = 1_000_000;
    })
    .await;
    let (_, _) = member(&app, "owner").await;
    set_role_quota(&app, "member", Some(4096)).await;
    let (cookie, id) = member(&app, "alice").await;
    app.state
        .users
        .set_quota(id, Some(20_000), OffsetDateTime::now_utc())
        .await
        .expect("store the quota");

    // 6000 is past the role's 4096 and inside the account's 20000.
    expect_status(
        upload_as(&app, &cookie, "?filename=big.bin", &vec![1_u8; 6000]).await,
        StatusCode::CREATED,
    )
    .await;
}

/// Nearest wins in both directions, or the account layer would only ever be a
/// way to grant more.
#[tokio::test]
async fn an_account_quota_can_also_lower_one() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.storage.max_total_bytes = 1_000_000;
    })
    .await;
    let (_, _) = member(&app, "owner").await;
    set_role_quota(&app, "member", Some(100_000)).await;
    let (cookie, id) = member(&app, "alice").await;
    app.state
        .users
        .set_quota(id, Some(2048), OffsetDateTime::now_utc())
        .await
        .expect("store the quota");

    let refused = upload_as(&app, &cookie, "?filename=big.bin", &vec![1_u8; 4000]).await;
    assert_eq!(refused.status(), StatusCode::INSUFFICIENT_STORAGE);
    assert_eq!(refusal_message(refused).await, "your storage quota is full");
}

/// `0` says "no ceiling here", which is not permission to fill the disk — the
/// person who sets a role is often not the person who owns the machine.
#[tokio::test]
async fn no_ceiling_of_its_own_still_meets_the_instance() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.storage.max_total_bytes = 8192;
    })
    .await;
    let (_, _) = member(&app, "owner").await;
    set_role_quota(&app, "member", Some(0)).await;
    let (cookie, _) = member(&app, "alice").await;

    expect_status(
        upload_as(&app, &cookie, "?filename=first.bin", &vec![1_u8; 6000]).await,
        StatusCode::CREATED,
    )
    .await;
    let refused = upload_as(&app, &cookie, "?filename=second.bin", &vec![2_u8; 4000]).await;
    assert_eq!(refused.status(), StatusCode::INSUFFICIENT_STORAGE);
    assert_eq!(
        refusal_message(refused).await,
        "this instance has no room for more content"
    );
}

/// Clearing an account's figure has to put it back on its role's, or `null`
/// would be a third state nobody can get out of.
#[tokio::test]
async fn clearing_an_account_quota_falls_back_to_the_role() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.storage.max_total_bytes = 1_000_000;
    })
    .await;
    let (_, _) = member(&app, "owner").await;
    set_role_quota(&app, "member", Some(4096)).await;
    let (cookie, id) = member(&app, "alice").await;
    let now = OffsetDateTime::now_utc();
    app.state
        .users
        .set_quota(id, Some(100_000), now)
        .await
        .expect("store the quota");
    app.state
        .users
        .set_quota(id, None, now)
        .await
        .expect("clear the quota");

    let refused = upload_as(&app, &cookie, "?filename=big.bin", &vec![1_u8; 6000]).await;
    assert_eq!(refused.status(), StatusCode::INSUFFICIENT_STORAGE);
    assert_eq!(
        refusal_message(refused).await,
        "the storage quota for your role is full"
    );
}

/// Anonymous work is charged to the guest role, which is how "let strangers
/// upload, but not more than this much of it" is sayable at all.
#[tokio::test]
async fn the_guest_role_bounds_anonymous_uploads() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.storage.max_total_bytes = 0;
    })
    .await;
    set_role_quota(&app, "guest", Some(4096)).await;

    upload(&app, "?filename=first.bin", &vec![1_u8; 3000]).await;

    let refused = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=second.bin"),
            Body::from(vec![2_u8; 2000]),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::INSUFFICIENT_STORAGE);

    // A member is untouched by it: the guest role's shares are the guest role's.
    let (_, _) = member(&app, "owner").await;
    let (cookie, _) = member(&app, "alice").await;
    expect_status(
        upload_as(&app, &cookie, "?filename=mine.bin", &vec![3_u8; 9000]).await,
        StatusCode::CREATED,
    )
    .await;
}

/// The tier that supplies the ceiling has to supply the denominator too. Read
/// against the instance's total, one member filling their allowance would
/// refuse every other member at their first upload.
#[tokio::test]
async fn one_accounts_usage_does_not_spend_anothers() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.storage.max_total_bytes = 1_000_000;
    })
    .await;
    let (_, _) = member(&app, "owner").await;
    let (alice, alice_id) = member(&app, "alice").await;
    let (bob, bob_id) = member(&app, "bob").await;
    let now = OffsetDateTime::now_utc();
    for id in [alice_id, bob_id] {
        app.state
            .users
            .set_quota(id, Some(8192), now)
            .await
            .expect("store the quota");
    }

    expect_status(
        upload_as(&app, &alice, "?filename=hers.bin", &vec![1_u8; 7000]).await,
        StatusCode::CREATED,
    )
    .await;
    expect_status(
        upload_as(&app, &bob, "?filename=his.bin", &vec![2_u8; 7000]).await,
        StatusCode::CREATED,
    )
    .await;
}

/// A client that knows the figure can say so before somebody picks a file
/// rather than after they have sent it.
#[tokio::test]
async fn capabilities_tells_an_account_its_own_room() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.storage.max_total_bytes = 1_000_000;
    })
    .await;
    let (_, _) = member(&app, "owner").await;
    set_role_quota(&app, "member", Some(4096)).await;
    let (cookie, _) = member(&app, "alice").await;

    expect_status(
        upload_as(&app, &cookie, "?filename=first.bin", &vec![1_u8; 3000]).await,
        StatusCode::CREATED,
    )
    .await;

    let capabilities = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/capabilities")
                    .header(header::COOKIE, &cookie),
                Body::empty(),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    // Their own remaining, not the instance's 997 kB — which they may not read.
    assert_eq!(capabilities["storage_remaining_bytes"], 1096);
}

/// An administrator with a quota of their own would otherwise be shown the
/// disk's free space and then refused at four kilobytes.
#[tokio::test]
async fn capabilities_shows_an_administrator_the_tighter_of_the_two() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.storage.max_total_bytes = 1_000_000;
    })
    .await;
    // The first account is the owner, which is what carries `stats:read`.
    let (cookie, id) = member(&app, "owner").await;
    app.state
        .users
        .set_quota(id, Some(4096), OffsetDateTime::now_utc())
        .await
        .expect("store the quota");

    let capabilities = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/capabilities")
                    .header(header::COOKIE, &cookie),
                Body::empty(),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(
        capabilities["storage_remaining_bytes"], 4096,
        "an administrator was shown the disk rather than their own ceiling"
    );
}

// ── 孤兒物件對帳 ──────────────────────────────────────────────────────────
//
// Everything else the sweep does deletes what the database knows about. This
// walks the store instead, so the tests that matter are the ones about what it
// must *not* touch.

/// Writes an object straight into the store, the way a crashed upload leaves one.
#[expect(
    clippy::expect_used,
    reason = "test fixture; a panic is how it reports"
)]
async fn put_object(app: &TestApp, key: &str, bytes: &[u8]) {
    app.state
        .storage
        .put(
            &object_store::path::Path::from(key),
            object_store::PutPayload::from(bytes.to_vec()),
        )
        .await
        .expect("write the object");
}

/// Whether the store still holds a key.
async fn object_exists(app: &TestApp, key: &str) -> bool {
    app.state
        .storage
        .head(&object_store::path::Path::from(key))
        .await
        .is_ok()
}

/// The whole point: bytes nothing points at are invisible to every total and
/// occupy real space for ever.
#[tokio::test]
async fn an_object_no_row_names_is_removed() {
    let app = TestApp::with_config(|config| config.storage.reconcile_grace_hours = 1).await;
    put_object(&app, "shares/2026/01/orphan", b"leaked").await;

    // An hour on, it is past the grace period.
    tessera_server::maintenance::reconcile_once(
        &app.state,
        OffsetDateTime::now_utc() + time::Duration::hours(2),
    )
    .await;

    assert!(!object_exists(&app, "shares/2026/01/orphan").await);
}

/// The bytes are written before the row, so for a moment every upload looks
/// like an orphan. Deleting inside that window deletes somebody's file.
#[tokio::test]
async fn a_freshly_written_object_is_left_alone() {
    let app = TestApp::with_config(|config| config.storage.reconcile_grace_hours = 24).await;
    put_object(&app, "shares/2026/01/just-arrived", b"mine").await;

    tessera_server::maintenance::reconcile_once(&app.state, OffsetDateTime::now_utc()).await;

    assert!(
        object_exists(&app, "shares/2026/01/just-arrived").await,
        "deleted an object inside the grace period"
    );
}

/// A real share's object is spoken for, however old it is.
#[tokio::test]
async fn a_live_shares_object_survives() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 8;
        config.storage.reconcile_grace_hours = 0;
    })
    .await;
    let share = upload(&app, "?filename=kept.bin", &vec![7_u8; 4096]).await;
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("parses");
    let key = first_key(&content_of(&app, id).await)
        .expect("an object key")
        .to_owned();

    tessera_server::maintenance::reconcile_once(
        &app.state,
        OffsetDateTime::now_utc() + time::Duration::days(30),
    )
    .await;

    assert!(
        object_exists(&app, &key).await,
        "deleted a live share's bytes"
    );
}

/// A bucket is often shared with something else. Listing from the root and
/// deleting what our database does not recognise would be a different feature.
#[tokio::test]
async fn objects_outside_our_prefix_are_never_touched() {
    let app = TestApp::with_config(|config| config.storage.reconcile_grace_hours = 0).await;
    put_object(&app, "somebody-elses/backup.tar", b"not ours").await;
    put_object(&app, "shares/2026/01/ours", b"ours").await;

    tessera_server::maintenance::reconcile_once(
        &app.state,
        OffsetDateTime::now_utc() + time::Duration::days(1),
    )
    .await;

    assert!(
        object_exists(&app, "somebody-elses/backup.tar").await,
        "deleted something outside the prefix this instance owns"
    );
    assert!(!object_exists(&app, "shares/2026/01/ours").await);
}

/// An open upload has an object but no share yet, so it looks exactly like an
/// orphan. Deleting it takes the bytes out from under somebody mid-upload.
#[tokio::test]
async fn an_in_flight_uploads_object_survives() {
    let app = TestApp::with_config(|config| config.storage.reconcile_grace_hours = 0).await;

    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/uploads?filename=half.bin")
                    .header("upload-length", "4096"),
                Body::empty(),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let id = tessera_core::UploadId::parse(created["id"].as_str().expect("an id")).expect("parses");
    let key = app
        .state
        .uploads
        .find(id)
        .await
        .expect("query")
        .expect("the session")
        .object_key;

    // The local sink writes beside the key while the upload is open.
    put_object(&app, &key, b"partial").await;

    tessera_server::maintenance::reconcile_once(
        &app.state,
        OffsetDateTime::now_utc() + time::Duration::days(7),
    )
    .await;

    assert!(
        object_exists(&app, &key).await,
        "deleted the bytes of an upload still in progress"
    );
}

// ── 單檔上限的分層 ────────────────────────────────────────────────────────

/// Sets a role's single-file limit by name.
#[expect(
    clippy::expect_used,
    reason = "test fixture; a panic is how it reports"
)]
async fn set_role_file_limit(app: &TestApp, name: &str, bytes: Option<u64>) {
    let role = app
        .state
        .roles
        .find_by_name(name)
        .await
        .expect("query")
        .expect("the role");
    app.state
        .roles
        .set_max_file_bytes(role.id, bytes, OffsetDateTime::now_utc())
        .await
        .expect("store the limit");
}

/// The whole point: an operator raises the instance figure so *they* can send a
/// large file, and every stranger who can reach the instance is raised with them.
#[tokio::test]
async fn a_role_can_be_held_below_the_instance_limit() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.server.max_body_bytes = 1_000_000;
    })
    .await;
    let (_, _) = member(&app, "owner").await;
    set_role_file_limit(&app, "member", Some(4096)).await;
    let (cookie, _) = member(&app, "alice").await;

    expect_status(
        upload_as(&app, &cookie, "?filename=small.bin", &vec![1_u8; 3000]).await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(
        upload_as(&app, &cookie, "?filename=big.bin", &vec![2_u8; 8000])
            .await
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE,
    );
}

/// Nearest wins in both directions, so one person can be raised without
/// inventing a role for them.
#[tokio::test]
async fn an_account_limit_beats_its_role() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.server.max_body_bytes = 1_000_000;
    })
    .await;
    let (_, _) = member(&app, "owner").await;
    set_role_file_limit(&app, "member", Some(4096)).await;
    let (cookie, id) = member(&app, "alice").await;
    app.state
        .users
        .set_max_file_bytes(id, Some(500_000), OffsetDateTime::now_utc())
        .await
        .expect("store the limit");

    expect_status(
        upload_as(&app, &cookie, "?filename=big.bin", &vec![1_u8; 8000]).await,
        StatusCode::CREATED,
    )
    .await;
}

/// `0` says "no limit here", which is not permission to exceed the body limit —
/// the layer that enforces that never sees these figures.
#[tokio::test]
async fn no_limit_of_its_own_still_meets_the_instance() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.server.max_body_bytes = 4096;
    })
    .await;
    let (_, _) = member(&app, "owner").await;
    set_role_file_limit(&app, "member", Some(0)).await;
    let (cookie, _) = member(&app, "alice").await;

    assert_eq!(
        upload_as(&app, &cookie, "?filename=big.bin", &vec![1_u8; 8000])
            .await
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE,
    );
}

/// Anonymous uploads are charged to the guest role, which is how "let strangers
/// upload, but nothing enormous" becomes sayable.
#[tokio::test]
async fn the_guest_role_bounds_anonymous_file_size() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 16;
        config.server.max_body_bytes = 1_000_000;
    })
    .await;
    set_role_file_limit(&app, "guest", Some(4096)).await;

    let refused = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=big.bin"),
            Body::from(vec![1_u8; 8000]),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::PAYLOAD_TOO_LARGE);

    // A member still gets the instance figure.
    let (_, _) = member(&app, "owner").await;
    let (cookie, _) = member(&app, "alice").await;
    expect_status(
        upload_as(&app, &cookie, "?filename=mine.bin", &vec![2_u8; 8000]).await,
        StatusCode::CREATED,
    )
    .await;
}

/// A client shown a number bigger than what it may send lets somebody pick a
/// file and then fail.
#[tokio::test]
async fn capabilities_reports_the_callers_own_file_limit() {
    let app = TestApp::with_config(|config| config.server.max_body_bytes = 1_000_000).await;
    let (_, _) = member(&app, "owner").await;
    set_role_file_limit(&app, "member", Some(4096)).await;
    let (cookie, _) = member(&app, "alice").await;

    let capabilities = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/capabilities")
                    .header(header::COOKIE, &cookie),
                Body::empty(),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(capabilities["max_upload_bytes"], 4096);
}

// ── 每個檔案記自己在哪個 backend ──────────────────────────────────────────

/// The point of the whole change: without this, pointing the configuration at
/// another store leaves every existing share reading as present and failing to
/// deliver, with nothing recording where the bytes went.
#[tokio::test]
async fn a_file_records_which_backend_holds_it() {
    let app = TestApp::with_config(|config| config.share.inline_threshold_bytes = 8).await;
    let share = upload(&app, "?filename=kept.bin", &vec![3_u8; 4096]).await;
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("parses");

    let content = content_of(&app, id).await;
    let file = content.files().first().expect("a file");
    assert_eq!(
        file.storage.backend(),
        Some(tessera_server::storage::DEFAULT_BACKEND),
    );
}

/// Reads have to follow the name the file recorded, not whichever store new
/// uploads go to — otherwise recording it bought nothing.
///
/// Asserting on the status alone is not enough: the lookup can happen, its
/// result be thrown away, and the response still look right. So the recorded
/// name is pointed at an *empty* store — if the bytes still arrive, they came
/// from somewhere this file never named.
#[tokio::test]
async fn a_download_reads_from_the_backend_the_file_named() {
    let app = TestApp::with_config(|config| config.share.inline_threshold_bytes = 8).await;
    let share = upload(&app, "?filename=kept.bin", &vec![5_u8; 4096]).await;
    let code = share["code"].as_str().expect("a code").to_owned();

    expect_status(
        app.get(&format!("/s/{code}/download")).await,
        StatusCode::OK,
    )
    .await;

    // Same name, nothing behind it.
    let empty: std::sync::Arc<dyn object_store::ObjectStore> =
        std::sync::Arc::new(object_store::memory::InMemory::new());
    app.state.replace_backends(tessera_server::storage::Pool {
        open: tessera_server::storage::Backends::from([(
            tessera_server::storage::DEFAULT_BACKEND.to_owned(),
            tessera_server::storage::Stores::plain(empty),
        )]),
        configured: Vec::new(),
    });

    assert_ne!(
        app.get(&format!("/s/{code}/download")).await.status(),
        StatusCode::OK,
        "served bytes from a store this file does not name"
    );
}

/// A backend dropped from the configuration is a recoverable operator mistake:
/// the row is fine and the bytes are probably fine, what is missing is the
/// credentials. `404` would say the share never existed.
#[tokio::test]
async fn an_unconfigured_backend_is_unavailable_not_missing() {
    let app = TestApp::with_config(|config| config.share.inline_threshold_bytes = 8).await;
    let share = upload(&app, "?filename=kept.bin", &vec![9_u8; 4096]).await;
    let code = share["code"].as_str().expect("a code").to_owned();

    app.state
        .replace_backends(tessera_server::storage::Pool::default());

    assert_eq!(
        app.get(&format!("/s/{code}/download")).await.status(),
        StatusCode::SERVICE_UNAVAILABLE,
    );
}

// ── 存在 DB 裡的 backend ──────────────────────────────────────────────────

/// The credentials are the reason this is not a settings row. A dump that hands
/// over the buckets is the failure an operator can do nothing about afterwards.
#[tokio::test]
async fn a_stored_backend_keeps_its_credentials_out_of_the_row() {
    use tessera_server::{
        db::repository::storage_backend::{StorageBackend, StorageBackendRepository},
        storage::secret::Sealer,
    };

    let app = TestApp::ready().await;
    let key = Sealer::new(&"ab".repeat(32)).expect("a key");
    let store = StorageBackendRepository::new(app.state.db.clone(), key);

    store
        .put(
            &StorageBackend {
                name: "cold".to_owned(),
                url: "s3://bucket-b?aws_secret_access_key=hunter2".to_owned(),
                options: std::collections::BTreeMap::from([(
                    "aws_region".to_owned(),
                    "garage".to_owned(),
                )]),
                is_default: false,
                capacity_bytes: None,
                read_only: false,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .expect("store the backend");

    // What actually landed in the table.
    let row = app
        .state
        .db
        .query_one_raw(sea_orm::Statement::from_string(
            app.state.db.get_database_backend(),
            "SELECT url, options FROM storage_backends",
        ))
        .await
        .expect("query")
        .expect("a row");
    let url: String = row.try_get("", "url").expect("url");
    let options: String = row.try_get("", "options").expect("options");
    assert!(
        !url.contains("hunter2"),
        "the key is in the table in the clear"
    );
    assert!(!url.contains("bucket-b"));
    assert!(!options.contains("garage"));

    // And it comes back whole for the process that has the key.
    let opened = store.all().await.expect("read back");
    let cold = opened.first().expect("the backend");
    assert_eq!(cold.url, "s3://bucket-b?aws_secret_access_key=hunter2");
    assert_eq!(
        cold.options.get("aws_region").map(String::as_str),
        Some("garage")
    );
}

/// A key that changed has to say so. Read as corruption it sends an operator to
/// look at the rows, which are fine.
#[tokio::test]
async fn another_key_cannot_open_a_stored_backend() {
    use tessera_server::{
        db::repository::storage_backend::{StorageBackend, StorageBackendRepository},
        storage::secret::Sealer,
    };

    let app = TestApp::ready().await;
    StorageBackendRepository::new(
        app.state.db.clone(),
        Sealer::new(&"ab".repeat(32)).expect("a key"),
    )
    .put(
        &StorageBackend {
            name: "cold".to_owned(),
            url: "s3://bucket-b".to_owned(),
            options: std::collections::BTreeMap::new(),
            is_default: false,
            capacity_bytes: None,
            read_only: false,
        },
        OffsetDateTime::now_utc(),
    )
    .await
    .expect("store the backend");

    let wrong = StorageBackendRepository::new(
        app.state.db.clone(),
        Sealer::new(&"cd".repeat(32)).expect("a key"),
    );
    assert!(wrong.all().await.is_err());
}

/// Without a key, storing a recoverable credential would do the one thing the
/// sealing exists to prevent — and nobody would find out until the dump leaked.
#[tokio::test]
async fn without_a_key_a_backend_cannot_be_stored() {
    use tessera_server::{
        db::repository::storage_backend::{StorageBackend, StorageBackendRepository},
        storage::secret::Sealer,
    };

    let app = TestApp::ready().await;
    let store =
        StorageBackendRepository::new(app.state.db.clone(), Sealer::new("").expect("no key"));

    assert!(
        store
            .put(
                &StorageBackend {
                    name: "cold".to_owned(),
                    url: "s3://bucket-b?aws_secret_access_key=hunter2".to_owned(),
                    options: std::collections::BTreeMap::new(),
                    is_default: false,
                    capacity_bytes: None,
                    read_only: false,
                },
                OffsetDateTime::now_utc(),
            )
            .await
            .is_err()
    );
}

/// Two defaults is a state nothing downstream knows how to read, and it is
/// reachable by two operators pressing the same button.
#[tokio::test]
async fn only_one_backend_takes_new_uploads() {
    use tessera_server::{
        db::repository::storage_backend::{StorageBackend, StorageBackendRepository},
        storage::secret::Sealer,
    };

    let app = TestApp::ready().await;
    let store = StorageBackendRepository::new(
        app.state.db.clone(),
        Sealer::new(&"ab".repeat(32)).expect("a key"),
    );
    let now = OffsetDateTime::now_utc();
    for name in ["hot", "cold"] {
        store
            .put(
                &StorageBackend {
                    name: name.to_owned(),
                    url: format!("s3://{name}"),
                    options: std::collections::BTreeMap::new(),
                    is_default: true,
                    capacity_bytes: None,
                    read_only: false,
                },
                now,
            )
            .await
            .expect("store");
    }

    assert!(store.set_default("cold", now).await.expect("set default"));

    let defaults: Vec<String> = store
        .all()
        .await
        .expect("read back")
        .into_iter()
        .filter(|backend| backend.is_default)
        .map(|backend| backend.name)
        .collect();
    assert_eq!(defaults, vec!["cold".to_owned()]);
}

// ── 容量與放置 ────────────────────────────────────────────────────────────

/// Registers a second, empty backend and returns the pool holding both.
fn pool_with_second(
    app: &TestApp,
    name: &str,
    capacity: Option<u64>,
) -> tessera_server::storage::Pool {
    use tessera_server::db::repository::storage_backend::StorageBackend;

    let second: std::sync::Arc<dyn object_store::ObjectStore> =
        std::sync::Arc::new(object_store::memory::InMemory::new());
    tessera_server::storage::Pool {
        open: tessera_server::storage::Backends::from([
            (
                tessera_server::storage::DEFAULT_BACKEND.to_owned(),
                tessera_server::storage::Stores::plain(std::sync::Arc::clone(&app.state.storage)),
            ),
            (
                name.to_owned(),
                tessera_server::storage::Stores::plain(second),
            ),
        ]),
        configured: vec![StorageBackend {
            name: name.to_owned(),
            url: "memory://".to_owned(),
            options: std::collections::BTreeMap::new(),
            is_default: false,
            capacity_bytes: capacity,
            read_only: false,
        }],
    }
}

/// The point of a capacity figure: the instance refuses before the provider
/// does. Hitting B2's cap is an upload that fails halfway with no explanation;
/// stopping short of it is a clean answer.
#[tokio::test]
async fn a_full_backend_hands_the_upload_to_another() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 8;
        // The default holds almost nothing, so the second backend has to take it.
        config.storage.capacity_bytes = 1024;
    })
    .await;
    app.state
        .replace_backends(pool_with_second(&app, "cold", None));

    let share = upload(&app, "?filename=big.bin", &vec![1_u8; 8192]).await;
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("parses");
    let content = content_of(&app, id).await;
    assert_eq!(
        content.files().first().expect("a file").storage.backend(),
        Some("cold"),
        "wrote to a backend that had no room for it"
    );
}

/// An operator who names a default meant it. Spreading away from it to balance
/// bytes would quietly undo a choice they made.
#[tokio::test]
async fn the_default_backend_keeps_taking_uploads_while_it_fits() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 8;
        config.storage.capacity_bytes = 1_000_000;
    })
    .await;
    app.state
        .replace_backends(pool_with_second(&app, "cold", None));

    let share = upload(&app, "?filename=small.bin", &vec![1_u8; 4096]).await;
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("parses");
    assert_eq!(
        content_of(&app, id)
            .await
            .files()
            .first()
            .expect("a file")
            .storage
            .backend(),
        Some(tessera_server::storage::DEFAULT_BACKEND),
    );
}

/// Nothing is split, so a pool with room spread thin still has to refuse — and
/// the refusal is a 507, not a 413: the file is not too big in itself.
#[tokio::test]
async fn a_file_bigger_than_every_backend_is_refused() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 8;
        config.storage.capacity_bytes = 2048;
    })
    .await;
    app.state
        .replace_backends(pool_with_second(&app, "cold", Some(2048)));

    // 4 KiB does not fit in either 2 KiB backend, though 4 KiB is free overall.
    let refused = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=too-wide.bin"),
            Body::from(vec![1_u8; 4096]),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::INSUFFICIENT_STORAGE);
}

/// Reporting the sum would tell a client it has 4 KiB when no single file over
/// 2 KiB can be accepted — a number that explains nothing when the upload is
/// refused.
#[tokio::test]
async fn capabilities_reports_the_largest_placeable_file_not_the_sum() {
    let app = TestApp::with_config(|config| config.storage.capacity_bytes = 2048).await;
    app.state
        .replace_backends(pool_with_second(&app, "cold", Some(2048)));

    let capabilities =
        body_json(expect_status(app.get("/api/v1/capabilities").await, StatusCode::OK).await).await;
    assert_eq!(capabilities["storage_remaining_bytes"], 2048);
}

/// Removing a backend that still holds content would make those shares
/// undeliverable, and there is no foreign key to stop it — `file_segments`
/// stores a name on purpose.
#[tokio::test]
async fn a_backend_holding_files_cannot_be_removed() {
    use tessera_server::{
        db::repository::storage_backend::{StorageBackend, StorageBackendRepository},
        storage::secret::Sealer,
    };

    let app = TestApp::with_config(|config| config.share.inline_threshold_bytes = 8).await;
    let store = StorageBackendRepository::new(
        app.state.db.clone(),
        Sealer::new(&"ab".repeat(32)).expect("a key"),
    );
    store
        .put(
            &StorageBackend {
                name: tessera_server::storage::DEFAULT_BACKEND.to_owned(),
                url: "memory://".to_owned(),
                options: std::collections::BTreeMap::new(),
                is_default: false,
                capacity_bytes: None,
                read_only: false,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .expect("store");

    // A share whose file records that backend.
    upload(&app, "?filename=kept.bin", &vec![1_u8; 4096]).await;

    assert!(
        store
            .remove(tessera_server::storage::DEFAULT_BACKEND)
            .await
            .is_err(),
        "removed a backend that still holds somebody's files"
    );
}

/// A file too large to write in one piece, on a backend that can only be
/// written in one piece.
///
/// The WebDAV backend is the real case: its only upload verb is a whole-body
/// `PUT`, so `put_multipart` is not implemented and a file past
/// `share.inline_threshold_bytes` used to reach the store, fail there, and
/// answer `500` with the body already half read. The placer has to decline
/// first, and say which of the two things went wrong — this is not "no room",
/// and adding room would not help.
#[tokio::test]
async fn a_streamed_file_is_refused_by_a_backend_that_cannot_take_one() {
    let app = TestApp::with_config(|config| config.share.inline_threshold_bytes = 8).await;

    // Room to spare, and no way to use it for a file this size.
    let store: std::sync::Arc<dyn object_store::ObjectStore> =
        std::sync::Arc::new(object_store::memory::InMemory::new());
    app.state.replace_backends(tessera_server::storage::Pool {
        open: tessera_server::storage::Backends::from([(
            tessera_server::storage::DEFAULT_BACKEND.to_owned(),
            tessera_server::storage::Stores::whole_body_only(store),
        )]),
        configured: Vec::new(),
    });

    let refused = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=big.bin"),
            Body::from(vec![7_u8; 4096]),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::PAYLOAD_TOO_LARGE);

    // The same backend still takes what it can write in one go, so this is a
    // refusal about shape rather than the backend being switched off.
    expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=small.bin"),
            Body::from(vec![7_u8; 4]),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
}

/// Uploads arriving together go through the gate that keeps the first write
/// into a new location alone.
///
/// The failure this guards is not visible on an object store, which has no
/// directories: it is WebDAV, where a write into a directory that does not
/// exist answers `409` and `object_store` retries after MKCOL exactly once —
/// not enough when several uploads are creating the same month at the same
/// moment. Measured as reliably flaky against `rclone serve webdav`.
///
/// What this checks is the wiring: that the write goes through
/// `FirstWrites::once` at all. That `once` serialises is its own unit test, and
/// the two together are the whole claim — an end-to-end version cannot be
/// written here because `nextest` gives every test its own process, so the
/// in-process gate never sees the sibling it exists to wait for.
#[tokio::test]
async fn concurrent_uploads_go_through_the_first_write_gate() {
    let app = TestApp::ready().await;

    let sending = (0..4).map(|i| {
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/shares?filename=together{i}.bin")),
            Body::from(vec![3_u8; 4096]),
        )
    });
    for response in futures_util::future::join_all(sending).await {
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let now = OffsetDateTime::now_utc();
    let location = format!("shares/{:04}/{:02}", now.year(), u8::from(now.month()));
    assert!(
        app.state
            .first_writes
            .knows(tessera_server::storage::DEFAULT_BACKEND, &location),
        "the upload path did not write through the first-write gate"
    );
}

/// A backend that fills up while the bytes are still arriving.
///
/// Every store the suite runs against has more room than any test uses, so this
/// path — the one where a partial object is left behind and the session has
/// recorded bytes that are not there — had never been exercised. `Brimming`
/// stops at a byte count the test chooses, which is what a bucket hitting its
/// quota does; a store that refused from the first byte would prove only that a
/// refusal is handled.
///
/// The share must not be created. A row saying "here is your file" whose bytes
/// stop halfway is worse than a refusal: the link works, the download is short,
/// and nothing says why.
#[tokio::test]
async fn a_backend_that_fills_up_mid_upload_does_not_leave_a_share_behind() {
    let app = TestApp::with_config(|config| config.share.inline_threshold_bytes = 8).await;

    // Room for a quarter of what is about to be sent.
    let brimming: std::sync::Arc<dyn object_store::ObjectStore> =
        std::sync::Arc::new(common::brimming::Brimming::with_room_for(16 * 1024));
    app.state.replace_backends(tessera_server::storage::Pool {
        open: tessera_server::storage::Backends::from([(
            tessera_server::storage::DEFAULT_BACKEND.to_owned(),
            tessera_server::storage::Stores::plain(std::sync::Arc::clone(&brimming)),
        )]),
        configured: Vec::new(),
    });

    let refused = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=too-big.bin"),
            Body::from(vec![1_u8; 64 * 1024]),
        )
        .await;
    assert_ne!(
        refused.status(),
        StatusCode::CREATED,
        "a share was created for bytes the store never took"
    );

    // The instance is still usable afterwards, given somewhere to put things. A
    // backend running out is an ordinary condition, not something that wedges
    // the next upload — so this swaps in a store with room rather than reusing
    // the one whose budget the failed upload has already spent.
    let roomy: std::sync::Arc<dyn object_store::ObjectStore> =
        std::sync::Arc::new(object_store::memory::InMemory::new());
    app.state.replace_backends(tessera_server::storage::Pool {
        open: tessera_server::storage::Backends::from([(
            tessera_server::storage::DEFAULT_BACKEND.to_owned(),
            tessera_server::storage::Stores::plain(roomy),
        )]),
        configured: Vec::new(),
    });

    let created = expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=fits.bin"),
            Body::from(vec![2_u8; 1024]),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let code = body_json(created).await["code"]
        .as_str()
        .expect("a code")
        .to_owned();
    expect_status(
        app.get(&format!("/s/{code}/download")).await,
        StatusCode::OK,
    )
    .await;
}
