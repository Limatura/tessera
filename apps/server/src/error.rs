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

    /// A refusal with its own code.
    ///
    /// Every failure a client should tell apart has a code, and this is how a
    /// handler raises one: [`Error::new`], then any detail the client needs to
    /// phrase it. No variant takes free text alone, so nothing can be refused
    /// without a code.
    #[error("{}", .code.message())]
    Api {
        /// What went wrong, for the client to branch on and translate.
        code: ErrorCode,
        /// English text for this occurrence, when the code's own is not
        /// specific enough. For logs and for a client with no translation.
        message: Option<String>,
        /// Figures and names a translated message needs: which field, what
        /// the limit is.
        details: Details,
    },

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

    /// The caller has been asked to slow down.
    #[error("too many requests")]
    TooManyRequests {
        /// Seconds until the caller may try again.
        retry_after_secs: u64,
    },

    /// The content is waiting for the malware scanner's verdict.
    ///
    /// Distinct from [`Error::Unavailable`]: it concerns this share, not the
    /// service, and resolves itself. The client waits and retries.
    #[error("still being scanned")]
    Scanning {
        /// Seconds until a retry is worthwhile.
        retry_after_secs: u64,
    },

    /// A value failed a domain rule.
    #[error("{0}")]
    Domain(#[from] tessera_core::DomainError),

    /// The body was not the type this route accepts.
    ///
    /// A separate variant because the caller can fix it without guessing: the
    /// tus `PATCH` takes exactly one content type, and a client sending another
    /// has usually forgotten to set it.
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

/// Declares every [`ErrorCode`] with its status and English message, so the
/// three cannot drift: a code without either does not compile.
macro_rules! error_codes {
    ($( $(#[doc = $doc:literal])* $code:ident => $status:ident, $message:literal; )*) => {
        /// Stable, machine-readable identifier for an [`Error`].
        ///
        /// One per thing a client should tell apart, in `snake_case`. Clients
        /// branch on this and translate it; `message` is English and free to
        /// change, and `details` carries what a translation needs.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
        #[serde(rename_all = "snake_case")]
        pub enum ErrorCode {
            $( $(#[doc = $doc])* $code, )*
        }

        impl ErrorCode {
            /// Every code, for the tests that hold them to one format.
            pub const ALL: &[Self] = &[$(Self::$code),*];

            /// HTTP status this code maps to.
            #[must_use]
            pub const fn status(self) -> StatusCode {
                match self {
                    $(Self::$code => StatusCode::$status,)*
                }
            }

            /// What to say when nothing more specific was given, in English.
            #[must_use]
            pub const fn message(self) -> &'static str {
                match self {
                    $(Self::$code => $message,)*
                }
            }
        }
    };
}

error_codes! {
    // ── Any route ───────────────────────────────────────────────────────
    /// Nothing by that name, or a feature this instance has switched off.
    NotFound => NOT_FOUND, "the requested resource does not exist";
    /// No credential, or one that has expired or been signed out.
    NotSignedIn => UNAUTHORIZED, "sign in to do that";
    /// Signed in, and the account's role does not allow this.
    PermissionDenied => FORBIDDEN, "your account is not allowed to do that";
    /// An API token was used where only a browser session may act.
    SessionRequired => FORBIDDEN, "this needs a signed-in browser session, not an API token";
    /// The account must set a new password before anything else.
    PasswordChangeRequired => FORBIDDEN, "set a new password before doing anything else";
    /// Larger than this caller may send in one file. `details.limit_bytes`,
    /// when the refusal comes from a limit and not from where the file would go.
    PayloadTooLarge => PAYLOAD_TOO_LARGE, "the request body exceeds the configured limit";
    /// A storage ceiling would be passed. `details.tier`: `account`, `role` or `instance`.
    InsufficientStorage => INSUFFICIENT_STORAGE, "there is no room for more content";
    /// The `Range` asks for bytes that do not exist. `details.size`.
    RangeNotSatisfiable => RANGE_NOT_SATISFIABLE, "the requested range is not available";
    /// Too many attempts. `details.retry_after_secs`.
    TooManyRequests => TOO_MANY_REQUESTS, "too many attempts; wait a little and try again";
    /// Still being scanned for malware. `details.retry_after_secs`.
    Scanning => LOCKED, "this is still being checked for malware; try again in a moment";
    /// The body is not the content type this route takes.
    UnsupportedMediaType => UNSUPPORTED_MEDIA_TYPE, "that content type is not accepted here";
    /// Nothing arrived for too long.
    Timeout => REQUEST_TIMEOUT, "nothing arrived for a while, so the request was dropped; try again";
    /// Starting, stopping, or something this depends on is down.
    Unavailable => SERVICE_UNAVAILABLE, "the service is not accepting requests right now";
    /// A failure whose cause is withheld from the caller.
    Internal => INTERNAL_SERVER_ERROR, "an internal error occurred";

    // ── The shape of a request ──────────────────────────────────────────
    /// The JSON body did not parse or had the wrong fields.
    InvalidBody => BAD_REQUEST, "the request body is not what this expects";
    /// The query string did not parse.
    InvalidQuery => BAD_REQUEST, "the query parameters are not what this expects";
    /// A path segment did not parse.
    InvalidPath => BAD_REQUEST, "the address is not what this expects";
    /// There was no body where one is needed.
    BodyEmpty => BAD_REQUEST, "the request body is empty";
    /// The body stopped before it was complete.
    BodyIncomplete => BAD_REQUEST, "the request body ended before it was complete";
    /// The body was longer or shorter than it said it would be.
    BodyLengthMismatch => BAD_REQUEST, "the request body is not the length it declared";

    // ── A value ─────────────────────────────────────────────────────────
    /// Required and empty. `details.field`.
    ValueEmpty => BAD_REQUEST, "a required value is empty";
    /// Too short or too long. `details.field`, `min`, `max`, `actual`.
    ValueLength => BAD_REQUEST, "a value is not an accepted length";
    /// Broke a format rule. `details.field`, `rule`.
    ValueInvalid => BAD_REQUEST, "a value is not in an accepted format";
    /// Reserved by the system. `details.value`.
    ValueReserved => BAD_REQUEST, "that value is reserved";
    /// A number outside its range. `details.field`, and `min` and `max` where they apply.
    ValueOutOfRange => BAD_REQUEST, "a number is outside the range this accepts";
    /// Two fields that cannot be sent together. `details.fields`.
    ConflictingFields => BAD_REQUEST, "those fields cannot be sent together";

    // ── Accounts ────────────────────────────────────────────────────────
    /// Signing in: no such account, wrong password, or an account that may not sign in.
    InvalidCredentials => UNAUTHORIZED, "wrong username or password";
    /// The current password, given to confirm a change, is wrong.
    CurrentPasswordIncorrect => UNAUTHORIZED, "the current password is wrong";
    /// An account with no password confirms changes by having just signed in,
    /// and this session signed in too long ago. The user signs in again with
    /// the provider, then repeats the request.
    ReauthenticationRequired => UNAUTHORIZED, "sign in again to confirm it is you";
    /// A code from the authenticator app, or a recovery code, is wrong, used,
    /// or from too long ago.
    MfaCodeIncorrect => UNAUTHORIZED, "that code is not right";
    /// The ticket from the password step is unknown or has run out. The user
    /// starts again from the password.
    MfaChallengeUnusable => UNAUTHORIZED, "this sign-in took too long; enter your password again";
    /// The account already has an authenticator, and setting up another would
    /// replace it. It is turned off first.
    MfaAlreadyEnabled => CONFLICT, "two-step sign-in is already on; turn it off before setting it up again";
    /// A passkey answer could not be used to sign in: unknown, from another
    /// site, signed wrongly, or answering a challenge that is gone. One code
    /// for all of them, so an answer cannot be refined one check at a time.
    PasskeyRejected => UNAUTHORIZED, "that passkey could not sign you in; try again";
    /// A passkey could not be added: the browser's answer did not check out,
    /// or the challenge it answered is gone.
    PasskeyInvalid => BAD_REQUEST, "that passkey could not be added; try again";
    /// That passkey is already registered, to this account or another.
    PasskeyAlreadyRegistered => CONFLICT, "that passkey is already registered";
    /// The password is in a known breach.
    PasswordBreached => BAD_REQUEST, "this password has appeared in a data breach; choose a different one";
    /// This instance takes no new accounts.
    RegistrationClosed => FORBIDDEN, "this instance is not accepting new accounts";
    /// This instance takes accounts by invite, and none was given.
    InviteRequired => BAD_REQUEST, "this instance takes new accounts by invite; give an invite code";
    /// The invite code is unknown, spent or expired.
    InviteUnusable => BAD_REQUEST, "that invite code is not usable";
    /// The registration link is unknown, spent or expired.
    RegistrationLinkUnusable => BAD_REQUEST, "this confirmation link is not usable; register again";
    /// The password reset link is unknown, spent or expired.
    ResetLinkUnusable => BAD_REQUEST, "this reset link is not usable; ask for a new one";
    /// The address confirmation link is unknown, spent, expired, or for an address the account has left.
    VerificationLinkUnusable => BAD_REQUEST, "this confirmation link is not usable; ask for a new one";
    /// A program signing in from another device polled before anyone answered.
    /// It waits `interval` seconds and asks again.
    DeviceAuthorizationPending => BAD_REQUEST, "waiting for the sign-in to be approved in a browser";
    /// The person at the browser refused the program's sign-in.
    DeviceAuthorizationDenied => FORBIDDEN, "the sign-in was denied";
    /// The device code is unknown, expired, or already redeemed. The program
    /// starts again.
    DeviceCodeUnusable => BAD_REQUEST, "this sign-in request is not usable; start again";
    /// Another account has this name.
    UsernameTaken => CONFLICT, "that username is taken";
    /// Another account has this address.
    EmailTaken => CONFLICT, "that address is taken";
    /// The name or the address is taken; which is not said.
    AccountExists => CONFLICT, "that username or address is already taken";
    /// The address is already confirmed.
    EmailAlreadyVerified => CONFLICT, "this address is already confirmed";
    /// This would leave no account able to administer the instance.
    LastAdministrator => CONFLICT, "that would leave nobody able to administer this instance";
    /// This is the account's only way to sign in.
    LastSignInMethod => CONFLICT, "that is the only way left into this account; set a password first";

    // ── Roles ───────────────────────────────────────────────────────────
    /// Not a permission this version has. `details.permission`.
    UnknownPermission => BAD_REQUEST, "that is not a permission this version has";
    /// Another role has this name.
    RoleNameTaken => CONFLICT, "a role already has that name";
    /// A role the instance was seeded with cannot be changed or removed.
    BuiltinRoleImmutable => CONFLICT, "the roles this instance was seeded with cannot be changed or removed";

    // ── Shares ──────────────────────────────────────────────────────────
    /// The share needs its password, and none was sent.
    SharePasswordRequired => UNAUTHORIZED, "this share needs its password";
    /// The share's password is wrong.
    SharePasswordIncorrect => UNAUTHORIZED, "that is not this share's password";
    /// Past its expiry.
    ShareExpired => GONE, "this share has expired";
    /// Retrieved as many times as it allowed.
    ShareExhausted => GONE, "this share has been retrieved as many times as it was allowed to be";
    /// The share ended and the sweep has removed its content, so no change to
    /// its expiry or use limit can revive it.
    ShareContentRemoved => GONE, "this share's content has been removed and cannot be restored";
    /// A file share, asked for as text.
    ShareIsFile => BAD_REQUEST, "this share is a file; use /download instead";
    /// An end-to-end encrypted share, asked for in a way that needs the plaintext.
    ShareEncrypted => BAD_REQUEST, "this share is end-to-end encrypted; fetch it from /download and open it with its key";
    /// The share's key comes from its password, so the password cannot change.
    SharePasswordIsKey => BAD_REQUEST, "this share's key comes from its password, so the password cannot be changed or removed";
    /// A text share, asked to take a file.
    ShareIsText => CONFLICT, "that share holds text; a text share holds one thing";
    /// Text too long to keep as text. `details.max_bytes`.
    TextTooLarge => BAD_REQUEST, "that is larger than this instance keeps as text; send it as a file instead";
    /// No unused retrieval code could be found.
    CodeSpaceExhausted => CONFLICT, "could not find an unused retrieval code; the operator should raise share.code_length";
    /// Too large for a zip archive.
    ArchiveTooLargeForZip => BAD_REQUEST, "this share is too large for a zip; ask for it as tar or 7z instead";
    /// This instance takes no end-to-end encrypted shares.
    EncryptionDisabled => BAD_REQUEST, "this instance does not accept end-to-end encrypted shares";
    /// The encryption parameters do not fit together.
    EncryptionInvalid => BAD_REQUEST, "the encryption parameters are not usable";

    // ── Uploads ─────────────────────────────────────────────────────────
    /// The chunk starts somewhere other than where the upload is. `details.offset`.
    UploadOffsetMismatch => CONFLICT, "this chunk does not start where the upload is";
    /// More bytes than the upload declared.
    UploadExceedsLength => BAD_REQUEST, "that is more than this upload declared";
    /// Another request moved the upload while this one wrote.
    UploadConflict => CONFLICT, "another request moved this upload while it was being written";
    /// A chunk that is not the last is too small. `details.min_bytes`.
    UploadChunkTooSmall => BAD_REQUEST, "a chunk that is not the last is too small";
    /// The parts sent do not add up to the upload. `details.expected`.
    UploadPartsMismatch => BAD_REQUEST, "the parts sent do not match this upload";
    /// Parts went through this server; finish with `PATCH`.
    UploadNotDirect => CONFLICT, "parts of this upload came through this server; finish it with PATCH";
    /// Parts cannot go straight to this upload's store.
    UploadDirectUnavailable => CONFLICT, "this upload cannot take parts directly; send them with PATCH";
    /// The upload was abandoned and cleaned up.
    UploadAbandoned => GONE, "that upload was abandoned";
    /// This instance's store cannot hold a resumable upload.
    ResumableUnsupported => BAD_REQUEST, "this instance's store cannot hold a resumable upload";
    /// This instance does not share files of this type, by its name or by its
    /// first bytes. `details.extension` is the entry it falls under.
    FileTypeBlocked => FORBIDDEN, "this instance does not share files of this type";
    /// This instance takes uploads only from signed-in accounts. Sent in place
    /// of `not_signed_in` so the client can say why, not just ask for a sign-in.
    AnonymousUploadDisabled => UNAUTHORIZED, "this instance only accepts uploads from signed-in accounts";

    // ── Settings and backends ───────────────────────────────────────────
    /// Not a setting this version has. `details.key`.
    SettingUnknown => BAD_REQUEST, "that is not a setting this version has";
    /// Holds a credential; set in the environment. `details.key`.
    SettingSecret => BAD_REQUEST, "that setting holds a credential and is set through the environment";
    /// Fixed at start-up; set in the environment. `details.key`.
    SettingFixedAtBoot => BAD_REQUEST, "that setting cannot change while the server runs; set it through the environment";
    /// The value will not load. `details.key` where one is to blame.
    SettingInvalid => BAD_REQUEST, "that value is not accepted";
    /// The change destroys something; send it again with `confirm: true`. `details.removes`.
    ConfirmationRequired => CONFLICT, "this change removes data; send it again with `confirm: true`";
    /// An identity provider over plain HTTP on a public address.
    ProviderInsecure => BAD_REQUEST, "an identity provider must be https, unless it is on a private address";
    /// The identity provider's discovery document could not be read.
    ProviderUnreachable => BAD_REQUEST, "the identity provider could not be reached";
    /// A storage backend that will not open.
    BackendUnreachable => BAD_REQUEST, "that storage backend will not open";
    /// A stored backend's credentials will not open with this key.
    BackendCredentialUnreadable => CONFLICT, "a stored credential will not open with this key";
    /// An edit moved a backend to another store without sending credentials.
    BackendCredentialsRequired => BAD_REQUEST, "the backend moved to another store; send its credentials again";
    /// A backend name that is already taken.
    BackendNameTaken => CONFLICT, "a storage backend with that name already exists";
    /// A backend that still holds files.
    BackendInUse => CONFLICT, "that backend still holds files; move them off first";
    /// A breach lookup prefix that is not five hex characters.
    InvalidHashPrefix => BAD_REQUEST, "give the first five characters of the password's SHA-1, in hex";
    /// Start-up configuration that will not work. Never reaches a client.
    InvalidConfiguration => BAD_REQUEST, "the configuration is not usable";
}

/// What an error adds for a client to say it well, by name.
pub type Details = serde_json::Map<String, serde_json::Value>;

/// The JSON body returned for every non-2xx response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorBody {
    /// Stable identifier the client can branch on.
    pub code: ErrorCode,
    /// English explanation, for logs and for a client with no translation of
    /// `code`. Never contains internal details.
    pub message: String,
    /// What a translated message needs: the field, the limit, how long to
    /// wait. Always an object, empty when the code says everything.
    #[schema(value_type = Object)]
    pub details: Details,
    /// The request's `x-request-id`, on a server error only. The log line
    /// for the failure carries the same id, so a person reporting it can say
    /// which one it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

impl Error {
    /// A refusal with this code and nothing more.
    #[must_use]
    pub fn new(code: ErrorCode) -> Self {
        Self::Api {
            code,
            message: None,
            details: Details::new(),
        }
    }

    /// The same, with English text for this occurrence in place of the
    /// code's own. Only on [`Error::Api`]; anything else is returned as it was.
    #[must_use]
    pub fn with_message(self, text: impl Into<String>) -> Self {
        match self {
            Self::Api { code, details, .. } => Self::Api {
                code,
                message: Some(text.into()),
                details,
            },
            other => other,
        }
    }

    /// The same, with one more detail for the client. Only on [`Error::Api`].
    #[must_use]
    pub fn with_detail(self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        match self {
            Self::Api {
                code,
                message,
                mut details,
            } => {
                details.insert(key.to_owned(), value.into());
                Self::Api {
                    code,
                    message,
                    details,
                }
            }
            other => other,
        }
    }

    /// A number outside its range, with the bounds that apply.
    #[must_use]
    pub fn out_of_range(field: &str, min: Option<u64>, max: Option<u64>) -> Self {
        let mut error = Self::new(ErrorCode::ValueOutOfRange).with_detail("field", field);
        if let Some(min) = min {
            error = error.with_detail("min", min);
        }
        if let Some(max) = max {
            error = error.with_detail("max", max);
        }
        error
    }

    /// A value that broke a format rule, named the way
    /// [`DomainError`](tessera_core::DomainError) names one.
    #[must_use]
    pub fn invalid(field: &str, rule: &str) -> Self {
        Self::new(ErrorCode::ValueInvalid)
            .with_detail("field", field)
            .with_detail("rule", rule)
    }

    /// Stable identifier for this error.
    ///
    /// This is the only place that classifies a variant. Status and message are
    /// both derived from the result, so adding a variant cannot leave three
    /// parallel `match`es two-thirds updated.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::Api { code, .. } => *code,
            Self::NotFound => ErrorCode::NotFound,
            Self::Domain(error) => match error {
                tessera_core::DomainError::Empty { .. } => ErrorCode::ValueEmpty,
                tessera_core::DomainError::Length { .. } => ErrorCode::ValueLength,
                tessera_core::DomainError::Reserved { .. } => ErrorCode::ValueReserved,
                _ => ErrorCode::ValueInvalid,
            },
            Self::Unauthorized => ErrorCode::NotSignedIn,
            Self::Forbidden => ErrorCode::PermissionDenied,
            Self::PasswordChangeRequired => ErrorCode::PasswordChangeRequired,
            Self::RangeNotSatisfiable { .. } => ErrorCode::RangeNotSatisfiable,
            Self::TooManyRequests { .. } => ErrorCode::TooManyRequests,
            Self::Scanning { .. } => ErrorCode::Scanning,
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

    /// Client-facing message, in English.
    ///
    /// Internal failures collapse to their code's generic message: a database
    /// or storage error can carry table names, paths and credentials, and an
    /// anonymous file service sends this body to unauthenticated clients.
    #[must_use]
    pub fn public_message(&self) -> String {
        match self {
            // Written by a handler for the caller, about what they sent.
            Self::Api {
                message: Some(message),
                ..
            } => message.clone(),
            Self::Domain(error) => error.to_string(),
            Self::InsufficientStorage(tier) => tier.message().to_owned(),
            other => other.code().message().to_owned(),
        }
    }

    /// What a client needs beyond the code to say this well.
    #[must_use]
    pub fn details(&self) -> Details {
        let mut details = Details::new();
        let mut put = |key: &str, value: serde_json::Value| {
            details.insert(key.to_owned(), value);
        };
        match self {
            Self::Api { details, .. } => return details.clone(),
            Self::Domain(error) => match error {
                tessera_core::DomainError::Empty { field } => put("field", (*field).into()),
                tessera_core::DomainError::Length {
                    field,
                    min,
                    max,
                    actual,
                } => {
                    put("field", (*field).into());
                    put("min", (*min).into());
                    put("max", (*max).into());
                    put("actual", (*actual).into());
                }
                tessera_core::DomainError::Malformed { field, rule, .. } => {
                    put("field", (*field).into());
                    put("rule", (*rule).into());
                }
                tessera_core::DomainError::Reserved { value } => put("value", value.clone().into()),
                _ => {}
            },
            Self::InsufficientStorage(tier) => put("tier", tier.name().into()),
            Self::TooManyRequests { retry_after_secs } | Self::Scanning { retry_after_secs } => {
                put("retry_after_secs", (*retry_after_secs).into());
            }
            Self::RangeNotSatisfiable { size } => put("size", (*size).into()),
            _ => {}
        }
        details
    }

    /// The wire representation of this error.
    #[must_use]
    pub fn body(&self) -> ErrorBody {
        ErrorBody {
            code: self.code(),
            message: self.public_message(),
            details: self.details(),
            request_id: None,
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
        // Some errors carry a header the client needs, each one HTTP already
        // defines for the purpose. A 429 or 423 without `Retry-After` says to
        // wait but not how long, so every client guesses; a 416 without
        // `Content-Range` does not say what range *would* have worked.
        let extra = match self {
            Self::TooManyRequests { retry_after_secs } | Self::Scanning { retry_after_secs } => {
                Some((
                    header::RETRY_AFTER,
                    HeaderValue::from_str(&retry_after_secs.to_string()),
                ))
            }
            Self::RangeNotSatisfiable { size } => Some((
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes */{size}")),
            )),
            _ => None,
        };

        let body = self.body();
        let mut response = (self.status(), Json(body.clone())).into_response();
        if let Some((name, Ok(value))) = extra {
            response.headers_mut().insert(name, value);
        }
        // Picked up by `api::name_the_request`, which knows the request's id;
        // this does not.
        if self.status().is_server_error() {
            response.extensions_mut().insert(body);
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
    /// `storage.max_total_bytes`, or every backend full.
    Instance,
    /// No single backend has room for a file this size, although together they
    /// do. A smaller file fits, and so does this one with
    /// `storage.split_across_backends` on.
    Backend,
}

impl Tier {
    /// How `details.tier` names it.
    const fn name(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::Role => "role",
            Self::Instance => "instance",
            Self::Backend => "backend",
        }
    }

    /// What to tell the caller.
    const fn message(self) -> &'static str {
        match self {
            Self::Account => "your storage quota is full",
            Self::Role => "the storage quota for your role is full",
            Self::Instance => "this instance has no room for more content",
            Self::Backend => "no single storage backend has room for a file this size",
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
repository_error!(device_authorization::DeviceAuthorizationRepositoryError);

/// Passkey-table failures are ours: a database error or a row that does not
/// read back. Logged, and a 500.
impl From<crate::db::repository::passkey::PasskeyRepositoryError> for Error {
    fn from(error: crate::db::repository::passkey::PasskeyRepositoryError) -> Self {
        use crate::db::repository::passkey::PasskeyRepositoryError as Source;
        match error {
            Source::Database(inner) => Self::Database(inner),
            Source::Corrupt(_) => {
                tracing::error!(error = %error, "a passkey row is not usable");
                Self::Database(sea_orm::DbErr::Custom(error.to_string()))
            }
        }
    }
}

/// Second-step failures are all ours: a database error, a secret the key will
/// not open, or a row that does not read back. Logged, and a 500.
impl From<crate::db::repository::mfa::MfaRepositoryError> for Error {
    fn from(error: crate::db::repository::mfa::MfaRepositoryError) -> Self {
        use crate::db::repository::mfa::MfaRepositoryError as Source;
        match error {
            Source::Database(inner) => Self::Database(inner),
            other => {
                tracing::error!(error = %other, "a second-step row is not usable");
                Self::Database(sea_orm::DbErr::Custom(other.to_string()))
            }
        }
    }
}

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
    fn a_refusal_carries_its_code_message_and_details() {
        let err = Error::new(ErrorCode::ValueLength)
            .with_message("`code` must be 6 characters")
            .with_detail("field", "code");
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        let body = err.body();
        assert_eq!(body.code, ErrorCode::ValueLength);
        assert_eq!(body.message, "`code` must be 6 characters");
        assert_eq!(body.details["field"], "code");
    }

    /// The format every code follows, checked so a new one cannot break it:
    /// `snake_case` on the wire, and an English message starting lower-case
    /// with no full stop.
    #[test]
    fn every_code_has_one_format() {
        let mut seen = std::collections::HashSet::new();
        for code in ErrorCode::ALL {
            let wire = serde_json::to_value(code).expect("serialises");
            let wire = wire.as_str().expect("a string");
            assert!(
                !wire.is_empty()
                    && wire
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                    && !wire.starts_with('_')
                    && !wire.ends_with('_')
                    && !wire.contains("__"),
                "`{wire}` is not snake_case"
            );
            assert!(seen.insert(wire.to_owned()), "`{wire}` is declared twice");

            let message = code.message();
            assert!(
                message.starts_with(|c: char| c.is_lowercase() || c == '`'),
                "`{wire}`: the message should start lower-case: {message}"
            );
            assert!(
                !message.ends_with('.'),
                "`{wire}`: the message should not end with a full stop: {message}"
            );
            assert!(
                code.status().is_client_error() || code.status().is_server_error(),
                "`{wire}` maps to {}, which is not an error",
                code.status()
            );
        }
    }

    /// Every rule name in this crate or the domain crate, whether a domain
    /// error's `rule` field or the second argument to [`Error::invalid`], is
    /// something clients translate, so it follows the code format too. Read
    /// from the source so a new rule cannot skip this check.
    #[test]
    fn every_rule_name_is_snake_case() {
        fn walk(dir: &std::path::Path, found: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).expect("a source directory") {
                let path = entry.expect("an entry").path();
                if path.is_dir() {
                    walk(&path, found);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    let text = std::fs::read_to_string(&path).expect("a source file");
                    let file = path.display().to_string();
                    for (at, _) in text.match_indices("rule: \"") {
                        let name = text
                            .get(at + 7..)
                            .unwrap_or_default()
                            .split('"')
                            .next()
                            .unwrap_or_default();
                        found.push((file.clone(), name.to_owned()));
                    }
                    // `Error::invalid("field", "rule")`: the last literal
                    // before the closing parenthesis. This file is skipped
                    // because it names the pattern in order to search for it.
                    if path.ends_with("src/error.rs") {
                        continue;
                    }
                    for (at, _) in text.match_indices("Error::invalid(") {
                        let call = text
                            .get(at..)
                            .unwrap_or_default()
                            .split(')')
                            .next()
                            .unwrap_or_default();
                        if call.trim_end().ends_with('"') {
                            let name = call.rsplit('"').nth(1).unwrap_or_default();
                            found.push((file.clone(), name.to_owned()));
                        }
                    }
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut found = Vec::new();
        walk(&root.join("src"), &mut found);
        walk(&root.join("../../crates/tessera-core/src"), &mut found);
        assert!(
            found.len() > 20,
            "found only {} rules; is the walk broken?",
            found.len()
        );
        for (file, name) in found {
            assert!(
                !name.is_empty()
                    && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                    && !name.starts_with('_')
                    && !name.ends_with('_'),
                "{file}: rule `{name}` is not snake_case"
            );
        }
    }

    /// The rule names a `value_invalid` carries come from the domain crate,
    /// and a client translates them, so they are held to the same shape.
    #[test]
    fn domain_errors_map_to_value_codes_with_their_rule() {
        let err = Error::from(tessera_core::DomainError::Malformed {
            field: "password",
            rule: "needs_digit",
            reason: "must contain a digit",
        });
        let body = err.body();
        assert_eq!(body.code, ErrorCode::ValueInvalid);
        assert_eq!(body.details["field"], "password");
        assert_eq!(body.details["rule"], "needs_digit");

        let err = Error::from(tessera_core::DomainError::Length {
            field: "password",
            min: 12,
            max: 128,
            actual: 3,
        });
        assert_eq!(err.body().code, ErrorCode::ValueLength);
        assert_eq!(err.body().details["min"], 12);
    }
}
