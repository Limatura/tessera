//! End-to-end encrypted shares from the server's side: opaque bytes, sealed
//! descriptions, and the rules that keep either from leaking in the clear.
//!
//! Nothing here encrypts anything. The server cannot tell ciphertext from any
//! other bytes, so these check what it stores, returns and refuses.

mod common;

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
};
use common::{Auth, TestApp, body_json, body_string, expect_status};

/// 16 bytes of unpadded URL-safe base64, as a client sends a salt.
const SALT: &str = "AwMDAwMDAwMDAwMDAwMDAw";

async fn encrypting() -> TestApp {
    TestApp::with_config(|config| config.features.encryption = true).await
}

async fn post(
    app: &TestApp,
    uri: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> axum::response::Response {
    let mut builder = Request::builder().method(Method::POST).uri(uri);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    app.send(builder, Body::from(body.to_vec())).await
}

async fn created(
    app: &TestApp,
    uri: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> serde_json::Value {
    body_json(expect_status(post(app, uri, headers, body).await, StatusCode::CREATED).await).await
}

#[expect(
    clippy::expect_used,
    reason = "test fixture; a panic is how it reports"
)]
async fn bytes_of(response: axum::response::Response) -> Vec<u8> {
    to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body")
        .to_vec()
}

#[expect(
    clippy::expect_used,
    reason = "test fixture; a panic is how it reports"
)]
fn code_of(share: &serde_json::Value) -> String {
    share["code"].as_str().expect("a code").to_owned()
}

