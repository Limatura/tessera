//! What an administrator can do to other people's accounts, over HTTP.

mod common;

use axum::http::{Method, StatusCode};
use common::{Auth, TestApp, body_json, expect_status};

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
