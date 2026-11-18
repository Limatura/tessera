//! `/admin/audit-events`: the audit log, as the panel reads it. See
//! [`crate::audit`] for what is in it.
//!
//! Both need `audit:read` and a session, for the reason `logs` gives: the log
//! names people and what they did. Exporting is itself recorded.

// utoipa's `params(..)` emits a fully-qualified path to the type outside the
// function it decorates, so the suppression has to be module-wide. Same
// arrangement as `super::admin`.
#![expect(
    unused_qualifications,
    reason = "emitted by utoipa's params(..), not written here"
)]

use std::collections::HashMap;

use axum::{
    Json,
    body::Body,
    extract::State,
    http::{HeaderValue, header},
    response::{IntoResponse as _, Response},
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tessera_core::{Permission, UserId};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use utoipa::{IntoParams, ToSchema};

use crate::{
    api::{
        ADMIN_TAG,
        admin::{admitted, require_admin_feature},
        extract::ApiQuery,
        tokens::format_timestamp,
    },
    audit::{self, Actor, Event, Request, action},
    auth::extract::CurrentUser,
    db::repository::audit_event::{ActionPattern, AuditEvent, AuditFilter, Outcome},
    error::{Error, ErrorCode, Result},
    state::AppState,
};

/// How many entries a page holds when the caller does not say.
const DEFAULT_PAGE: u64 = 50;
/// The most entries one page holds.
const MAX_PAGE: u64 = 200;
/// How many rows the export reads at a time.
const EXPORT_BATCH: u64 = 500;

/// Query of `GET /admin/audit-events`.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct AuditQueryParams {
    /// Comma-separated actions. `auth.*` matches every action under `auth.`.
    #[param(example = "share.create,auth.*")]
    #[serde(default)]
    pub action: Option<String>,
    /// `success` or `failure`.
    #[serde(default)]
    pub outcome: Option<String>,
    /// Only what this account did, by id.
    #[serde(default)]
    pub actor: Option<String>,
    /// Only what was done to this kind of thing: `share`, `user`, `role`,
    /// `invite`, `setting` or `api_token`.
    #[serde(default)]
    pub target_type: Option<String>,
    /// Only what was done to this one.
    #[serde(default)]
    pub target_id: Option<String>,
    /// Nothing before this, RFC 3339.
    #[serde(default)]
    pub from: Option<String>,
    /// Nothing after this, RFC 3339.
    #[serde(default)]
    pub to: Option<String>,
    /// `next_cursor` from the previous page.
    #[serde(default)]
    pub cursor: Option<String>,
    /// How many, 50 when omitted, at most 200.
    #[serde(default)]
    pub limit: Option<u64>,
}

/// Query of `GET /admin/audit-events/export`: the same filters, no paging.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct AuditExportParams {
    /// Comma-separated actions. `auth.*` matches every action under `auth.`.
    #[serde(default)]
    pub action: Option<String>,
    /// `success` or `failure`.
    #[serde(default)]
    pub outcome: Option<String>,
    /// Only what this account did, by id.
    #[serde(default)]
    pub actor: Option<String>,
    /// Only what was done to this kind of thing.
    #[serde(default)]
    pub target_type: Option<String>,
    /// Only what was done to this one.
    #[serde(default)]
    pub target_id: Option<String>,
    /// Nothing before this, RFC 3339.
    #[serde(default)]
    pub from: Option<String>,
    /// Nothing after this, RFC 3339.
    #[serde(default)]
    pub to: Option<String>,
}

/// Who did it, with enough to show without another request.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AuditActor {
    /// The account's id.
    pub id: String,
    /// What to call them: the username.
    pub display_name: String,
    /// Their address. Empty once the account has been removed and scrubbed.
    pub email: String,
}

