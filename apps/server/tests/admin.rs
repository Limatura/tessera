//! What an administrator can do to other people's accounts, over HTTP.

mod common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use common::{Auth, TestApp, body_json, body_string, expect_status};
use tessera_server::{config::RegistrationPolicy, db::repository::role::RoleFields};
use time::OffsetDateTime;

const PASSWORD: &str = "a perfectly adequate passphrase";

// `clippy.toml`'s test exemptions only reach code inside a `#[test]` body, and
// these are helpers. Failing is the only thing a fixture that cannot find what
// it needs has to report.
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test fixture; a panic is how it reports"
)]
/// Registers an account and returns its cookie and identifier.
async fn register(app: &TestApp, username: &str) -> (String, String) {
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
    let cookie = common::session_cookie(&response);
    let id = body_json(response).await["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    (cookie, id)
}

async fn list(app: &TestApp, auth: Auth<'_>) -> Vec<serde_json::Value> {
    let response = expect_status(
        app.empty(Method::GET, "/api/v1/admin/users", auth).await,
        StatusCode::OK,
    )
    .await;
    // The listing is a page now, so the rows are under `data`.
    body_json(response)
        .await
        .get("data")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The first account to exist administers the instance, so it is the one that
/// can see the others.
#[tokio::test]
async fn an_administrator_sees_every_account() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    register(&app, "second").await;
    register(&app, "third").await;

    let rows = list(&app, Auth::Cookie(&admin)).await;
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter().filter(|row| row["is_admin"] == true).count(),
        1,
        "only the first one administers it"
    );
}

