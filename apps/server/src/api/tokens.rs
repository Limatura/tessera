//! API tokens: the credential for calling this service from something that is
//! not a browser.

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tessera_core::{ApiTokenId, Permission, PermissionSet};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    api::{
        TOKENS_TAG,
        extract::{ApiJson, ApiPath},
    },
    auth::extract::CurrentUser,
    db::repository::api_token::ApiToken,
    error::{Error, ErrorCode, Result},
    state::AppState,
};

/// Longest accepted token name.
const MAX_NAME_LENGTH: usize = 64;

/// Longest lifetime a caller may ask for, in days.
///
/// Not a security boundary, since a caller can always create another, but a
/// ten-year expiry is one that never gets revisited. This makes the choice
/// explicit.
const MAX_EXPIRY_DAYS: u32 = 3650;

/// An issued token, as a listing describes it. Never carries the secret.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenResponse {
    /// Stable identifier; what `DELETE /tokens/{id}` takes.
    #[schema(example = "01K1ZQK9V8QF5S3T4M2N6P7R8S")]
    pub id: String,
    /// What the owner called it.
    #[schema(example = "backup script")]
    pub name: String,
    /// Leading characters of the secret, so a person can tell two tokens apart
    /// without the service having to store either of them.
    #[schema(example = "tsk_a1b2")]
    pub display_prefix: String,
    /// What it may do, as `resource:verb` strings.
    ///
    /// What it was minted with, not what it can do right now. Those differ
    /// when the owner has since been demoted, and the owner is who this list is
    /// for. Sending the narrowed set instead would make a token look as though
    /// it had been quietly edited.
    #[schema(example = json!(["shares:create"]))]
    pub permissions: Vec<String>,
    /// When it was issued, RFC 3339.
    pub created_at: String,
    /// When it was last accepted, if ever.
    pub last_used_at: Option<String>,
    /// When it stops being accepted, or `null` to live until revoked.
    pub expires_at: Option<String>,
    /// When it was revoked, if it was.
    pub revoked_at: Option<String>,
}

/// A newly issued token. The only response that ever carries the secret.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreatedTokenResponse {
    /// The token's metadata, identical to what a listing returns.
    #[serde(flatten)]
    pub token: TokenResponse,

    /// The secret, in full.
    ///
    /// **Shown once.** Only its SHA-256 digest is stored, so this value cannot be
    /// recovered, so a client that does not keep it must issue another token.
    #[schema(example = "tsk_a1b2c3d4e5f6...")]
    pub secret: String,
}

/// Body of `POST /tokens`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTokenRequest {
    /// A label for the token, so its owner can recognise it later.
    #[schema(example = "backup script")]
    pub name: String,

    /// What the token may do, as `resource:verb` strings.
    ///
    /// Omitted means `shares:create` and nothing else, since a token exists to
    /// upload, and a default that errs safe has to be widened explicitly. The
    /// other default would hand every screenshot tool the ability to delete
    /// everything its owner has.
    ///
    /// A permission the caller does not hold is refused: a token cannot be a
    /// way to exceed the account that minted it.
    #[serde(default)]
    pub permissions: Option<Vec<String>>,

    /// How long the token should last, in days. Omit for a token that lives
    /// until it is revoked.
    #[schema(example = 90)]
    pub expires_in_days: Option<u32>,
}

/// RFC 3339 is what every client's date parser accepts without being told a
/// format, which matters more here than the two bytes a Unix timestamp saves.
pub(crate) fn format_timestamp(value: OffsetDateTime) -> String {
    value
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| value.unix_timestamp().to_string())
}

impl From<ApiToken> for TokenResponse {
    fn from(token: ApiToken) -> Self {
        Self {
            id: token.id.to_string(),
            name: token.name,
            display_prefix: token.display_prefix,
            permissions: token
                .permissions
                .iter()
                .map(|permission| permission.as_str().to_owned())
                .collect(),
            created_at: format_timestamp(token.created_at),
            last_used_at: token.last_used_at.map(format_timestamp),
            expires_at: token.expires_at.map(format_timestamp),
            revoked_at: token.revoked_at.map(format_timestamp),
        }
    }
}

/// Lists the caller's tokens, newest first.
///
/// # Errors
///
/// `401` when nothing was signed in. An instance with accounts turned off
/// answers the same, having no account to be. `403` when the account
/// still owes a password change.
#[utoipa::path(
    get,
    path = "/tokens",
    operation_id = "list_tokens",
    tag = TOKENS_TAG,
    responses(
        (status = 200, description = "The caller's tokens", body = Vec<TokenResponse>),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "A password change is due", body = crate::error::ErrorBody),
    ),
    security(("session" = []), ("bearer" = [])),
)]
pub async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<TokenResponse>>> {
    current.require(Permission::TokensManage)?;

    let tokens = state.tokens.list_for_user(current.user.id).await?;
    Ok(Json(tokens.into_iter().map(TokenResponse::from).collect()))
}

/// What a token being minted may do.
///
/// # Why omitting the field is not "everything"
///
/// A token exists to upload. Defaulting to the whole account would mean every
/// screenshot tool and backup script handed out the ability to delete
/// everything its owner has, and that surfaces only when one leaks.
///
/// # Why a permission the caller lacks is refused
///
/// A token must not be a way to exceed the account that minted it. Without
/// this, an account demoted to `member` could still mint a token carrying
/// `users:delete`.
///
/// # Errors
///
/// `400` when a name is not a permission, `403` when the caller does not hold
/// one of them.
fn requested_permissions(current: &CurrentUser, asked: Option<&[String]>) -> Result<PermissionSet> {
    let Some(asked) = asked else {
        return Ok([Permission::SharesCreate].into_iter().collect());
    };
    let wanted: PermissionSet = asked
        .iter()
        .map(|name| {
            name.parse::<Permission>().map_err(|_| {
                Error::new(ErrorCode::UnknownPermission)
                    .with_detail("permission", name.as_str())
                    .with_message(format!("`{name}` is not a permission this version has"))
            })
        })
        .collect::<Result<_>>()?;
    for permission in wanted.iter() {
        current.require(permission)?;
    }
    Ok(wanted)
}

