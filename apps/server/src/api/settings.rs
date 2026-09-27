//! Reading and changing how the instance is configured.
//!
//! # What this surface promises
//!
//! Every setting it offers takes effect on the next request. There is no
//! "saved, now restart": a setting that would need one is listed read-only with
//! the reason and cannot be written. See [`crate::settings`] for how that line
//! is drawn.

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
    error::{Error, ErrorCode, Result},
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
    /// explain instead of only greying the field out.
    #[schema(example = "fixed_at_boot")]
    pub refusal: Option<String>,
    /// Whether this holds a credential this endpoint writes but never returns.
    ///
    /// When true, `value` is a boolean saying whether one is set, never the
    /// credential itself. A screen shows "configured" next to an empty field
    /// instead of pre-filling something it cannot read back.
    pub secret: bool,
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
    /// `null` clears a setting, so it falls back to whichever layer comes next:
    /// the environment, or the default. That is the only way back to a value
    /// the panel did not choose, and why clearing is a value and not a second
    /// endpoint.
    #[schema(value_type = Object, example = json!({"share.code_length": 10}))]
    pub changes: BTreeMap<String, serde_json::Value>,

    /// Go ahead even though the change destroys something.
    ///
    /// Without it, a change that would remove things is refused and the answer
    /// says what and how many. Two round trips by design: an empty "are you
    /// sure?" is a button everyone clicks, and a number gives pause.
    #[serde(default)]
    pub confirm: bool,
}

/// What `POST /admin/settings/preview` answers.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PreviewResponse {
    /// What the change would destroy. Empty when it destroys nothing.
    pub consequences: Vec<Consequence>,
}

const fn refusal_name(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::FixedAtBoot => "fixed_at_boot",
        Refusal::Secret => "secret",
        Refusal::Unknown => "unknown",
    }
}

fn describe(stored: &BTreeMap<String, serde_json::Value>) -> SettingsResponse {
    SettingsResponse {
        settings: settings::describe(stored)
            .into_iter()
            .map(|view| SettingResponse {
                key: view.key,
                value: view.value,
                source: view.source,
                editable: view.editable,
                refusal: view.refusal.map(refusal_name).map(ToOwned::to_owned),
                secret: view.secret,
            })
            .collect(),
    }
}

/// What a setting change would destroy the next time the sweep runs.
///
/// # Why this exists
///
/// Three settings are retention periods, and shortening one means the next
/// sweep removes things that were going to be kept. Nothing warns about it and
/// nothing brings them back.
///
/// An empty "are you sure?" does not help, because everyone clicks it. A number
/// does: "this removes 23 shares, 4.2 GB" is a sentence that gives pause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Consequence {
    /// Which setting causes it.
    #[schema(example = "share.retain_after_death_hours")]
    pub key: String,
    /// What would go, as a plural noun a screen can print.
    #[schema(example = "shares")]
    pub subject: String,
    /// How many.
    #[schema(example = 23)]
    pub count: u64,
    /// How many bytes of stored content go with them, when that applies.
    #[schema(example = 4_500_000_000_u64)]
    pub bytes: Option<u64>,
}

/// Works out what a change would cost, before it is made.
///
/// # Why only shortening counts
///
/// Lengthening a retention keeps more and destroys nothing. Only the direction
/// that removes things is worth a warning; warning about both teaches an
/// operator to click through.
///
/// # Errors
///
/// Returns [`Error`] if a count cannot be read.
async fn consequences_of(
    state: &AppState,
    current: &Config,
    next: &Config,
    changed: &BTreeMap<String, serde_json::Value>,
) -> Result<Vec<Consequence>> {
    let now = OffsetDateTime::now_utc();
    let mut found = Vec::new();

    // Shares past their retention: the row and whatever it still holds.
    if changed.contains_key("share.retain_after_death_hours")
        && next.share.retain_after_death_hours < current.share.retain_after_death_hours
    {
        let cutoff = now.saturating_sub(duration_of(next.share.retention()));
        let (count, bytes) = state.shares.count_dead_before(cutoff).await?;
        if count > 0 {
            found.push(Consequence {
                key: "share.retain_after_death_hours".to_owned(),
                subject: "shares".to_owned(),
                count,
                bytes: Some(bytes),
            });
        }
    }

    // Recorded visits.
    if changed.contains_key("share.access_log.retention_days")
        && next.share.access_log.retention_days < current.share.access_log.retention_days
    {
        let cutoff = now.saturating_sub(duration_of(next.share.access_log.retention()));
        let count = state.events.count_before(cutoff).await?;
        if count > 0 {
            found.push(Consequence {
                key: "share.access_log.retention_days".to_owned(),
                subject: "recorded accesses".to_owned(),
                count,
                bytes: None,
            });
        }
    }

    // The login history that answers "has anyone else been in here".
    // Shortening it destroys the answer.
    if changed.contains_key("auth.login_history_days")
        && next.auth.login_history_days < current.auth.login_history_days
    {
        let cutoff = now.saturating_sub(duration_of(next.auth.login_history()));
        let count = state.sessions.count_ended_before(cutoff).await?;
        if count > 0 {
            found.push(Consequence {
                key: "auth.login_history_days".to_owned(),
                subject: "past sign-ins".to_owned(),
                count,
                bytes: None,
            });
        }
    }

    Ok(found)
}

