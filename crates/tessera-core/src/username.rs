//! Account names.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::DomainError;

/// Names the service needs for itself, or that would mislead if a stranger held
/// them.
///
/// The prefix a deleted account's name is rebuilt from.
///
/// Reserved as a prefix rather than as a name, because what follows it is an
/// identifier — and identifiers are public. Every response that names an
/// account carries one, so without this a person could read somebody else's,
/// rename themselves to the tombstone that account will one day need, and the
/// unique index would then refuse the deletion. The account becomes
/// undeletable, by its owner and by an administrator alike.
///
/// Declared here and used by `tombstone` so the two cannot drift apart; a test
/// there checks that what it produces is refused by this.
pub(crate) const TOMBSTONE_PREFIX: &str = "gone-";

/// Compared after normalisation, so `Admin` and `ADMIN` are caught too.
const RESERVED: [&str; 12] = [
    "admin",
    "administrator",
    "root",
    "system",
    "tessera",
    "api",
    "auth",
    "login",
    "logout",
    "register",
    "support",
    "security",
];

/// A validated account name.
///
/// Constructing one is the only way to have it, so any function taking a
/// `Username` knows the value has been through [`Username::parse`] — the check
/// happens once, at the edge, rather than defensively at every call site.
///
/// Stored lowercased. Two people cannot register `Alice` and `alice`: to a human
/// those are the same name, and an account system where they are not is a
/// phishing tool.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Username(String);

impl Username {
    /// Longest accepted username, in characters.
    ///
    /// Bounded so that a username can never be the reason a row, a log line or a
    /// rendered page grows without limit.
    pub const MAX_LENGTH: usize = 32;
    /// Shortest accepted username, in characters.
    pub const MIN_LENGTH: usize = 3;

