//! Passwords, before they are hashed.

use std::fmt;

use serde::Deserialize;

use crate::DomainError;

/// What a password must be to be accepted.
///
/// Length, and optionally composition. The composition rules are off unless
/// an operator enables them. NIST 800-63B dropped them because they push users
/// toward `Password1!` and away from long passphrases that resist guessing, so
/// a longer minimum is worth more than a wider alphabet. They exist for
/// operators bound by a policy that still requires them.
///
/// # Character classes
///
/// Unicode, not ASCII, so a password in any script is judged the same way. A
/// client checking before the round trip should use the same properties:
///
/// | Rule | Rust | JavaScript |
/// | --- | --- | --- |
/// | [`require_mixed_case`](Self::require_mixed_case) | [`char::is_uppercase`] and [`char::is_lowercase`] | `/\p{Uppercase}/u` and `/\p{Lowercase}/u` |
/// | [`require_digit`](Self::require_digit) | [`char::is_numeric`] | `/\p{N}/u` |
/// | [`require_symbol`](Self::require_symbol) | not [`char::is_alphanumeric`], not [`char::is_whitespace`] | `/[^\p{Alphabetic}\p{N}\p{White_Space}]/u` |
///
/// A space is not a symbol. Counting it would let any passphrase satisfy the
/// rule, which is not what an operator enabling it wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasswordPolicy {
    /// Shortest accepted password, in characters.
    pub min_length: usize,
    /// Longest accepted password, in characters.
    pub max_length: usize,
    /// Whether a password needs both an uppercase and a lowercase letter.
    pub require_mixed_case: bool,
    /// Whether a password needs a digit.
    pub require_digit: bool,
    /// Whether a password needs a symbol.
    pub require_symbol: bool,
}

impl PasswordPolicy {
    /// Longest maximum an operator may configure.
    ///
    /// Argon2's cost is driven by its parameters, not input length, but
    /// the input is still hashed and allocated, so an unbounded password field
    /// is an unauthenticated request that makes the server do arbitrary work. A
    /// low ceiling for a passphrase and a hard stop for abuse.
    pub const CEILING: usize = 1024;
    /// Shortest minimum an operator may configure.
    ///
    /// Below this the field stops being a password, and letting an instance
    /// choose "1" would make the type's guarantee meaningless.
    pub const FLOOR: usize = 8;

    /// The widest policy any instance could ever have had.
    ///
    /// # Purpose
    ///
    /// A policy describes passwords a user may *choose*, not ones they may
    /// *present*. Otherwise raising `min_length` from 8 to 16 would lock out
    /// every account with a 12-character password: the input would be rejected
    /// before reaching the hash, and the user would see "wrong password" for
    /// the right one.
    ///
    /// So sign-in and change-password parse against this, and only a *new*
    /// password is checked against the configured policy. The bounds are still
    /// real: they make [`Password`] a meaningful type and cap the work an
    /// unauthenticated request can cause. The composition rules are all off,
    /// for the same reason as the minimum.
    #[must_use]
    pub const fn widest() -> Self {
        Self {
            min_length: Self::FLOOR,
            max_length: Self::CEILING,
            require_mixed_case: false,
            require_digit: false,
            require_symbol: false,
        }
    }

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
                rule: "below_minimum",
                reason: "must be at least 8",
            });
        }
        if self.max_length > Self::CEILING {
            return Err(DomainError::Malformed {
                field: "auth.password.max_length",
                rule: "above_maximum",
                reason: "must be at most 1024",
            });
        }
        if self.min_length > self.max_length {
            return Err(DomainError::Malformed {
                field: "auth.password.min_length",
                rule: "min_exceeds_max",
                reason: "must not exceed the maximum length",
            });
        }
        Ok(())
    }
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        // The floor. An operator who wants longer passwords raises it; a
        // higher default would only turn away users the operator never meant
        // to.
        Self {
            min_length: Self::FLOOR,
            max_length: 256,
            require_mixed_case: false,
            require_digit: false,
            require_symbol: false,
        }
    }
}