/// Issues a token and returns its secret, once.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when authenticated with a token
/// and not a session, `400` when the name or lifetime is unusable.
#[utoipa::path(
    post,
    path = "/tokens",
    operation_id = "create_token",
    tag = TOKENS_TAG,
    request_body = CreateTokenRequest,
    responses(
        (status = 201, description = "Issued; the secret is shown only here", body = CreatedTokenResponse),
        (status = 400, description = "The name or lifetime is unusable", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token, or a password change is due", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn create(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(body): ApiJson<CreateTokenRequest>,
) -> Result<Response> {
    // A token may not mint another. Otherwise one leaked token is permanent
    // access: revoking it does nothing, because whoever took it has already
    // issued three more under names that look like yours.
    if !current.credential.is_session() {
        return Err(Error::new(ErrorCode::SessionRequired));
    }
    current.require(Permission::TokensManage)?;

    let name = body.name.trim();
    if name.is_empty() {
        return Err(Error::new(ErrorCode::ValueEmpty).with_detail("field", "name"));
    }
    if name.chars().count() > MAX_NAME_LENGTH {
        return Err(Error::new(ErrorCode::ValueLength)
            .with_detail("field", "name")
            .with_detail("max", MAX_NAME_LENGTH)
            .with_message(format!(
                "`name` must be at most {MAX_NAME_LENGTH} characters"
            )));
    }

    let permissions = requested_permissions(&current, body.permissions.as_deref())?;

    let now = OffsetDateTime::now_utc();
    let expires_at = match body.expires_in_days {
        None => None,
        Some(0) => {
            return Err(Error::out_of_range(
                "expires_in_days",
                Some(1),
                Some(u64::from(MAX_EXPIRY_DAYS)),
            )
            .with_message(
                "`expires_in_days` must be at least 1; omit it for a token that never expires",
            ));
        }
        Some(days) if days > MAX_EXPIRY_DAYS => {
            return Err(Error::out_of_range(
                "expires_in_days",
                Some(1),
                Some(u64::from(MAX_EXPIRY_DAYS)),
            )
            .with_message(format!(
                "`expires_in_days` must be at most {MAX_EXPIRY_DAYS}"
            )));
        }
        Some(days) => Some(now.saturating_add(time::Duration::days(i64::from(days)))),
    };

    let (token, secret) = state
        .tokens
        .create(
            current.user.id,
            name.to_owned(),
            &permissions,
            now,
            expires_at,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(CreatedTokenResponse {
            token: token.into(),
            secret: secret.expose().to_owned(),
        }),
    )
        .into_response())
}

/// Revokes one of the caller's tokens.
///
/// # Errors
///
/// `401` when nothing was signed in, `403` when authenticated with a token,
/// `404` when no live token of the caller's has that id.
#[utoipa::path(
    delete,
    path = "/tokens/{id}",
    tag = TOKENS_TAG,
    params(("id" = String, Path, description = "The token's identifier")),
    responses(
        (status = 204, description = "Revoked"),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "Authenticated with an API token, or a password change is due", body = crate::error::ErrorBody),
        (status = 404, description = "No live token of yours has that id", body = crate::error::ErrorBody),
    ),
    security(("session" = [])),
)]
pub async fn revoke(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<String>,
) -> Result<StatusCode> {
    // Same reasoning as creation: a leaked token must not be able to revoke the
    // owner's other credentials.
    if !current.credential.is_session() {
        return Err(Error::new(ErrorCode::SessionRequired));
    }
    current.require(Permission::TokensManage)?;

    // An unparsable id and another account's id get the same answer. Telling
    // them apart would confirm which identifiers exist.
    let token_id = ApiTokenId::parse(&id).map_err(|_| Error::NotFound)?;

    let revoked = state
        .tokens
        .revoke(token_id, current.user.id, OffsetDateTime::now_utc())
        .await?;

    if revoked {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(Error::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_are_rfc_3339() {
        let stamp = OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("representable");
        assert_eq!(format_timestamp(stamp), "2023-11-14T22:13:20Z");
    }

    /// The secret must be the only field a listing cannot produce; everything
    /// else has to be identical, or a client has to branch on which call it made.
    #[test]
    fn a_created_token_serialises_its_metadata_flat() {
        let response = CreatedTokenResponse {
            token: TokenResponse {
                id: "01K1ZQK9V8QF5S3T4M2N6P7R8S".to_owned(),
                name: "backup script".to_owned(),
                display_prefix: "tsk_a1b2".to_owned(),
                permissions: vec!["shares:create".to_owned()],
                created_at: "2023-11-14T22:13:20Z".to_owned(),
                last_used_at: None,
                expires_at: None,
                revoked_at: None,
            },
            secret: "tsk_secret".to_owned(),
        };

        let json = serde_json::to_value(&response).expect("serialisable");
        assert_eq!(json["name"], "backup script");
        assert_eq!(json["secret"], "tsk_secret");
        assert!(json.get("token").is_none(), "metadata must not be nested");
    }
}