    /// Validates and normalises a username.
    ///
    /// Accepts ASCII letters, digits, `_` and `-`, and must start with a letter
    /// or digit. The alphabet is deliberately narrow: usernames appear in URLs,
    /// in logs and next to other people's names, and Unicode look-alikes are how
    /// one account impersonates another.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError`] if the value is empty, outside
    /// [`MIN_LENGTH`](Self::MIN_LENGTH)..=[`MAX_LENGTH`](Self::MAX_LENGTH), contains a character
    /// outside the alphabet, starts with punctuation, or is one of the reserved
    /// names.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        Self::parse_inner(raw, false)
    }

    /// [`Username::parse`], but the reserved list does not apply.
    ///
    /// For the administrator an instance creates for itself at first run, and
    /// nothing else. The reserved list exists to stop a *stranger* claiming a
    /// name that would mislead — `admin`, `support`, `security`. The operator's
    /// own administrator account is the one case where holding such a name is
    /// accurate rather than misleading, and refusing it would mean the shipped
    /// default configuration cannot start.
    ///
    /// Every other rule still applies, so this cannot produce a name that is
    /// unrepresentable, unroutable or a Unicode look-alike.
    ///
    /// # Errors
    ///
    /// As [`Username::parse`], minus [`DomainError::Reserved`].
    pub fn parse_privileged(raw: &str) -> Result<Self, DomainError> {
        Self::parse_inner(raw, true)
    }

    fn parse_inner(raw: &str, allow_reserved: bool) -> Result<Self, DomainError> {
        let normalised = raw.trim().to_ascii_lowercase();

        if normalised.is_empty() {
            return Err(DomainError::Empty { field: "username" });
        }

        let length = normalised.chars().count();
        if !(Self::MIN_LENGTH..=Self::MAX_LENGTH).contains(&length) {
            return Err(DomainError::Length {
                field: "username",
                min: Self::MIN_LENGTH,
                max: Self::MAX_LENGTH,
                actual: length,
            });
        }

        if !normalised
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(DomainError::Malformed {
                field: "username",
                reason: "may contain only letters, digits, underscores and hyphens",
            });
        }

        if !normalised.starts_with(|c: char| c.is_ascii_alphanumeric()) {
            return Err(DomainError::Malformed {
                field: "username",
                reason: "must start with a letter or a digit",
            });
        }

        if !allow_reserved
            && (RESERVED.contains(&normalised.as_str()) || normalised.starts_with(TOMBSTONE_PREFIX))
        {
            return Err(DomainError::Reserved { value: normalised });
        }

        Ok(Self(normalised))
    }

    /// The normalised name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Username {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<String> for Username {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<Username> for String {
    fn from(value: Username) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rstest::rstest;

    use super::*;

    /// The reserved list stops a stranger claiming a misleading name. The
    /// instance's own administrator is the one caller for whom `admin` is
    /// accurate, and the shipped default configuration depends on it.
    #[test]
    fn privileged_parsing_admits_reserved_names_and_nothing_else() {
        for name in RESERVED {
            assert!(Username::parse(name).is_err(), "{name} should be reserved");
            assert_eq!(
                Username::parse_privileged(name).map(|n| n.as_str().to_owned()),
                Ok(name.to_owned()),
                "{name} should be available to the instance itself"
            );
        }

        // Every other rule still applies.
        for bad in ["", "ab", "_leading", "has space", "\u{430}lice"] {
            assert!(
                Username::parse_privileged(bad).is_err(),
                "`{bad}` should still be refused"
            );
        }
    }

    #[rstest]
    #[case::simple("alice")]
    #[case::digits("user2026")]
    #[case::underscore("first_last")]
    #[case::hyphen("first-last")]
    #[case::starts_with_digit("2fast")]
    #[case::shortest("abc")]
    fn accepts_reasonable_names(#[case] raw: &str) {
        assert_eq!(Username::parse(raw).unwrap().as_str(), raw);
    }

    #[rstest]
    #[case::empty("")]
    #[case::whitespace_only("   ")]
    #[case::too_short("ab")]
    #[case::space_inside("first last")]
    #[case::at_sign("alice@example.com")]
    #[case::slash("alice/bob")]
    #[case::dot("alice.bob")]
    #[case::leading_underscore("_alice")]
    #[case::leading_hyphen("-alice")]
    #[case::reserved("admin")]
    #[case::reserved_cased("AdMiN")]
    fn rejects_bad_names(#[case] raw: &str) {
        assert!(
            Username::parse(raw).is_err(),
            "should have rejected `{raw}`"
        );
    }

    #[test]
    fn rejects_names_that_are_too_long() {
        let raw = "a".repeat(Username::MAX_LENGTH + 1);
        assert!(matches!(
            Username::parse(&raw),
            Err(DomainError::Length { .. })
        ));
    }

    #[test]
    fn normalises_case_and_surrounding_space() {
        assert_eq!(Username::parse("  Alice  ").unwrap().as_str(), "alice");
    }

    /// The reason usernames are lowercased: two spellings of the same name must
    /// not be two accounts.
    #[test]
    fn differently_cased_spellings_are_the_same_username() {
        assert_eq!(
            Username::parse("Alice").unwrap(),
            Username::parse("aLiCe").unwrap()
        );
    }

    /// Unicode look-alikes are the classic impersonation trick — `аlice` with a
    /// Cyrillic а renders identically to `alice` in most fonts.
    #[test]
    fn rejects_unicode_lookalikes() {
        assert!(Username::parse("\u{430}lice").is_err());
    }

    proptest! {
        /// Whatever the input, a name that survives is normalised, in range, and
        /// made only of the permitted alphabet. Anything else is a rejection,
        /// never a panic.
        #[test]
        fn accepted_names_are_always_well_formed(raw in ".{0,80}") {
            if let Ok(username) = Username::parse(&raw) {
                let value = username.as_str();
                prop_assert_eq!(value, value.to_ascii_lowercase());
                prop_assert!((Username::MIN_LENGTH..=Username::MAX_LENGTH).contains(&value.chars().count()));
                prop_assert!(
                    value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                );
                prop_assert!(value.starts_with(|c: char| c.is_ascii_alphanumeric()));
            }
        }

        /// Parsing is idempotent: feeding a valid name back in yields the same
        /// value. Without this, normalisation could drift on a round trip
        /// through the database.
        #[test]
        fn parsing_is_idempotent(raw in "[a-zA-Z0-9][a-zA-Z0-9_-]{2,31}") {
            if let Ok(first) = Username::parse(&raw) {
                let second = Username::parse(first.as_str()).expect("a valid name must re-parse");
                prop_assert_eq!(first, second);
            }
        }
    }
}
