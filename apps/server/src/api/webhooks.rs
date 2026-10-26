//! Webhooks: an account's own under `/webhooks`, the instance's under
//! `/admin/webhooks`.
//!
//! The two sets of routes do the same things to different rows. An account's
//! webhooks hear about its own shares and may only send to public addresses;
//! the instance's hear about every share and may send anywhere. See
//! [`crate::webhook`].

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse as _};
use serde::{Deserialize, Serialize};
use tessera_core::{Permission, UserId, WebhookId};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    api::{
        ADMIN_TAG, WEBHOOKS_TAG,
        extract::{ApiJson, ApiPath},
        tokens::format_timestamp,
    },
    auth::extract::CurrentUser,
    db::repository::webhook::{Delivery, Event, Format, Webhook},
    error::{Error, ErrorCode, Result},
    state::AppState,
    webhook::{address, worker::generate_secret},
};

/// How many webhooks an account, or the instance, may have. Each one is a
/// request per event, so this bounds what one download can set off.
const MAX_WEBHOOKS: u64 = 10;

/// The longest URL accepted.
const MAX_URL_LENGTH: usize = 2048;

/// How many recent events a listing shows.
const DELIVERIES_SHOWN: u64 = 50;

/// A webhook, as a listing describes it. Never carries the secret.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WebhookResponse {
    /// Stable identifier.
    #[schema(example = "01K1ZQK9V8QF5S3T4M2N6P7R8S")]
    pub id: String,
    /// Where events are sent.
    #[schema(example = "https://example.com/hooks/tessera")]
    pub url: String,
    /// What it is sent: `share.created`, `share.downloaded`, `share.expired`.
    #[schema(example = json!(["share.downloaded"]))]
    pub events: Vec<String>,
    /// `json`, or `discord` for a Discord channel's webhook.
    #[schema(example = "json")]
    pub format: String,
    /// When it was added, RFC 3339.
    pub created_at: String,
}

impl From<Webhook> for WebhookResponse {
    fn from(hook: Webhook) -> Self {
        Self {
            id: hook.id.to_string(),
            url: hook.url,
            events: hook
                .events
                .iter()
                .map(|event| event.as_str().to_owned())
                .collect(),
            format: hook.format.as_str().to_owned(),
            created_at: format_timestamp(hook.created_at),
        }
    }
}

/// A newly added webhook. The only response that carries the secret.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreatedWebhookResponse {
    /// The webhook, as a listing describes it.
    #[serde(flatten)]
    pub webhook: WebhookResponse,
    /// What each body is signed with, as Standard Webhooks describes: the
    /// `webhook-signature` header is an HMAC-SHA256 of
    /// `{webhook-id}.{webhook-timestamp}.{body}`, keyed with the base64 after
    /// `whsec_`.
    ///
    /// **Shown once.**
    #[schema(example = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw")]
    pub secret: String,
}

/// Body of `POST /webhooks` and `POST /admin/webhooks`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateWebhookRequest {
    /// Where to send events. `http` or `https`; an account's must be a public
    /// address.
    #[schema(example = "https://example.com/hooks/tessera")]
    pub url: String,
    /// What to send: any of `share.created`, `share.downloaded`,
    /// `share.expired`.
    #[schema(example = json!(["share.downloaded"]))]
    pub events: Vec<String>,
    /// `json`, the default, or `discord` for a Discord channel's webhook URL.
    #[serde(default)]
    #[schema(example = "json")]
    pub format: Option<String>,
}

/// One event sent, or being sent, to a webhook.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeliveryResponse {
    /// Stable identifier, sent as the `webhook-id` header.
    pub id: String,
    /// Which event.
    #[schema(example = "share.downloaded")]
    pub event: String,
    /// `pending` while it is still being tried, then `delivered` or `failed`.
    #[schema(example = "delivered")]
    pub state: String,
    /// How many times it has been sent.
    pub attempts: u32,
    /// The status the receiver last answered with.
    pub last_status: Option<u16>,
    /// Why the last attempt failed.
    pub last_error: Option<String>,
    /// When it is tried next, while it is pending.
    pub next_attempt_at: Option<String>,
    /// When the receiver accepted it.
    pub delivered_at: Option<String>,
    /// When the event happened.
    pub created_at: String,
}

impl From<Delivery> for DeliveryResponse {
    fn from(delivery: Delivery) -> Self {
        let state = if delivery.delivered_at.is_some() {
            "delivered"
        } else if delivery.next_attempt_at.is_some() {
            "pending"
        } else {
            "failed"
        };
        Self {
            id: delivery.id.to_string(),
            event: delivery.event,
            state: state.to_owned(),
            attempts: delivery.attempts,
            last_status: delivery.last_status,
            last_error: delivery.last_error,
            next_attempt_at: delivery.next_attempt_at.map(format_timestamp),
            delivered_at: delivery.delivered_at.map(format_timestamp),
            created_at: format_timestamp(delivery.created_at),
        }
    }
}

