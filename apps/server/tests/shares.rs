//! Shares, at the storage layer and over HTTP.

mod common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use common::{Auth, TestApp, body_json, body_string, expect_status, share_repository_round_trip};
use futures_util::TryStreamExt as _;
use object_store::{ObjectStore as _, ObjectStoreExt as _};
use tower::ServiceExt as _;

#[tokio::test]
async fn shares_round_trip_through_the_repository() {
    share_repository_round_trip(&TestApp::ready().await, "SQ").await;
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
async fn a_small_share_is_kept_in_the_database() {
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
    let stored = app
        .state
        .shares
        .find_by_id(id)
        .await
        .expect("query")
        .expect("the share");
    assert_eq!(
        stored.storage,
        tessera_server::db::repository::share::StorageLocation::Inline
    );
    assert_eq!(
        app.state.shares.inline_content(id).await.expect("query"),
        Some(content.to_vec())
    );
}

/// The point of the whole ingest path: past the threshold the bytes go to the
/// blob store rather than into a column, and they get there without the body
/// ever being held whole.
#[tokio::test]
async fn a_large_share_is_streamed_to_the_blob_store() {
    let app = TestApp::with_config(|config| {
        config.share.inline_threshold_bytes = 1024;
    })
    .await;
    let content = vec![0x5A_u8; 64 * 1024];

    let share = upload(&app, "?filename=blob.bin", &content).await;
    assert_eq!(share["kind"], "file");
    assert_eq!(share["size_bytes"], content.len());

    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");
    let stored = app
        .state
        .shares
        .find_by_id(id)
        .await
        .expect("query")
        .expect("the share");
    let key = stored
        .storage
        .object_key()
        .unwrap_or_else(|| panic!("expected object storage, got {:?}", stored.storage))
        .to_owned();
    assert!(
        key.starts_with("shares/") && key.ends_with(&id.to_string()),
        "keys are sharded by date: {key}"
    );
    assert!(
        app.state
            .shares
            .inline_content(id)
            .await
            .expect("query")
            .is_none(),
        "an object-backed share must not also occupy a content row"
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
    let stored = app
        .state
        .shares
        .find_by_id(id)
        .await
        .expect("query")
        .expect("share");
    let key = stored
        .storage
        .object_key()
        .unwrap_or_else(|| panic!("expected object storage, got {:?}", stored.storage))
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
    let titles: Vec<_> = listed
        .as_array()
        .expect("an array")
        .iter()
        .map(|share| share["title"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(titles, vec!["mine"]);
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
