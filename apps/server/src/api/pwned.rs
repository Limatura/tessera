//! Looking a password up in the breach list while it is being typed.
//!
//! The same check sign-up, a change of password and a reset already make when
//! the form is sent (see [`crate::auth::pwned`]), offered earlier so a person
//! finds out before they submit rather than after. The browser hashes the
//! password, sends the first five characters of the hash, and compares the
//! suffixes that come back itself: the password never leaves it, and this
//! server passes on nothing more than those five characters.

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
    error::{Error, Result},
    state::AppState,
};

/// The breached suffixes under one prefix.
#[derive(Debug, Serialize, ToSchema)]
pub struct BreachedSuffixes {
    /// The remaining 35 characters of every breached hash under the prefix,
    /// uppercase. The padding the service adds to hide how many there are is
    /// taken out, so a match here is a real one.
    #[schema(example = json!(["1E4C9B93F3F0682250B6CF8331B7EE68FD8"]))]
    pub suffixes: Vec<String>,
}

/// The breached hashes under the first five characters of a password's SHA-1.
///
/// # Errors
///
/// `404` when this instance does not check passwords against the breach list,
/// `400` for anything but five hex characters, `429` past this source's
/// allowance, `503` when the breach list cannot be reached. A form treats the
/// last as "no warning"; the check still runs when it is sent.
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
        return Err(Error::bad_request(
            "give the first five characters of the password's SHA-1, in hex",
        ));
    }

    // Counted before the lookup, on every lookup: each one is a request to
    // somebody else's service made on a stranger's behalf.
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
