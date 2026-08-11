//! What this instance can actually do.

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    api::SYSTEM_TAG,
    config::{FeatureConfig, RegistrationPolicy},
    error::Result,
    state::AppState,
};

/// Body of `GET /api/v1/capabilities`.
///
/// # Why this endpoint exists
///
/// `openapi.json` is a build artefact: it describes every endpoint the software
/// has. Which of them an *instance* exposes is a runtime decision its operator
/// made. A single-page app generated from the contract therefore cannot know
/// whether to render a login button — the contract says login exists, and on
/// this deployment it may not.
///
/// So the contract describes the whole surface and this reports the subset in
/// force. It is deliberately unauthenticated: a client needs the answer before
/// it can know whether logging in is even possible.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Capabilities {
    /// Whether accounts exist at all on this instance.
    ///
    /// `false` means there is no registration, no login and no session — the
    /// shape for a personal instance behind a VPN.
    pub accounts: bool,

    /// Whether the administrative endpoints are mounted.
    ///
    /// The administration front-end is a separate deployment, so this says only
    /// that the API half is present.
    pub admin: bool,

    /// Whether a caller without a session may upload.
    ///
    /// Independent of [`accounts`](Self::accounts): an instance can have
    /// accounts *and* accept anonymous uploads, which is the default.
    pub anonymous_upload: bool,

    /// Who may create an account. Meaningless when
    /// [`accounts`](Self::accounts) is `false`.
    #[schema(example = "open")]
    pub registration: RegistrationPolicy,

    /// Shortest password this instance accepts, so a client can say so before
    /// the round trip rather than after it.
    #[schema(example = 12)]
    pub password_min_length: usize,

    /// Longest accepted request body, in bytes.
    #[schema(example = 104_857_600_u64)]
    pub max_upload_bytes: u64,

    /// Version of the running server, for a client that wants to warn about a
    /// mismatch with the contract it was generated from.
    #[schema(example = "0.1.0")]
    pub version: String,
}

impl Capabilities {
    /// Derives the effective answer from configuration.
    fn from_state(state: &AppState) -> Self {
        let config = &state.config;
        let FeatureConfig {
            accounts,
            admin,
            anonymous_upload,
            docs: _,
        } = config.features;

        Self {
            accounts,
            admin,
            anonymous_upload,
            registration: config.auth.registration,
            password_min_length: config.auth.password.min_length,
            // `usize` is the type axum's body limit wants; `u64` is the type a
            // file size is. The conversion cannot fail on any target this runs
            // on, but saturating is still better than a cast that would wrap.
            max_upload_bytes: u64::try_from(config.server.max_body_bytes).unwrap_or(u64::MAX),
            version: crate::VERSION.to_owned(),
        }
    }
}

/// Reports which optional parts of the service this instance exposes.
///
/// # Errors
///
/// Infallible in practice; the signature matches the rest of the API so that a
/// future version can start failing without changing every caller.
#[utoipa::path(
    get,
    path = "/capabilities",
    tag = SYSTEM_TAG,
    responses(
        (status = 200, description = "What this instance exposes", body = Capabilities),
    ),
)]
pub async fn capabilities(State(state): State<AppState>) -> Result<Json<Capabilities>> {
    Ok(Json(Capabilities::from_state(&state)))
}
