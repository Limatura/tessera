//! Passwords, before they are hashed.

use std::fmt;

use serde::Deserialize;

use crate::DomainError;

/// How long a password must be to be accepted.
///
/// Length is the only rule. Composition requirements — an uppercase, a digit, a
/// symbol — are not here on purpose: NIST 800-63B dropped them because they push
/// people towards `Password1!` and away from the long passphrases that actually
/// resist guessing. Making the minimum longer is worth more than making the
/// alphabet wider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasswordPolicy {
    /// Shortest accepted password, in characters.
    pub min_length: usize,
    /// Longest accepted password, in characters.
    pub max_length: usize,
}

impl PasswordPolicy {
    /// Longest maximum an operator may configure.
    ///
    /// Argon2's cost is driven by its parameters rather than input length, but
    /// the input is still hashed and still allocated — an unbounded password
    /// field is an unauthenticated request that makes the server do arbitrary
    /// work. This is a low ceiling for a passphrase and a hard stop for abuse.
    pub const CEILING: usize = 1024;
    /// Shortest minimum an operator may configure.
    ///
    /// Below this the field stops being a password, and letting an instance
    /// choose "1" would make the type's guarantee meaningless.
    pub const FLOOR: usize = 8;

    /// Whether the policy is usable.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError`] if the bounds are inverted or outside
    /// [`FLOOR`](Self::FLOOR)..=[`CEILING`](Self::CEILING).
    pub const fn validate(&self) -> Result<(), DomainError> {
        if self.min_length < Self::FLOOR {
            return Err(DomainError::Malformed {
                field: "auth.password.min_length",
                reason: "must be at least 8",
            });
        }
        if self.max_length > Self::CEILING {
            return Err(DomainError::Malformed {
                field: "auth.password.max_length",
                reason: "must be at most 1024",
            });
        }
        if self.min_length > self.max_length {
            return Err(DomainError::Malformed {
                field: "auth.password.min_length",
                reason: "must not exceed the maximum length",
            });
        }
        Ok(())
    }
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        // 12 rather than the more common 8: this is a self-hosted service whose
        // login page may face the internet, and the operator is often the only
        // user, so the cost of a slightly longer minimum is small and paid once.
        Self {
            min_length: 12,
            max_length: 256,
        }
    }
}

/// A password that has passed the policy but has not been hashed yet.
///
/// # Why this is a type and not a `String`
///
/// A `String` password is one careless `{:?}` away from a log file. This type
/// cannot be printed, cannot be serialised, and its `Debug` shows nothing —
/// so the compiler stops the mistake rather than a reviewer having to.
///
/// It deliberately implements neither [`Display`](fmt::Display) nor
/// `serde::Serialize`. `Deserialize` *is* implemented, because the whole point
/// is to receive one from a request body.
///
/// Zeroing on drop is not attempted. `String` reallocates as it grows, so any
/// wipe would be best-effort against copies we no longer have a pointer to;
/// promising a guarantee that cannot be kept is worse than not promising it.
#[derive(Clone, Deserialize)]
#[serde(try_from = "String")]
pub struct Password(String);

impl Password {
    /// Checks a password against `policy`.
    ///
    /// The value is taken as-is: no trimming, no case folding, no normalisation.
    /// A password is a byte sequence the user chose, and "helpfully" trimming a
    /// trailing space would silently lock out anyone whose password ends in one.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError`] if the password is empty or outside the policy's
    /// length bounds.
    pub fn parse(raw: impl Into<String>, policy: PasswordPolicy) -> Result<Self, DomainError> {
        let raw = raw.into();

        if raw.is_empty() {
            return Err(DomainError::Empty { field: "password" });
        }

        let length = raw.chars().count();
        if !(policy.min_length..=policy.max_length).contains(&length) {
            return Err(DomainError::Length {
                field: "password",
                min: policy.min_length,
                max: policy.max_length,
                actual: length,
            });
        }

        Ok(Self(raw))
    }

    /// The plaintext, for handing to a hasher.
    ///
    /// The only way out of this type, named so that a call site reads as a
    /// deliberate act and shows up in a search for anywhere passwords are
    /// handled.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// Prints a fixed placeholder. A password must not reach a log because someone
/// derived `Debug` on a struct that happened to contain one.
impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(<redacted>)")
    }
}

