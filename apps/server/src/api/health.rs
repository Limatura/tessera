//! Liveness and readiness endpoint.

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    api::SYSTEM_TAG,
    error::{Error, Result},
    state::{AppState, ServiceStatus},
};

/// Body of a successful `GET /health`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    /// Where the process is in its lifecycle.
    pub status: ServiceStatus,
    /// Version of the running binary.
    #[schema(example = "0.1.0")]
    pub version: String,
    /// Whole seconds since the process started.
    #[schema(example = 3600)]
    pub uptime_seconds: u64,
    /// Relational backend currently in use.
    #[schema(example = "sqlite")]
    pub database: String,
}

/// Reports whether the service is ready to take traffic.
///
/// Deliberately both a liveness and a readiness probe: the process only reaches
/// [`ServiceStatus::Ready`] once migrations have run, and it flips to
/// [`ServiceStatus::Draining`] the moment a shutdown signal arrives, so an
/// orchestrator can stop routing before the listener actually closes.
///
/// # Errors
///
/// Returns [`Error::Unavailable`] (503) while starting up or draining, and when
/// the database round-trip fails.
#[utoipa::path(
    get,
    path = "/health",
    tag = SYSTEM_TAG,
    responses(
        (status = 200, description = "The service is ready", body = HealthResponse),
        (
            status = 503,
            description = "Starting up, draining, or a dependency is unreachable",
            body = crate::error::ErrorBody
        ),
    ),
)]
pub async fn health(State(state): State<AppState>) -> Result<Json<HealthResponse>> {
    let status = state.status();
    if status != ServiceStatus::Ready {
        return Err(Error::Unavailable);
    }

    // A pool that cannot answer `SELECT 1` is not ready, whatever the lifecycle
    // flag says. The cause goes to the log; the client gets a bare 503.
    if let Err(error) = state.db.ping().await {
        tracing::warn!(?error, "health check could not reach the database");
        return Err(Error::Unavailable);
    }

    Ok(Json(HealthResponse {
        status,
        version: crate::VERSION.to_owned(),
        uptime_seconds: state.uptime_seconds(),
        database: state.db.backend().as_str().to_owned(),
    }))
}
