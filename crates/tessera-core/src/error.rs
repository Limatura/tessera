//! Rejections produced by the domain rules.
//!
//! These describe what is wrong with a *value*, in the vocabulary of the domain.
//! They know nothing about HTTP: mapping them to a status code is the server's
//! job, at its own boundary. That separation is what lets the rules be tested
//! without a request in sight.

/// A value that the domain refuses to accept.
///
/// Every variant names the field and says what would have been acceptable,
/// because these messages are shown to whoever typed the value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DomainError {
    /// A required value was empty or only whitespace.
    #[error("`{field}` must not be empty")]
    Empty {
        /// Field the caller supplied.
        field: &'static str,
    },

    /// A value was outside its permitted length.
    #[error("`{field}` must be between {min} and {max} characters, but was {actual}")]
    Length {
        /// Field the caller supplied.
        field: &'static str,
        /// Shortest accepted length, in characters.
        min: usize,
        /// Longest accepted length, in characters.
        max: usize,
        /// What the caller actually supplied.
        actual: usize,
    },

    /// A value contained something the format does not allow.
    #[error("`{field}` {reason}")]
    Malformed {
        /// Field the caller supplied.
        field: &'static str,
        /// What is wrong, phrased to be shown to the person who typed it.
        reason: &'static str,
    },

    /// A value is well-formed but reserved by the system.
    #[error("`{value}` is reserved and cannot be used")]
    Reserved {
        /// The offending value, safe to echo because the caller just typed it.
        value: String,
    },
}

impl DomainError {
    /// Field the rejection relates to, for callers that group errors by field.
    #[must_use]
    pub const fn field(&self) -> &'static str {
        match self {
            Self::Empty { field } | Self::Length { field, .. } | Self::Malformed { field, .. } => {
                field
            }
            Self::Reserved { .. } => "value",
        }
    }
}
