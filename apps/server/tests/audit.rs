//! `/admin/audit-events`: who may read it, what gets recorded, and how it is
//! filtered and exported.
//!
//! The helpers below sit outside a `#[test]` body, where `clippy.toml`'s test
//! exemptions do not reach.
#![expect(clippy::expect_used, reason = "test fixtures; see module docs")]

mod common;

use std::time::Duration;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use common::{Auth, TestApp, body_json, body_string, expect_status};

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

async fn login(app: &TestApp, identifier: &str, password: &str) -> StatusCode {
    app.json(
        Method::POST,
        "/api/v1/auth/login",
        Auth::None,
        &serde_json::json!({ "identifier": identifier, "password": password }),
    )
    .await
    .status()
}

async fn events(app: &TestApp, query: &str, cookie: &str) -> serde_json::Value {
    body_json(
        expect_status(
            app.empty(
                Method::GET,
                &format!("/api/v1/admin/audit-events{query}"),
                Auth::Cookie(cookie),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await
}

fn actions(page: &serde_json::Value) -> Vec<String> {
    page["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["action"].as_str().expect("action").to_owned())
        .collect()
}

/// Polls until `query` returns at least one entry, for the ones written in the
/// background.
async fn eventually(app: &TestApp, query: &str, cookie: &str) -> serde_json::Value {
    for _ in 0..100 {
        let page = events(app, query, cookie).await;
        let found = page
            .get("items")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|items| !items.is_empty());
        if found {
            return page;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    events(app, query, cookie).await
}

async fn upload(app: &TestApp, body: &str) -> serde_json::Value {
    body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/shares")
                    .header(header::USER_AGENT, "audit-test/1.0"),
                Body::from(body.to_owned()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await
}

#[tokio::test]
async fn only_an_account_holding_audit_read_may_read_it() {
    let app = TestApp::ready().await;
    let owner = register(&app, "first").await;
    let member = register(&app, "second").await;

    for uri in [
        "/api/v1/admin/audit-events",
        "/api/v1/admin/audit-events/export",
    ] {
        assert_eq!(
            app.empty(Method::GET, uri, Auth::None).await.status(),
            StatusCode::UNAUTHORIZED,
            "{uri}"
        );
        assert_eq!(
            app.empty(Method::GET, uri, Auth::Cookie(&member))
                .await
                .status(),
            StatusCode::FORBIDDEN,
            "{uri}"
        );
        assert_eq!(
            app.empty(Method::GET, uri, Auth::Cookie(&owner))
                .await
                .status(),
            StatusCode::OK,
            "{uri}"
        );
    }
}

#[tokio::test]
async fn a_sign_in_is_recorded_with_its_method_and_who() {
    let app = TestApp::ready().await;
    let owner = register(&app, "first").await;
    assert_eq!(login(&app, "first", PASSWORD).await, StatusCode::OK);

    let page = events(&app, "?action=auth.login&outcome=success", &owner).await;
    let item = &page["items"][0];
    assert_eq!(item["actor_kind"], "user");
    assert_eq!(item["actor"]["display_name"], "first");
    assert_eq!(item["actor"]["email"], "first@example.com");
    assert_eq!(item["target_type"], "user");
    assert_eq!(item["metadata"]["method"], "password");
}

/// A failed sign-in for an account that does not exist names nothing, so the
/// log cannot be read for which addresses were tried.
#[tokio::test]
async fn a_failed_sign_in_never_carries_what_was_typed() {
    let app = TestApp::ready().await;
    let owner = register(&app, "first").await;

    assert_eq!(
        login(&app, "nobody@example.com", PASSWORD).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&app, "first", "the wrong passphrase entirely").await,
        StatusCode::UNAUTHORIZED
    );

    let page = events(&app, "?action=auth.login&outcome=failure", &owner).await;
    let items = page["items"].as_array().expect("items");
    assert_eq!(items.len(), 2);
    let text = page.to_string();
    assert!(!text.contains("nobody"), "the typed name reached the log");

    // Newest first: the wrong password, on a real account.
    assert_eq!(items[0]["target_type"], "user");
    assert_eq!(items[0]["metadata"]["reason"], "invalid_credentials");
    // Then the account that does not exist: no target.
    assert!(items[1]["target_type"].is_null());
    assert_eq!(items[1]["metadata"]["reason"], "unknown_account");
}

#[tokio::test]
async fn a_setting_change_records_before_and_after() {
    let app = TestApp::ready().await;
    let owner = register(&app, "first").await;

    expect_status(
        app.json(
            Method::PUT,
            "/api/v1/admin/settings",
            Auth::Cookie(&owner),
            &serde_json::json!({ "changes": { "share.code_length": 10 } }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    let page = events(&app, "?action=settings.update", &owner).await;
    let item = &page["items"][0];
    assert_eq!(item["target_type"], "setting");
    assert_eq!(item["target_id"], "share.code_length");
    assert_eq!(item["metadata"]["before"], 8);
    assert_eq!(item["metadata"]["after"], 10);
}

/// A sealed value is never read back, so the log must not hold it either.
#[tokio::test]
async fn a_sealed_setting_only_says_it_changed() {
    let app = TestApp::ready().await;
    let owner = register(&app, "first").await;

    expect_status(
        app.json(
            Method::PUT,
            "/api/v1/admin/settings",
            Auth::Cookie(&owner),
            &serde_json::json!({ "changes": { "scan.authorization": "Bearer hunter2" } }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    let page = events(
        &app,
        "?target_type=setting&target_id=scan.authorization",
        &owner,
    )
    .await;
    assert_eq!(page["items"][0]["metadata"]["after"], "changed");
    assert!(!page.to_string().contains("hunter2"));
}

#[tokio::test]
async fn creating_and_fetching_a_share_are_recorded() {
    let app = TestApp::ready().await;
    let owner = register(&app, "first").await;
    let share = upload(&app, "hello").await;
    let id = share["id"].as_str().expect("id").to_owned();
    let code = share["code"].as_str().expect("code").to_owned();

    let created = events(&app, "?action=share.create", &owner).await;
    assert_eq!(created["items"][0]["target_id"], id);
    assert_eq!(created["items"][0]["actor_kind"], "anonymous");
    assert_eq!(created["items"][0]["metadata"]["size_bytes"], 5);
    // The access log is off, so a visitor's address and browser are not kept.
    assert!(created["items"][0]["user_agent"].is_null());

    expect_status(
        app.empty(Method::GET, &format!("/s/{code}/download"), Auth::None)
            .await,
        StatusCode::OK,
    )
    .await;
    let query = format!("?action=share.download&target_type=share&target_id={id}");
    let downloaded = eventually(&app, &query, &owner).await;
    assert_eq!(downloaded["items"][0]["metadata"]["size_bytes"], 5);
    eventually(&app, "?action=share.retrieve&outcome=success", &owner).await;
}

#[tokio::test]
async fn a_wrong_code_is_a_failure_with_no_target() {
    let app = TestApp::ready().await;
    let owner = register(&app, "first").await;

    assert_eq!(
        app.empty(Method::GET, "/s/ZZZZZZZZ/download", Auth::None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let page = eventually(&app, "?action=share.retrieve&outcome=failure", &owner).await;
    assert!(page["items"][0]["target_type"].is_null());
    assert_eq!(page["items"][0]["metadata"]["reason"], "unknown_code");
    assert!(!page.to_string().contains("ZZZZZZZZ"));
}

#[tokio::test]
async fn share_events_can_be_switched_off_and_admin_ones_cannot() {
    let app = TestApp::with_config(|config| {
        config.audit.record_share_events = false;
    })
    .await;
    let owner = register(&app, "first").await;
    upload(&app, "hello").await;
    assert_eq!(login(&app, "first", PASSWORD).await, StatusCode::OK);

    let page = events(&app, "", &owner).await;
    let recorded = actions(&page);
    assert!(
        !recorded.contains(&"share.create".to_owned()),
        "{recorded:?}"
    );
    assert!(recorded.contains(&"auth.login".to_owned()), "{recorded:?}");
}

#[tokio::test]
async fn a_namespace_filter_and_paging_work_together() {
    let app = TestApp::ready().await;
    let owner = register(&app, "first").await;
    for _ in 0..3 {
        assert_eq!(login(&app, "first", PASSWORD).await, StatusCode::OK);
    }
    upload(&app, "noise").await;

    let first = events(&app, "?action=auth.*&limit=2", &owner).await;
    assert_eq!(actions(&first), ["auth.login", "auth.login"]);
    let cursor = first["next_cursor"].as_str().expect("a next page");
    let second = events(
        &app,
        &format!("?action=auth.*&limit=2&cursor={cursor}"),
        &owner,
    )
    .await;
    // Registration signs the first account in too, so four in all.
    assert_eq!(actions(&second), ["auth.login", "auth.login"]);
    assert!(second["next_cursor"].is_null());
}

#[tokio::test]
async fn a_filter_that_cannot_match_is_refused() {
    let app = TestApp::ready().await;
    let owner = register(&app, "first").await;
    for query in [
        "?action=share.nonsense",
        "?outcome=maybe",
        "?target_type=planet",
        "?from=yesterday",
        "?actor=not-an-id",
    ] {
        assert_eq!(
            app.empty(
                Method::GET,
                &format!("/api/v1/admin/audit-events{query}"),
                Auth::Cookie(&owner),
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST,
            "{query}"
        );
    }
}

#[tokio::test]
async fn exporting_is_csv_and_is_itself_recorded() {
    let app = TestApp::ready().await;
    let owner = register(&app, "first").await;

    let response = expect_status(
        app.empty(
            Method::GET,
            "/api/v1/admin/audit-events/export?action=auth.*",
            Auth::Cookie(&owner),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert!(
        response.headers()[header::CONTENT_TYPE]
            .to_str()
            .expect("ascii")
            .starts_with("text/csv")
    );
    let csv = body_string(response).await;
    let mut lines = csv.lines();
    assert!(
        lines
            .next()
            .expect("a header")
            .starts_with("id,occurred_at,action")
    );
    assert!(lines.all(|line| line.contains(",auth.login,")), "{csv}");

    let page = events(&app, "?action=audit.export", &owner).await;
    assert_eq!(page["items"][0]["metadata"]["action"], "auth.*");
}

#[tokio::test]
async fn an_administrators_changes_are_recorded() {
    let app = TestApp::ready().await;
    let owner = register(&app, "first").await;
    register(&app, "second").await;

    let invite = body_json(
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/admin/invites",
                Auth::Cookie(&owner),
                &serde_json::json!({ "uses": 2, "note": "family" }),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let invite_code = invite["code"].as_str().expect("code").to_owned();
    expect_status(
        app.empty(
            Method::DELETE,
            &format!(
                "/api/v1/admin/invites/{}",
                invite["id"].as_str().expect("id")
            ),
            Auth::Cookie(&owner),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;

    let page = events(&app, "?action=invite.*", &owner).await;
    assert_eq!(actions(&page), ["invite.revoke", "invite.create"]);
    assert_eq!(page["items"][1]["metadata"]["uses"], 2);
    assert!(
        !page.to_string().contains(&invite_code),
        "the invite code reached the log"
    );
}