/// A `std::time::Duration` as `time`'s, saturating instead of panicking.
fn duration_of(value: std::time::Duration) -> time::Duration {
    time::Duration::try_from(value).unwrap_or(time::Duration::MAX)
}

/// Spells consequences out for an error message.
fn describe_consequences(consequences: &[Consequence]) -> String {
    let mut parts = Vec::with_capacity(consequences.len());
    for consequence in consequences {
        let size = consequence
            .bytes
            .filter(|bytes| *bytes > 0)
            .map_or_else(String::new, |bytes| {
                format!(
                    ", {:.1} GB",
                    f64::from(u32::try_from(bytes / 1_000_000).unwrap_or(u32::MAX)) / 1000.0
                )
            });
        parts.push(format!(
            "{} {}{size} (from `{}`)",
            consequence.count, consequence.subject, consequence.key
        ));
    }
    parts.join("; ")
}

/// Lists every setting, with its value and where that value came from.
///
/// # Why the ones that cannot be changed are here too
///
/// Leaving them out prompts the question of where the port setting went. They
/// come back marked `editable: false` with a reason: a screen that explains
/// itself instead of one with a hole in it.
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
    Ok(Json(describe(&stored)))
}

/// Applies the changes to what is stored and proves the result loads.
///
/// Shared by the preview and the update so the two cannot disagree about what a
/// change means: a preview describing a configuration the update would refuse
/// is worse than no preview.
async fn resolve(
    state: &AppState,
    changes: &BTreeMap<String, serde_json::Value>,
) -> Result<(BTreeMap<String, serde_json::Value>, Config)> {
    for key in changes.keys() {
        if let Err(refusal) = settings::admits(key) {
            let code = match refusal {
                Refusal::Unknown => ErrorCode::SettingUnknown,
                Refusal::Secret => ErrorCode::SettingSecret,
                Refusal::FixedAtBoot => ErrorCode::SettingFixedAtBoot,
            };
            return Err(Error::new(code)
                .with_detail("key", key.as_str())
                .with_message(format!("`{key}`: {}", code.message())));
        }
    }

    let mut candidate = state.settings.all().await?;
    for (key, value) in changes {
        if value.is_null() {
            candidate.remove(key);
        } else {
            candidate.insert(key.clone(), value.clone());
        }
    }

    // The proof that it works, before anything is written. A value that will
    // not load is refused here, not dropped at the next start-up.
    let (config, rejected) = Config::load_with_overrides(&candidate)
        .map_err(|error| Error::new(ErrorCode::SettingInvalid).with_message(error.to_string()))?;
    if let Some(bad) = rejected
        .into_iter()
        .find(|bad| changes.contains_key(&bad.key))
    {
        return Err(Error::new(ErrorCode::SettingInvalid)
            .with_detail("key", bad.key.as_str())
            .with_message(format!(
                "`{}` will not be accepted: {}",
                bad.key, bad.reason
            )));
    }
    Ok((candidate, config))
}

/// Says what a change would destroy, without making it.
///
/// # Errors
///
/// `400` for a key or value this instance will not accept, the same answers
/// [`update`] gives, so a client cannot get a clean preview for a change that
/// would then be refused.
#[utoipa::path(
    post,
    path = "/admin/settings/preview",
    operation_id = "preview_settings",
    tag = ADMIN_TAG,
    request_body = UpdateSettingsRequest,
    responses(
        (status = 200, description = "What the change would destroy", body = PreviewResponse),
        (status = 400, description = "A key or value this instance will not accept", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not change settings, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no administration", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn preview(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(body): ApiJson<UpdateSettingsRequest>,
) -> Result<Json<PreviewResponse>> {
    require_admin_feature(&state)?;
    // The write permission, not the read one. This reports what a change would
    // cost, useful only to a caller who could make it.
    admitted(&current, tessera_core::Permission::SettingsWrite)?;

    let (_, config) = resolve(&state, &body.changes).await?;
    Ok(Json(PreviewResponse {
        consequences: consequences_of(&state, &state.config(), &config, &body.changes).await?,
    }))
}

/// Body of `POST /admin/settings/oidc/discovery`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryProbeRequest {
    /// The issuer, or the `.well-known` URL copied from the provider's docs.
    /// Either works: the suffix is stripped before fetching, and `issuer` is
    /// what comes back.
    #[schema(example = "https://id.example.com")]
    pub url: String,
}

