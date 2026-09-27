//! Checking a password against the breach list while it is being typed.
//!
//! The same check sign-up, password change and reset already run on submit
//! (see [`crate::auth::pwned`]), offered earlier so the user finds out before
//! submitting. The browser hashes the password, sends the first five
//! characters of the hash, and compares the returned suffixes itself: the
//! password never leaves it, and this server forwards only those five
//! characters.

use axum::{Json, extract::State};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    api::extract::ApiPath,
    auth::{
        extract::ClientAddress,
        proxy::Origin,
        pwned::{is_prefix, range},
        throttle::Decision,
    },
    error::{Error, ErrorCode, Result},
    state::AppState,
};

/// The breached suffixes under one prefix.
#[derive(Debug, Serialize, ToSchema)]
pub struct BreachedSuffixes {
    /// The remaining 35 characters of every breached hash under the prefix,
    /// uppercase. The service's padding entries, which hide the real count,
    /// are removed, so any match here is real.
    #[schema(example = json!(["1E4C9B93F3F0682250B6CF8331B7EE68FD8"]))]
    pub suffixes: Vec<String>,
}

/// The breached hashes under the first five characters of a password's SHA-1.
///
/// # Errors
///
/// `404` when this instance does not check passwords against the breach list,
/// `400` for anything but five hex characters, `429` when this source is over
/// its limit, `503` when the breach list is unreachable. A form treats the
/// last as "no warning"; the check still runs on submit.
#[utoipa::path(
    get,
    path = "/auth/pwned/{prefix}",
    operation_id = "pwned_range",
    tag = crate::api::AUTH_TAG,
    params(("prefix" = String, Path, description = "The first five characters of the password's SHA-1, in hex, either case")),
    responses(
        (status = 200, description = "The breached suffixes under that prefix", body = BreachedSuffixes),
        (status = 400, description = "Not five hex characters", body = crate::error::ErrorBody),
        (status = 404, description = "This instance does not check passwords against the breach list", body = crate::error::ErrorBody),
        (status = 429, description = "Too many lookups from this source", body = crate::error::ErrorBody),
        (status = 503, description = "The breach list could not be reached", body = crate::error::ErrorBody),
    ),
)]
pub async fn range_of(
    State(state): State<AppState>,
    ApiPath(prefix): ApiPath<String>,
    ClientAddress(Origin { source, .. }): ClientAddress,
) -> Result<Json<BreachedSuffixes>> {
    {
        let config = state.config();
        if !config.features.accounts || !config.auth.pwned.enabled {
            return Err(Error::NotFound);
        }
    }
    if !is_prefix(&prefix) {
        return Err(Error::new(ErrorCode::InvalidHashPrefix));
    }

    // Counted before the lookup, on every lookup: each is a third-party request
    // made on an anonymous client's behalf.
    let counter = &state.throttles.pwned;
    if let Decision::Deny { retry_after } = counter.check(&source) {
        return Err(Error::TooManyRequests {
            retry_after_secs: retry_after.as_secs(),
        });
    }
    counter.record_failure(&source);

    let suffixes = range(&state, &prefix).await.map_err(|error| {
        tracing::warn!(%error, "could not look a prefix up in the breach list");
        Error::Unavailable
    })?;
    Ok(Json(BreachedSuffixes { suffixes }))
}