/// One entry.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AuditEventResponse {
    /// Identifier, sortable by when it was written.
    pub id: String,
    /// When it happened, RFC 3339.
    pub occurred_at: String,
    /// What was done, such as `share.create`.
    #[schema(example = "share.create")]
    pub action: String,
    /// `success` or `failure`.
    #[schema(example = "success")]
    pub outcome: String,
    /// `anonymous`, `user`, `api_token` or `system`.
    #[schema(example = "user")]
    pub actor_kind: String,
    /// The account that did it, when one did and it can still be read.
    pub actor: Option<AuditActor>,
    /// The account's id, kept even when the account can no longer be read.
    pub actor_user_id: Option<String>,
    /// The token it was done with.
    pub api_token_id: Option<String>,
    /// What kind of thing it was done to.
    pub target_type: Option<String>,
    /// Which one.
    pub target_id: Option<String>,
    /// Where it came from, when there is an honest answer and it was kept.
    pub ip_address: Option<String>,
    /// What the client called itself.
    pub user_agent: Option<String>,
    /// Whatever this action recorded.
    #[schema(value_type = Option<Object>)]
    pub metadata: Option<serde_json::Value>,
}

/// A page of `GET /admin/audit-events`, newest first.
#[derive(Debug, Serialize, ToSchema)]
pub struct AuditEventPage {
    /// The entries.
    pub items: Vec<AuditEventResponse>,
    /// Pass back as `cursor` for older entries. `null` when there are none.
    pub next_cursor: Option<String>,
}

/// Reads the audit log, newest first, a page at a time.
///
/// # Errors
///
/// `400` for a filter that does not parse, `401` when nothing was signed in,
/// `403` without `audit:read` or with an API token, `404` when this instance
/// has no administration.
#[utoipa::path(
    get,
    path = "/admin/audit-events",
    operation_id = "list_audit_events",
    tag = ADMIN_TAG,
    params(AuditQueryParams),
    responses(
        (status = 200, description = "A page of entries", body = AuditEventPage),
        (status = 400, description = "A filter does not parse", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not read the audit log, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no administration", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiQuery(params): ApiQuery<AuditQueryParams>,
) -> Result<Json<AuditEventPage>> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::AuditRead)?;

    let filter = filter_of(&AuditExportParams {
        action: params.action,
        outcome: params.outcome,
        actor: params.actor,
        target_type: params.target_type,
        target_id: params.target_id,
        from: params.from,
        to: params.to,
    })?;
    let cursor = params
        .cursor
        .as_deref()
        .map(|cursor| {
            tessera_core::AuditEventId::parse(cursor)
                .map(|id| id.to_string())
                .map_err(|_| invalid("`cursor` is not one this endpoint handed out"))
        })
        .transpose()?;
    let limit = params.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);

    // One more than asked for says whether there is a next page without a
    // second query.
    let mut rows = state
        .audit
        .list(&filter, cursor.as_deref(), limit.saturating_add(1))
        .await?;
    let more = rows.len() > usize::try_from(limit).unwrap_or(usize::MAX);
    rows.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    let next_cursor = more
        .then(|| rows.last().map(|row| row.id.clone()))
        .flatten();

    let actors = actors_of(&state, &rows).await?;
    Ok(Json(AuditEventPage {
        items: rows.into_iter().map(|row| respond(row, &actors)).collect(),
        next_cursor,
    }))
}

