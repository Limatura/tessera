//! What one person has chosen about how the site looks to them.
//!
//! # Why this is not part of `api::settings`
//!
//! That module is built around an administrator changing the instance: it asks
//! `admits()` whether a key may be stored at all, seals the ones that hold
//! credentials, refuses the ones that would need a restart, and gates
//! everything behind a permission. None of that applies here. These are one
//! person's own choices, they hold nothing secret, and anybody signed in may
//! change their own. Sharing a module would mean two sets of premises in one
//! file, each of which has to remember not to apply to the other.
//!
//! # Why the whole object is replaced
//!
//! Two devices editing at once is possible and does not matter: the loser sees
//! a colour they did not pick and clicks again. That is cheaper than the
//! version numbers and merge rules it would take to do better.
//!
//! Note that replacing is not the same as filling in. An absent field means
//! "no opinion", so somebody who sets only an accent keeps following the
//! instance's default for the other two, including after the operator changes
//! it. See [`tessera_core::Appearance`].

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use tessera_core::Appearance;
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    api::{PREFERENCES_TAG, extract::ApiJson},
    auth::extract::CurrentUser,
    error::Result,
    state::AppState,
};

/// One person's preferences.
///
/// One field so far. Language, time zone and clock format are the ones already
/// asked for, and each will be another field here rather than another endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PreferencesResponse {
    /// The appearance choices, `{}` when none have been made.
    ///
    /// `mode` is one of `light`, `dark`, `system`; `theme` one of `default`,
    /// `graphite`, `sepia`; `accent` one of `violet`, `rose`, `orange`,
    /// `amber`, `green`, `teal`, `blue`. Any of the three may be absent,
    /// meaning this person has no opinion and follows the instance.
    #[schema(value_type = Object, example = json!({"mode": "dark", "theme": "sepia", "accent": "rose"}))]
    pub appearance: Appearance,
}

/// Body of `PUT /me/preferences`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdatePreferencesRequest {
    /// The appearance choices, replacing whatever was stored.
    ///
    /// Omit a field, or send `{}`, to have no opinion and follow the
    /// instance's default. The accepted names are the ones
    /// [`PreferencesResponse`] lists; anything else is a `400`.
    #[serde(default)]
    #[schema(value_type = Object, example = json!({"mode": "dark", "theme": "sepia", "accent": "rose"}))]
    pub appearance: Appearance,
}

/// The caller's own preferences.
///
/// # Errors
///
/// `401` when nothing was signed in.
#[utoipa::path(
    get,
    path = "/me/preferences",
    operation_id = "get_preferences",
    tag = PREFERENCES_TAG,
    responses(
        (status = 200, description = "The caller's preferences", body = PreferencesResponse),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
    ),
    security(("session" = []), ("bearer" = [])),
)]
pub async fn get(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<PreferencesResponse>> {
    let appearance = state.preferences.appearance_for(current.user.id).await?;
    Ok(Json(PreferencesResponse { appearance }))
}

/// Replaces the caller's own preferences.
///
/// # Why an unknown name is a `400` and not a shrug
///
/// These end up as attributes on the `<html>` element. Refusing anything
/// outside the lists is the whole of the check, and it belongs here rather than
/// at the point of rendering, where a value has already been stored and
/// whatever wrote it has already gone.
///
/// # Errors
///
/// `400` when a name is not one this build knows, `401` when nothing was signed
/// in.
#[utoipa::path(
    put,
    path = "/me/preferences",
    operation_id = "update_preferences",
    tag = PREFERENCES_TAG,
    request_body = UpdatePreferencesRequest,
    responses(
        (status = 200, description = "The preferences as they now stand", body = PreferencesResponse),
        (status = 400, description = "A name outside the lists this build knows", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
    ),
    security(("session" = []), ("bearer" = [])),
)]
pub async fn update(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(body): ApiJson<UpdatePreferencesRequest>,
) -> Result<Json<PreferencesResponse>> {
    state
        .preferences
        .set_appearance(current.user.id, &body.appearance, OffsetDateTime::now_utc())
        .await?;
    Ok(Json(PreferencesResponse {
        appearance: body.appearance,
    }))
}
