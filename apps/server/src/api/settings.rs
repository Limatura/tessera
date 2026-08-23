//! Reading and changing how the instance is configured.
//!
//! # What this surface promises
//!
//! Every setting it offers takes effect on the next request. There is no
//! "saved — now restart": a setting that would need one is listed as read-only
//! with the reason, and cannot be written at all. See [`crate::settings`] for
//! how that line is drawn.

use std::{collections::BTreeMap, sync::Arc};

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    api::{
        ADMIN_TAG,
        admin::{admitted, require_admin_feature},
        extract::ApiJson,
    },
    auth::extract::CurrentUser,
    config::Config,
    error::{Error, Result},
    settings::{self, Refusal, Source},
    state::AppState,
};

/// One setting, as the panel draws it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SettingResponse {
    /// Dotted key, the same spelling the file and the environment use.
    #[schema(example = "share.code_length")]
    pub key: String,
    /// The value in force.
    #[schema(value_type = Object)]
    pub value: serde_json::Value,
    /// Which layer supplied it.
    pub source: Source,
    /// Whether this endpoint will change it.
    pub editable: bool,
    /// Why it will not, when it will not.
    ///
    /// `fixed_at_boot` for something the running process cannot pick up,
    /// `secret` for something that holds a credential. Present so a screen can
    /// explain rather than only grey the field out.
    #[schema(example = "fixed_at_boot")]
    pub refusal: Option<String>,
}

/// Everything this instance can be configured with.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SettingsResponse {
    /// Every setting, editable or not.
    pub settings: Vec<SettingResponse>,
}

/// Body of `PUT /admin/settings`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateSettingsRequest {
    /// The changes to apply, by dotted key.
    ///
    /// `null` clears a setting, so it falls back to the environment, the file,
    /// or the default — whichever comes next. That is the only way back to a
    /// value the panel did not choose, and it is why clearing is a value rather
    /// than a second endpoint.
    #[schema(value_type = Object, example = json!({"share.code_length": 10}))]
    pub changes: BTreeMap<String, serde_json::Value>,
}

const fn refusal_name(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::FixedAtBoot => "fixed_at_boot",
        Refusal::Secret => "secret",
        Refusal::Unknown => "unknown",
    }
}

fn describe(state: &AppState, stored: &BTreeMap<String, serde_json::Value>) -> SettingsResponse {
    SettingsResponse {
        settings: settings::describe(&state.config_path, stored)
            .into_iter()
            .map(|view| SettingResponse {
                key: view.key,
                value: view.value,
                source: view.source,
                editable: view.editable,
                refusal: view.refusal.map(refusal_name).map(ToOwned::to_owned),
            })
            .collect(),
    }
}

/// Lists every setting, with its value and where that value came from.
///
/// # Why the ones that cannot be changed are here too
///
/// Leaving them out is what makes somebody ask where the port setting went.
/// They come back marked `editable: false` with a reason, which is a screen
/// that explains itself rather than one with a hole in it.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the caller may not read
/// settings or used a bearer token, `404` when this instance has no
/// administration.
#[utoipa::path(
    get,
    path = "/admin/settings",
    operation_id = "list_settings",
    tag = ADMIN_TAG,
    responses(
        (status = 200, description = "Every setting", body = SettingsResponse),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not read settings, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no administration", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<SettingsResponse>> {
    require_admin_feature(&state)?;
    admitted(&current, tessera_core::Permission::SettingsRead)?;

    let stored = state.settings.all().await?;
    Ok(Json(describe(&state, &stored)))
}

/// Changes settings, and applies them.
///
/// # Why the change is tried before it is stored
///
/// A value this binary will not accept is refused here, with the key named,
/// rather than written and quietly dropped at the next start-up. The start-up
/// fallback exists so a bad row cannot make an instance unbootable — it is a
/// safety net, not the validation path, and letting the panel lean on it would
/// mean a save that reports success and reverts on restart.
///
/// # Why one request can carry several
///
/// Settings that only make sense together — turning the access log on and
/// setting its retention — should not have a moment where one is applied and
/// the other is not. They are validated as a set, and stored only if the set
/// loads.
///
/// # Errors
///
/// `400` for an unknown key, one that cannot be changed, or a value the
/// configuration will not accept. `401`, `403` and `404` as for [`list`].
#[utoipa::path(
    put,
    path = "/admin/settings",
    operation_id = "update_settings",
    tag = ADMIN_TAG,
    request_body = UpdateSettingsRequest,
    responses(
        (status = 200, description = "Every setting, as it now stands", body = SettingsResponse),
        (status = 400, description = "A key or value this instance will not accept", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not change settings, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no administration", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn update(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(body): ApiJson<UpdateSettingsRequest>,
) -> Result<Json<SettingsResponse>> {
    require_admin_feature(&state)?;
    admitted(&current, tessera_core::Permission::SettingsWrite)?;

    // Every key first, so a request naming one bad setting changes none of
    // them. Half-applying a set is worse than refusing it: the operator has to
    // work out which half.
    for key in body.changes.keys() {
        if let Err(refusal) = settings::admits(key) {
            return Err(Error::bad_request(match refusal {
                Refusal::Unknown => format!("`{key}` is not a setting this version has"),
                Refusal::Secret => format!(
                    "`{key}` holds a credential and is set through the environment, not here"
                ),
                Refusal::FixedAtBoot => format!(
                    "`{key}` cannot change while the server is running, so it is set through \
                     the environment"
                ),
            }));
        }
    }

    let mut candidate = state.settings.all().await?;
    for (key, value) in &body.changes {
        if value.is_null() {
            candidate.remove(key);
        } else {
            candidate.insert(key.clone(), value.clone());
        }
    }

    // The proof that it works, before anything is written.
    let (config, rejected) = Config::load_with_overrides(&state.config_path, &candidate)
        .map_err(|error| Error::bad_request(error.to_string()))?;
    if let Some(bad) = rejected
        .into_iter()
        .find(|bad| body.changes.contains_key(&bad.key))
    {
        return Err(Error::bad_request(format!(
            "`{}` will not be accepted: {}",
            bad.key, bad.reason
        )));
    }

    let now = OffsetDateTime::now_utc();
    for (key, value) in &body.changes {
        if value.is_null() {
            state.settings.clear(key).await?;
        } else {
            state.settings.set(key, value, now).await?;
        }
    }

    state.replace_config(Arc::new(config));
    tracing::info!(
        by = %current.user.id,
        changed = ?body.changes.keys().collect::<Vec<_>>(),
        "settings changed"
    );

    let stored = state.settings.all().await?;
    Ok(Json(describe(&state, &stored)))
}