/// Compares in constant time with respect to content, so that using `==` cannot
/// leak a prefix through timing. Comparing two plaintexts is not how login
/// works — that is the hasher's job — but the operator exists, so it should not
/// be the naive one.
impl PartialEq for Password {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (self.0.as_bytes(), other.0.as_bytes());
        if a.len() != b.len() {
            return false;
        }
        a.iter()
            .zip(b.iter())
            .fold(0_u8, |acc, (x, y)| acc | (x ^ y))
            == 0
    }
}

impl Eq for Password {}

impl TryFrom<String> for Password {
    type Error = DomainError;

    /// Uses the default policy. Endpoints that have the operator's configured
    /// policy to hand should call [`Password::parse`] with it instead; this
    /// exists so `Password` can appear directly in a deserialised request body.
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value, PasswordPolicy::default())
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rstest::rstest;

    use super::*;

    const POLICY: PasswordPolicy = PasswordPolicy {
        min_length: 12,
        max_length: 256,
    };

    #[test]
    fn accepts_a_passphrase() {
        let password = Password::parse("correct horse battery staple", POLICY).unwrap();
        assert_eq!(password.expose(), "correct horse battery staple");
    }

    #[rstest]
    #[case::empty("")]
    #[case::too_short("short")]
    #[case::one_below_minimum("elevenchars")]
    fn rejects_passwords_below_the_policy(#[case] raw: &str) {
        assert!(Password::parse(raw, POLICY).is_err());
    }

    #[test]
    fn rejects_passwords_above_the_policy() {
        let raw = "a".repeat(POLICY.max_length + 1);
        assert!(matches!(
            Password::parse(raw, POLICY),
            Err(DomainError::Length { .. })
        ));
    }

    /// Trimming would silently change a password whose owner meant the space.
    #[test]
    fn preserves_surrounding_whitespace() {
        let raw = "  a passphrase with space  ";
        assert_eq!(Password::parse(raw, POLICY).unwrap().expose(), raw);
    }

    /// Stands in for the real thing: a request struct that derives `Debug` and
    /// happens to contain a password. This is how passwords reach logs.
    #[derive(Debug)]
    struct LoginRequest {
        #[expect(dead_code, reason = "only its Debug output is under test")]
        password: Password,
    }

    /// The whole reason this is a newtype.
    #[test]
    fn debug_output_never_contains_the_password() {
        let password = Password::parse("hunter2 hunter2 hunter2", POLICY).unwrap();
        let rendered = format!("{password:?}");
        assert!(!rendered.contains("hunter2"), "leaked: {rendered}");

        let rendered = format!("{:?}", LoginRequest { password });
        assert!(!rendered.contains("hunter2"), "leaked: {rendered}");
    }

    #[rstest]
    #[case::below_floor(PasswordPolicy { min_length: 4, max_length: 64 })]
    #[case::above_ceiling(PasswordPolicy { min_length: 12, max_length: 100_000 })]
    #[case::inverted(PasswordPolicy { min_length: 64, max_length: 12 })]
    fn rejects_unusable_policies(#[case] policy: PasswordPolicy) {
        assert!(policy.validate().is_err());
    }

    #[test]
    fn the_default_policy_is_usable() {
        PasswordPolicy::default().validate().unwrap();
    }

    proptest! {
        /// Equality is by content and never panics, whatever the lengths.
        #[test]
        fn equality_is_by_content(a in ".{12,60}", b in ".{12,60}") {
            let (pa, pb) = (
                Password::parse(a.clone(), POLICY),
                Password::parse(b.clone(), POLICY),
            );
            if let (Ok(pa), Ok(pb)) = (pa, pb) {
                prop_assert_eq!(pa == pb, a == b);
            }
        }

        /// Anything the policy accepts comes back out unchanged.
        #[test]
        fn accepted_passwords_are_unmodified(raw in ".{12,200}") {
            if let Ok(password) = Password::parse(raw.clone(), POLICY) {
                prop_assert_eq!(password.expose(), raw);
            }
        }
    }
}
