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