/// What a provider says about itself, as the panel needs it.
#[derive(Debug, Serialize, ToSchema)]
pub struct DiscoveryProbeResponse {
    /// The provider's own issuer value, which is what `auth.oidc.issuer` must
    /// be set to.
    #[schema(example = "https://id.example.com")]
    pub issuer: String,
    /// The scopes it advertises, so a form can offer them instead of having the
    /// operator type them.
    pub scopes_supported: Vec<String>,
    /// Whether it notifies this instance when a user signs out.
    pub backchannel_logout_supported: bool,
    /// The claims it advertises. Without `email_verified`,
    /// `link_by_email = when_verified` will never link anyone through this
    /// provider; the options left are `always`, or signing in and linking it.
    pub claims_supported: Vec<String>,
}

/// Reads a provider's discovery document from the server, not the browser.
///
/// # Why the server and not the panel
///
/// Discovery only says a provider *should* let browsers read its document, and
/// many do not: the panel's own fetch is blocked by CORS on the very providers
/// self-hosted instances tend to use. The server has no origin, so the fetch
/// works, and it is the same fetch sign-in will make: a document reachable
/// here is reachable from the sign-in path, which a browser could never
/// confirm.
///
/// # What it does not do
///
/// Touch the cache. This is a URL being tried out in a form; caching its
/// answer would let an unsaved value decide what a real sign-in talks to.
///
/// # Errors
///
/// `400` for an address this instance would not accept as an issuer, or one
/// the provider's document cannot be read from. `401`, `403` and `404` as for
/// [`list`].
#[utoipa::path(
    post,
    path = "/admin/settings/oidc/discovery",
    operation_id = "probe_oidc_discovery",
    tag = ADMIN_TAG,
    request_body = DiscoveryProbeRequest,
    responses(
        (status = 200, description = "What the provider says about itself", body = DiscoveryProbeResponse),
        (status = 400, description = "Not an address this accepts, or the document could not be read", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not change settings, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no administration", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn probe_oidc_discovery(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(body): ApiJson<DiscoveryProbeRequest>,
) -> Result<Json<DiscoveryProbeResponse>> {
    require_admin_feature(&state)?;
    // The write permission, as for `preview`: this is only useful to someone
    // who could make the change. It also stops anyone else using this to make
    // the server fetch arbitrary URLs.
    admitted(&current, tessera_core::Permission::SettingsWrite)?;

    let issuer = issuer_of(&body.url);
    if issuer.is_empty() {
        return Err(Error::new(ErrorCode::ValueEmpty)
            .with_detail("field", "issuer")
            .with_message("give the provider's address"));
    }
    // The same rule the issuer setting enforces, so this cannot reach anything
    // that could not be configured directly.
    if !crate::auth::oidc::is_private_or_tls(&issuer) {
        return Err(Error::new(ErrorCode::ProviderInsecure));
    }
    let provider = crate::auth::oidc::discovery::probe(&issuer)
        .await
        .map_err(|error| {
            Error::new(ErrorCode::ProviderUnreachable).with_message(error.to_string())
        })?;
    Ok(Json(DiscoveryProbeResponse {
        issuer: provider.issuer.clone(),
        scopes_supported: provider.scopes_supported.clone(),
        backchannel_logout_supported: provider.backchannel_logout_supported,
        claims_supported: provider.claims_supported,
    }))
}

/// The issuer inside whatever was pasted in.
///
/// Operators copy the `.well-known` URL from provider docs as often as the
/// issuer, and the difference is a suffix that can be stripped. Anything else
/// is left alone: an unrecognised address is better refused by the fetch, with
/// the provider's own answer, than silently rewritten here.
fn issuer_of(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    trimmed
        .strip_suffix("/.well-known/openid-configuration")
        .unwrap_or(trimmed)
        .trim_end_matches('/')
        .to_owned()
}

/// Changes settings, and applies them.
///
/// # Why the change is tried before it is stored
///
/// A value this binary will not accept is refused here, with the key named,
/// instead of written and quietly dropped at the next start-up. The start-up
/// fallback exists so a bad row cannot make an instance unbootable: a safety
/// net, not the validation path, and letting the panel lean on it would mean a
/// save that reports success and reverts on restart.
///
/// # Why one request can carry several
///
/// Settings that only make sense together, such as turning the access log on
/// and setting its retention, should not have a moment where one is applied and
/// the other is not. They are validated as a set and stored only if the set
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
        (status = 409, description = "The change destroys something; the message says what, and `confirm` goes ahead", body = crate::error::ErrorBody),
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

    let (_candidate, config) = resolve(&state, &body.changes).await?;

    // What it would destroy, and a refusal carrying the numbers unless the
    // caller has already seen them.
    let consequences = consequences_of(&state, &state.config(), &config, &body.changes).await?;
    if !consequences.is_empty() && !body.confirm {
        return Err(
            Error::new(ErrorCode::ConfirmationRequired).with_message(format!(
                "this removes {}. Send `confirm: true` to go ahead.",
                describe_consequences(&consequences)
            )),
        );
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
    Ok(Json(describe(&stored)))
}
