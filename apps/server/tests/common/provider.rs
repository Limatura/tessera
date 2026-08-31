//! A stand-in identity provider, on a loopback port.
//!
//! Enough of one to drive the whole sign-in: the discovery document, the token
//! endpoint, and optionally userinfo. It never checks a signature because
//! nothing on the other side reads one, and it hands back exactly the claims a
//! test asks for, which is what makes the refusals testable.
//!
//! Loopback and `http`, which `auth::oidc::is_private_or_tls` allows and
//! nothing else does. A test cannot obtain a certificate a client would trust.

use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};

use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use parking_lot::Mutex;
use tokio::net::TcpListener;

/// What the provider will say next.
#[derive(Debug, Clone, Default)]
pub(crate) struct Script {
    /// The claims to put in the ID token, minus `iss` and `aud`, which the
    /// provider fills in so that a test cannot accidentally agree with itself.
    pub(crate) claims: serde_json::Value,
    /// Refuse the exchange with this OAuth error instead of issuing a token.
    pub(crate) refuse: Option<String>,
    /// Claims to release from the userinfo endpoint rather than in the token.
    pub(crate) userinfo: Option<serde_json::Value>,
}

/// Writes one claim, creating the object if the script left it empty.
fn set(claims: &mut serde_json::Value, key: &str, value: serde_json::Value) {
    if let Some(fields) = claims.as_object_mut() {
        fields.insert(key.to_owned(), value);
        return;
    }
    *claims = serde_json::json!({ key: value });
}

/// A running provider. Stopped when dropped.
pub(crate) struct FakeProvider {
    pub(crate) issuer: String,
    script: Arc<Mutex<Script>>,
    /// Whether the current script pinned its own nonce.
    ///
    /// Held here rather than read back off the claims, because by the time it
    /// is read this provider may have written one of its own and the two are no
    /// longer distinguishable.
    nonce_is_pinned: Mutex<bool>,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for FakeProvider {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

#[derive(Clone)]
struct Shared {
    issuer: String,
    client_id: String,
    script: Arc<Mutex<Script>>,
}

impl FakeProvider {
    /// Starts one on a port the operating system picks, so tests can run
    /// alongside each other.
    pub(crate) async fn start(client_id: &str) -> Self {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .await
            .expect("a loopback port");
        let issuer = format!(
            "http://127.0.0.1:{}",
            listener.local_addr().expect("an address").port()
        );
        let script = Arc::new(Mutex::new(Script::default()));
        let shared = Shared {
            issuer: issuer.clone(),
            client_id: client_id.to_owned(),
            script: Arc::clone(&script),
        };

        let router = Router::new()
            .route("/.well-known/openid-configuration", get(document))
            .route("/token", post(token))
            .route("/userinfo", get(userinfo))
            .route("/jwks", get(jwks))
            .with_state(shared);
        let handle = tokio::spawn(async move {
            let _ignored = axum::serve(listener, router).await;
        });

        Self {
            issuer,
            script,
            nonce_is_pinned: Mutex::new(false),
            handle,
        }
    }

    /// Sets what the next exchange returns.
    pub(crate) fn will_say(&self, script: Script) {
        *self.nonce_is_pinned.lock() = script.claims.get("nonce").is_some();
        *self.script.lock() = script;
    }

    /// Puts the nonce from the authorisation request into the token, which is
    /// what a provider does with it.
    ///
    /// Skipped when the test pinned one: a token from somebody else's sign-in
    /// is a case worth testing, and this must not quietly repair it.
    pub(crate) fn echo_nonce(&self, nonce: &str) {
        if !*self.nonce_is_pinned.lock() {
            set(
                &mut self.script.lock().claims,
                "nonce",
                serde_json::json!(nonce),
            );
        }
    }
}

async fn document(State(shared): State<Shared>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "issuer": shared.issuer,
        "authorization_endpoint": format!("{}/authorize", shared.issuer),
        "token_endpoint": format!("{}/token", shared.issuer),
        "userinfo_endpoint": format!("{}/userinfo", shared.issuer),
        "jwks_uri": format!("{}/jwks", shared.issuer),
        "backchannel_logout_supported": true,
        "code_challenge_methods_supported": ["S256"],
    }))
}

