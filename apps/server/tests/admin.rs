//! What an administrator can do to other people's accounts, over HTTP.

mod common;

use axum::http::{Method, StatusCode};
use common::{Auth, TestApp, body_json, expect_status};
use tessera_server::config::RegistrationPolicy;

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
    body_json(response)
        .await
        .as_array()
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

/// Everybody else gets the same answer as somebody who is not signed in would
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

/// An instance with no administrator cannot be repaired through the API — there
/// is no endpoint left that would grant the flag back. The way out is the
/// command line against the database file, which somebody who locked themselves
/// out of a container may not have.
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

/// And stepping down is allowed once somebody else can do the job.
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
/// the instance in exactly the state the demotion guard refuses.
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
    assert_eq!(list(&app, Auth::Cookie(&admin)).await.len(), 1);
}

/// An identifier that is not an identifier answers the same as one that names
/// nobody.
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

/// A leaked token must not be able to promote an account it controls, which is
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
/// Even the first account needs one — an operator would not hit this, because
/// the first administrator is created at start-up rather than registered, but a
/// test that makes accounts through the API does.
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test fixture; a panic is how it reports"
)]
async fn register_invited(app: &TestApp, username: &str) -> (String, String) {
    let now = time::OffsetDateTime::now_utc();
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

/// The command line is the rescue path — it works with the service stopped.
/// This is the ordinary one: letting a friend in should not need a shell on the
/// box.
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

    // And it actually admits somebody.
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
/// nothing to show — an administrator who lost the code issues another one.
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

/// What somebody who posted a code in the wrong chat window wants: it stops
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
/// why it stopped working rather than wondering where it went.
#[tokio::test]
async fn an_expired_code_is_listed_as_inactive() {
    let app = TestApp::ready().await;
    let (admin, _) = register(&app, "first").await;

    let now = time::OffsetDateTime::now_utc();
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

    let now = time::OffsetDateTime::now_utc();
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
/// seeing it is the point: a large one means retention is longer than somebody
/// expected, or the sweep is not running.
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

    let past = time::OffsetDateTime::now_utc() - time::Duration::hours(2);
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

/// The check is the role, not the column.
///
/// `users.is_admin` is still written so this version can be rolled back, so
/// every other test here passes whichever one is consulted. This one makes them
/// disagree: the column says administrator, the roles say nothing at all.
#[tokio::test]
async fn the_stale_is_admin_column_grants_nothing() {
    let app = TestApp::ready().await;
    register(&app, "first").await;
    let (ordinary, _) = register(&app, "second").await;

    let statement = sea_orm::Statement::from_string(
        sea_orm::ConnectionTrait::get_database_backend(&app.state.db),
        "UPDATE users SET is_admin = 1 WHERE username = 'second'".to_owned(),
    );
    sea_orm::ConnectionTrait::execute_raw(&app.state.db, statement)
        .await
        .expect("set the old flag");

    assert_eq!(
        app.empty(Method::GET, "/api/v1/admin/users", Auth::Cookie(&ordinary))
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "the column must not be what decides"
    );
}

/// One permission at a time, which is the point of the change.
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
            "auditor".to_owned(),
            Some("Can look, cannot touch.".to_owned()),
            &[tessera_core::Permission::UsersRead].into_iter().collect(),
            time::OffsetDateTime::now_utc(),
        )
        .await
        .expect("create the role");
    app.state
        .roles
        .set_roles_of(user.id, &[role.id], time::OffsetDateTime::now_utc())
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

/// Somebody in no role is signed in and allowed nothing, and that has to be a
/// clean refusal rather than a crash.
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
        .set_roles_of(user.id, &[], time::OffsetDateTime::now_utc())
        .await
        .expect("take every role away");

    assert_eq!(
        app.empty(Method::GET, "/api/v1/admin/users", Auth::Cookie(&stripped))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
}

/// The instance must keep somebody who can hand out roles.
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
    // And somebody who never held them is not caught by the same guard.
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
