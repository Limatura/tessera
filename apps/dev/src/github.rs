//! A fake GitHub, reached the way GitHub Enterprise Server is: under a base
//! URL, with the API at `/api/v3`.
//!
//! The sign-in page asks who to be; the token and user endpoints then answer
//! for that person. One person at a time, which is all a development machine
//! signs in as.

use std::sync::Arc;

use axum::{
    Form, Json, Router,
    extract::{Query, State},
    response::{IntoResponse as _, Redirect, Response},
    routing::{get, post},
};
use parking_lot::Mutex;
use serde::Deserialize;

use crate::page::{self, Field};

#[derive(Clone, Default)]
struct Shared {
    /// Whoever signed in last. The access token stands for them.
    who: Arc<Mutex<Option<Person>>>,
}

#[derive(Clone)]
struct Person {
    id: u64,
    login: String,
    name: String,
    email: String,
    verified: bool,
}

pub(crate) fn router() -> Router {
    Router::new()
        .route("/login/oauth/authorize", get(ask).post(answer))
        .route("/login/oauth/access_token", post(token))
        .route("/api/v3/user", get(user))
        .route("/api/v3/user/emails", get(emails))
        .with_state(Shared::default())
}

async fn ask(Query(request): Query<Vec<(String, String)>>) -> Response {
    page::sign_in(
        "GitHub",
        "/login/oauth/authorize",
        &request,
        &[
            Field {
                name: "login",
                label: "Login",
                value: "octocat",
                hint: "Suggested as the username.",
            },
            Field {
                name: "id",
                label: "Account id",
                value: "4242",
                hint: "What GitHub accounts are matched on. The same id is the same person.",
            },
            Field {
                name: "email",
                label: "Primary email",
                value: "octocat@example.com",
                hint: "",
            },
            Field {
                name: "name",
                label: "Name",
                value: "The Octocat",
                hint: "",
            },
            Field {
                name: "verified",
                label: "Email verified",
                value: "true",
                hint: "Anything but true is sent as false.",
            },
        ],
    )
    .into_response()
}

#[derive(Deserialize)]
struct Answer {
    redirect_uri: String,
    state: String,
    login: String,
    id: String,
    email: String,
    name: String,
    verified: String,
}

async fn answer(State(shared): State<Shared>, Form(answer): Form<Answer>) -> Response {
    *shared.who.lock() = Some(Person {
        id: answer.id.trim().parse().unwrap_or(4242),
        login: answer.login,
        name: answer.name,
        email: answer.email,
        verified: answer.verified.trim() == "true",
    });
    let separator = if answer.redirect_uri.contains('?') {
        '&'
    } else {
        '?'
    };
    Redirect::to(&format!(
        "{}{separator}code={}&state={}",
        answer.redirect_uri,
        page::code(),
        answer.state
    ))
    .into_response()
}

async fn token() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "access_token": "fake-access-token",
        "token_type": "bearer",
        "scope": "read:user,user:email",
    }))
}

async fn user(State(shared): State<Shared>) -> Json<serde_json::Value> {
    let who = shared.who.lock().clone();
    Json(who.map_or_else(
        || serde_json::json!({ "id": 4242, "login": "octocat" }),
        |person| serde_json::json!({ "id": person.id, "login": person.login, "name": person.name }),
    ))
}

async fn emails(State(shared): State<Shared>) -> Json<serde_json::Value> {
    let who = shared.who.lock().clone();
    Json(who.map_or_else(
        || serde_json::json!([]),
        |person| {
            serde_json::json!([
                { "email": person.email, "primary": true, "verified": person.verified },
            ])
        },
    ))
}