async fn token(State(shared): State<Shared>) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    let script = shared.script.lock().clone();
    if let Some(error) = script.refuse {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": error })),
        );
    }

    let mut claims = script.claims;
    set(&mut claims, "iss", serde_json::json!(shared.issuer));
    set(&mut claims, "aud", serde_json::json!(shared.client_id));

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "id_token": id_token(&claims),
            "access_token": "an-access-token",
            "token_type": "Bearer",
        })),
    )
}

async fn userinfo(State(shared): State<Shared>) -> Json<serde_json::Value> {
    Json(
        shared
            .script
            .lock()
            .userinfo
            .clone()
            .unwrap_or_else(|| serde_json::json!({})),
    )
}

/// A JWT whose signature is a fixed string, because nothing reads it. See the
/// module documentation on `auth::oidc::claims` for why that is allowed here
/// and where it stops being allowed.
fn id_token(claims: &serde_json::Value) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
    let body = URL_SAFE_NO_PAD.encode(claims.to_string());
    format!("{header}.{body}.not-checked")
}

/// The key this fixture signs logout tokens with, and the JWKS that describes
/// it. A fixed pair rather than a generated one: generating RSA takes long
/// enough to notice, and a test key is not a secret.
const SIGNING_KEY: &str = include_str!("keys/logout-signing.pem");

/// A key the provider never published, for the case that matters most: a token
/// whose `kid` names a real key but whose signature was made with another one.
/// Nothing but verifying the signature catches that.
const IMPOSTOR_KEY: &str = include_str!("keys/impostor.pem");
const SIGNING_JWK: &str = include_str!("keys/logout-signing.jwk.json");

/// The key id in the JWK above.
pub(crate) const SIGNING_KID: &str = "test-key-1";

async fn jwks() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "keys": [serde_json::from_str::<serde_json::Value>(SIGNING_JWK).expect("a jwk")],
    }))
}

impl FakeProvider {
    /// A signed back-channel logout token, as this provider would send one.
    ///
    /// The caller supplies the claims so that a test can leave one out, or put
    /// in one that must be refused.
    pub(crate) fn logout_token(claims: &serde_json::Value) -> String {
        Self::sign_with(claims, SIGNING_KID, SIGNING_KEY)
    }

    /// The same, signed as if by a key the provider does not publish.
    ///
    /// The signature is real; the key id is not one the JWKS names, which is
    /// what an unknown key looks like from the outside.
    pub(crate) fn logout_token_with_unknown_key(claims: &serde_json::Value) -> String {
        Self::sign_with(claims, "a-key-that-was-never-published", SIGNING_KEY)
    }

    /// A token claiming the published key id, signed with a different key.
    ///
    /// The one forgery that a key lookup cannot catch: the `kid` resolves, the
    /// key is found, and only checking the signature says no.
    pub(crate) fn forged_logout_token(claims: &serde_json::Value) -> String {
        Self::sign_with(claims, SIGNING_KID, IMPOSTOR_KEY)
    }

    fn sign_with(claims: &serde_json::Value, kid: &str, pem: &str) -> String {
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        header.kid = Some(kid.to_owned());
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(pem.as_bytes()).expect("a signing key");
        jsonwebtoken::encode(&header, claims, &key).expect("a signed token")
    }

    /// The claims a provider puts in a logout token, with `iss` and `aud`
    /// filled in so a test cannot accidentally agree with itself.
    pub(crate) fn logout_claims(
        &self,
        client_id: &str,
        named: &serde_json::Value,
    ) -> serde_json::Value {
        let mut claims = serde_json::json!({
            "iss": self.issuer,
            "aud": client_id,
            "iat": time::OffsetDateTime::now_utc().unix_timestamp(),
            "jti": "a-logout-token",
            "events": { "http://schemas.openid.net/event/backchannel-logout": {} },
        });
        if let (Some(fields), Some(extra)) = (claims.as_object_mut(), named.as_object()) {
            for (key, value) in extra {
                fields.insert(key.clone(), value.clone());
            }
        }
        claims
    }
}
