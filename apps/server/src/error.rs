//! The crate error type and its HTTP representation.
//!
//! One error type crosses every layer, and exactly one place decides what a given
//! failure looks like on the wire ([`Error::into_response`]). Handlers therefore
//! never construct status codes by hand, and the response body shape is part of
//! the OpenAPI contract, not an accident of whatever each handler returned.

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

    /// Accepting this would take some storage ceiling past its figure.
    ///
    /// Distinct from [`PayloadTooLarge`](Self::PayloadTooLarge) because the two
    /// call for different actions: that one says the file is too big and always
    /// will be, this one says something is *full* and a smaller file, or the
    /// same file later, may well fit.
    ///
    /// It carries which ceiling refused, because the answer decides who can do
    /// anything about it: "the disk is full" is the operator's problem, "your
    /// quota is full" is the caller's, and a bare 507 leaves them unable to tell
    /// whether deleting their own shares would help.
    #[error("insufficient storage")]
    InsufficientStorage(Tier),

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

    /// The thing existed and does not any more.
    ///
    /// Distinct from [`Error::NotFound`] because a client can act on it: "that
    /// code is wrong, check it" and "this expired" are different things to tell
    /// a person, and only one of them is worth retyping.
    #[error("gone")]
    Gone(String),

    /// The `Range` asked for bytes the object does not have.
    ///
    /// Carries the real size so the response can say what *is* available, which
    /// is what RFC 9110 asks for and what lets a client retry sensibly instead
    /// of guessing.
    #[error("range not satisfiable")]
    RangeNotSatisfiable {
        /// Size of the object, in bytes.
        size: u64,
    },

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

    /// The body was not the type this route accepts.
    ///
    /// Distinct from [`Error::BadRequest`] because it names something the
    /// caller can fix without guessing: the tus `PATCH` takes exactly one
    /// content type, and a client that sends another has usually forgotten to
    /// set it at all.
    #[error("unsupported media type")]
    UnsupportedMediaType,

    /// The caller stopped sending, or took longer than this route allows.
    ///
    /// For an upload it means no byte arrived within `server.upload_stall_secs`,
    /// which is a statement about the connection, not about how long the
    /// transfer was going to take. For everything else it is
    /// `server.request_timeout_secs`.
    #[error("timeout")]
    Timeout,

    /// The service is not ready to take traffic (starting up or shutting down).
    #[error("service unavailable")]
    Unavailable,

    /// A relational store operation failed. Covers migrations too, since SeaORM
    /// reports both through `DbErr`.
    #[error("database error")]
    Database(#[from] sea_orm::DbErr),

    /// A blob store operation failed.
    #[error("storage error")]
    Storage(#[from] object_store::Error),

    /// Hashing or verifying a password failed. Never means "wrong password",
    /// which is a `bool`, not an error.
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
    /// See [`Error::Gone`].
    Gone,
    /// See [`Error::RangeNotSatisfiable`].
    RangeNotSatisfiable,
    /// See [`Error::Conflict`].
    Conflict,
    /// See [`Error::TooManyRequests`].
    TooManyRequests,
    /// See [`Error::InsufficientStorage`].
    InsufficientStorage,
    /// See [`Error::UnsupportedMediaType`].
    UnsupportedMediaType,
    /// See [`Error::Timeout`].
    Timeout,
    /// See [`Error::Unavailable`].
    Unavailable,
    /// Any failure whose cause is withheld from the caller.
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
            Self::Gone => StatusCode::GONE,
            Self::RangeNotSatisfiable => StatusCode::RANGE_NOT_SATISFIABLE,
            Self::Conflict => StatusCode::CONFLICT,
            Self::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::InsufficientStorage => StatusCode::INSUFFICIENT_STORAGE,
            Self::UnsupportedMediaType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::Timeout => StatusCode::REQUEST_TIMEOUT,
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
            Self::Gone => "that is no longer available",
            Self::RangeNotSatisfiable => "the requested range is not available",
            Self::Conflict => "that already exists",
            Self::TooManyRequests => "too many attempts; wait a little and try again",
            Self::PayloadTooLarge => "the request body exceeds the configured limit",
            // The generic answer for the code. A real refusal says which
            // ceiling it was, through `Error::public_message`.
            Self::InsufficientStorage => "there is no room for more content",
            Self::UnsupportedMediaType => "that content type is not accepted here",
            Self::Timeout => "nothing arrived for a while, so the request was dropped; try again",
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
            Self::Gone(_) => ErrorCode::Gone,
            Self::RangeNotSatisfiable { .. } => ErrorCode::RangeNotSatisfiable,
            Self::Conflict(_) => ErrorCode::Conflict,
            Self::TooManyRequests { .. } => ErrorCode::TooManyRequests,
            Self::PayloadTooLarge => ErrorCode::PayloadTooLarge,
            Self::InsufficientStorage(_) => ErrorCode::InsufficientStorage,
            Self::UnsupportedMediaType => ErrorCode::UnsupportedMediaType,
            Self::Timeout => ErrorCode::Timeout,
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
            Self::BadRequest(message) | Self::Conflict(message) | Self::Gone(message) => {
                message.clone()
            }
            Self::Domain(error) => error.to_string(),
            Self::InsufficientStorage(tier) => tier.message().to_owned(),
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
        // The full error, including the source chain, goes to the log, so the
        // operator gets everything the client does not.
        if self.status().is_server_error() {
            tracing::error!(error = ?self, "request failed");
        } else {
            tracing::debug!(error = %self, "request rejected");
        }
        // Two errors carry a header the client needs, and both are headers HTTP
        // already has for the purpose. A 429 without `Retry-After` tells a
        // client to back off but not for how long, so every client invents its
        // own answer; a 416 without `Content-Range` does not say what range
        // *would* have worked.
        let extra = match self {
            Self::TooManyRequests { retry_after_secs } => Some((
                header::RETRY_AFTER,
                HeaderValue::from_str(&retry_after_secs.to_string()),
            )),
            Self::RangeNotSatisfiable { size } => Some((
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes */{size}")),
            )),
            _ => None,
        };

        let mut response = (self.status(), Json(self.body())).into_response();
        if let Some((name, Ok(value))) = extra {
            response.headers_mut().insert(name, value);
        }
        response
    }
}

/// Which storage ceiling a [`Error::InsufficientStorage`] hit.
///
/// No figures: how full the instance is is not the caller's business, and how
/// full a role is would let one member watch another's usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// The caller's own quota.
    Account,
    /// The quota on the role the caller is in, or the guest role for an
    /// anonymous caller.
    Role,
    /// `storage.max_total_bytes`.
    Instance,
}

impl Tier {
    /// What to tell the caller.
    const fn message(self) -> &'static str {
        match self {
            Self::Account => "your storage quota is full",
            Self::Role => "the storage quota for your role is full",
            Self::Instance => "this instance has no room for more content",
        }
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
repository_error!(share::ShareRepositoryError);
repository_error!(upload::UploadRepositoryError);

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