/// A password that has passed the policy but has not been hashed yet.
///
/// # Why this is a type and not a `String`
///
/// A `String` password is one careless `{:?}` away from a log file. This type
/// cannot be printed, cannot be serialised, and its `Debug` shows nothing, so
/// the compiler stops the mistake, not a reviewer.
///
/// It implements neither [`Display`](fmt::Display) nor `serde::Serialize`.
/// `Deserialize` *is* implemented, because receiving one from a request body is
/// the point.
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
    /// The value is taken as-is: no trimming, case folding or normalisation. A
    /// password is what the user chose, and trimming a trailing space would
    /// silently lock out anyone whose password ends in one.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError`] if the password is empty, outside the policy's
    /// length bounds, or missing a character class the policy requires.
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

        if policy.require_mixed_case
            && !(raw.chars().any(char::is_uppercase) && raw.chars().any(char::is_lowercase))
        {
            return Err(Self::lacks(
                "needs_mixed_case",
                "must contain an uppercase and a lowercase letter",
            ));
        }
        if policy.require_digit && !raw.chars().any(char::is_numeric) {
            return Err(Self::lacks("needs_digit", "must contain a digit"));
        }
        if policy.require_symbol
            && !raw
                .chars()
                .any(|c| !c.is_alphanumeric() && !c.is_whitespace())
        {
            return Err(Self::lacks("needs_symbol", "must contain a symbol"));
        }

        Ok(Self(raw))
    }

    const fn lacks(rule: &'static str, reason: &'static str) -> DomainError {
        DomainError::Malformed {
            field: "password",
            rule,
            reason,
        }
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

/// Compares in constant time with respect to content, so `==` cannot leak a
/// prefix through timing. Comparing two plaintexts is not how login works,
/// the hasher's job, but the operator exists and should not be naive.
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

    /// The widest policy must accept anything any configured policy could have
    /// accepted, or raising `min_length` silently locks existing accounts out.
    #[test]
    fn the_widest_policy_admits_every_valid_policy() {
        let widest = PasswordPolicy::widest();
        assert!(widest.validate().is_ok());

        for min in [PasswordPolicy::FLOOR, 12, 64] {
            for max in [64, 256, PasswordPolicy::CEILING] {
                if min > max {
                    continue;
                }
                let policy = PasswordPolicy {
                    min_length: min,
                    max_length: max,
                    require_mixed_case: true,
                    require_digit: true,
                    require_symbol: true,
                };
                assert!(policy.validate().is_ok(), "{policy:?} should be valid");
                assert!(
                    widest.min_length <= policy.min_length
                        && widest.max_length >= policy.max_length
                        && !widest.require_mixed_case
                        && !widest.require_digit
                        && !widest.require_symbol,
                    "{policy:?} is not covered by the widest policy"
                );
            }
        }
    }

    const POLICY: PasswordPolicy = PasswordPolicy {
        min_length: 12,
        max_length: 256,
        require_mixed_case: false,
        require_digit: false,
        require_symbol: false,
    };

    const COMPOSED: PasswordPolicy = PasswordPolicy {
        require_mixed_case: true,
        require_digit: true,
        require_symbol: true,
        ..POLICY
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

    #[rstest]
    #[case::no_uppercase("lower case 1 and !")]
    #[case::no_lowercase("UPPER CASE 1 AND !")]
    #[case::no_digit("Mixed Case and !")]
    #[case::no_symbol("Mixed Case and 1")]
    #[case::space_is_not_a_symbol("Mixed Case and 1 ")]
    fn rejects_passwords_missing_a_required_class(#[case] raw: &str) {
        assert!(matches!(
            Password::parse(raw, COMPOSED),
            Err(DomainError::Malformed { .. })
        ));
    }

    #[rstest]
    #[case::ascii("Mixed Case 1 and !")]
    #[case::other_scripts("Ωmega ωmega ٣ 。")]
    fn accepts_passwords_with_every_required_class(#[case] raw: &str) {
        assert!(Password::parse(raw, COMPOSED).is_ok());
    }

    /// Off by default: a passphrase of lowercase words is what the default
    /// should encourage, not refuse.
    #[test]
    fn the_default_policy_has_no_composition_rules() {
        assert!(Password::parse("correct horse battery staple", PasswordPolicy::default()).is_ok());
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
    #[case::below_floor(PasswordPolicy { min_length: 4, max_length: 64, ..POLICY })]
    #[case::above_ceiling(PasswordPolicy { min_length: 12, max_length: 100_000, ..POLICY })]
    #[case::inverted(PasswordPolicy { min_length: 64, max_length: 12, ..POLICY })]
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