#[tokio::test]
async fn an_instance_that_has_not_turned_it_on_refuses_encrypted_shares() {
    let app = TestApp::ready().await;
    let capabilities =
        body_json(expect_status(app.get("/api/v1/capabilities").await, StatusCode::OK).await).await;
    assert_eq!(capabilities["encryption"], false);

    let refused = expect_status(
        post(
            &app,
            "/api/v1/shares?encryption=link&sealed_name=AAAA",
            &[],
            b"sealed",
        )
        .await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    assert!(body_string(refused).await.contains("encrypted"));
}

/// What the uploader sealed comes back unchanged, and nothing about the content
/// is stored in the clear, including the claimed media type, which describes
/// ciphertext on an encrypted share.
#[tokio::test]
async fn an_encrypted_file_share_keeps_only_what_was_sealed() {
    let app = encrypting().await;
    let capabilities =
        body_json(expect_status(app.get("/api/v1/capabilities").await, StatusCode::OK).await).await;
    assert_eq!(capabilities["encryption"], true);

    let share = created(
        &app,
        "/api/v1/shares?encryption=link&sealed_name=bmFtZQ&sealed_details=dGl0bGU",
        &[("content-type", "text/html")],
        b"\x01 pretend this is sealed",
    )
    .await;
    assert_eq!(share["kind"], "file");
    assert_eq!(share["encryption"], "link");
    assert_eq!(share["key_salt"], serde_json::Value::Null);
    assert_eq!(share["title"], serde_json::Value::Null);
    assert_eq!(share["sealed_details"], "dGl0bGU");
    assert_eq!(share["files"][0]["filename"], serde_json::Value::Null);
    assert_eq!(share["files"][0]["sealed_name"], "bmFtZQ");

    let code = code_of(&share);
    let described = body_json(
        expect_status(
            app.empty(Method::GET, &format!("/api/v1/shares/{code}"), Auth::None)
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(described["encryption"], "link");
    assert_eq!(described["sealed_details"], "dGl0bGU");
    assert_eq!(described["files"][0]["sealed_name"], "bmFtZQ");

    let stored = app
        .state
        .shares
        .files_of(tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("id"))
        .await
        .expect("query");
    assert_eq!(
        stored[0].content_type, None,
        "the claimed type describes ciphertext"
    );
}

/// No archiver can open an archive of sealed files, so an encrypted share is
/// fetched one file at a time, and requesting the whole is refused before it
/// uses a retrieval.
#[tokio::test]
async fn an_encrypted_share_is_fetched_one_file_at_a_time() {
    let app = encrypting().await;
    let share = created(
        &app,
        "/api/v1/shares?encryption=link&sealed_name=Zmlyc3Q&max_uses=1",
        &[],
        b"first sealed file",
    )
    .await;
    let code = code_of(&share);
    let token = share["management_token"]
        .as_str()
        .expect("a token")
        .to_owned();

    // A plain name on an encrypted share is what encryption keeps off the
    // server.
    expect_status(
        post(
            &app,
            &format!("/api/v1/shares/{code}/files?filename=second.txt"),
            &[("x-share-management-token", &token)],
            b"second",
        )
        .await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    expect_status(
        post(
            &app,
            &format!("/api/v1/shares/{code}/files?sealed_name=c2Vjb25k"),
            &[("x-share-management-token", &token)],
            b"second sealed file",
        )
        .await,
        StatusCode::CREATED,
    )
    .await;

    let whole = expect_status(
        app.get(&format!("/s/{code}/download")).await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    assert!(body_string(whole).await.contains("?file="));

    // Still downloadable: the refusal did not use its one retrieval.
    let ticket = body_json(
        expect_status(
            app.empty(
                Method::POST,
                &format!("/api/v1/shares/{code}/unlock"),
                Auth::None,
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await["ticket"]
        .as_str()
        .expect("a ticket")
        .to_owned();

    for (position, expected) in [
        (1, &b"second sealed file"[..]),
        (0, &b"first sealed file"[..]),
    ] {
        let response = expect_status(
            app.get(&format!("/s/{code}/download?file={position}&t={ticket}"))
                .await,
            StatusCode::OK,
        )
        .await;
        assert_eq!(bytes_of(response).await, expected);
    }
    expect_status(
        app.get(&format!("/s/{code}/download?file=2&t={ticket}"))
            .await,
        StatusCode::NOT_FOUND,
    )
    .await;
}

/// The same parameter works on a share the server can read, returning one file
/// alone instead of an archive.
#[tokio::test]
async fn one_file_of_a_plain_share_comes_without_an_archive() {
    let app = TestApp::ready().await;
    let share = created(&app, "/api/v1/shares?filename=one.txt", &[], b"one").await;
    let code = code_of(&share);
    let token = share["management_token"]
        .as_str()
        .expect("a token")
        .to_owned();
    created(
        &app,
        &format!("/api/v1/shares/{code}/files?filename=two.txt"),
        &[("x-share-management-token", &token)],
        b"two",
    )
    .await;

    let response = expect_status(
        app.get(&format!("/s/{code}/download?file=1")).await,
        StatusCode::OK,
    )
    .await;
    let disposition = response
        .headers()
        .get("content-disposition")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    assert!(disposition.contains("two.txt"), "{disposition}");
    assert_eq!(bytes_of(response).await, b"two");
}

#[tokio::test]
async fn an_encrypted_note_is_not_served_as_text() {
    let app = encrypting().await;
    let share = created(
        &app,
        "/api/v1/shares?encryption=link&sealed_details=bGFuZw",
        &[],
        b"sealed note",
    )
    .await;
    assert_eq!(share["kind"], "text");
    let code = code_of(&share);

    expect_status(
        app.get(&format!("/s/{code}/raw")).await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    let response = expect_status(
        app.get(&format!("/s/{code}/download")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(bytes_of(response).await, b"sealed note");
}

#[tokio::test]
async fn a_request_that_describes_its_content_the_wrong_way_is_refused() {
    let app = encrypting().await;
    for uri in [
        // A name in the clear on a share that calls itself encrypted.
        "/api/v1/shares?encryption=link&filename=plain.txt",
        "/api/v1/shares?encryption=link&title=plain",
        "/api/v1/shares?encryption=link&language=rust",
        // Sealed descriptions no one could open.
        "/api/v1/shares?sealed_name=AAAA",
        // Keys no client could hold.
        "/api/v1/shares?encryption=link&key_salt=AwMDAwMDAwMDAwMDAwMDAw",
        "/api/v1/shares?encryption=password",
        "/api/v1/shares?encryption=password&key_salt=short",
        "/api/v1/shares?encryption=rot13",
        // Not base64.
        "/api/v1/shares?encryption=link&sealed_name=a%2Bb",
    ] {
        expect_status(post(&app, uri, &[], b"body").await, StatusCode::BAD_REQUEST).await;
    }
}

/// A client whose key comes from the password needs the salt before it can
/// derive the token it presents as the password.
#[tokio::test]
async fn a_locked_password_share_still_gives_its_salt() {
    let app = encrypting().await;
    let share = created(
        &app,
        &format!("/api/v1/shares?encryption=password&key_salt={SALT}&sealed_name=bmFtZQ"),
        &[("x-share-password", "derived-access-token")],
        b"sealed",
    )
    .await;
    let code = code_of(&share);

    let locked = body_json(
        expect_status(
            app.empty(Method::GET, &format!("/api/v1/shares/{code}"), Auth::None)
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(locked["unlocked"], false);
    assert_eq!(locked["encryption"], "password");
    assert_eq!(locked["key_salt"], SALT);
    assert_eq!(locked["files"], serde_json::json!([]));
}

#[tokio::test]
async fn a_password_that_a_key_came_from_cannot_be_changed_or_removed() {
    let app = encrypting().await;
    let share = created(
        &app,
        &format!("/api/v1/shares?encryption=password&key_salt={SALT}&sealed_name=bmFtZQ"),
        &[],
        b"sealed",
    )
    .await;
    let code = code_of(&share);
    let token = share["management_token"]
        .as_str()
        .expect("a token")
        .to_owned();
    let patch = |password: Option<&'static str>, body: serde_json::Value| {
        let mut builder = Request::builder()
            .method(Method::PATCH)
            .uri(format!("/api/v1/shares/{code}"))
            .header("content-type", "application/json")
            .header("x-share-management-token", token.clone());
        if let Some(password) = password {
            builder = builder.header("x-share-password", password);
        }
        app.send(builder, Body::from(body.to_string()))
    };

    // Set once, by a client that uploaded before it had a password to give.
    expect_status(
        patch(Some("first-token"), serde_json::json!({})).await,
        StatusCode::OK,
    )
    .await;
    expect_status(
        patch(Some("second-token"), serde_json::json!({})).await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    expect_status(
        patch(None, serde_json::json!({ "remove_password": true })).await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    // Other settings remain the uploader's to change.
    expect_status(
        patch(None, serde_json::json!({ "max_uses": 2 })).await,
        StatusCode::OK,
    )
    .await;
}

/// The page an unfurler reads, and the card's placeholders, say the share is
/// encrypted and how large it is, and nothing the uploader sealed.
#[tokio::test]
async fn the_link_preview_of_an_encrypted_share_names_nothing() {
    let app = encrypting().await;
    let share = created(
        &app,
        "/api/v1/shares?encryption=link&sealed_name=U0VBTEVETkFNRQ&sealed_details=U0VBTEVEVElUTEU",
        &[],
        &[0_u8; 2048],
    )
    .await;
    let code = code_of(&share);

    let page =
        body_string(expect_status(app.get(&format!("/s/{code}")).await, StatusCode::OK).await)
            .await;
    assert!(page.contains("End-to-end encrypted share"), "{page}");
    assert!(page.contains("2.0 KiB"), "{page}");
    assert!(!page.contains("U0VBTEVE"), "{page}");
}

#[tokio::test]
async fn a_resumable_upload_can_make_an_encrypted_share() {
    let app = encrypting().await;
    let open = |uri: String| {
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header("upload-length", "6"),
            Body::empty(),
        )
    };
    let session = body_json(
        expect_status(
            open("/api/v1/uploads?encryption=link&sealed_name=bmFtZQ".to_owned()).await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let id = session["id"].as_str().expect("an id");
    let token = session["token"].as_str().expect("a token");

    let finished = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::PATCH)
                    .uri(format!("/api/v1/uploads/{id}"))
                    .header("content-type", "application/offset+octet-stream")
                    .header("upload-offset", "0")
                    .header("x-upload-token", token),
                Body::from("sealed"),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    assert_eq!(finished["encryption"], "link");
    assert_eq!(finished["files"][0]["sealed_name"], "bmFtZQ");
    assert_eq!(finished["files"][0]["filename"], serde_json::Value::Null);

    // An upload into an existing share is sealed as that share is, so
    // specifying how is refused, and so is a plain name.
    let code = code_of(&finished);
    let management = finished["management_token"].as_str().expect("a token");
    for uri in [
        format!("/api/v1/uploads?code={code}&encryption=link&sealed_name=bmFtZQ"),
        format!("/api/v1/uploads?code={code}&filename=plain.txt"),
    ] {
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri(uri)
                    .header("upload-length", "6")
                    .header("x-share-management-token", management),
                Body::empty(),
            )
            .await,
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
}

/// Turning the switch off stops new encrypted shares and leaves existing ones
/// downloadable: serving them never required the server to understand them.
#[tokio::test]
async fn switching_it_off_leaves_existing_encrypted_shares_retrievable() {
    let app = encrypting().await;
    let share = created(
        &app,
        "/api/v1/shares?encryption=link&sealed_name=bmFtZQ",
        &[],
        b"sealed before",
    )
    .await;
    let code = code_of(&share);

    let mut config = tessera_server::Config::clone(&app.state.config());
    config.features.encryption = false;
    app.state.replace_config(std::sync::Arc::new(config));

    expect_status(
        post(
            &app,
            "/api/v1/shares?encryption=link&sealed_name=bmFtZQ",
            &[],
            b"after",
        )
        .await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    let response = expect_status(
        app.get(&format!("/s/{code}/download?file=0")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(bytes_of(response).await, b"sealed before");
}
