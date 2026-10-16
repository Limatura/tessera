//! The malware scanner's state, for the operator.
//!
//! A scanner that stops answering changes nothing anyone else sees: files are
//! served unscanned until it is back (`docs/scanning.md`). So this is the one
//! place that says so.

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    api::{
        ADMIN_TAG,
        admin::{admitted, require_admin_feature},
    },
    auth::extract::CurrentUser,
    db::repository::scan::Scan,
    error::Result,
    state::AppState,
};

/// Body of `GET /admin/scanner`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ScannerResponse {
    /// Whether `scan.url` names a scanner. When `false` the rest says nothing
    /// about one.
    pub enabled: bool,
    /// Whether it answered the last time it was called, or `true` before it
    /// has been.
    ///
    /// Learned from the scans themselves; one left idle is sent an empty file
    /// every fifteen minutes, and one that failed every five.
    pub reachable: bool,
    /// When it was last called, or `null` before it has been since start-up
    /// or since `scan.url` changed.
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime, example = "2026-10-15T08:00:00Z")]
    pub last_checked_at: Option<OffsetDateTime>,
    /// Why the last call failed, or `null` when it answered.
    #[schema(example = "could not talk to the scanner: Connection refused (os error 111)")]
    pub last_error: Option<String>,
    /// Files and notes waiting on their first verdict, held until it comes.
    pub pending: u64,
    /// Files and notes the scanner could not be reached for, served unscanned
    /// and retried until it answers.
    pub deferred: u64,
}

/// The scanner's state, and how much is waiting on it.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the caller may not read
/// settings, `404` when this instance has no administration.
#[utoipa::path(
    get,
    path = "/admin/scanner",
    operation_id = "scanner_status",
    tag = ADMIN_TAG,
    responses(
        (status = 200, description = "The scanner's state", body = ScannerResponse),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not read settings", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no administration", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn status(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<ScannerResponse>> {
    require_admin_feature(&state)?;
    admitted(&current, tessera_core::Permission::SettingsRead)?;

    let config = state.config();
    let seen = state.scanner_health.get(&config.scan.url);
    Ok(Json(ScannerResponse {
        enabled: config.scan.is_enabled(),
        reachable: seen.as_ref().is_none_or(|seen| seen.error.is_none()),
        last_checked_at: seen.as_ref().map(|seen| seen.at),
        last_error: seen.and_then(|seen| seen.error),
        pending: state.scans.count_in_state(&Scan::Pending).await?,
        deferred: state.scans.count_in_state(&Scan::Deferred).await?,
    }))
}