/// Downloads every matching entry as CSV, newest first.
///
/// Streamed, so a year of entries is not held in memory. The export is itself
/// recorded, with the filters it was asked for.
///
/// # Errors
///
/// As for [`list`].
#[utoipa::path(
    get,
    path = "/admin/audit-events/export",
    operation_id = "export_audit_events",
    tag = ADMIN_TAG,
    params(AuditExportParams),
    responses(
        (status = 200, description = "Every matching entry, one per line", content_type = "text/csv", body = String),
        (status = 400, description = "A filter does not parse", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "May not read the audit log, or authenticated with an API token", body = crate::error::ErrorBody),
        (status = 404, description = "This instance has no administration", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn export(
    State(state): State<AppState>,
    current: CurrentUser,
    request: Request,
    ApiQuery(params): ApiQuery<AuditExportParams>,
) -> Result<Response> {
    require_admin_feature(&state)?;
    admitted(&current, Permission::AuditRead)?;

    let filter = filter_of(&params)?;

    // Recorded before the first row is read, so the export appears in itself
    // when its filters allow, and a download abandoned halfway is still on
    // the record.
    Event::success(action::AUDIT_EXPORT, Actor::of(&current))
        .with(json!({
            "action": params.action,
            "outcome": params.outcome,
            "actor": params.actor,
            "target_type": params.target_type,
            "target_id": params.target_id,
            "from": params.from,
            "to": params.to,
        }))
        .record(&state, &request)
        .await;

    let rows = async_stream::stream! {
        yield Ok::<_, std::io::Error>(Bytes::from_static(CSV_HEADER.as_bytes()));
        let mut cursor: Option<String> = None;
        loop {
            let batch = match state.audit.list(&filter, cursor.as_deref(), EXPORT_BATCH).await {
                Ok(batch) => batch,
                Err(error) => {
                    tracing::warn!(?error, "could not read the audit log for an export");
                    yield Err(std::io::Error::other("the audit log could not be read"));
                    break;
                }
            };
            let Ok(actors) = actors_of(&state, &batch).await else {
                yield Err(std::io::Error::other("the audit log could not be read"));
                break;
            };
            let full = batch.len() == usize::try_from(EXPORT_BATCH).unwrap_or(usize::MAX);
            cursor = batch.last().map(|row| row.id.clone());
            let mut chunk = String::new();
            for row in batch {
                csv_line(&mut chunk, &respond(row, &actors));
            }
            yield Ok(Bytes::from(chunk));
            if !full {
                break;
            }
        }
    };

    let stamp = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default()
        .replace(':', "-");
    let disposition = format!("attachment; filename=\"tessera-audit-{stamp}.csv\"");
    let mut response = Body::from_stream(rows).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    Ok(response)
}

/// Builds the filter both endpoints share, refusing anything that would
/// silently match nothing.
fn filter_of(params: &AuditExportParams) -> Result<AuditFilter> {
    let outcome = params.outcome.as_deref();
    let actor = params.actor.as_deref();
    let target_type = params.target_type.as_deref();
    let target_id = params.target_id.as_deref();
    let mut filter = AuditFilter::default();
    for wanted in params
        .action
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|wanted| !wanted.is_empty())
    {
        filter.actions.push(action_pattern(wanted)?);
    }
    filter.outcome = outcome
        .filter(|value| !value.is_empty())
        .map(|value| {
            Outcome::parse(value).ok_or_else(|| invalid("`outcome` must be `success` or `failure`"))
        })
        .transpose()?;
    filter.actor_user_id = actor
        .filter(|value| !value.is_empty())
        .map(|value| UserId::parse(value).map_err(|_| invalid("`actor` is not an account id")))
        .transpose()?;
    if let Some(kind) = target_type.filter(|value| !value.is_empty()) {
        if !audit::TARGET_TYPES.contains(&kind) {
            return Err(invalid(&format!(
                "`target_type` must be one of {}",
                audit::TARGET_TYPES.join(", ")
            )));
        }
        filter.target_type = Some(kind.to_owned());
    }
    filter.target_id = target_id
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    filter.from = instant("from", params.from.as_deref())?;
    filter.to = instant("to", params.to.as_deref())?;
    Ok(filter)
}

/// One entry of `action`: a known action, or a namespace ending in `.*`.
fn action_pattern(wanted: &str) -> Result<ActionPattern> {
    if let Some(namespace) = wanted.strip_suffix(".*") {
        let prefix = format!("{namespace}.");
        if action::ALL.iter().any(|known| known.starts_with(&prefix)) {
            return Ok(ActionPattern::Prefix(prefix));
        }
    } else if action::ALL.contains(&wanted) {
        return Ok(ActionPattern::Exact(wanted.to_owned()));
    }
    Err(invalid(&format!(
        "`{wanted}` is not an action this log records"
    )))
}

fn instant(name: &str, value: Option<&str>) -> Result<Option<OffsetDateTime>> {
    value
        .filter(|value| !value.is_empty())
        .map(|value| {
            OffsetDateTime::parse(value, &Rfc3339)
                .map_err(|_| invalid(&format!("`{name}` is not an RFC 3339 time")))
        })
        .transpose()
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorCode::InvalidQuery).with_message(message.to_owned())
}

/// The accounts named in a page, read once each.
async fn actors_of(state: &AppState, rows: &[AuditEvent]) -> Result<HashMap<String, AuditActor>> {
    let mut actors = HashMap::new();
    for id in rows.iter().filter_map(|row| row.actor_user_id.as_deref()) {
        if actors.contains_key(id) {
            continue;
        }
        let Ok(user_id) = UserId::parse(id) else {
            continue;
        };
        if let Some(user) = state.users.find_by_id(user_id).await? {
            actors.insert(
                id.to_owned(),
                AuditActor {
                    id: id.to_owned(),
                    display_name: user.username.to_string(),
                    email: user.email.to_string(),
                },
            );
        }
    }
    Ok(actors)
}

