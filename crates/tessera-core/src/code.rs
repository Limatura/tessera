//! Retrieval codes.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::DomainError;

/// Crockford's base32 alphabet.
///
/// `I`, `L`, `O` and `U` are absent. The first three because people confuse
/// them with `1`, `1` and `0`, and this value gets read off a screen, said out
/// loud, and typed on a phone keyboard. `U` because leaving it out keeps a
/// generated code from spelling most of the words you would not want printed on
/// a share page.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A code that identifies a share to whoever holds it.
///
/// # Why it is not the primary key
///
/// A share's identity is its ULID; this is how a *person* refers to it. Keeping
/// them apart lets the code be regenerated, after an accidental paste into a
/// public channel say, without rewriting anything that points at the share.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RetrievalCode(String);

impl RetrievalCode {
    /// Longest accepted code.
    pub const MAX_LENGTH: usize = 32;
    /// Shortest accepted code.
    ///
    /// Short codes are guessable, and a public endpoint answering "here is a
    /// file" to a guessed code is how these services leak. A backstop, not a
    /// recommendation: see `share.code_length` for the figure
    /// an instance uses, and note that a short code needs throttled lookups to
    /// mean anything.
    pub const MIN_LENGTH: usize = 4;

    /// Builds a code from random bytes, one character per byte.
    ///
    /// # Why bytes, and not an internal generator
    ///
    /// The mapping is the part worth testing: that a given byte always yields a
    /// given character, and that every byte maps somewhere. Taking the entropy
    /// as an argument makes that a plain assertion, not a statistical
    /// one, and leaves "where randomness comes from" to the caller that owns
    /// that question.
    ///
    /// Each byte is reduced modulo 32. That is unbiased here and only here:
    /// 256 is an exact multiple of 32, so every character has exactly eight
    /// preimages.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError`] if `entropy` is outside
    /// [`MIN_LENGTH`](Self::MIN_LENGTH)..=[`MAX_LENGTH`](Self::MAX_LENGTH).
    // Indexing is total here: the operand is reduced modulo the array's own
    // length, so it cannot leave the array. The alternatives are worse. A
    // `get().unwrap_or(..)` would substitute a wrong character if the invariant
    // broke, and a `filter_map` would silently emit a shorter code, which on
    // this type is a security bug, not a cosmetic one.
    #[expect(
        clippy::indexing_slicing,
        reason = "index is reduced modulo the array length; see comment"
    )]
    pub fn from_entropy(entropy: &[u8]) -> Result<Self, DomainError> {
        if !(Self::MIN_LENGTH..=Self::MAX_LENGTH).contains(&entropy.len()) {
            return Err(DomainError::Length {
                field: "code",
                min: Self::MIN_LENGTH,
                max: Self::MAX_LENGTH,
                actual: entropy.len(),
            });
        }

        let code = entropy
            .iter()
            .map(|byte| char::from(ALPHABET[usize::from(*byte) % ALPHABET.len()]))
            .collect();
        Ok(Self(code))
    }

    /// Validates and normalises a code someone typed.
    ///
    /// Normalisation is Crockford's, and that is why the alphabet was chosen:
    /// `O` becomes `0`, `I` and `L` become `1`, and case is folded, so a person
    /// reading `0ZPQ0` off a screen and typing `ozpqO` still arrives at their
    /// file. Hyphens and spaces are dropped too, because people group
    /// characters when they write a code down.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError`] if the value is empty, outside the length bounds,
    /// or contains a character that is not in the alphabet after normalisation.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let mut normalised = String::with_capacity(raw.len());
        for character in raw.chars() {
            let mapped = match character.to_ascii_uppercase() {
                ' ' | '-' | '_' => continue,
                'O' => '0',
                'I' | 'L' => '1',
                other => other,
            };
            // Non-ASCII cannot be in the alphabet, so failing the conversion and
            // failing the membership test are the same answer.
            let in_alphabet = u8::try_from(mapped).is_ok_and(|byte| ALPHABET.contains(&byte));
            if !in_alphabet {
                return Err(DomainError::Malformed {
                    field: "code",
                    reason: "may contain only digits and the letters A-Z, without I, L, O or U",
                });
            }
            normalised.push(mapped);
        }

        if normalised.is_empty() {
            return Err(DomainError::Empty { field: "code" });
        }
        let length = normalised.len();
        if !(Self::MIN_LENGTH..=Self::MAX_LENGTH).contains(&length) {
            return Err(DomainError::Length {
                field: "code",
                min: Self::MIN_LENGTH,
                max: Self::MAX_LENGTH,
                actual: length,
            });
        }

        Ok(Self(normalised))
    }

    /// The normalised code.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RetrievalCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for RetrievalCode {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<RetrievalCode> for String {
    fn from(value: RetrievalCode) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rstest::rstest;

    use super::*;

    #[test]
    fn every_byte_maps_into_the_alphabet() {
        let entropy: Vec<u8> = (0..=255).collect();
        for window in entropy.chunks(8) {
            let code = RetrievalCode::from_entropy(window).expect("8 bytes");
            assert!(
                code.as_str().bytes().all(|b| ALPHABET.contains(&b)),
                "{code} left the alphabet"
            );
        }
    }

    /// 256 is an exact multiple of 32, so the modulo introduces no bias. If the
    /// alphabet ever changes size, this is the test that will notice.
    #[test]
    fn the_mapping_is_unbiased() {
        let mut counts = [0_u32; 32];
        for byte in 0..=255_u8 {
            counts[usize::from(byte) % ALPHABET.len()] += 1;
        }
        assert!(
            counts.iter().all(|count| *count == 8),
            "every character must have the same number of preimages: {counts:?}"
        );
    }

    /// Byte 0 → `0`, 31 → `Z`, and 32 wraps back to `0`.
    #[test]
    fn the_mapping_is_deterministic() {
        let entropy = [0, 1, 31, 32, 33, 255, 128, 64];
        assert_eq!(
            RetrievalCode::from_entropy(&entropy).unwrap().as_str(),
            "01Z01Z00"
        );
    }

    /// The point of Crockford's alphabet: someone reading a code off a screen
    /// and typing what they see still arrives at the right share.
    #[rstest]
    #[case::lowercase("abcd1234", "ABCD1234")]
    #[case::letter_o_for_zero("OZPQ0123", "0ZPQ0123")]
    #[case::letter_i_for_one("IZPQ0123", "1ZPQ0123")]
    #[case::letter_l_for_one("lZPQ0123", "1ZPQ0123")]
    #[case::grouped_with_hyphens("0ZPQ-0123", "0ZPQ0123")]
    #[case::grouped_with_spaces("0ZPQ 0123", "0ZPQ0123")]
    fn normalises_what_people_actually_type(#[case] raw: &str, #[case] expected: &str) {
        assert_eq!(RetrievalCode::parse(raw).unwrap().as_str(), expected);
    }

    #[rstest]
    #[case::empty("")]
    #[case::separators_only("---")]
    #[case::too_short("abc")]
    #[case::excluded_letter_u("0ZPQU123")]
    #[case::punctuation("0ZPQ/123")]
    #[case::non_ascii("0ZPQ中123")]
    fn rejects_what_cannot_be_a_code(#[case] raw: &str) {
        assert!(
            RetrievalCode::parse(raw).is_err(),
            "should have rejected `{raw}`"
        );
    }

    #[test]
    fn rejects_entropy_outside_the_length_bounds() {
        assert!(RetrievalCode::from_entropy(&[0; 3]).is_err());
        assert!(RetrievalCode::from_entropy(&[0; 33]).is_err());
    }

    proptest! {
        /// Anything generated must parse back to itself, or a code we hand out
        /// is one our own lookup would reject.
        #[test]
        fn generated_codes_round_trip(entropy in prop::collection::vec(any::<u8>(), 4..=32)) {
            let code = RetrievalCode::from_entropy(&entropy).unwrap();
            prop_assert_eq!(RetrievalCode::parse(code.as_str()).unwrap(), code);
        }

        /// Normalising twice must not differ from normalising once.
        #[test]
        fn parsing_is_idempotent(entropy in prop::collection::vec(any::<u8>(), 4..=32)) {
            let once = RetrievalCode::from_entropy(&entropy).unwrap();
            let twice = RetrievalCode::parse(once.as_str()).unwrap();
            prop_assert_eq!(RetrievalCode::parse(twice.as_str()).unwrap(), once);
        }
    }
}