/// Every other caller gets the same answer as one that is not signed in
/// get from an endpoint they may not use.
#[tokio::test]
async fn an_ordinary_account_may_not_look() {
    let app = TestApp::ready().await;
    register(&app, "first").await;
    let (ordinary, _) = register(&app, "second").await;

    assert_eq!(
        app.empty(Method::GET, "/api/v1/admin/users", Auth::Cookie(&ordinary))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.empty(Method::GET, "/api/v1/admin/users", Auth::None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

/// The switch exists so that an operator who does not want this surface can turn
/// it off. Before this it made no difference at all.
#[tokio::test]
async fn the_feature_switch_turns_the_whole_surface_off() {
    let app = TestApp::with_config(|config| {
        config.features.admin = false;
    })
    .await;
    let (admin, id) = register(&app, "first").await;

    for (method, uri) in [
        (Method::GET, "/api/v1/admin/users".to_owned()),
        (Method::PATCH, format!("/api/v1/admin/users/{id}")),
        (Method::DELETE, format!("/api/v1/admin/users/{id}")),
    ] {
        assert_eq!(
            app.json(
                method.clone(),
                &uri,
                Auth::Cookie(&admin),
                &serde_json::json!({ "is_admin": true }),
            )
            .await
            .status(),
            StatusCode::NOT_FOUND,
            "{method} {uri}"
        );
    }
}

#[tokio::test]
async fn an_administrator_can_promote_and_demote_somebody_else() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    let (_, other) = register(&app, "second").await;

    let promoted = expect_status(
        app.json(
            Method::PATCH,
            &format!("/api/v1/admin/users/{other}"),
            Auth::Cookie(&admin),
            &serde_json::json!({ "is_admin": true }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(body_json(promoted).await["is_admin"], true);

    let demoted = expect_status(
        app.json(
            Method::PATCH,
            &format!("/api/v1/admin/users/{other}"),
            Auth::Cookie(&admin),
            &serde_json::json!({ "is_admin": false }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(body_json(demoted).await["is_admin"], false);
}

/// An instance with no administrator cannot be repaired through the API: no
/// endpoint is left that would grant the flag back. The way out is the command
/// line against the database file, which an operator locked out of
/// a container may not have.
#[tokio::test]
async fn the_last_administrator_cannot_step_down() {
    let app = TestApp::ready().await;
    let (admin, id) = register(&app, "first").await;
    register(&app, "second").await;

    assert_eq!(
        app.json(
            Method::PATCH,
            &format!("/api/v1/admin/users/{id}"),
            Auth::Cookie(&admin),
            &serde_json::json!({ "is_admin": false }),
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );

    // Still an administrator, so the instance is still usable.
    assert_eq!(list(&app, Auth::Cookie(&admin)).await.len(), 2);
}

/// And stepping down is allowed once another account can do the job.
#[tokio::test]
async fn stepping_down_works_when_there_is_a_successor() {
    let app = TestApp::ready().await;
    let (admin, id) = register(&app, "first").await;
    let (successor, other) = register(&app, "second").await;

    expect_status(
        app.json(
            Method::PATCH,
            &format!("/api/v1/admin/users/{other}"),
            Auth::Cookie(&admin),
            &serde_json::json!({ "is_admin": true }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    expect_status(
        app.json(
            Method::PATCH,
            &format!("/api/v1/admin/users/{id}"),
            Auth::Cookie(&admin),
            &serde_json::json!({ "is_admin": false }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    assert_eq!(list(&app, Auth::Cookie(&successor)).await.len(), 2);
}

/// The same guard on the other door: deleting the only administrator would leave
/// the instance in the very state the demotion guard refuses.
#[tokio::test]
async fn the_last_administrator_cannot_be_deleted() {
    let app = TestApp::ready().await;
    let (admin, id) = register(&app, "first").await;

    assert_eq!(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/users/{id}"),
            Auth::Cookie(&admin),
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
}

/// Deleting an account ends its sessions, because the database says so.
#[tokio::test]
async fn deleting_an_account_ends_what_it_was_holding() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    let (theirs, id) = register(&app, "second").await;

    expect_status(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/users/{id}"),
            Auth::Cookie(&admin),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;

    assert_eq!(
        app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&theirs))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    // The listing shows the one account left. The row the deletion left behind
    // is not an account an operator manages, so it takes asking to see it.
    assert_eq!(list(&app, Auth::Cookie(&admin)).await.len(), 1);

    let with_deleted = body_json(
        expect_status(
            app.empty(
                Method::GET,
                "/api/v1/admin/users?include_deleted=true",
                Auth::Cookie(&admin),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    let rows = with_deleted["data"].as_array().expect("a list");
    assert_eq!(rows.len(), 2);

    // And what is left of it identifies no person.
    let gone = rows
        .iter()
        .find(|row| row["id"] == id)
        .expect("the row the deletion left");
    let name = gone["username"].as_str().expect("a name");
    let email = gone["email"].as_str().expect("an address");
    assert!(name.starts_with("gone-"), "{name}");
    assert!(email.ends_with("@deleted.invalid"), "{email}");
    assert!(!name.contains("second"), "the original name is gone");
    // Stated explicitly, so a client need not recognise the tombstone name.
    assert_eq!(gone["state"], "deleted");
    let kept = rows
        .iter()
        .find(|row| row["id"] != id)
        .expect("the account left");
    assert_eq!(kept["state"], "active");
}

/// The listing and the response to a change both report the account state, so
/// a disabled account looks disabled and re-enabling it is the obvious action.
#[tokio::test]
async fn an_accounts_state_is_shown_and_follows_a_change() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    let (_, id) = register(&app, "second").await;

    let changed = body_json(
        expect_status(
            app.json(
                Method::PATCH,
                &format!("/api/v1/admin/users/{id}"),
                Auth::Cookie(&admin),
                &serde_json::json!({ "state": "disabled" }),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(changed["state"], "disabled", "{changed}");
    assert_eq!(
        changed["username"], "second",
        "the account fields are still there"
    );

    let listed = list(&app, Auth::Cookie(&admin)).await;
    let row = listed
        .iter()
        .find(|row| row["id"] == id)
        .expect("the disabled account is still listed");
    assert_eq!(row["state"], "disabled");
}

/// An identifier that is not an identifier answers the same as one that names
/// no person.
#[tokio::test]
async fn an_unknown_account_says_only_that() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;

    for target in [
        tessera_core::UserId::generate().to_string(),
        "not-an-identifier".to_owned(),
    ] {
        assert_eq!(
            app.empty(
                Method::DELETE,
                &format!("/api/v1/admin/users/{target}"),
                Auth::Cookie(&admin),
            )
            .await
            .status(),
            StatusCode::NOT_FOUND,
            "{target}"
        );
    }
}

/// A leaked token must not be able to promote an account it controls, and that
/// is
/// how a leak becomes permanent.
#[tokio::test]
async fn a_bearer_token_may_not_administer() {
    let app = TestApp::ready().await;
    let (admin, id) = register(&app, "first").await;

    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/tokens",
            Auth::Cookie(&admin),
            &serde_json::json!({ "name": "a script" }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let secret = body_json(response).await["secret"]
        .as_str()
        .expect("a secret")
        .to_owned();

    assert_eq!(
        app.empty(Method::GET, "/api/v1/admin/users", Auth::Bearer(&secret))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.json(
            Method::PATCH,
            &format!("/api/v1/admin/users/{id}"),
            Auth::Bearer(&secret),
            &serde_json::json!({ "is_admin": true }),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
}

// ── invite codes ────────────────────────────────────────────────────

async fn create_invite(
    app: &TestApp,
    auth: Auth<'_>,
    body: &serde_json::Value,
) -> axum::response::Response {
    app.json(Method::POST, "/api/v1/admin/invites", auth, body)
        .await
}

/// Registers on an instance that asks for a code.
///
/// Even the first account needs one. An operator would not hit this, since the
/// first administrator is created at start-up, not registered, but a
/// test that makes accounts through the API does.
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test fixture; a panic is how it reports"
)]
async fn register_invited(app: &TestApp, username: &str) -> (String, String) {
    let now = OffsetDateTime::now_utc();
    let (_, code) = app
        .state
        .invites
        .issue(None, 1, now, None)
        .await
        .expect("issue");
    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": username,
                "email": format!("{username}@example.com"),
                "password": PASSWORD,
                "invite_code": code.expose(),
            }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let cookie = common::session_cookie(&response);
    let id = body_json(response).await["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    (cookie, id)
}

async fn list_invites(app: &TestApp, auth: Auth<'_>) -> Vec<serde_json::Value> {
    let response = expect_status(
        app.empty(Method::GET, "/api/v1/admin/invites", auth).await,
        StatusCode::OK,
    )
    .await;
    body_json(response)
        .await
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// The command line is the rescue path, working with the service stopped. This
/// is the ordinary one: letting a friend in should not need a shell on the box.
#[tokio::test]
async fn an_administrator_can_mint_a_code_and_use_it() {
    let app = TestApp::with_config(|config| {
        config.auth.registration = RegistrationPolicy::Invite;
    })
    .await;
    let (admin, _) = register_invited(&app, "first").await;

    let response = expect_status(
        create_invite(
            &app,
            Auth::Cookie(&admin),
            &serde_json::json!({ "uses": 2, "note": "family" }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let created = body_json(response).await;
    let code = created["code"].as_str().expect("a code").to_owned();
    assert_eq!(created["max_uses"], 2);
    assert_eq!(created["used"], 0);
    assert_eq!(created["active"], true);
    assert_eq!(created["note"], "family");

    // And it actually admits an account.
    assert_eq!(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": "invited",
                "email": "invited@example.com",
                "password": PASSWORD,
                "invite_code": code,
            }),
        )
        .await
        .status(),
        StatusCode::CREATED
    );

    // Two: the one that let the administrator in, and the one just minted.
    let rows = list_invites(&app, Auth::Cookie(&admin)).await;
    let family = rows
        .iter()
        .find(|row| row["note"] == "family")
        .expect("the one just minted");
    assert_eq!(family["used"], 1);
}

/// The list never carries the code. Only a digest is stored, so there is
/// nothing to show, and an administrator who lost the code issues another one.
#[tokio::test]
async fn the_list_never_shows_a_code() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    expect_status(
        create_invite(
            &app,
            Auth::Cookie(&admin),
            &serde_json::json!({ "uses": 1 }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;

    let rows = list_invites(&app, Auth::Cookie(&admin)).await;
    assert_eq!(rows.len(), 1);
    assert!(
        rows[0].get("code").is_none(),
        "a listing must not carry the code"
    );
}

/// What an operator who posted a code in the wrong chat window wants: it stops
/// admitting anybody, and the accounts it already let in stay.
#[tokio::test]
async fn revoking_a_code_stops_it_without_undoing_what_it_did() {
    let app = TestApp::with_config(|config| {
        config.auth.registration = RegistrationPolicy::Invite;
    })
    .await;
    let (admin, _) = register_invited(&app, "first").await;

    let response = expect_status(
        create_invite(
            &app,
            Auth::Cookie(&admin),
            &serde_json::json!({ "uses": 5 }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let created = body_json(response).await;
    let code = created["code"].as_str().expect("a code").to_owned();
    let id = created["id"].as_str().expect("an id").to_owned();

    let register_with = |name: &'static str, code: String| {
        let app = &app;
        async move {
            app.json(
                Method::POST,
                "/api/v1/auth/register",
                Auth::None,
                &serde_json::json!({
                    "username": name,
                    "email": format!("{name}@example.com"),
                    "password": PASSWORD,
                    "invite_code": code,
                }),
            )
            .await
            .status()
        }
    };

    assert_eq!(
        register_with("early", code.clone()).await,
        StatusCode::CREATED
    );

    expect_status(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/invites/{id}"),
            Auth::Cookie(&admin),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;

    assert_eq!(
        register_with("late", code).await,
        StatusCode::BAD_REQUEST,
        "the code must not admit anybody else"
    );

    // The account it did admit is untouched.
    let users = list(&app, Auth::Cookie(&admin)).await;
    assert!(users.iter().any(|user| user["username"] == "early"));
}

#[tokio::test]
async fn an_invite_that_admits_nobody_is_refused() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;

    for uses in [0, -1] {
        assert_eq!(
            create_invite(
                &app,
                Auth::Cookie(&admin),
                &serde_json::json!({ "uses": uses })
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST,
            "{uses}"
        );
    }
}

/// A code past its date is listed but not active, so an administrator can see
/// why it stopped working instead of wondering where it went.
#[tokio::test]
async fn an_expired_code_is_listed_as_inactive() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;

    let now = OffsetDateTime::now_utc();
    app.state
        .invites
        .issue(
            Some("last week".to_owned()),
            5,
            now - time::Duration::days(8),
            Some(now - time::Duration::days(1)),
        )
        .await
        .expect("issue");

    let rows = list_invites(&app, Auth::Cookie(&admin)).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["active"], false);
    assert_eq!(rows[0]["used"], 0);
}

/// The same rules as the rest of this surface.
#[tokio::test]
async fn invites_need_an_administrator_with_a_cookie() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    let (ordinary, _) = register(&app, "second").await;

    assert_eq!(
        app.empty(
            Method::GET,
            "/api/v1/admin/invites",
            Auth::Cookie(&ordinary)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );

    let response = expect_status(
        app.json(
            Method::POST,
            "/api/v1/tokens",
            Auth::Cookie(&admin),
            &serde_json::json!({ "name": "a script" }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let secret = body_json(response).await["secret"]
        .as_str()
        .expect("a secret")
        .to_owned();
    assert_eq!(
        create_invite(
            &app,
            Auth::Bearer(&secret),
            &serde_json::json!({ "uses": 1 })
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
}

// ── what the instance is holding ────────────────────────────────────

async fn stats(app: &TestApp, auth: Auth<'_>) -> serde_json::Value {
    let response = expect_status(
        app.empty(Method::GET, "/api/v1/admin/stats", auth).await,
        StatusCode::OK,
    )
    .await;
    body_json(response).await
}

#[tokio::test]
async fn the_stats_count_what_is_there() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    register(&app, "second").await;
    register(&app, "third").await;

    let now = OffsetDateTime::now_utc();
    app.state
        .invites
        .issue(None, 5, now, None)
        .await
        .expect("issue");
    app.state
        .invites
        .issue(
            None,
            5,
            now - time::Duration::days(2),
            Some(now - time::Duration::days(1)),
        )
        .await
        .expect("issue");

    let body = stats(&app, Auth::Cookie(&admin)).await;
    assert_eq!(body["accounts"], 3);
    assert_eq!(body["administrators"], 1);
    assert_eq!(
        body["live_invites"], 1,
        "the expired one is not one somebody could use"
    );
    assert_eq!(body["shares"], 0);
    assert_eq!(body["stored_bytes"], 0);
}

/// Zero is how the configuration spells "no ceiling", but a client drawing a
/// usage bar against zero would show every instance as full.
#[tokio::test]
async fn an_unset_quota_is_null_rather_than_zero() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    assert!(stats(&app, Auth::Cookie(&admin)).await["quota_bytes"].is_null());

    let bounded = TestApp::with_config(|config| {
        config.storage.max_total_bytes = 5_000_000;
    })
    .await;
    let (admin, _) = register(&bounded, "first").await;
    assert_eq!(
        stats(&bounded, Auth::Cookie(&admin)).await["quota_bytes"],
        5_000_000
    );
}

/// The gap between the two share counts is what the sweep has not reached, and
/// seeing it is the point: a large one means retention is longer than expected,
/// or the sweep is not running.
#[tokio::test]
async fn a_share_that_nobody_can_open_still_counts_as_stored() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;

    expect_status(
        app.json(
            Method::POST,
            "/api/v1/shares",
            Auth::Cookie(&admin),
            &serde_json::json!({ "kind": "text", "content": "hello", "expiry_hours": 1 }),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;

    let before = stats(&app, Auth::Cookie(&admin)).await;
    assert_eq!(before["shares"], 1);
    assert_eq!(before["live_shares"], 1);

    let past = OffsetDateTime::now_utc() - time::Duration::hours(2);
    let statement = sea_orm::Statement::from_string(
        sea_orm::ConnectionTrait::get_database_backend(&app.state.db),
        format!(
            "UPDATE shares SET expires_at = '{}'",
            past.format(&time::format_description::well_known::Rfc3339)
                .expect("formattable")
        ),
    );
    sea_orm::ConnectionTrait::execute_raw(&app.state.db, statement)
        .await
        .expect("expire it");

    let after = stats(&app, Auth::Cookie(&admin)).await;
    assert_eq!(after["shares"], 1, "the row is still there");
    assert_eq!(after["live_shares"], 0, "but nobody can open it");
}

#[tokio::test]
async fn stats_need_an_administrator_with_a_cookie() {
    let app = TestApp::ready().await;
    register(&app, "first").await;
    let (ordinary, _) = register(&app, "second").await;

    assert_eq!(
        app.empty(Method::GET, "/api/v1/admin/stats", Auth::Cookie(&ordinary))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
}

/// The sweep writes today's row, replaces it on the same day, and the history
/// endpoint returns the recorded days oldest first.
#[tokio::test]
async fn the_sweep_records_a_day_of_stats() {
    let app = TestApp::with_config(|config| config.features.anonymous_upload = true).await;
    let (admin, _) = register(&app, "first").await;
    let today = OffsetDateTime::now_utc();
    let yesterday = today - time::Duration::days(1);

    tessera_server::maintenance::sweep_once(&app.state, yesterday).await;
    expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=a.bin"),
            Body::from(vec![0_u8; 300]),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    // Twice in a day is one row, holding the later pass.
    tessera_server::maintenance::sweep_once(&app.state, today).await;
    tessera_server::maintenance::sweep_once(&app.state, today).await;

    let history = |query: &'static str| {
        let admin = admin.clone();
        let app = &app;
        async move {
            app.empty(
                Method::GET,
                &format!("/api/v1/admin/stats/history{query}"),
                Auth::Cookie(&admin),
            )
            .await
        }
    };
    let days = body_json(expect_status(history("").await, StatusCode::OK).await).await;
    let days = days["data"].as_array().expect("days").clone();
    assert_eq!(days.len(), 2, "{days:?}");
    assert_eq!(days[0]["date"], yesterday.date().to_string());
    assert_eq!(days[0]["stored_bytes"], 0);
    assert_eq!(days[1]["date"], today.date().to_string());
    assert_eq!(days[1]["stored_bytes"], 300);
    assert_eq!(days[1]["shares"], 1);
    assert_eq!(days[1]["live_shares"], 1);

    // A range that ends yesterday leaves today out.
    let only = format!("?from={}&to={}", yesterday.date(), yesterday.date());
    let only = body_json(
        expect_status(
            app.empty(
                Method::GET,
                &format!("/api/v1/admin/stats/history{only}"),
                Auth::Cookie(&admin),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(only["data"].as_array().expect("days").len(), 1);

    for (query, rule) in [
        ("?from=2026-13-01", "date"),
        ("?from=2026-09-30&to=2026-09-01", "not_after_to"),
        ("?from=2024-01-01&to=2026-01-01", "range_too_long"),
    ] {
        let refused =
            body_json(expect_status(history(query).await, StatusCode::BAD_REQUEST).await).await;
        assert_eq!(refused["details"]["rule"], rule, "{query}");
    }
}

/// One permission at a time, the point of the change.
///
/// Under the boolean, reaching any administrative endpoint meant reaching all
/// of them. A role holding only `users:read` proves that is no longer true.
#[tokio::test]
async fn a_role_can_grant_looking_without_granting_changing() {
    let app = TestApp::ready().await;
    register(&app, "first").await;
    let (watcher, _) = register(&app, "second").await;

    let user = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("second").expect("a name"))
        .await
        .expect("query")
        .expect("the account");
    let role = app
        .state
        .roles
        .create(
            RoleFields {
                name: "auditor",
                description: Some("Can look, cannot touch.".to_owned()),
                quota_bytes: None,
                max_file_bytes: None,
                permissions: &[tessera_core::Permission::UsersRead].into_iter().collect(),
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .expect("create the role");
    app.state
        .roles
        .set_role_of(user.id, Some(role.id), OffsetDateTime::now_utc())
        .await
        .expect("put them in it");

    assert_eq!(
        app.empty(Method::GET, "/api/v1/admin/users", Auth::Cookie(&watcher))
            .await
            .status(),
        StatusCode::OK,
        "users:read was granted"
    );
    assert_eq!(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/users/{}", user.id),
            Auth::Cookie(&watcher)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN,
        "users:delete was not"
    );
}

/// An account in no role is signed in and allowed nothing, and that has to be a
/// clean refusal, not a crash.
#[tokio::test]
async fn an_account_with_no_roles_may_do_nothing() {
    let app = TestApp::ready().await;
    register(&app, "first").await;
    let (stripped, _) = register(&app, "second").await;

    let user = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("second").expect("a name"))
        .await
        .expect("query")
        .expect("the account");
    app.state
        .roles
        .set_role_of(user.id, None, OffsetDateTime::now_utc())
        .await
        .expect("take every role away");

    assert_eq!(
        app.empty(Method::GET, "/api/v1/admin/users", Auth::Cookie(&stripped))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
}

/// The instance must keep an account that can hand out roles.
#[tokio::test]
async fn the_last_account_holding_the_keys_cannot_be_deleted() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    register(&app, "second").await;

    let first = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("first").expect("a name"))
        .await
        .expect("query")
        .expect("the account");
    let second = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("second").expect("a name"))
        .await
        .expect("query")
        .expect("the account");

    assert_eq!(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/users/{}", first.id),
            Auth::Cookie(&admin)
        )
        .await
        .status(),
        StatusCode::CONFLICT,
        "the only holder of roles:write"
    );
    // And an account that never held them is not caught by the same guard.
    assert_eq!(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/users/{}", second.id),
            Auth::Cookie(&admin)
        )
        .await
        .status(),
        StatusCode::NO_CONTENT,
    );
}

/// The dashboard number has to mean the same thing the guard means.
///
/// It counted `users.is_admin`, now a mirror kept for rollback. A role
/// granted directly does not touch that column, so the number would say one
/// while two people could administer the instance.
#[tokio::test]
async fn the_administrator_count_follows_roles_not_the_column() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    register(&app, "second").await;

    let second = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("second").expect("a name"))
        .await
        .expect("query")
        .expect("the account");
    // Straight into the seeded administrator role, without going through the
    // endpoint that also writes the column.
    let admin_role = tessera_core::RoleId::parse(tessera_core::RoleId::OWNER).expect("an id");
    app.state
        .roles
        .set_role_of(second.id, Some(admin_role), OffsetDateTime::now_utc())
        .await
        .expect("grant it");

    let response = expect_status(
        app.empty(Method::GET, "/api/v1/admin/stats", Auth::Cookie(&admin))
            .await,
        StatusCode::OK,
    )
    .await;
    let stats = body_json(response).await;
    assert_eq!(
        stats["administrators"], 2,
        "the column still says one; the roles say two"
    );
}

/// `roles:write` must not be a way to obtain every other permission.
///
/// Without this check an account holding only `roles:write` mints a role
/// granting
/// `users:delete`, puts themselves in it, and has it. The permission would mean
/// "may hand out anything" instead of "may hand out what I have".
#[tokio::test]
async fn a_role_cannot_hand_out_a_permission_its_author_lacks() {
    let app = TestApp::ready().await;
    register(&app, "first").await;
    let (deputy, _) = register(&app, "second").await;

    let user = account(&app, "second").await;
    let limited = app
        .state
        .roles
        .create(
            RoleFields {
                name: "deputy",
                description: None,
                quota_bytes: None,
                max_file_bytes: None,
                permissions: &[tessera_core::Permission::RolesWrite].into_iter().collect(),
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .expect("create the role");
    app.state
        .roles
        .set_role_of(user.id, Some(limited.id), OffsetDateTime::now_utc())
        .await
        .expect("grant it");

    let response = app
        .json(
            Method::POST,
            "/api/v1/admin/roles",
            Auth::Cookie(&deputy),
            &serde_json::json!({
                "name": "sneaky",
                "permissions": ["users:delete"],
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // And the same door from the other side: granting it to another account.
    let existing = app
        .state
        .roles
        .find_by_name("admin")
        .await
        .expect("query")
        .expect("the seeded role");
    let response = app
        .json(
            Method::PUT,
            &format!("/api/v1/admin/users/{}/roles", user.id),
            Auth::Cookie(&deputy),
            &serde_json::json!({ "role": existing.id.to_string() }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// A permission this version does not have is refused, not dropped.
///
/// Ignoring it would save a role that does less than the screen it was ticked
/// on says it does.
#[tokio::test]
async fn an_unknown_permission_is_refused_rather_than_ignored() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;

    let response = app
        .json(
            Method::POST,
            "/api/v1/admin/roles",
            Auth::Cookie(&admin),
            &serde_json::json!({ "name": "odd", "permissions": ["shares:teleport"] }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// The seeded roles are what an instance is administered through.
#[tokio::test]
async fn a_seeded_role_cannot_be_deleted() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;

    let member = app
        .state
        .roles
        .find_by_name("member")
        .await
        .expect("query")
        .expect("the seeded role");
    assert_eq!(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/roles/{}", member.id),
            Auth::Cookie(&admin)
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
}

/// Editing a role is the other way to lock everybody out.
#[tokio::test]
async fn the_last_role_carrying_the_keys_cannot_drop_them() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;

    let role = app
        .state
        .roles
        .find_by_name("admin")
        .await
        .expect("query")
        .expect("the seeded role");
    let without_the_keys: Vec<String> = tessera_core::Permission::ALL
        .into_iter()
        .filter(|permission| *permission != tessera_core::Permission::RolesWrite)
        .map(|permission| permission.as_str().to_owned())
        .collect();

    let response = app
        .json(
            Method::PUT,
            &format!("/api/v1/admin/roles/{}", role.id),
            Auth::Cookie(&admin),
            &serde_json::json!({ "name": "admin", "permissions": without_the_keys }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

/// A role is created, edited and used, end to end.
#[tokio::test]
async fn a_role_can_be_created_and_granted() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    let (watcher, _) = register(&app, "second").await;
    let user = account(&app, "second").await;

    let created = body_json(
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/admin/roles",
                Auth::Cookie(&admin),
                &serde_json::json!({
                    "name": "auditor",
                    "description": "Can look.",
                    "permissions": ["users:read"],
                }),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let role_id = created["id"].as_str().expect("an id").to_owned();
    assert_eq!(created["members"], 0);
    assert_eq!(created["is_system"], false);

    expect_status(
        app.json(
            Method::PUT,
            &format!("/api/v1/admin/users/{}/roles", user.id),
            Auth::Cookie(&admin),
            &serde_json::json!({ "role": role_id }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    // It works, and only as far as it says.
    assert_eq!(
        app.empty(Method::GET, "/api/v1/admin/users", Auth::Cookie(&watcher))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.empty(Method::GET, "/api/v1/admin/stats", Auth::Cookie(&watcher))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );

    // And the listing counts the member.
    let roles = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/admin/roles", Auth::Cookie(&admin))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    let auditor = roles
        .as_array()
        .expect("a list")
        .iter()
        .find(|row| row["name"] == "auditor")
        .expect("the role");
    assert_eq!(auditor["members"], 1);
}

/// The permission list is served, not built into the client, so a client
/// cannot silently stop offering permissions added since it was built.
#[tokio::test]
async fn every_permission_is_offered_to_the_screen() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;

    let listed = body_json(
        expect_status(
            app.empty(
                Method::GET,
                "/api/v1/admin/permissions",
                Auth::Cookie(&admin),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    let names: Vec<&str> = listed
        .as_array()
        .expect("a list")
        .iter()
        .map(|row| row["name"].as_str().expect("a name"))
        .collect();
    for permission in tessera_core::Permission::ALL {
        assert!(names.contains(&permission.as_str()), "{permission}");
    }
}

#[expect(
    clippy::expect_used,
    reason = "test fixture; a panic is how it reports"
)]
/// The account whose roles are being read, by name.
async fn account(app: &TestApp, username: &str) -> tessera_server::db::repository::user::User {
    app.state
        .users
        .find_by_username(&tessera_core::Username::parse(username).expect("a name"))
        .await
        .expect("query")
        .expect("the account")
}

/// A built-in role means the same thing on every instance, and that is the
/// whole of its value.
#[tokio::test]
async fn a_built_in_role_can_be_renamed_but_not_repermissioned() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;

    let member = app
        .state
        .roles
        .find_by_name("member")
        .await
        .expect("query")
        .expect("the seeded role");
    let held: Vec<String> = member
        .permissions
        .iter()
        .map(|permission| permission.as_str().to_owned())
        .collect();

    // A label is not a meaning.
    expect_status(
        app.json(
            Method::PUT,
            &format!("/api/v1/admin/roles/{}", member.id),
            Auth::Cookie(&admin),
            &serde_json::json!({ "name": "一般使用者", "permissions": held }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    // Taking something away from it is not.
    let fewer: Vec<String> = member
        .permissions
        .iter()
        .filter(|permission| *permission != tessera_core::Permission::SharesDelete)
        .map(|permission| permission.as_str().to_owned())
        .collect();
    assert_eq!(
        app.json(
            Method::PUT,
            &format!("/api/v1/admin/roles/{}", member.id),
            Auth::Cookie(&admin),
            &serde_json::json!({ "name": "一般使用者", "permissions": fewer }),
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
}

/// The way to make ordinary accounts do less.
#[tokio::test]
async fn new_accounts_go_into_the_configured_role() {
    let app = TestApp::with_config(|config| {
        config.auth.default_role = "restricted".to_owned();
    })
    .await;

    // The role has to exist before anybody registers into it.
    app.state
        .roles
        .create(
            RoleFields {
                name: "restricted",
                description: None,
                quota_bytes: None,
                max_file_bytes: None,
                permissions: &[tessera_core::Permission::SharesCreate]
                    .into_iter()
                    .collect(),
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .expect("create the role");

    register(&app, "first").await;
    let (ordinary, _) = register(&app, "second").await;
    let user = account(&app, "second").await;

    let held = app
        .state
        .roles
        .permissions_of(user.id)
        .await
        .expect("read the permissions");
    assert!(held.allows(tessera_core::Permission::SharesCreate));
    assert!(
        !held.allows(tessera_core::Permission::SharesDelete),
        "member's permissions must not leak in"
    );

    // And the first account still administers the instance whatever the setting
    // says, since an instance whose first account cannot administer it has
    // no way to fix that.
    assert_eq!(
        app.empty(Method::GET, "/api/v1/admin/users", Auth::Cookie(&ordinary))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
}

/// After every migration has run, the administrator role holds everything.
///
/// The per-migration tests each check what their own file wrote, all a migration
/// is answerable for. Nothing checked the state they add up to, and a
/// permission that cannot be held is an endpoint that cannot be reached on a
/// fresh instance, with no error anywhere to say so.
#[tokio::test]
async fn the_owner_role_ends_up_holding_every_permission() {
    let app = TestApp::ready().await;

    let admin = app
        .state
        .roles
        .find_by_name("owner")
        .await
        .expect("query")
        .expect("the seeded role");
    for permission in tessera_core::Permission::ALL {
        assert!(
            admin.permissions.allows(permission),
            "the owner role cannot be granted {permission}"
        );
    }
}

/// The gap this endpoint exists to close.
///
/// A share whose uploader is gone has no owner, so the ordinary delete
/// route, which asks whether the caller owns it, could never reach one. With no
/// expiry and no use limit the sweep never reaches it either, so it served its
/// content for ever and nothing could stop it.
#[tokio::test]
async fn an_administrator_can_remove_a_share_nobody_owns() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    let (leaver, _) = register(&app, "second").await;

    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/shares?filename=left-behind.bin")
                    .header(header::COOKIE, &leaver),
                Body::from(b"orphan".to_vec()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let share_id = created["id"].as_str().expect("an id").to_owned();
    let code = created["code"].as_str().expect("a code").to_owned();

    // The uploader's account goes. Today that orphans the share.
    let second = account(&app, "second").await;
    expect_status(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/users/{}", second.id),
            Auth::Cookie(&admin),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;

    // It is still served, and that is what makes this worth fixing.
    assert_eq!(
        app.empty(Method::GET, &format!("/s/{code}/download"), Auth::None)
            .await
            .status(),
        StatusCode::OK,
        "an orphaned share keeps working"
    );

    // And now it can be removed.
    expect_status(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/shares/{share_id}"),
            Auth::Cookie(&admin),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(
        app.empty(Method::GET, &format!("/s/{code}/download"), Auth::None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

/// The listing tells an anonymous upload apart from one whose owner is gone.
///
/// Both have no reachable account, and collapsing them would throw away which
/// is which, information nothing could recover afterwards.
#[tokio::test]
async fn the_listing_says_who_uploaded_each_share() {
    let app = TestApp::with_config(|config| {
        config.features.anonymous_upload = true;
    })
    .await;
    let (admin, admin_id) = register(&app, "first").await;

    expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=mine.bin")
                .header(header::COOKIE, &admin),
            Body::from(b"mine".to_vec()),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=nobodys.bin"),
            Body::from(b"nobody".to_vec()),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;

    let page = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/admin/shares", Auth::Cookie(&admin))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(page["meta"]["total"], 2);

    let rows = page["data"].as_array().expect("a list");
    let mine = rows
        .iter()
        .find(|row| row["filename"] == "mine.bin")
        .expect("the signed-in upload");
    let nobodys = rows
        .iter()
        .find(|row| row["filename"] == "nobodys.bin")
        .expect("the anonymous upload");
    assert_eq!(mine["owner_id"], admin_id);
    assert_eq!(
        mine["owner_username"], "first",
        "who, without a lookup per row"
    );
    assert!(nobodys["owner_id"].is_null(), "anonymous means nobody");
    assert!(nobodys["owner_username"].is_null());
}

/// A share outliving its uploader still shows whose it was, under the name the
/// account was left with.
#[tokio::test]
async fn a_deleted_uploader_is_named_by_what_is_left_of_them() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    let (theirs, id) = register(&app, "second").await;
    expect_status(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=left-behind.bin")
                .header(header::COOKIE, &theirs),
            Body::from(b"theirs".to_vec()),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    expect_status(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/users/{id}"),
            Auth::Cookie(&admin),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;

    let page = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/admin/shares", Auth::Cookie(&admin))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    // Deleting an account leaves its shares working for anyone with the code.
    let row = page["data"]
        .as_array()
        .expect("a list")
        .iter()
        .find(|row| row["filename"] == "left-behind.bin")
        .expect("the share outlives its uploader");
    assert_eq!(row["owner_id"], id);
    assert_eq!(row["owner_username"], format!("gone-{}", id.to_lowercase()));
}

/// Reading every share and deleting every share are separate powers.
#[tokio::test]
async fn reading_every_share_does_not_imply_removing_one() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    let (watcher, _) = register(&app, "second").await;
    let user = account(&app, "second").await;

    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/shares?filename=someone-elses.bin")
                    .header(header::COOKIE, &admin),
                Body::from(b"content".to_vec()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let share_id = created["id"].as_str().expect("an id").to_owned();

    let role = app
        .state
        .roles
        .create(
            RoleFields {
                name: "moderator",
                description: None,
                quota_bytes: None,
                max_file_bytes: None,
                permissions: &[tessera_core::Permission::SharesReadAny]
                    .into_iter()
                    .collect(),
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .expect("create the role");
    app.state
        .roles
        .set_role_of(user.id, Some(role.id), OffsetDateTime::now_utc())
        .await
        .expect("grant it");

    assert_eq!(
        app.empty(Method::GET, "/api/v1/admin/shares", Auth::Cookie(&watcher))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/shares/{share_id}"),
            Auth::Cookie(&watcher)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
}

/// Suspending stops the person, not their content.
///
/// Whoever holds a link that account handed out did nothing wrong, and a
/// suspension that broke every one of those links would be a different act from
/// the one the operator asked for. Stopping the content too is what a share's
/// own `disabled_at` is for.
#[tokio::test]
async fn suspending_an_account_stops_it_signing_in_but_keeps_its_shares_served() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    let (theirs, id) = register(&app, "second").await;

    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/shares?filename=theirs.bin")
                    .header(header::COOKIE, &theirs),
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
            Method::PATCH,
            &format!("/api/v1/admin/users/{id}"),
            Auth::Cookie(&admin),
            &serde_json::json!({ "is_admin": false, "state": "disabled" }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    // The session they already held stops working, not only the next sign-in.
    assert_eq!(
        app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&theirs))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    // Signing in again answers the same as a wrong password, so no caller can
    // ask
    // this endpoint which accounts are suspended.
    assert_eq!(
        app.json(
            Method::POST,
            "/api/v1/auth/login",
            Auth::None,
            &serde_json::json!({ "identifier": "second", "password": PASSWORD }),
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );

    // Their share is still served.
    assert_eq!(
        app.empty(Method::GET, &format!("/s/{code}/download"), Auth::None)
            .await
            .status(),
        StatusCode::OK
    );

    // And letting them back in works.
    expect_status(
        app.json(
            Method::PATCH,
            &format!("/api/v1/admin/users/{id}"),
            Auth::Cookie(&admin),
            &serde_json::json!({ "is_admin": false, "state": "active" }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        app.json(
            Method::POST,
            "/api/v1/auth/login",
            Auth::None,
            &serde_json::json!({ "identifier": "second", "password": PASSWORD }),
        )
        .await
        .status(),
        StatusCode::OK,
        "a suspension is something an account comes back from"
    );
}

/// Suspending an account is not a demotion. `is_admin` rewrites the role when
/// sent, so a patch that omits it must leave the role alone.
#[tokio::test]
async fn suspending_an_account_keeps_its_role() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    let (_, id) = register(&app, "second").await;

    let roles_of_second = |rows: Vec<serde_json::Value>| {
        rows.into_iter()
            .find(|row| row["id"] == id.as_str())
            .map(|row| row["roles"].clone())
            .expect("the second account is listed")
    };
    let before = roles_of_second(list(&app, Auth::Cookie(&admin)).await);

    for wanted in ["disabled", "active"] {
        expect_status(
            app.json(
                Method::PATCH,
                &format!("/api/v1/admin/users/{id}"),
                Auth::Cookie(&admin),
                &serde_json::json!({ "state": wanted }),
            )
            .await,
            StatusCode::OK,
        )
        .await;
    }

    assert_eq!(
        roles_of_second(list(&app, Auth::Cookie(&admin)).await),
        before
    );
}

/// A deletion is not.
#[tokio::test]
async fn a_deleted_account_cannot_be_reactivated() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    let (_, id) = register(&app, "second").await;

    expect_status(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/users/{id}"),
            Auth::Cookie(&admin),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;

    // The patch reports success, since the row is there and the flag was
    // written, but the state does not move.
    expect_status(
        app.json(
            Method::PATCH,
            &format!("/api/v1/admin/users/{id}"),
            Auth::Cookie(&admin),
            &serde_json::json!({ "is_admin": false, "state": "active" }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        app.json(
            Method::POST,
            "/api/v1/auth/login",
            Auth::None,
            &serde_json::json!({ "identifier": "second", "password": PASSWORD }),
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
}

/// The name and the address a deleted account gave up are free again.
#[tokio::test]
async fn a_deleted_accounts_name_can_be_registered_again() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    let (_, id) = register(&app, "second").await;

    expect_status(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/users/{id}"),
            Auth::Cookie(&admin),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;

    assert_eq!(
        app.json(
            Method::POST,
            "/api/v1/auth/register",
            Auth::None,
            &serde_json::json!({
                "username": "second",
                "email": "second@example.com",
                "password": PASSWORD,
            }),
        )
        .await
        .status(),
        StatusCode::CREATED,
        "the scrubbed row must not keep the name out of circulation"
    );
}

/// Who did it is recorded, because a tombstone cannot say it by itself.
#[tokio::test]
async fn a_deletion_records_who_did_it() {
    let app = TestApp::ready().await;
    let (admin, admin_id) = register(&app, "first").await;
    let (_, id) = register(&app, "second").await;

    expect_status(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/users/{id}"),
            Auth::Cookie(&admin),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;

    let removed = app
        .state
        .users
        .find_by_id(tessera_core::UserId::parse(&id).expect("an id"))
        .await
        .expect("query")
        .expect("the row the deletion left");
    assert_eq!(removed.state, tessera_core::AccountState::Deleted);
    assert_eq!(
        removed.deleted_by.map(|by| by.to_string()),
        Some(admin_id),
        "a row alone cannot say who did it"
    );
    assert!(removed.deleted_at.is_some());
}

/// Suspending an account has to stop its API tokens too.
///
/// Ending the sessions is what makes the cookie stop working, and for a while
/// that looked like enough. It is not: a token is not a session, so nothing
/// ended it, and the state check at authentication is the only thing that does.
#[tokio::test]
async fn suspending_an_account_stops_its_tokens() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    let (theirs, id) = register(&app, "second").await;

    let secret = body_json(
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/tokens",
                Auth::Cookie(&theirs),
                &serde_json::json!({ "name": "a script", "permissions": ["shares:read"] }),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await["secret"]
        .as_str()
        .expect("a secret")
        .to_owned();

    assert_eq!(
        app.empty(Method::GET, "/api/v1/shares", Auth::Bearer(&secret))
            .await
            .status(),
        StatusCode::OK
    );

    expect_status(
        app.json(
            Method::PATCH,
            &format!("/api/v1/admin/users/{id}"),
            Auth::Cookie(&admin),
            &serde_json::json!({ "is_admin": false, "state": "disabled" }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    assert_eq!(
        app.empty(Method::GET, "/api/v1/shares", Auth::Bearer(&secret))
            .await
            .status(),
        StatusCode::UNAUTHORIZED,
        "a token is not a session; nothing else stops it"
    );
}

/// The three powers that separate an account that helps from one whose instance
/// it is.
///
/// Before this split the seeded set had nothing between `admin` (everything)
/// and `member`, so "help me run this" meant handing over deleting accounts,
/// destroying other people's content, and granting permissions.
#[tokio::test]
async fn an_admin_helps_run_the_instance_without_being_able_to_take_it() {
    let app = TestApp::ready().await;
    let (owner, _) = register(&app, "first").await;
    let (helper, _) = register(&app, "second").await;
    let (victim, victim_id) = register(&app, "third").await;

    let helper_account = account(&app, "second").await;
    let admin_role = tessera_core::RoleId::parse(tessera_core::RoleId::ADMIN).expect("an id");
    app.state
        .roles
        .set_role_of(
            helper_account.id,
            Some(admin_role),
            OffsetDateTime::now_utc(),
        )
        .await
        .expect("make them an admin");

    // What an admin is for.
    assert_eq!(
        app.empty(Method::GET, "/api/v1/admin/users", Auth::Cookie(&helper))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.empty(Method::GET, "/api/v1/admin/shares", Auth::Cookie(&helper))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.json(
            Method::PATCH,
            &format!("/api/v1/admin/users/{victim_id}"),
            Auth::Cookie(&helper),
            &serde_json::json!({ "is_admin": false, "state": "disabled" }),
        )
        .await
        .status(),
        StatusCode::OK,
        "suspending is help, not a takeover"
    );

    // What an admin is not for.
    assert_eq!(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/users/{victim_id}"),
            Auth::Cookie(&helper)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN,
        "users:delete"
    );
    assert_eq!(
        app.json(
            Method::POST,
            "/api/v1/admin/roles",
            Auth::Cookie(&helper),
            &serde_json::json!({ "name": "mine", "permissions": ["users:delete"] }),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN,
        "roles:write is what would let them raise themselves"
    );

    // And another account's share, the content half.
    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/shares?filename=theirs.bin")
                    .header(header::COOKIE, &victim),
                Body::from(b"content".to_vec()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let share_id = created["id"].as_str().expect("an id").to_owned();
    assert_eq!(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/shares/{share_id}"),
            Auth::Cookie(&helper)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN,
        "shares:delete:any"
    );

    // The owner can do all three.
    assert_eq!(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/shares/{share_id}"),
            Auth::Cookie(&owner)
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
}

/// The sequence the split must not break: hand over, then leave.
#[tokio::test]
async fn an_owner_can_promote_a_successor_and_step_down() {
    let app = TestApp::ready().await;
    let (owner, owner_id) = register(&app, "first").await;
    let (successor, successor_id) = register(&app, "second").await;

    expect_status(
        app.json(
            Method::PATCH,
            &format!("/api/v1/admin/users/{successor_id}"),
            Auth::Cookie(&owner),
            &serde_json::json!({ "is_admin": true }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    // The successor can do what the outgoing owner could.
    expect_status(
        app.json(
            Method::PATCH,
            &format!("/api/v1/admin/users/{owner_id}"),
            Auth::Cookie(&successor),
            &serde_json::json!({ "is_admin": false }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    assert_eq!(
        app.empty(Method::GET, "/api/v1/admin/users", Auth::Cookie(&owner))
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "they stepped down"
    );
}

/// Clearing space is one call, not one per share.
#[tokio::test]
async fn an_operator_can_end_everything_one_account_uploaded() {
    let app = TestApp::ready().await;
    let (owner, _) = register(&app, "first").await;
    let (uploader, id) = register(&app, "second").await;

    let mut codes = Vec::new();
    for name in ["one.bin", "two.bin", "three.bin"] {
        let created = body_json(
            expect_status(
                app.send(
                    Request::builder()
                        .method(Method::POST)
                        .uri(format!("/api/v1/shares?filename={name}"))
                        .header(header::COOKIE, &uploader),
                    Body::from(b"content".to_vec()),
                )
                .await,
                StatusCode::CREATED,
            )
            .await,
        )
        .await;
        codes.push(created["code"].as_str().expect("a code").to_owned());
    }

    let answer = body_json(
        expect_status(
            app.empty(
                Method::DELETE,
                &format!("/api/v1/admin/users/{id}/shares"),
                Auth::Cookie(&owner),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(answer["removed"], 3);

    for code in &codes {
        assert_eq!(
            app.empty(Method::GET, &format!("/s/{code}/download"), Auth::None)
                .await
                .status(),
            StatusCode::GONE,
            "{code}"
        );
    }

    // The bytes come back when the reaper reaches them, not in the request.
    tessera_server::maintenance::sweep_once(&app.state, OffsetDateTime::now_utc()).await;
    let stored = app.state.shares.total_stored_bytes().await.expect("query");
    assert_eq!(stored, 0, "the reaper reclaims what the update marked");
}

/// The account is untouched: this is about content, not about people.
#[tokio::test]
async fn ending_an_accounts_shares_leaves_the_account_alone() {
    let app = TestApp::ready().await;
    let (owner, _) = register(&app, "first").await;
    let (uploader, id) = register(&app, "second").await;

    expect_status(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/users/{id}/shares"),
            Auth::Cookie(&owner),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    assert_eq!(
        app.empty(Method::GET, "/api/v1/auth/session", Auth::Cookie(&uploader))
            .await
            .status(),
        StatusCode::OK,
        "they are still here"
    );
}

/// `guest` is seeded, holds nothing, and is a name `auth.default_role` can use.
///
/// The instance this exists for is one running `features.admin: false`: no role
/// endpoints, so only the seeded roles, and their permissions are locked. Before
/// this row its operator chose between open registration where every stranger
/// can upload and closed registration where no one can join.
#[tokio::test]
async fn a_new_account_can_be_given_nothing_at_all() {
    let app = TestApp::with_config(|config| {
        config.auth.default_role = "guest".to_owned();
    })
    .await;

    // The first account is the administrator and does not read the setting,
    // since an instance whose first account cannot administer it cannot be
    // repaired.
    register(&app, "first").await;
    let (guest, _) = register(&app, "second").await;
    let user = account(&app, "second").await;

    let held = app
        .state
        .roles
        .permissions_of(user.id)
        .await
        .expect("read the permissions");
    assert!(held.is_empty(), "guest holds {held:?}");

    // Signed in, and stopped at the first thing an account is for.
    //
    // Worth knowing before choosing this role: the refusal is about the
    // account, not the instance. With `features.anonymous_upload` on, the
    // default, the same person could sign out and upload as a stranger. So
    // `guest` restricts an account only on an instance that has already closed
    // the anonymous door; on one that has not, it is a label.
    assert_eq!(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares?filename=x.bin")
                .header("content-type", "application/octet-stream")
                .header("cookie", &guest),
            Body::from("hello".as_bytes().to_vec()),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
}

/// Deleting it would break an instance whose configuration names it.
#[tokio::test]
async fn guest_cannot_be_deleted() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;

    assert_eq!(
        app.empty(
            Method::DELETE,
            &format!("/api/v1/admin/roles/{}", tessera_core::RoleId::GUEST),
            Auth::Cookie(&admin),
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
}

/// The listing says how much each share has sent, not just how much it holds.
///
/// The instance-wide figure answers "what is this costing me". This is the
/// question after it: one link posted somewhere busy is how an operator's
/// transfer gets spent, and a total cannot say which link.
#[tokio::test]
async fn the_share_listing_says_what_each_one_has_sent() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;

    let created = body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/shares?filename=note.txt")
                    .header("content-type", "application/octet-stream")
                    .header("cookie", &admin),
                Body::from("0123456789".as_bytes().to_vec()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    let code = created
        .get("code")
        .and_then(serde_json::Value::as_str)
        .expect("a code")
        .to_owned();

    // Nothing sent yet.
    assert_eq!(admin_share(&app, &admin).await, 0);

    // A whole download, then a partial one. The second is why this cannot be
    // worked out from `size_bytes` and a count.
    //
    // The bodies are read, not dropped. The meter counts what leaves
    // through the response body, so a test that only looks at the status line
    // measures a transfer that never happened.
    let whole = expect_status(
        app.empty(Method::GET, &format!("/s/{code}/download"), Auth::None)
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(body_string(whole).await.len(), 10);

    let partial = expect_status(
        app.send(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/s/{code}/download"))
                .header("range", "bytes=0-3"),
            Body::empty(),
        )
        .await,
        StatusCode::PARTIAL_CONTENT,
    )
    .await;
    assert_eq!(body_string(partial).await.len(), 4);

    // The meter writes from a `Drop`, spawned, so it lands just after the body.
    for _ in 0..50 {
        if admin_share(&app, &admin).await == 14 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(
        admin_share(&app, &admin).await,
        14,
        "ten bytes whole plus four from the range"
    );
}

/// `bytes_served` of the one share in the listing.
#[expect(
    clippy::expect_used,
    reason = "test fixture; a panic is how it reports"
)]
async fn admin_share(app: &TestApp, admin: &str) -> u64 {
    let body = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/admin/shares", Auth::Cookie(admin))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    body.get("data")
        .and_then(serde_json::Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("bytes_served"))
        .and_then(serde_json::Value::as_u64)
        .expect("bytes_served in the listing")
}

/// One page of a share's access log.
async fn access_log_page(
    app: &TestApp,
    owner: &str,
    id: &str,
    limit: u64,
    offset: u64,
) -> serde_json::Value {
    let uri = format!("/api/v1/admin/shares/{id}/access-log?limit={limit}&offset={offset}");
    body_json(
        expect_status(
            app.empty(Method::GET, &uri, Auth::Cookie(owner)).await,
            StatusCode::OK,
        )
        .await,
    )
    .await
}

/// `offset` used to be read and ignored, so the second page of an access log
/// was the first one again, while the docs advertised the parameter.
#[tokio::test]
async fn the_access_log_is_paged_and_offset_moves() {
    let app = TestApp::with_config(|config| {
        config.share.access_log.enabled = true;
    })
    .await;
    let (owner, _) = register(&app, "first").await;
    let share = upload(&app).await;
    let code = share["code"].as_str().expect("a code").to_owned();

    // Three retrievals, so a page of one has somewhere to move. `/download`,
    // not `/raw`, as the other access-log tests use.
    for _ in 0..3 {
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
    }

    let id = share["id"].as_str().expect("an id").to_owned();

    let first = access_log_page(&app, &owner, &id, 1, 0).await;
    assert_eq!(
        first["meta"]["total"], 3,
        "the total is the collection, not the page"
    );
    assert_eq!(first["meta"]["limit"], 1);
    assert_eq!(first["meta"]["offset"], 0);
    assert_eq!(first["meta"]["enabled"], true);
    assert_eq!(first["data"].as_array().map(Vec::len), Some(1));

    let second = access_log_page(&app, &owner, &id, 1, 1).await;
    assert_eq!(second["meta"]["offset"], 1);
    assert_eq!(second["data"].as_array().map(Vec::len), Some(1));

    // Past the end is an empty page, not the first one again.
    let past = access_log_page(&app, &owner, &id, 1, 9).await;
    assert_eq!(past["meta"]["total"], 3);
    assert!(past["data"].as_array().is_some_and(Vec::is_empty));
}

/// An anonymous share, for the access-log tests below.
async fn upload(app: &TestApp) -> serde_json::Value {
    body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/shares?filename=fetched.bin"),
                Body::from(b"content".to_vec()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await
}

/// The record is for whoever runs the instance, because the switch is theirs.
/// An ordinary member seeing visitors' addresses would be a second decision
/// about a visitor's privacy that no operator agreed to.
#[tokio::test]
async fn only_an_administrator_can_read_a_share_s_access_log() {
    let app = TestApp::ready().await;
    register(&app, "first").await;
    let (member, _) = register(&app, "second").await;

    let share = upload(&app).await;
    let id = share["id"].as_str().expect("an id").to_owned();

    assert_eq!(
        app.empty(
            Method::GET,
            &format!("/api/v1/admin/shares/{id}/access-log"),
            Auth::Cookie(&member),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
}

/// An empty list means the share was never fetched, or nothing is being written
/// down. A screen that cannot tell them apart reports the second as the first,
/// so the answer says which.
#[tokio::test]
async fn the_access_log_says_whether_it_is_even_recording() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;

    let share = upload(&app).await;
    let id = share["id"].as_str().expect("an id").to_owned();

    let body = body_json(
        expect_status(
            app.empty(
                Method::GET,
                &format!("/api/v1/admin/shares/{id}/access-log"),
                Auth::Cookie(&admin),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;

    assert_eq!(body["meta"]["enabled"], false, "the log is off by default");
    assert!(body["data"].as_array().expect("a list").is_empty());
}

/// And when it is on, an administrator can see what was recorded.
#[tokio::test]
async fn an_administrator_reads_what_was_recorded() {
    let app = TestApp::with_config(|config| {
        config.share.access_log.enabled = true;
    })
    .await;
    let (admin, _) = register(&app, "first").await;

    let share = upload(&app).await;
    let id = share["id"].as_str().expect("an id").to_owned();
    let code = share["code"].as_str().expect("a code").to_owned();

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

    let body = body_json(
        expect_status(
            app.empty(
                Method::GET,
                &format!("/api/v1/admin/shares/{id}/access-log"),
                Auth::Cookie(&admin),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;

    assert_eq!(body["meta"]["enabled"], true);
    let events = body["data"].as_array().expect("a list");
    assert_eq!(events.len(), 1, "one download, one row: {body}");
    assert_eq!(events[0]["user_agent"], "Tessera-Test/1.0");
    assert!(
        events[0]["occurred_at"].is_string(),
        "the time is the point: {body}"
    );
}

/// The list has to include what cannot be changed, or the question becomes
/// where the
/// port setting went, and it has to say why, or the greyed-out field is a
/// mystery.
#[tokio::test]
async fn the_settings_list_marks_what_cannot_be_changed() {
    let app = TestApp::ready().await;
    let (owner, _) = register(&app, "first").await;

    let body = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/admin/settings", Auth::Cookie(&owner))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;

    let settings = body["settings"].as_array().expect("a list");
    let find = |key: &str| {
        settings
            .iter()
            .find(|entry| entry["key"] == key)
            .unwrap_or_else(|| panic!("{key} should be listed"))
    };

    assert_eq!(find("share.code_length")["editable"], true);
    assert_eq!(find("share.code_length")["source"], "default");

    let port = find("server.port");
    assert_eq!(port["editable"], false);
    assert_eq!(port["refusal"], "fixed_at_boot");

    // Never storable, and named as such.
    assert_eq!(find("database.url")["refusal"], "secret");
    assert_eq!(find("server.data_dir")["refusal"], "fixed_at_boot");

    // The other kind: writable, encrypted at rest, and reported as a boolean
    // for whether one is set instead of the credential.
    let relay = find("mail.password");
    assert_eq!(relay["editable"], true);
    assert_eq!(relay["secret"], true);
    assert_eq!(relay["value"], false, "nothing is configured yet");
    assert!(relay["refusal"].is_null());
}

/// A credential set from the panel comes back as "configured", never as
/// itself, and the stored value is not the value sent.
#[tokio::test]
async fn a_credential_set_from_the_panel_is_never_handed_back() {
    // No key configured: the one generated in the data directory seals it, as
    // on a fresh instance with no variable set.
    let app = TestApp::ready().await;
    let (owner, _) = register(&app, "first").await;

    expect_status(
        app.json(
            Method::PUT,
            "/api/v1/admin/settings",
            Auth::Cookie(&owner),
            &serde_json::json!({
                "changes": { "mail.password": "a-relay-password" },
            }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    let body = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/admin/settings", Auth::Cookie(&owner))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;

    let listed = body["settings"].as_array().expect("a list");
    let relay = listed
        .iter()
        .find(|entry| entry["key"] == "mail.password")
        .expect("the setting");
    assert_eq!(relay["value"], true, "it should read as configured");
    assert!(
        !body.to_string().contains("a-relay-password"),
        "the credential came back out of the settings endpoint"
    );

    // And the process uses it, which is the point of storing it.
    assert_eq!(app.state.config().mail.password, "a-relay-password");
}

/// What the feature is for: change it here and the next request uses it,
/// with no restart.
#[tokio::test]
async fn a_changed_setting_applies_to_the_next_request() {
    let app = TestApp::ready().await;
    let (owner, _) = register(&app, "first").await;

    expect_status(
        app.json(
            Method::PUT,
            "/api/v1/admin/settings",
            Auth::Cookie(&owner),
            &serde_json::json!({ "changes": { "features.anonymous_upload": false } }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    assert_eq!(
        app.send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/shares"),
            Body::from(b"anonymous".to_vec()),
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED,
        "the change should be in force immediately"
    );
}

/// Sending null is the way back to whatever the deployment said. Without it a
/// setting could be changed but never un-changed.
#[tokio::test]
async fn null_clears_a_setting_back_to_the_layer_below() {
    let app = TestApp::ready().await;
    let (owner, _) = register(&app, "first").await;

    let set = async |value: serde_json::Value| {
        body_json(
            expect_status(
                app.json(
                    Method::PUT,
                    "/api/v1/admin/settings",
                    Auth::Cookie(&owner),
                    &serde_json::json!({ "changes": { "share.code_length": value } }),
                )
                .await,
                StatusCode::OK,
            )
            .await,
        )
        .await
    };

    let after = set(serde_json::json!(12)).await;
    let entry = |body: &serde_json::Value| {
        body["settings"]
            .as_array()
            .expect("a list")
            .iter()
            .find(|entry| entry["key"] == "share.code_length")
            .expect("listed")
            .clone()
    };
    assert_eq!(entry(&after)["value"], 12);
    assert_eq!(entry(&after)["source"], "panel");

    let after = set(serde_json::Value::Null).await;
    assert_eq!(entry(&after)["source"], "default", "back to the default");
}

/// A value the configuration refuses is refused here, naming the key. The
/// start-up fallback that drops a bad row is a safety net, not the
/// validation path; leaning on it would mean a save that succeeds and reverts
/// on restart.
#[tokio::test]
async fn a_value_the_configuration_refuses_is_rejected_with_its_key() {
    let app = TestApp::ready().await;
    let (owner, _) = register(&app, "first").await;

    let response = app
        .json(
            Method::PUT,
            "/api/v1/admin/settings",
            Auth::Cookie(&owner),
            // Below `RetrievalCode::MIN_LENGTH`.
            &serde_json::json!({ "changes": { "share.code_length": 2 } }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        body_string(response).await.contains("code_length"),
        "the message should name the setting"
    );
}

/// A request naming one bad key must change none of them, or the operator has
/// to work out which half went through.
#[tokio::test]
async fn one_bad_key_leaves_the_whole_request_unapplied() {
    let app = TestApp::ready().await;
    let (owner, _) = register(&app, "first").await;

    expect_status(
        app.json(
            Method::PUT,
            "/api/v1/admin/settings",
            Auth::Cookie(&owner),
            &serde_json::json!({ "changes": {
                "share.ticket_ttl_hours": 3,
                "server.port": 9999,
            }}),
        )
        .await,
        StatusCode::BAD_REQUEST,
    )
    .await;

    let body = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/admin/settings", Auth::Cookie(&owner))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    let ttl = body["settings"]
        .as_array()
        .expect("a list")
        .iter()
        .find(|entry| entry["key"] == "share.ticket_ttl_hours")
        .expect("listed");
    assert_eq!(
        ttl["source"], "default",
        "the good half must not have been applied either"
    );
}

/// `admin` fields questions about the instance, so it reads. Changing how the
/// instance runs belongs with the role that can already grant itself anything.
#[tokio::test]
async fn admin_reads_settings_but_does_not_write_them() {
    let app = TestApp::ready().await;
    let (owner, _) = register(&app, "first").await;
    let (helper, _) = register(&app, "second").await;

    let helper_id = app
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
            &format!("/api/v1/admin/users/{helper_id}/roles"),
            Auth::Cookie(&owner),
            &serde_json::json!({ "role": tessera_core::RoleId::ADMIN }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    expect_status(
        app.empty(Method::GET, "/api/v1/admin/settings", Auth::Cookie(&helper))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        app.json(
            Method::PUT,
            "/api/v1/admin/settings",
            Auth::Cookie(&helper),
            &serde_json::json!({ "changes": { "share.code_length": 9 } }),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
}

/// The listing is read over HTTP and pasted into issues. `database.url` can
/// embed a password, so the endpoint names it but withholds its value.
#[tokio::test]
async fn the_settings_endpoint_never_returns_a_credential() {
    let app = TestApp::ready().await;
    let (owner, _) = register(&app, "first").await;

    let response = app
        .empty(Method::GET, "/api/v1/admin/settings", Auth::Cookie(&owner))
        .await;
    let raw = body_string(expect_status(response, StatusCode::OK).await).await;

    let body: serde_json::Value = serde_json::from_str(&raw).expect("json");
    let url = body["settings"]
        .as_array()
        .expect("a list")
        .iter()
        .find(|entry| entry["key"] == "database.url")
        .expect("it should still be listed, so nobody asks where it went");
    assert_eq!(url["refusal"], "secret");
    assert!(url["value"].is_null(), "the value must not come back");
}

/// Shortening a retention means the next sweep removes things that were going
/// to be kept, and nothing brings them back. An empty "are you sure?" is a
/// button everyone clicks; a number is what gives pause.
#[tokio::test]
async fn shortening_a_retention_is_refused_until_it_is_confirmed() {
    let app = TestApp::ready().await;
    let (owner, _) = register(&app, "first").await;

    // A share that died a long time ago, so any shorter retention reaches it.
    let share = upload(&app).await;
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");
    let long_ago = OffsetDateTime::now_utc() - time::Duration::days(60);
    app.state
        .shares
        .update_settings(
            id,
            tessera_server::db::repository::share::Change::Set(Some(long_ago)),
            tessera_server::db::repository::share::Change::Keep,
            tessera_server::db::repository::share::Change::Keep,
            OffsetDateTime::now_utc(),
        )
        .await
        .expect("expire it");

    let change = serde_json::json!({ "changes": { "share.retain_after_death_hours": 1 } });

    let refused = app
        .json(
            Method::PUT,
            "/api/v1/admin/settings",
            Auth::Cookie(&owner),
            &change,
        )
        .await;
    assert_eq!(refused.status(), StatusCode::CONFLICT);
    let message = body_string(refused).await;
    assert!(
        message.contains("shares"),
        "the refusal should say what goes: {message}"
    );

    // The same change, confirmed, goes through.
    expect_status(
        app.json(
            Method::PUT,
            "/api/v1/admin/settings",
            Auth::Cookie(&owner),
            &serde_json::json!({
                "changes": { "share.retain_after_death_hours": 1 },
                "confirm": true,
            }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
}

/// The numbers have to be available on their own, so a screen can show a table
/// instead of parsing them out of an error message.
#[tokio::test]
async fn preview_reports_what_a_change_would_destroy() {
    let app = TestApp::ready().await;
    let (owner, _) = register(&app, "first").await;

    let share = upload(&app).await;
    let id = tessera_core::ShareId::parse(share["id"].as_str().expect("an id")).expect("an id");
    let long_ago = OffsetDateTime::now_utc() - time::Duration::days(60);
    app.state
        .shares
        .update_settings(
            id,
            tessera_server::db::repository::share::Change::Set(Some(long_ago)),
            tessera_server::db::repository::share::Change::Keep,
            tessera_server::db::repository::share::Change::Keep,
            OffsetDateTime::now_utc(),
        )
        .await
        .expect("expire it");

    let body = body_json(
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/admin/settings/preview",
                Auth::Cookie(&owner),
                &serde_json::json!({ "changes": { "share.retain_after_death_hours": 1 } }),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;

    let consequences = body["consequences"].as_array().expect("a list");
    assert_eq!(consequences.len(), 1, "{body}");
    assert_eq!(consequences[0]["key"], "share.retain_after_death_hours");
    assert_eq!(consequences[0]["count"], 1);

    // And the preview changed nothing.
    let settings = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/admin/settings", Auth::Cookie(&owner))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    let retention = settings["settings"]
        .as_array()
        .expect("a list")
        .iter()
        .find(|entry| entry["key"] == "share.retain_after_death_hours")
        .expect("listed");
    assert_eq!(retention["source"], "default", "a preview must not apply");
}

/// Lengthening a retention keeps more, which destroys nothing. Warning about
/// both directions would train an operator to click through the warning.
#[tokio::test]
async fn lengthening_a_retention_needs_no_confirmation() {
    let app = TestApp::ready().await;
    let (owner, _) = register(&app, "first").await;

    expect_status(
        app.json(
            Method::PUT,
            "/api/v1/admin/settings",
            Auth::Cookie(&owner),
            &serde_json::json!({ "changes": { "share.retain_after_death_hours": 24 * 30 } }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
}

// ── quotas over HTTP ────────────────────────────────────────────────

/// A role's quota has to travel with the rest of the role, or the panel would
/// need a second screen for one number.
#[tokio::test]
async fn a_role_carries_its_quota_through_the_api() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;

    let created = body_json(
        expect_status(
            app.json(
                Method::POST,
                "/api/v1/admin/roles",
                Auth::Cookie(&admin),
                &serde_json::json!({
                    "name": "generous",
                    "permissions": ["shares:create"],
                    "quota_bytes": 10_737_418_240_u64,
                }),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await;
    assert_eq!(created["quota_bytes"], 10_737_418_240_u64);

    // And an edit can take it away again, back to the instance ceiling.
    let id = created["id"].as_str().expect("an id").to_owned();
    let updated = body_json(
        expect_status(
            app.json(
                Method::PUT,
                &format!("/api/v1/admin/roles/{id}"),
                Auth::Cookie(&admin),
                &serde_json::json!({
                    "name": "generous",
                    "permissions": ["shares:create"],
                    "quota_bytes": serde_json::Value::Null,
                }),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(updated["quota_bytes"], serde_json::Value::Null);
}

/// Absent and `null` are opposite instructions on this field, so a patch that
/// does not mention it must leave it alone.
#[tokio::test]
async fn an_account_quota_survives_a_patch_that_does_not_mention_it() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    let (_, id) = register(&app, "second").await;

    let set = body_json(
        expect_status(
            app.json(
                Method::PATCH,
                &format!("/api/v1/admin/users/{id}"),
                Auth::Cookie(&admin),
                &serde_json::json!({ "is_admin": false, "quota_bytes": 4096 }),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(set["quota_bytes"], 4096);

    // A patch about something else entirely.
    let untouched = body_json(
        expect_status(
            app.json(
                Method::PATCH,
                &format!("/api/v1/admin/users/{id}"),
                Auth::Cookie(&admin),
                &serde_json::json!({ "is_admin": false, "state": "active" }),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(untouched["quota_bytes"], 4096, "an absent field cleared it");

    // `null` is the way to clear it, and it has to actually work.
    let cleared = body_json(
        expect_status(
            app.json(
                Method::PATCH,
                &format!("/api/v1/admin/users/{id}"),
                Auth::Cookie(&admin),
                &serde_json::json!({
                    "is_admin": false,
                    "quota_bytes": serde_json::Value::Null,
                }),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(cleared["quota_bytes"], serde_json::Value::Null);
}

// ── storage backends ────────────────────────────────────────────────

/// The credentials are the reason this is a table and not a settings row, and
/// a surface that echoed them back would leave the sealing protecting the
/// database against everybody except the one request that asks nicely.
#[tokio::test]
async fn the_listing_never_sends_a_credential() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;

    expect_status(
        app.json(
            Method::POST,
            "/api/v1/admin/storage/backends",
            Auth::Cookie(&admin),
            &serde_json::json!({
                "name": "cold",
                "url": "memory://",
                "options": { "aws_region": "garage" },
            }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    let listed = body_string(
        expect_status(
            app.empty(
                Method::GET,
                "/api/v1/admin/storage/backends",
                Auth::Cookie(&admin),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert!(listed.contains("cold"), "the backend is not listed at all");
    assert!(!listed.contains("memory://"), "sent the URL back");
    assert!(!listed.contains("garage"), "sent the options back");
}

/// A typo is something the operator can fix while the form is still in front of
/// them. Storing it first would surface the failure as a later upload going
/// wrong later.
#[tokio::test]
async fn a_backend_that_will_not_open_is_refused_before_it_is_stored() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;

    let refused = app
        .json(
            Method::POST,
            "/api/v1/admin/storage/backends",
            Auth::Cookie(&admin),
            &serde_json::json!({ "name": "broken", "url": "nonsense://what" }),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);

    let listed = body_string(
        expect_status(
            app.empty(
                Method::GET,
                "/api/v1/admin/storage/backends",
                Auth::Cookie(&admin),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert!(
        !listed.contains("broken"),
        "stored a backend that will not open"
    );
}

/// The backend a fresh instance starts with is an ordinary row: its switch
/// flips, and once empty it can be removed.
#[tokio::test]
async fn the_seeded_backend_is_an_ordinary_one() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;

    let switched = body_json(
        expect_status(
            app.json(
                Method::PATCH,
                "/api/v1/admin/storage/backends/default",
                Auth::Cookie(&admin),
                &serde_json::json!({ "direct_access": true }),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(switched["is_default"], true, "{switched}");
    assert_eq!(switched["direct_access"], true, "{switched}");

    expect_status(
        app.empty(
            Method::DELETE,
            "/api/v1/admin/storage/backends/default",
            Auth::Cookie(&admin),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert!(app.state.backend("default").is_none());
}

/// Adding a bucket that needs a restart is the thing this whole feature exists
/// to stop, so the new one has to be reachable to the next request.
#[tokio::test]
async fn a_new_backend_is_usable_without_a_restart() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    assert!(app.state.backend("cold").is_none());

    expect_status(
        app.json(
            Method::POST,
            "/api/v1/admin/storage/backends",
            Auth::Cookie(&admin),
            &serde_json::json!({ "name": "cold", "url": "memory://" }),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    assert!(
        app.state.backend("cold").is_some(),
        "the backend was stored but the process cannot reach it"
    );
}

/// Flipping the switch must not require the credentials again: they are sealed
/// and never shown, so resending the whole backend would mean digging up a
/// bucket's secret key to change one boolean.
#[tokio::test]
async fn direct_access_is_switched_without_sending_the_credentials_again() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    expect_status(
        app.json(
            Method::POST,
            "/api/v1/admin/storage/backends",
            Auth::Cookie(&admin),
            &serde_json::json!({ "name": "cold", "url": "memory://" }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert!(!app.state.direct_access("cold"), "off unless asked for");

    let switched = body_json(
        expect_status(
            app.json(
                Method::PATCH,
                "/api/v1/admin/storage/backends/cold",
                Auth::Cookie(&admin),
                &serde_json::json!({ "direct_access": true }),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(switched["direct_access"], true, "{switched}");
    assert!(
        app.state.direct_access("cold"),
        "the switch did not reach the pool"
    );
    assert!(
        app.state.backend("cold").is_some(),
        "switching it lost the credentials it opens with"
    );

    expect_status(
        app.json(
            Method::PATCH,
            "/api/v1/admin/storage/backends/cold",
            Auth::Cookie(&admin),
            &serde_json::json!({ "direct_access": false }),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert!(!app.state.direct_access("cold"));
}

/// A name that matches no backend is `404`, not a switch set on nothing.
#[tokio::test]
async fn switching_a_backend_that_is_not_there_is_not_found() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    expect_status(
        app.json(
            Method::PATCH,
            "/api/v1/admin/storage/backends/nothing-by-this-name",
            Auth::Cookie(&admin),
            &serde_json::json!({ "direct_access": true }),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
}

/// Reading settings is not writing them, and a backend is a credential.
#[tokio::test]
async fn writing_a_backend_needs_more_than_reading_one() {
    let app = TestApp::ready().await;
    let (owner, _) = register(&app, "first").await;
    let (watcher, _) = register(&app, "second").await;

    let user = app
        .state
        .users
        .find_by_username(&tessera_core::Username::parse("second").expect("a name"))
        .await
        .expect("query")
        .expect("the account");
    let role = app
        .state
        .roles
        .create(
            RoleFields {
                name: "reader",
                description: None,
                quota_bytes: None,
                max_file_bytes: None,
                permissions: &[tessera_core::Permission::SettingsRead]
                    .into_iter()
                    .collect(),
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .expect("create the role");
    app.state
        .roles
        .set_role_of(user.id, Some(role.id), OffsetDateTime::now_utc())
        .await
        .expect("grant it");

    expect_status(
        app.empty(
            Method::GET,
            "/api/v1/admin/storage/backends",
            Auth::Cookie(&watcher),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        app.json(
            Method::POST,
            "/api/v1/admin/storage/backends",
            Auth::Cookie(&watcher),
            &serde_json::json!({ "name": "cold", "url": "memory://" }),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN,
    );
    drop(owner);
}

/// A role name where its id belongs is a malformed request, not a missing
/// role: the fix is on the client, and a `404` sent the panel looking for a
/// role that exists.
#[tokio::test]
async fn a_role_given_by_name_is_refused_as_malformed() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    let (_, id) = register(&app, "second").await;

    let refused = body_json(
        expect_status(
            app.json(
                Method::PUT,
                &format!("/api/v1/admin/users/{id}/roles"),
                Auth::Cookie(&admin),
                &serde_json::json!({ "role": "admin" }),
            )
            .await,
            StatusCode::BAD_REQUEST,
        )
        .await,
    )
    .await;
    assert_eq!(refused["code"], "value_invalid", "{refused}");
    assert_eq!(refused["details"]["field"], "role");
    assert_eq!(refused["details"]["rule"], "not_an_identifier");
}

/// Reads the administrative share listing.
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test fixture; a panic is how it reports"
)]
async fn admin_share_rows(app: &TestApp, cookie: &str) -> Vec<serde_json::Value> {
    let page = body_json(
        expect_status(
            app.empty(Method::GET, "/api/v1/admin/shares", Auth::Cookie(cookie))
                .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    page["data"].as_array().expect("rows").clone()
}

/// Uploads a share as `cookie`'s account with `query`, returning the response.
async fn upload_with(app: &TestApp, cookie: &str, query: &str) -> serde_json::Value {
    body_json(
        expect_status(
            app.send(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/shares{query}"))
                    .header(header::COOKIE, cookie),
                Body::from(b"bytes".to_vec()),
            )
            .await,
            StatusCode::CREATED,
        )
        .await,
    )
    .await
}

/// Uploads a share as `cookie`'s account and returns its code and id.
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test fixture; a panic is how it reports"
)]
async fn upload_as(app: &TestApp, cookie: &str) -> (String, String) {
    let share = upload_with(app, cookie, "?filename=a.bin").await;
    (
        share["code"].as_str().expect("a code").to_owned(),
        share["id"].as_str().expect("an id").to_owned(),
    )
}

/// An expired share's row goes `share.retain_after_death_hours` after its
/// expiry; a live one has no such date.
#[tokio::test]
async fn the_listing_says_when_an_expired_share_goes() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    let retention = time::Duration::try_from(app.state.config().share.retention())
        .expect("a retention that fits");
    upload_with(&app, &admin, "?filename=live.bin").await;
    let expired = upload_with(&app, &admin, "?filename=expired.bin").await;
    let expired_at = OffsetDateTime::now_utc() - time::Duration::hours(2);
    app.state
        .shares
        .update_settings(
            tessera_core::ShareId::parse(expired["id"].as_str().expect("an id")).expect("an id"),
            tessera_server::db::repository::share::Change::Set(Some(expired_at)),
            tessera_server::db::repository::share::Change::Keep,
            tessera_server::db::repository::share::Change::Keep,
            OffsetDateTime::now_utc(),
        )
        .await
        .expect("expire it");

    let rows = admin_share_rows(&app, &admin).await;
    let row = |name: &str| {
        rows.iter()
            .find(|row| row["filename"] == name)
            .cloned()
            .expect("the row")
    };
    let live = row("live.bin");
    assert_eq!(live["content_removed"], false);
    assert!(live["purges_at"].is_null());

    let expired = row("expired.bin");
    assert_eq!(expired["status"], "expired");
    let purges_at = OffsetDateTime::parse(
        expired["purges_at"].as_str().expect("a date"),
        &time::format_description::well_known::Rfc3339,
    )
    .expect("RFC 3339");
    assert!(
        (purges_at - (expired_at + retention)).abs() < time::Duration::seconds(1),
        "{expired}"
    );
}

/// A share that ran out of uses has a removal date from its last retrieval,
/// and once the sweep has run its content is reported as removed.
#[tokio::test]
async fn the_listing_says_when_a_used_up_shares_content_is_gone() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    let once = upload_with(&app, &admin, "?filename=once.bin&max_uses=1").await;
    let code = once["code"].as_str().expect("a code").to_owned();
    expect_status(
        app.empty(Method::GET, &format!("/s/{code}/download"), Auth::None)
            .await,
        StatusCode::OK,
    )
    .await;

    let rows = admin_share_rows(&app, &admin).await;
    let row = rows.first().expect("the row");
    assert_eq!(row["status"], "exhausted");
    assert!(!row["purges_at"].is_null(), "{row}");
    assert_eq!(row["content_removed"], false, "not swept yet");

    tessera_server::maintenance::sweep_once(&app.state, OffsetDateTime::now_utc()).await;
    let rows = admin_share_rows(&app, &admin).await;
    let row = rows.first().expect("the row");
    assert_eq!(row["content_removed"], true, "{row}");
    assert!(!row["purges_at"].is_null(), "{row}");
}

/// A batch delete removes the caller's own shares and reports the rest one by
/// one, someone else's answering like one that does not exist.
#[tokio::test]
async fn a_batch_delete_removes_only_the_callers_shares() {
    let app = TestApp::ready().await;
    register(&app, "first").await;
    let (alice, _) = register(&app, "alice").await;
    let (bob, _) = register(&app, "bob").await;
    let (a1, _) = upload_as(&app, &alice).await;
    let (a2, _) = upload_as(&app, &alice).await;
    let (b1, _) = upload_as(&app, &bob).await;

    let result = body_json(
        expect_status(
            app.json(
                Method::DELETE,
                "/api/v1/shares",
                Auth::Cookie(&alice),
                &serde_json::json!({ "codes": [a1, a2, b1, "ZZZZZZZZ", a1] }),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(result["deleted"], serde_json::json!([a1, a2]));
    assert_eq!(
        result["failed"],
        serde_json::json!([
            { "code": b1, "error": "not_found" },
            { "code": "ZZZZZZZZ", "error": "not_found" },
        ])
    );
    assert_eq!(
        app.empty(Method::GET, &format!("/s/{a1}/download"), Auth::None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.empty(Method::GET, &format!("/s/{b1}/download"), Auth::None)
            .await
            .status(),
        StatusCode::OK,
        "someone else's share is untouched"
    );

    let too_many: Vec<String> = (0..201).map(|n| format!("{n:08}")).collect();
    expect_status(
        app.json(
            Method::DELETE,
            "/api/v1/shares",
            Auth::Cookie(&alice),
            &serde_json::json!({ "codes": too_many }),
        )
        .await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    expect_status(
        app.json(
            Method::DELETE,
            "/api/v1/shares",
            Auth::None,
            &serde_json::json!({ "codes": [] }),
        )
        .await,
        StatusCode::UNAUTHORIZED,
    )
    .await;
}

/// The administrative batch removes anyone's shares; an ordinary account may
/// not use it.
#[tokio::test]
async fn an_administrator_batch_deletes_anyones_shares() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;
    let (alice, _) = register(&app, "alice").await;
    let (a1, a1_id) = upload_as(&app, &alice).await;
    let (_a2, a2_id) = upload_as(&app, &alice).await;

    expect_status(
        app.json(
            Method::DELETE,
            "/api/v1/admin/shares",
            Auth::Cookie(&alice),
            &serde_json::json!({ "ids": [a1_id] }),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;

    let result = body_json(
        expect_status(
            app.json(
                Method::DELETE,
                "/api/v1/admin/shares",
                Auth::Cookie(&admin),
                &serde_json::json!({ "ids": [a1_id, a2_id, "not-an-id"] }),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(result["deleted"], serde_json::json!([a1_id, a2_id]));
    assert_eq!(
        result["failed"],
        serde_json::json!([{ "id": "not-an-id", "error": "not_found" }])
    );
    assert_eq!(
        app.empty(Method::GET, &format!("/s/{a1}/download"), Auth::None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}
