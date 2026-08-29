//! Email addresses.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::DomainError;

/// A syntactically plausible email address.
///
/// # What this does not do
///
/// It does not implement RFC 5322. That grammar permits quoted strings,
/// comments and nested folding whitespace, and essentially every regex claiming
/// to match it is wrong in some direction, usually by rejecting addresses that
/// really work.
///
/// It also cannot tell you the address exists. **Nothing can, except sending
/// mail to it.** Verification is a message with a link, not a parser, so the
/// job here is to catch the obvious mistakes (a missing `@`, a typed space, an
/// empty half) and let the rest be proven by delivery.
///
/// Erring towards acceptance is deliberate: a rejected address that works is a
/// user who cannot sign up and has no way to argue.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EmailAddress {
    /// Normalised form, used for comparison and storage.
    normalised: String,
    /// Byte index of the `@` in `normalised`, so the halves need no re-scan.
    at: usize,
}

/// The domain a deleted account's address is rebuilt at.
///
/// Declared here and used by `tombstone` so the two cannot drift apart.
pub(crate) const TOMBSTONE_DOMAIN: &str = "deleted.invalid";

impl EmailAddress {
    /// Longest accepted address, in characters.
    ///
    /// RFC 5321 caps a path at 256 octets including the angle brackets, which
    /// leaves 254 for the address itself.
    pub const MAX_LENGTH: usize = 254;
    /// Longest accepted local part, in characters (RFC 5321 § 4.5.3.1.1).
    pub const MAX_LOCAL_LENGTH: usize = 64;

    /// Validates and normalises an address.
    ///
    /// The domain is lowercased because DNS is case-insensitive. The local part
    /// is **not**: RFC 5321 says it belongs to the receiving server, and while
    /// nearly every provider treats it case-insensitively, that is their choice
    /// to make and not ours to assume.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError`] if the value is empty, too long, has no `@` or
    /// more than one, has an empty half, contains whitespace or control
    /// characters, or has a domain without a dot.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        Self::parse_inner(raw, false)
    }

    /// Reads an address the program itself is producing.
    ///
    /// The one caller is `tombstone`, which has to build an address at the very
    /// domain [`parse`](Self::parse) refuses, and that refusal is what stops
    /// anybody else taking it. Reading a stored row back goes through here too,
    /// since the row is already there.
    ///
    /// # Errors
    ///
    /// The same as [`parse`](Self::parse), less the reserved domain.
    pub fn parse_privileged(raw: &str) -> Result<Self, DomainError> {
        Self::parse_inner(raw, true)
    }

    fn parse_inner(raw: &str, allow_reserved: bool) -> Result<Self, DomainError> {
        let trimmed = raw.trim();

        if trimmed.is_empty() {
            return Err(DomainError::Empty { field: "email" });
        }

        let length = trimmed.chars().count();
        if length > Self::MAX_LENGTH {
            return Err(DomainError::Length {
                field: "email",
                min: 3,
                max: Self::MAX_LENGTH,
                actual: length,
            });
        }

        if trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(DomainError::Malformed {
                field: "email",
                reason: "must not contain spaces or control characters",
            });
        }

        let Some((local, domain)) = trimmed.split_once('@') else {
            return Err(DomainError::Malformed {
                field: "email",
                reason: "must contain an `@`",
            });
        };

        // The domain a deleted account's address is rebuilt at. Refused for the
        // same reason `Username` refuses the `gone-` prefix: what precedes it
        // is a public identifier, so without this the address a deletion will
        // need could be registered first and the unique index would refuse
        // that deletion.
        //
        // `.invalid` is reserved by RFC 2606 and can never be a real domain, so
        // nothing legitimate is being taken away.
        if !allow_reserved && domain.eq_ignore_ascii_case(TOMBSTONE_DOMAIN) {
            return Err(DomainError::Reserved {
                value: trimmed.to_owned(),
            });
        }

        if domain.contains('@') {
            return Err(DomainError::Malformed {
                field: "email",
                reason: "must contain exactly one `@`",
            });
        }

        if local.is_empty() || domain.is_empty() {
            return Err(DomainError::Malformed {
                field: "email",
                reason: "must have text on both sides of the `@`",
            });
        }

        if local.chars().count() > Self::MAX_LOCAL_LENGTH {
            return Err(DomainError::Malformed {
                field: "email",
                reason: "has a local part that is too long",
            });
        }

        // A domain with no dot is either a local hostname or a typo. Rejecting it
        // costs intranet-only addresses, which a file-courier signup does not need
        // to serve, and catches the far more common `alice@gmailcom`.
        if !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.') {
            return Err(DomainError::Malformed {
                field: "email",
                reason: "must have a domain such as `example.com`",
            });
        }

        if domain.contains("..") {
            return Err(DomainError::Malformed {
                field: "email",
                reason: "has a domain with an empty label",
            });
        }

        let normalised = format!("{local}@{}", domain.to_lowercase());
        Ok(Self {
            at: local.len(),
            normalised,
        })
    }

    /// The normalised address.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.normalised
    }

    /// The part before the `@`, with its original case.
    #[must_use]
    pub fn local_part(&self) -> &str {
        self.normalised.get(..self.at).unwrap_or_default()
    }

    /// The part after the `@`, lowercased.
    #[must_use]
    pub fn domain(&self) -> &str {
        self.normalised
            .get(self.at.saturating_add(1)..)
            .unwrap_or_default()
    }
}