/// Lists the caller's webhooks.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when the account cannot see its
/// shares.
#[utoipa::path(
    get,
    path = "/webhooks",
    operation_id = "list_webhooks",
    tag = WEBHOOKS_TAG,
    responses(
        (status = 200, description = "The caller's webhooks", body = Vec<WebhookResponse>),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Not allowed", body = crate::error::ErrorBody),
    ),
    security(("session" = []), ("bearer" = [])),
)]
pub async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<WebhookResponse>>> {
    current.require(Permission::SharesRead)?;
    listed(&state, Some(current.user.id)).await
}

/// Adds a webhook told about the caller's own shares, and returns its signing
/// secret, once.
///
/// # Errors
///
/// `400` when the URL or events are unusable, or the URL is not a public
/// address; `403` when authenticated with a token, or the account cannot see
/// its shares.
#[utoipa::path(
    post,
    path = "/webhooks",
    operation_id = "create_webhook",
    tag = WEBHOOKS_TAG,
    request_body = CreateWebhookRequest,
    responses(
        (status = 201, description = "Added; the secret is shown only here", body = CreatedWebhookResponse),
        (status = 400, description = "The URL, events or format are unusable, or there are too many", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token, or not allowed", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn create(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(body): ApiJson<CreateWebhookRequest>,
) -> Result<axum::response::Response> {
    session_with(&current, Permission::SharesRead)?;
    created(&state, Some(current.user.id), body).await
}

/// Removes one of the caller's webhooks, and whatever was queued for it.
///
/// # Errors
///
/// `404` when the caller has no webhook with that id.
#[utoipa::path(
    delete,
    path = "/webhooks/{id}",
    operation_id = "delete_webhook",
    tag = WEBHOOKS_TAG,
    params(("id" = String, Path, description = "The webhook's identifier")),
    responses(
        (status = 204, description = "Removed"),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token, or not allowed", body = crate::error::ErrorBody),
        (status = 404, description = "No webhook of yours has that id", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn delete(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<String>,
) -> Result<StatusCode> {
    session_with(&current, Permission::SharesRead)?;
    deleted(&state, Some(current.user.id), &id).await
}

/// The most recent events sent to one of the caller's webhooks, newest first.
///
/// # Errors
///
/// `404` when the caller has no webhook with that id.
#[utoipa::path(
    get,
    path = "/webhooks/{id}/deliveries",
    operation_id = "list_webhook_deliveries",
    tag = WEBHOOKS_TAG,
    params(("id" = String, Path, description = "The webhook's identifier")),
    responses(
        (status = 200, description = "Its most recent events", body = Vec<DeliveryResponse>),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Not allowed", body = crate::error::ErrorBody),
        (status = 404, description = "No webhook of yours has that id", body = crate::error::ErrorBody),
    ),
    security(("session" = []), ("bearer" = [])),
)]
pub async fn deliveries(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<String>,
) -> Result<Json<Vec<DeliveryResponse>>> {
    current.require(Permission::SharesRead)?;
    delivered(&state, Some(current.user.id), &id).await
}

/// Lists the instance's webhooks.
///
/// # Errors
///
/// `403` when the caller cannot change settings, or used a bearer token.
#[utoipa::path(
    get,
    path = "/admin/webhooks",
    operation_id = "admin_list_webhooks",
    tag = ADMIN_TAG,
    responses(
        (status = 200, description = "The instance's webhooks", body = Vec<WebhookResponse>),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Not an administrator, or a bearer token", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn admin_list(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<WebhookResponse>>> {
    session_with(&current, Permission::SettingsWrite)?;
    listed(&state, None).await
}

/// Adds a webhook told about every share, and returns its signing secret,
/// once.
///
/// # Errors
///
/// `400` when the URL or events are unusable, `403` when the caller cannot
/// change settings.
#[utoipa::path(
    post,
    path = "/admin/webhooks",
    operation_id = "admin_create_webhook",
    tag = ADMIN_TAG,
    request_body = CreateWebhookRequest,
    responses(
        (status = 201, description = "Added; the secret is shown only here", body = CreatedWebhookResponse),
        (status = 400, description = "The URL, events or format are unusable, or there are too many", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Not an administrator, or a bearer token", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn admin_create(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(body): ApiJson<CreateWebhookRequest>,
) -> Result<axum::response::Response> {
    session_with(&current, Permission::SettingsWrite)?;
    created(&state, None, body).await
}

/// Removes one of the instance's webhooks, and whatever was queued for it.
///
/// # Errors
///
/// `404` when the instance has no webhook with that id.
#[utoipa::path(
    delete,
    path = "/admin/webhooks/{id}",
    operation_id = "admin_delete_webhook",
    tag = ADMIN_TAG,
    params(("id" = String, Path, description = "The webhook's identifier")),
    responses(
        (status = 204, description = "Removed"),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Not an administrator, or a bearer token", body = crate::error::ErrorBody),
        (status = 404, description = "No webhook of the instance has that id", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn admin_delete(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<String>,
) -> Result<StatusCode> {
    session_with(&current, Permission::SettingsWrite)?;
    deleted(&state, None, &id).await
}

/// The most recent events sent to one of the instance's webhooks, newest
/// first.
///
/// # Errors
///
/// `404` when the instance has no webhook with that id.
#[utoipa::path(
    get,
    path = "/admin/webhooks/{id}/deliveries",
    operation_id = "admin_list_webhook_deliveries",
    tag = ADMIN_TAG,
    params(("id" = String, Path, description = "The webhook's identifier")),
    responses(
        (status = 200, description = "Its most recent events", body = Vec<DeliveryResponse>),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Not an administrator, or a bearer token", body = crate::error::ErrorBody),
        (status = 404, description = "No webhook of the instance has that id", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn admin_deliveries(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<String>,
) -> Result<Json<Vec<DeliveryResponse>>> {
    session_with(&current, Permission::SettingsWrite)?;
    delivered(&state, None, &id).await
}

/// A browser session holding `permission`. A token may not add a webhook: one
/// that leaked would otherwise be a way to keep hearing about the account.
fn session_with(current: &CurrentUser, permission: Permission) -> Result<()> {
    if !current.credential.is_session() {
        return Err(Error::new(ErrorCode::SessionRequired));
    }
    current.require(permission)
}

async fn listed(state: &AppState, owner: Option<UserId>) -> Result<Json<Vec<WebhookResponse>>> {
    let hooks = state.webhooks.list(owner).await?;
    Ok(Json(hooks.into_iter().map(WebhookResponse::from).collect()))
}

async fn created(
    state: &AppState,
    owner: Option<UserId>,
    body: CreateWebhookRequest,
) -> Result<axum::response::Response> {
    let url = checked_url(&body.url, owner.is_some())?;
    let events = checked_events(&body.events)?;
    let format = match body.format.as_deref() {
        None => Format::Json,
        Some(name) => Format::parse(name).ok_or_else(|| {
            Error::invalid("format", "not_a_format")
                .with_message("`format` must be `json` or `discord`")
        })?,
    };
    if state.webhooks.count(owner).await? >= MAX_WEBHOOKS {
        return Err(Error::out_of_range("webhooks", None, Some(MAX_WEBHOOKS))
            .with_message(format!("at most {MAX_WEBHOOKS} webhooks; remove one first")));
    }
    let hook = Webhook {
        id: WebhookId::generate(),
        owner,
        url,
        events,
        format,
        secret: generate_secret(),
        created_at: OffsetDateTime::now_utc(),
    };
    state.webhooks.create(&hook).await?;
    let secret = hook.secret.clone();
    Ok((
        StatusCode::CREATED,
        Json(CreatedWebhookResponse {
            webhook: hook.into(),
            secret,
        }),
    )
        .into_response())
}

async fn deleted(state: &AppState, owner: Option<UserId>, id: &str) -> Result<StatusCode> {
    // An unparsable id and someone else's get the same answer.
    let id = WebhookId::parse(id).map_err(|_| Error::NotFound)?;
    if state.webhooks.delete(id, owner).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(Error::NotFound)
    }
}

async fn delivered(
    state: &AppState,
    owner: Option<UserId>,
    id: &str,
) -> Result<Json<Vec<DeliveryResponse>>> {
    let id = WebhookId::parse(id).map_err(|_| Error::NotFound)?;
    if state.webhooks.find(id, owner).await?.is_none() {
        return Err(Error::NotFound);
    }
    let deliveries = state.webhooks.deliveries(id, DELIVERIES_SHOWN).await?;
    Ok(Json(
        deliveries.into_iter().map(DeliveryResponse::from).collect(),
    ))
}

/// The URL, if it is one a webhook can send to. An account's has to be a
/// public address.
fn checked_url(raw: &str, account: bool) -> Result<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(Error::new(ErrorCode::ValueEmpty).with_detail("field", "url"));
    }
    if raw.len() > MAX_URL_LENGTH {
        return Err(Error::new(ErrorCode::ValueLength)
            .with_detail("field", "url")
            .with_detail("max", MAX_URL_LENGTH));
    }
    let url = url::Url::parse(raw)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https") && url.host().is_some())
        .ok_or_else(|| {
            Error::invalid("url", "not_a_url").with_message("`url` must be an http or https URL")
        })?;
    if account && address::check(&url).is_err() {
        return Err(Error::invalid("url", "not_public")
            .with_message("`url` must be a public address, not one on this server's network"));
    }
    Ok(url.into())
}

fn checked_events(names: &[String]) -> Result<Vec<Event>> {
    let mut events = Vec::new();
    for name in names {
        let event = Event::parse(name.trim()).ok_or_else(|| {
            Error::invalid("events", "not_an_event")
                .with_detail("event", name.as_str())
                .with_message(format!(
                    "`{name}` is not an event; use share.created, share.downloaded or share.expired"
                ))
        })?;
        if !events.contains(&event) {
            events.push(event);
        }
    }
    if events.is_empty() {
        return Err(Error::new(ErrorCode::ValueEmpty).with_detail("field", "events"));
    }
    Ok(events)
}
