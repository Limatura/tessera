//! A fake `OpenID Connect` provider.
//!
//! The discovery document, a sign-in page, and a token endpoint that returns
//! an ID token for whoever was typed in. The token is unsigned: Tessera takes
//! it from the token endpoint over a connection it opened, which is what makes
//! it trustworthy, and never checks the signature.

use std::{collections::HashMap, sync::Arc};

use axum::{
    Form, Json, Router,
    extract::{Query, State},
    response::{IntoResponse as _, Redirect, Response},
    routing::get,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use parking_lot::Mutex;
use serde::Deserialize;

use crate::page::{self, Field};

#[derive(Clone)]
struct Shared {
    issuer: String,
    /// Codes handed out and not yet exchanged, with the claims each one buys.
    codes: Arc<Mutex<HashMap<String, serde_json::Value>>>,
}

pub(crate) fn router(issuer: String) -> Router {
    Router::new()
        .route("/.well-known/openid-configuration", get(document))
        .route("/authorize", get(ask).post(answer))
        .route("/token", axum::routing::post(token))
        .route("/userinfo", get(userinfo))
        .route("/jwks", get(jwks))
        .with_state(Shared {
            issuer,
            codes: Arc::default(),
        })
}

async fn document(State(shared): State<Shared>) -> Json<serde_json::Value> {
    let issuer = &shared.issuer;
    Json(serde_json::json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/token"),
        "userinfo_endpoint": format!("{issuer}/userinfo"),
        "jwks_uri": format!("{issuer}/jwks"),
        "scopes_supported": ["openid", "email", "profile", "groups"],
        "claims_supported": ["sub", "email", "email_verified", "name", "groups"],
        "backchannel_logout_supported": false,
        "code_challenge_methods_supported": ["S256"],
    }))
}

/// The authorisation request, shown as a form that carries it along.
async fn ask(Query(request): Query<Vec<(String, String)>>) -> Response {
    page::sign_in(
        "Identity provider",
        "/authorize",
        &request,
        &[
            Field {
                name: "email",
                label: "Email",
                value: "alice@example.com",
                hint: "Also the account's identity here: the same email is the same person.",
            },
            Field {
                name: "name",
                label: "Name",
                value: "Alice",
                hint: "",
            },
            Field {
                name: "groups",
                label: "Groups",
                value: "",
                hint: "Comma-separated, for trying auth.oidc.required_group.",
            },
            Field {
                name: "email_verified",
                label: "Email verified",
                value: "true",
                hint: "true, false, or empty to leave the claim out, as Synology and Entra ID do.",
            },
        ],
    )
    .into_response()
}

#[derive(Deserialize)]
struct Answer {
    redirect_uri: String,
    state: String,
    nonce: String,
    client_id: String,
    email: String,
    name: String,
    groups: String,
    email_verified: String,
}

/// Issues a code for whoever was typed in and sends the browser back.
async fn answer(State(shared): State<Shared>, Form(answer): Form<Answer>) -> Response {
    let groups: Vec<&str> = answer
        .groups
        .split(',')
        .map(str::trim)
        .filter(|group| !group.is_empty())
        .collect();
    let now = unix_now();
    let mut claims = serde_json::json!({
        "iss": shared.issuer,
        "sub": format!("dev|{}", answer.email.to_lowercase()),
        "aud": answer.client_id,
        "exp": now + 600,
        "iat": now,
        "nonce": answer.nonce,
        "email": answer.email,
        "email_verified": answer.email_verified.trim() == "true",
        "name": answer.name,
        "groups": groups,
    });
    // Omitted when the field is empty, as several providers do; this is the
    // case `auth.oidc.link_by_email` exists for.
    if answer.email_verified.trim().is_empty()
        && let Some(fields) = claims.as_object_mut()
    {
        fields.remove("email_verified");
    }
    let code = page::code();
    shared.codes.lock().insert(code.clone(), claims);

    let separator = if answer.redirect_uri.contains('?') {
        '&'
    } else {
        '?'
    };
    Redirect::to(&format!(
        "{}{separator}code={code}&state={}",
        answer.redirect_uri,
        encode(&answer.state)
    ))
    .into_response()
}

#[derive(Deserialize)]
struct Exchange {
    code: String,
}

async fn token(State(shared): State<Shared>, Form(exchange): Form<Exchange>) -> Response {
    let Some(claims) = shared.codes.lock().remove(&exchange.code) else {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid_grant" })),
        )
            .into_response();
    };
    Json(serde_json::json!({
        "access_token": "fake-access-token",
        "token_type": "Bearer",
        "expires_in": 600,
        "id_token": id_token(&claims),
    }))
    .into_response()
}

/// Nothing is released here that was not already in the token.
async fn userinfo() -> Json<serde_json::Value> {
    Json(serde_json::json!({}))
}

/// No keys: only back-channel logout reads them, and this fake does not send
/// one.
async fn jwks() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "keys": [] }))
}

fn id_token(claims: &serde_json::Value) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
    let body = URL_SAFE_NO_PAD.encode(claims.to_string());
    format!("{header}.{body}.unsigned")
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0))
}

/// Enough percent-encoding for a state value, which Tessera makes URL-safe
/// already; this only guards against one that is not.
fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(byte).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}
