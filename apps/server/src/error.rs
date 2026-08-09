//! The crate error type and its HTTP representation.
//!
//! One error type crosses every layer, and exactly one place decides what a given
//! failure looks like on the wire ([`Error::into_response`]). Handlers therefore
//! never construct status codes by hand, and the response body shape is part of
//! the OpenAPI contract rather than an accident of whatever each handler returned.

use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Convenience alias for fallible operations inside this crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Every failure the server can surface to a client.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The request was well-formed but asked for something that does not exist.
    #[error("not found")]
    NotFound,

    /// The request itself was malformed.
    #[error("{0}")]
    BadRequest(String),

    /// The request body exceeded `server.max_body_bytes`.
    #[error("payload too large")]
    PayloadTooLarge,

    /// The caller did not prove who they are, or the proof is no longer good.
    #[error("unauthorized")]
    Unauthorized,

    /// The caller is known but not allowed to do this.
    #[error("forbidden")]
    Forbidden,

    /// The account must set a new password before it may do anything else.
    ///
    /// Distinct from [`Error::Forbidden`] because a client can act on it: the
    /// answer is always "send the user to the change-password screen", and
    /// collapsing it into a generic 403 leaves the client guessing which of the
    /// several reasons it was.
    #[error("password change required")]
    PasswordChangeRequired,

    /// The request conflicts with something that already exists.
    #[error("{0}")]
    Conflict(String),

    /// The caller has been asked to slow down.
    #[error("too many requests")]
    TooManyRequests {
        /// Seconds until the caller may try again.
        retry_after_secs: u64,
    },

    /// A value failed a domain rule.
    #[error("{0}")]
    Domain(#[from] tessera_core::DomainError),

    /// The service is not ready to take traffic (starting up or shutting down).
    #[error("service unavailable")]
    Unavailable,

    /// A relational store operation failed. Covers migrations too — SeaORM
    /// reports both through `DbErr`.
    #[error("database error")]
    Database(#[from] sea_orm::DbErr),

    /// A blob store operation failed.
    #[error("storage error")]
    Storage(#[from] object_store::Error),

    /// Hashing or verifying a password failed. Never means "wrong password" —
    /// that is a `bool`, not an error.
    #[error("password hashing error")]
    Password(#[from] crate::auth::password::PasswordError),

    /// Local filesystem access failed.
    #[error("io error")]
    Io(#[from] std::io::Error),

    /// Configuration was rejected. Reachable only during start-up.
    #[error("configuration error")]
    Config(#[from] crate::config::ConfigError),
}

/// Stable, machine-readable identifier for an [`Error`].
///
/// Clients branch on this; the human-readable `message` is free to change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// See [`Error::NotFound`].
    NotFound,
    /// See [`Error::BadRequest`].
    BadRequest,
    /// See [`Error::PayloadTooLarge`].
    PayloadTooLarge,
    /// See [`Error::Unauthorized`].
    Unauthorized,
    /// See [`Error::Forbidden`].
    Forbidden,
    /// See [`Error::PasswordChangeRequired`].
    PasswordChangeRequired,
    /// See [`Error::Conflict`].
    Conflict,
    /// See [`Error::TooManyRequests`].
    TooManyRequests,
    /// See [`Error::Unavailable`].
    Unavailable,
    /// Any failure whose cause is deliberately not exposed to the caller.
    Internal,
}

/// The JSON body returned for every non-2xx response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorBody {
    /// Stable identifier the client can branch on.
    pub code: ErrorCode,
    /// Human-readable explanation. Never contains internal details.
    pub message: String,
}

impl ErrorCode {
    /// HTTP status this code maps to.
    #[must_use]
    pub const fn status(self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::BadRequest => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden | Self::PasswordChangeRequired => StatusCode::FORBIDDEN,
            Self::Conflict => StatusCode::CONFLICT,
            Self::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Generic, client-safe description of this class of failure.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::NotFound => "the requested resource does not exist",
            Self::BadRequest => "the request could not be understood",
            Self::Unauthorized => "sign in to do that",
            Self::Forbidden => "your account is not allowed to do that",
            Self::PasswordChangeRequired => "set a new password before doing anything else",
            Self::Conflict => "that already exists",
            Self::TooManyRequests => "too many attempts; wait a little and try again",
            Self::PayloadTooLarge => "the request body exceeds the configured limit",
            Self::Unavailable => "the service is not accepting requests right now",
            Self::Internal => "an internal error occurred",
        }
    }
}

impl Error {
    /// Convenience constructor for [`Error::BadRequest`].
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    /// Stable identifier for this error.
    ///
    /// This is the only place that classifies a variant. Status and message are
    /// both derived from the result, so adding a variant cannot leave three
    /// parallel `match`es two-thirds updated.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::NotFound => ErrorCode::NotFound,
            Self::BadRequest(_) | Self::Domain(_) => ErrorCode::BadRequest,
            Self::Unauthorized => ErrorCode::Unauthorized,
            Self::Forbidden => ErrorCode::Forbidden,
            Self::PasswordChangeRequired => ErrorCode::PasswordChangeRequired,
            Self::Conflict(_) => ErrorCode::Conflict,
            Self::TooManyRequests { .. } => ErrorCode::TooManyRequests,
            Self::PayloadTooLarge => ErrorCode::PayloadTooLarge,
            Self::Unavailable => ErrorCode::Unavailable,
            Self::Database(_)
            | Self::Storage(_)
            | Self::Password(_)
            | Self::Io(_)
            | Self::Config(_) => ErrorCode::Internal,
        }
    }

    /// HTTP status this error maps to.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.code().status()
    }

    /// Client-facing message.
    ///
    /// Only [`Error::BadRequest`] carries its own text, because only it describes
    /// something the caller did. Everything else collapses to the generic message
    /// for its code: a database or storage error can carry table names, paths and
    /// credentials, and an anonymous file service hands this body to
    /// unauthenticated strangers.
    #[must_use]
    pub fn public_message(&self) -> String {
        match self {
            // These carry text the caller needs and that is safe to echo: it
            // describes what they sent, not what the server holds.
            Self::BadRequest(message) | Self::Conflict(message) => message.clone(),
            Self::Domain(error) => error.to_string(),
            other => other.code().message().to_owned(),
        }
    }

    /// The wire representation of this error.
    #[must_use]
    pub fn body(&self) -> ErrorBody {
        ErrorBody {
            code: self.code(),
            message: self.public_message(),
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        // The full error, including the source chain, goes to the log — the
        // operator gets everything the client does not.
        if self.status().is_server_error() {
            tracing::error!(error = ?self, "request failed");
        } else {
            tracing::debug!(error = %self, "request rejected");
        }
        // A 429 without `Retry-After` tells a client to back off but not for how
        // long, so every client invents its own answer. The body carries nothing
        // the header does not — this is the field HTTP already has for it.
        let retry_after = match self {
            Self::TooManyRequests { retry_after_secs } => {
                HeaderValue::from_str(&retry_after_secs.to_string()).ok()
            }
            _ => None,
        };

        let mut response = (self.status(), Json(self.body())).into_response();
        if let Some(value) = retry_after {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
        response
    }
}

/// Repository failures collapse to [`Error::Database`].
///
/// A row that violates today's rules is an internal problem, not the caller's:
/// they asked for something perfectly reasonable and our storage could not
/// answer. It is logged with the offending row's id and reported as a 500.
macro_rules! repository_error {
    ($module:ident :: $error:ident) => {
        impl From<crate::db::repository::$module::$error> for Error {
            fn from(error: crate::db::repository::$module::$error) -> Self {
                use crate::db::repository::$module::$error as Source;
                match error {
                    Source::Database(inner) => Self::Database(inner),
                    Source::Corrupt(inner) => {
                        tracing::error!(%inner, "a stored row is not usable");
                        Self::Database(sea_orm::DbErr::Custom(inner.to_string()))
                    }
                }
            }
        }
    };
}

repository_error!(user::UserRepositoryError);
repository_error!(session::SessionRepositoryError);
repository_error!(api_token::ApiTokenRepositoryError);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_errors_never_leak_their_cause() {
        let err = Error::Io(std::io::Error::other("/srv/secret/path is unreadable"));
        let body = err.body();
        assert_eq!(body.code, ErrorCode::Internal);
        assert!(!body.message.contains("secret"));
    }

    #[test]
    fn client_errors_keep_their_message() {
        let err = Error::bad_request("`code` must be 6 characters");
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert_eq!(err.body().message, "`code` must be 6 characters");
    }
}
