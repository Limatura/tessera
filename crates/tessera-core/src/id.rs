//! Identifiers.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::DomainError;

/// Identifies a user account.
///
/// A ULID rather than an auto-incrementing integer or a UUIDv4:
///
/// - Sequential integers leak how many accounts exist and let anyone enumerate them, which matters
///   for a service that may face the internet.
/// - UUIDv4 fixes that but is random, so an index on it scatters writes across the B-tree — the
///   usual reason a table gets slower as it grows.
///
/// ULIDs are time-ordered *and* unguessable, so inserts stay at the hot end of
/// the index and the value still gives nothing away. They are also shorter than
/// a UUID in text form, which is the form that ends up in URLs and logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct UserId(Ulid);

impl UserId {
    /// Mints an identifier for a new account.
    ///
    /// Takes no clock argument, unlike the rules in this crate: a fresh
    /// identifier has no observable behaviour to test against a fixed time, and
    /// threading a timestamp through every construction would buy nothing.
    #[must_use]
    pub fn generate() -> Self {
        Self(Ulid::generate())
    }

    /// Reconstructs an identifier from its canonical text form.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError`] if the value is not a 26-character Crockford
    /// base32 ULID.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        Ulid::from_string(raw.trim())
            .map(Self)
            .map_err(|_| DomainError::Malformed {
                field: "id",
                reason: "is not a valid identifier",
            })
    }

    /// The underlying ULID, for callers that need its timestamp.
    #[must_use]
    pub const fn as_ulid(self) -> Ulid {
        self.0
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for UserId {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<String> for UserId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<UserId> for String {
    fn from(value: UserId) -> Self {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rstest::rstest;

    use super::*;

    #[test]
    fn round_trips_through_text() {
        let id = UserId::generate();
        assert_eq!(UserId::parse(&id.to_string()).unwrap(), id);
    }

    #[test]
    fn generated_identifiers_are_distinct() {
        let ids: std::collections::BTreeSet<_> = (0..1_000).map(|_| UserId::generate()).collect();
        assert_eq!(ids.len(), 1_000);
    }

    /// The property that makes ULIDs worth choosing over UUIDv4: text order
    /// matches creation order, so a database index stays append-mostly.
    #[test]
    fn identifiers_sort_in_creation_order() {
        let earlier = UserId::generate();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let later = UserId::generate();
        assert!(earlier < later);
        assert!(earlier.to_string() < later.to_string());
    }

    #[rstest]
    #[case::empty("")]
    #[case::too_short("01ARZ3NDEK")]
    #[case::not_base32("01ARZ3NDEKTSV4RRFFQ69G5FAU!")]
    #[case::plain_text("hello")]
    fn rejects_values_that_are_not_identifiers(#[case] raw: &str) {
        assert!(UserId::parse(raw).is_err(), "should have rejected `{raw}`");
    }

    proptest! {
        /// Arbitrary input is rejected, never panics, never yields an identifier
        /// that fails to round-trip.
        #[test]
        fn parsing_arbitrary_text_is_total(raw in ".{0,60}") {
            if let Ok(id) = UserId::parse(&raw) {
                prop_assert_eq!(UserId::parse(&id.to_string()).unwrap(), id);
            }
        }
    }
}