impl fmt::Display for EmailAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.normalised)
    }
}

impl FromStr for EmailAddress {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<String> for EmailAddress {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<EmailAddress> for String {
    fn from(value: EmailAddress) -> Self {
        value.normalised
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::simple("alice@example.com")]
    #[case::subdomain("alice@mail.example.co.uk")]
    #[case::plus_tag("alice+files@example.com")]
    #[case::dots("first.last@example.com")]
    #[case::digits("2026@example.com")]
    #[case::dashes("alice@my-host.example")]
    #[case::apostrophe("o'brien@example.com")]
    fn accepts_addresses_that_really_exist(#[case] raw: &str) {
        assert!(
            EmailAddress::parse(raw).is_ok(),
            "should have accepted `{raw}`"
        );
    }

    #[rstest]
    #[case::empty("")]
    #[case::whitespace("   ")]
    #[case::no_at("alice.example.com")]
    #[case::two_ats("alice@bob@example.com")]
    #[case::no_local("@example.com")]
    #[case::no_domain("alice@")]
    #[case::space_inside("alice bob@example.com")]
    #[case::bare_domain("alice@localhost")]
    #[case::leading_dot_domain("alice@.example.com")]
    #[case::trailing_dot_domain("alice@example.com.")]
    #[case::empty_label("alice@example..com")]
    fn rejects_obvious_mistakes(#[case] raw: &str) {
        assert!(
            EmailAddress::parse(raw).is_err(),
            "should have rejected `{raw}`"
        );
    }

    #[test]
    fn lowercases_the_domain_but_not_the_local_part() {
        let email = EmailAddress::parse("Alice.Smith@Example.COM").unwrap();
        assert_eq!(email.as_str(), "Alice.Smith@example.com");
        assert_eq!(email.local_part(), "Alice.Smith");
        assert_eq!(email.domain(), "example.com");
    }

    #[test]
    fn the_same_mailbox_written_two_ways_compares_equal() {
        // Only the domain half; the local part is the receiving server's business.
        assert_eq!(
            EmailAddress::parse("alice@EXAMPLE.com").unwrap(),
            EmailAddress::parse("alice@example.com").unwrap()
        );
    }

    proptest! {
        /// Whatever the input, an accepted address splits cleanly and its halves
        /// reassemble into exactly what `as_str` reports. Anything else is a
        /// rejection, never a panic and never a truncated value.
        #[test]
        fn accepted_addresses_split_and_reassemble(raw in ".{0,120}") {
            if let Ok(email) = EmailAddress::parse(&raw) {
                prop_assert!(!email.local_part().is_empty());
                prop_assert!(!email.domain().is_empty());
                prop_assert_eq!(
                    format!("{}@{}", email.local_part(), email.domain()),
                    email.as_str()
                );
                prop_assert_eq!(email.domain(), &email.domain().to_lowercase());
            }
        }

        #[test]
        fn parsing_is_idempotent(local in "[a-z][a-z0-9.]{0,20}", domain in "[a-z][a-z0-9]{0,10}") {
            let raw = format!("{local}@{domain}.example");
            if let Ok(first) = EmailAddress::parse(&raw) {
                let second = EmailAddress::parse(first.as_str()).expect("must re-parse");
                prop_assert_eq!(first, second);
            }
        }
    }
}
