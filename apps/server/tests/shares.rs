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
use time::OffsetDateTime;
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
        app.state.shares.inline_content(id).await.expect("a lookup"),
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
    let stored = app
        .state
        .shares
        .find_by_id(id)
        .await
        .expect("a lookup")
        .expect("the share");
    let key = stored
        .storage
        .object_key()
        .expect("an object key")
        .to_owned();
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
        .unwrap_or_else(|| panic!("expected object storage, got {:?}", stored.storage));

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

    let stored = app
        .state
        .shares
        .find_by_id(id)
        .await
        .expect("query")
        .expect("the share");
    assert!(
        stored.storage.object_key().is_none(),
        "text must not reach the object store, got {:?}",
        stored.storage
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

/// Content an older version put somewhere else is moved by the sweep, not by an
/// upgrade step somebody has to remember to run.
#[tokio::test]
async fn the_sweep_moves_a_file_that_was_kept_inline() {
    let app = TestApp::ready().await;
    let share = upload(&app, "?filename=old.bin", b"was inline").await;
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");

    // Put it back the way the previous rule would have stored it: bytes in the
    // database, the row saying inline, and no object anywhere.
    let stored = app
        .state
        .shares
        .find_by_id(id)
        .await
        .expect("query")
        .expect("the share");
    let key = stored.storage.object_key().expect("an object").to_owned();
    app.state
        .storage
        .delete(&object_store::path::Path::from(key.as_str()))
        .await
        .expect("remove the object");
    let statement = sea_orm::Statement::from_string(
        sea_orm::ConnectionTrait::get_database_backend(&app.state.db),
        format!("UPDATE shares SET storage = 'inline', object_key = NULL WHERE id = '{id}'"),
    );
    sea_orm::ConnectionTrait::execute_raw(&app.state.db, statement)
        .await
        .expect("rewrite the row");
    let statement = sea_orm::Statement::from_string(
        sea_orm::ConnectionTrait::get_database_backend(&app.state.db),
        format!(
            "INSERT INTO share_contents (share_id, content) VALUES ('{id}', x'77617320696e6c696e65')"
        ),
    );
    sea_orm::ConnectionTrait::execute_raw(&app.state.db, statement)
        .await
        .expect("put the bytes back");

    tessera_server::maintenance::sweep_once(&app.state, OffsetDateTime::now_utc()).await;

    let moved = app
        .state
        .shares
        .find_by_id(id)
        .await
        .expect("query")
        .expect("the share");
    let new_key = moved
        .storage
        .object_key()
        .unwrap_or_else(|| panic!("expected the object store, got {:?}", moved.storage));
    let bytes = app
        .state
        .storage
        .get(&object_store::path::Path::from(new_key))
        .await
        .expect("fetch")
        .bytes()
        .await
        .expect("read");
    assert_eq!(bytes.as_ref(), b"was inline");
    assert!(
        app.state
            .shares
            .inline_content(id)
            .await
            .expect("query")
            .is_none(),
        "the database copy goes once the object is there"
    );
}

/// And the other direction: text an older version streamed out comes back.
#[tokio::test]
async fn the_sweep_brings_text_back_from_the_store() {
    let app = TestApp::ready().await;
    let share = upload(&app, "?language=rust", b"fn main() {}").await;
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");

    // Store it the way the previous rule would have: bytes in the object store,
    // the row pointing at them, nothing in the database.
    let key = "old/text-share";
    app.state
        .storage
        .put(
            &object_store::path::Path::from(key),
            object_store::PutPayload::from_static(b"fn main() {}"),
        )
        .await
        .expect("write the object");
    for sql in [
        format!("UPDATE shares SET storage = 'object', object_key = '{key}' WHERE id = '{id}'"),
        format!("DELETE FROM share_contents WHERE share_id = '{id}'"),
        format!("DELETE FROM text_shares WHERE share_id = '{id}'"),
    ] {
        let statement = sea_orm::Statement::from_string(
            sea_orm::ConnectionTrait::get_database_backend(&app.state.db),
            sql,
        );
        sea_orm::ConnectionTrait::execute_raw(&app.state.db, statement)
            .await
            .expect("rewrite the row");
    }

    tessera_server::maintenance::sweep_once(&app.state, OffsetDateTime::now_utc()).await;

    let text = app
        .state
        .shares
        .text_of(id)
        .await
        .expect("query")
        .expect("the text should have come back");
    assert_eq!(text.content, b"fn main() {}");
    assert_eq!(text.language.as_deref(), Some("rust"));

    let moved = app
        .state
        .shares
        .find_by_id(id)
        .await
        .expect("query")
        .expect("the share");
    assert!(
        moved.storage.object_key().is_none(),
        "the row must stop pointing at the object, got {:?}",
        moved.storage
    );
}

/// Proves which table the answer comes from.
///
/// Both are written, so every other test passes either way. This one makes them
/// disagree: the `shares` row is rewritten to say the wrong name and the wrong
/// size, `file_shares` keeps the right ones, and the response has to show the
/// right ones. Before the readers were flipped it showed `WRONG.bin`.
#[tokio::test]
async fn what_a_share_says_about_itself_comes_from_file_shares() {
    let app = TestApp::ready().await;
    let share = upload(&app, "?filename=holiday.jpg", b"0123456789").await;
    let code = share["code"].as_str().expect("a code").to_owned();
    let id = share["id"].as_str().expect("an id").to_owned();

    let statement = sea_orm::Statement::from_string(
        sea_orm::ConnectionTrait::get_database_backend(&app.state.db),
        format!("UPDATE shares SET filename = 'WRONG.bin', size_bytes = 999999 WHERE id = '{id}'"),
    );
    sea_orm::ConnectionTrait::execute_raw(&app.state.db, statement)
        .await
        .expect("rewrite the old columns");

    let response = expect_status(
        app.send(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/shares/{code}")),
            Body::empty(),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let described = body_json(response).await;
    assert_eq!(described["filename"], "holiday.jpg");
    assert_eq!(described["size_bytes"], 10);

    // And the download path, which builds its own headers from the same source.
    let response = expect_status(
        app.send(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/s/{code}/download")),
            Body::empty(),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let disposition = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .expect("a disposition")
        .to_str()
        .expect("ascii")
        .to_owned();
    let length = response
        .headers()
        .get(header::CONTENT_LENGTH)
        .expect("a length")
        .to_str()
        .expect("ascii")
        .to_owned();
    assert!(disposition.contains("holiday.jpg"), "{disposition}");
    assert_eq!(length, "10");
}

/// A share whose subtype row is not there yet is still served.
///
/// Text that an older version streamed to the object store was not copied into
/// `text_shares` by the migration — it joined `share_contents`, which such a
/// share has no row in. The sweep gets to those a few at a time, and until it
/// does the old columns are what answer.
#[tokio::test]
async fn a_share_without_its_new_row_falls_back_to_the_old_columns() {
    let app = TestApp::ready().await;
    let share = upload(&app, "?language=rust", b"fn main() {}").await;
    let code = share["code"].as_str().expect("a code").to_owned();
    let id = share["id"].as_str().expect("an id").to_owned();

    let statement = sea_orm::Statement::from_string(
        sea_orm::ConnectionTrait::get_database_backend(&app.state.db),
        format!("DELETE FROM text_shares WHERE share_id = '{id}'"),
    );
    sea_orm::ConnectionTrait::execute_raw(&app.state.db, statement)
        .await
        .expect("remove the new row");

    let response = expect_status(
        app.send(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/shares/{code}")),
            Body::empty(),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let described = body_json(response).await;
    assert_eq!(described["language"], "rust");
    assert_eq!(described["size_bytes"], 12);

    // And it still serves the bytes, which is the part that matters.
    let response = expect_status(
        app.send(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/s/{code}/raw")),
            Body::empty(),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read");
    assert_eq!(body.as_ref(), b"fn main() {}");
}

/// A share whose content has been reaped must say so from the new table too.
///
/// The sweep marks `shares.storage` as gone. Readers believe `file_shares` now,
/// so if that were left pointing at the object, a reader would ask the store for
/// something that had just been deleted.
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

    let stored = app
        .state
        .shares
        .find_by_id(id)
        .await
        .expect("query")
        .expect("the share");
    let content = app
        .state
        .shares
        .content_of(&stored)
        .await
        .expect("read the content details");
    assert_eq!(
        content.storage,
        tessera_server::db::repository::share::StorageLocation::Gone,
        "the new table still points at a deleted object"
    );
}