fn respond(row: AuditEvent, actors: &HashMap<String, AuditActor>) -> AuditEventResponse {
    AuditEventResponse {
        actor: row
            .actor_user_id
            .as_deref()
            .and_then(|id| actors.get(id))
            .cloned(),
        id: row.id,
        occurred_at: format_timestamp(row.occurred_at),
        action: row.action,
        outcome: row.outcome,
        actor_kind: row.actor_kind.as_str().to_owned(),
        actor_user_id: row.actor_user_id,
        api_token_id: row.api_token_id,
        target_type: row.target_type,
        target_id: row.target_id,
        ip_address: row.ip_address,
        user_agent: row.user_agent,
        metadata: row.metadata,
    }
}

const CSV_HEADER: &str = "id,occurred_at,action,outcome,actor_kind,actor_user_id,actor_name,\
                          api_token_id,target_type,target_id,ip_address,user_agent,metadata\r\n";

fn csv_line(out: &mut String, event: &AuditEventResponse) {
    let metadata = event
        .metadata
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default();
    let fields = [
        event.id.as_str(),
        event.occurred_at.as_str(),
        event.action.as_str(),
        event.outcome.as_str(),
        event.actor_kind.as_str(),
        event.actor_user_id.as_deref().unwrap_or_default(),
        event
            .actor
            .as_ref()
            .map(|actor| actor.display_name.as_str())
            .unwrap_or_default(),
        event.api_token_id.as_deref().unwrap_or_default(),
        event.target_type.as_deref().unwrap_or_default(),
        event.target_id.as_deref().unwrap_or_default(),
        event.ip_address.as_deref().unwrap_or_default(),
        event.user_agent.as_deref().unwrap_or_default(),
        metadata.as_str(),
    ];
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        csv_field(out, field);
    }
    out.push_str("\r\n");
}

/// Writes one field, quoted when it needs to be.
///
/// A field a spreadsheet would read as a formula gets a leading `'`. A User-Agent
/// or a setting key is whatever someone sent, and an export is opened in a
/// spreadsheet more often than not.
fn csv_field(out: &mut String, value: &str) {
    let formula = value.starts_with(['=', '+', '-', '@', '\t', '\r']);
    let quote = formula || value.contains([',', '"', '\n', '\r']);
    if quote {
        out.push('"');
    }
    if formula {
        out.push('\'');
    }
    for character in value.chars() {
        if character == '"' {
            out.push('"');
        }
        out.push(character);
    }
    if quote {
        out.push('"');
    }
}

#[cfg(test)]
mod tests {
    use super::{action_pattern, csv_field};
    use crate::db::repository::audit_event::ActionPattern;

    fn field(value: &str) -> String {
        let mut out = String::new();
        csv_field(&mut out, value);
        out
    }

    #[test]
    fn a_plain_field_is_left_alone() {
        assert_eq!(field("share.create"), "share.create");
    }

    #[test]
    fn a_field_with_a_comma_or_quote_is_quoted() {
        assert_eq!(field("a,b"), "\"a,b\"");
        assert_eq!(field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    /// A User-Agent of `=HYPERLINK(...)` must not become a link in whatever
    /// spreadsheet the export is opened in.
    #[test]
    fn a_formula_is_defused() {
        assert_eq!(field("=1+1"), "\"'=1+1\"");
        assert_eq!(field("@SUM(A1)"), "\"'@SUM(A1)\"");
    }

    #[test]
    fn a_namespace_matches_by_prefix() {
        assert_eq!(
            action_pattern("auth.*").ok(),
            Some(ActionPattern::Prefix("auth.".to_owned()))
        );
        assert_eq!(
            action_pattern("share.create").ok(),
            Some(ActionPattern::Exact("share.create".to_owned()))
        );
    }

    #[test]
    fn an_unknown_action_is_refused() {
        assert!(action_pattern("share.nonsense").is_err());
        assert!(action_pattern("nothing.*").is_err());
    }
}
