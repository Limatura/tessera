//! The second sign-in step: time-based codes, and recovery codes for when the
//! authenticator is gone.
//!
//! # Why TOTP is computed here and not by a crate
//!
//! RFC 6238 is an HMAC and a truncation, a few lines on top of `aws-lc-rs`,
//! which the binary already uses for every other piece of cryptography. The
//! crates that package it bring their own HMAC and SHA-1 at versions the rest
//! of the tree does not use, so a second copy of each for the sake of twenty
//! lines. The RFC's own test vectors below hold this to the standard.
//!
//! # The parameters
//!
//! SHA-1, six digits, thirty seconds: what every authenticator app assumes
//! when a QR code says nothing else, and the only combination some of them
//! support. SHA-1's weaknesses are in collision resistance, which HMAC does
//! not rely on.

use aws_lc_rs::hmac;
use subtle::ConstantTimeEq as _;

use crate::auth::secret::fill_random;

/// Seconds each code is valid for.
pub const STEP_SECONDS: i64 = 30;

/// Digits in a code.
const DIGITS: usize = 6;

/// Ten to the power of [`DIGITS`]: what the truncated HMAC is reduced by.
const MODULUS: u32 = 1_000_000;

/// Bytes in a shared secret: 160 bits, the HMAC-SHA-1 block the RFC
/// recommends.
const SECRET_BYTES: usize = 20;

/// How many steps either side of now a code may come from.
///
/// One: a phone whose clock is half a minute out still works, and the window
/// stays at three codes out of a million.
const DRIFT_STEPS: i64 = 1;

/// How many recovery codes are issued at once.
pub const RECOVERY_CODE_COUNT: usize = 10;

/// Characters in a recovery code, not counting the separator.
const RECOVERY_CODE_LEN: usize = 10;

/// Crockford's base32: no `i`, `l`, `o` or `u`, so a code read off paper cannot
/// be misread as another. 32 symbols, so each is five uniformly random bits
/// and a code carries fifty.
const RECOVERY_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// The RFC 4648 alphabet authenticator apps expect a secret in.
const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// A shared TOTP secret, in the clear.
///
/// Its `Debug` prints nothing, like [`Secret`](crate::auth::secret::Secret):
/// anything holding one may derive `Debug`, and a log line must not be a way to
/// generate somebody's codes.
#[derive(Clone)]
pub struct TotpSecret(Vec<u8>);

impl core::fmt::Debug for TotpSecret {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TotpSecret(<redacted>)")
    }
}

impl TotpSecret {
    /// A new secret from the operating system's random source.
    #[must_use]
    pub fn generate() -> Self {
        let mut bytes = vec![0_u8; SECRET_BYTES];
        fill_random(&mut bytes);
        Self(bytes)
    }

    /// The secret as authenticator apps take it: unpadded base32.
    ///
    /// Also the form it is sealed and stored in, so what is stored is exactly
    /// what was shown.
    #[must_use]
    pub fn to_base32(&self) -> String {
        base32_encode(&self.0)
    }

    /// Reads a secret back from [`TotpSecret::to_base32`].
    ///
    /// `None` for anything that is not base32, which from the database means a
    /// corrupted row.
    #[must_use]
    pub fn from_base32(encoded: &str) -> Option<Self> {
        base32_decode(encoded).map(Self)
    }

    /// The `otpauth://` link a QR code carries.
    ///
    /// The issuer appears twice, as the label's prefix and as a parameter,
    /// because apps disagree on which they read.
    #[must_use]
    pub fn uri(&self, issuer: &str, account: &str) -> String {
        let issuer = percent_encode(issuer);
        format!(
            "otpauth://totp/{issuer}:{account}?secret={secret}&issuer={issuer}\
             &algorithm=SHA1&digits={DIGITS}&period={STEP_SECONDS}",
            account = percent_encode(account),
            secret = self.to_base32(),
        )
    }

    /// The code an app would show at `unix_time`, as six digits.
    ///
    /// For a client of this crate that has to act as the app: the tests, and
    /// any tool that scripts a sign-in.
    #[must_use]
    pub fn code(&self, unix_time: i64) -> Option<String> {
        self.code_at(unix_time.div_euclid(STEP_SECONDS))
            .map(|code| format!("{code:06}"))
    }

    /// The code for one time step.
    ///
    /// `None` only if the digest were shorter than SHA-1's twenty bytes, which
    /// it is not; a code that cannot be computed then matches nothing.
    fn code_at(&self, step: i64) -> Option<u32> {
        let key = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, &self.0);
        let tag = hmac::sign(&key, &step.to_be_bytes());
        let digest = tag.as_ref();
        // Dynamic truncation, RFC 4226 §5.3: the low nibble of the last byte
        // picks where four bytes are read from.
        let offset = usize::from(digest.last()? & 0x0f);
        let &[high, b, c, d] = digest.get(offset..offset + 4)? else {
            return None;
        };
        Some(u32::from_be_bytes([high & 0x7f, b, c, d]) % MODULUS)
    }

    /// The time step a code belongs to, if it is right for `unix_time`.
    ///
    /// Returned so the caller can refuse the same step twice. Every step in the
    /// window is compared, matching or not, so how long this takes says
    /// nothing about which one matched.
    #[must_use]
    pub fn verify(&self, code: &str, unix_time: i64) -> Option<i64> {
        let code = parse_code(code)?;
        let now = unix_time.div_euclid(STEP_SECONDS);
        let mut matched = None;
        for step in (now - DRIFT_STEPS)..=(now + DRIFT_STEPS) {
            let Some(expected) = self.code_at(step) else {
                continue;
            };
            let equal: bool = expected.ct_eq(&code).into();
            if equal && matched.is_none() {
                matched = Some(step);
            }
        }
        matched
    }
}

/// Reads a six-digit code, allowing the spaces some apps show in the middle.
fn parse_code(code: &str) -> Option<u32> {
    let digits: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    if digits.len() != DIGITS || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// Whether what was typed has the shape of a TOTP code and not a recovery code.
///
/// The two never overlap: a recovery code is ten characters.
#[must_use]
pub fn looks_like_totp(code: &str) -> bool {
    parse_code(code).is_some()
}

/// Issues a set of recovery codes, formatted for a person to copy:
/// `abcde-fghjk`.
#[must_use]
pub fn generate_recovery_codes() -> Vec<String> {
    (0..RECOVERY_CODE_COUNT)
        .map(|_| {
            let mut bytes = [0_u8; RECOVERY_CODE_LEN];
            fill_random(&mut bytes);
            let symbols: String = bytes
                .iter()
                .map(|byte| symbol(RECOVERY_ALPHABET, u32::from(*byte)))
                .collect();
            let (head, tail) = symbols.split_at(RECOVERY_CODE_LEN / 2);
            format!("{head}-{tail}")
        })
        .collect()
}

/// A recovery code as typed, reduced to the form that was hashed.
///
/// Case, separators and spaces are dropped, and the letters Crockford's
/// alphabet leaves out are read as the digits they look like, so a code copied
/// by hand still works.
#[must_use]
pub fn normalise_recovery_code(code: &str) -> String {
    code.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| match c.to_ascii_lowercase() {
            'o' => '0',
            'i' | 'l' => '1',
            other => other,
        })
        .collect()
}

/// The symbol for the low five bits of `value`.
fn symbol(alphabet: &[u8; 32], value: u32) -> char {
    // Five bits index a 32-entry table, so the lookup cannot miss.
    usize::try_from(value & 0x1f)
        .ok()
        .and_then(|index| alphabet.get(index))
        .map_or('0', |&byte| char::from(byte))
}

// The buffers below are masked to sixteen bits: no more than twelve are ever
// pending, and an unmasked one would overflow after a few bytes.

fn base32_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for &byte in bytes {
        buffer = ((buffer << 8) | u32::from(byte)) & 0xffff;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(symbol(BASE32_ALPHABET, buffer >> bits));
        }
    }
    if bits > 0 {
        out.push(symbol(BASE32_ALPHABET, buffer << (5 - bits)));
    }
    out
}

fn base32_decode(encoded: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(encoded.len() * 5 / 8);
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for symbol in encoded.bytes() {
        let value = BASE32_ALPHABET.iter().position(|&a| a == symbol)?;
        buffer = ((buffer << 5) | u32::try_from(value).ok()?) & 0xffff;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((buffer >> bits) & 0xff).ok()?);
        }
    }
    Some(out)
}

/// Percent-encodes everything but RFC 3986's unreserved characters, for a
/// label or a parameter in an `otpauth://` link.
fn percent_encode(value: &str) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            // Writing into a String cannot fail.
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    /// RFC 6238 Appendix B's SHA-1 key: the ASCII digits, twice.
    fn rfc_secret() -> TotpSecret {
        TotpSecret(b"12345678901234567890".to_vec())
    }

    /// The RFC's vectors are eight digits; six is the same number's last six.
    #[rstest]
    #[case(59, "287082")]
    #[case(1_111_111_109, "081804")]
    #[case(1_111_111_111, "050471")]
    #[case(1_234_567_890, "005924")]
    #[case(2_000_000_000, "279037")]
    #[case(20_000_000_000, "353130")]
    fn matches_the_rfc_test_vectors(#[case] time: i64, #[case] code: &str) {
        let secret = rfc_secret();
        assert_eq!(
            secret
                .code_at(time / STEP_SECONDS)
                .map(|c| format!("{c:06}"))
                .as_deref(),
            Some(code)
        );
        assert_eq!(secret.verify(code, time), Some(time / STEP_SECONDS));
    }

    #[test]
    fn a_code_from_the_next_or_last_step_is_accepted_and_one_further_is_not() {
        let secret = rfc_secret();
        let now = 1_234_567_890;
        let step = now / STEP_SECONDS;
        for (offset, accepted) in [(-2, false), (-1, true), (1, true), (2, false)] {
            let code = format!("{:06}", secret.code_at(step + offset).unwrap_or_default());
            assert_eq!(
                secret.verify(&code, now).is_some(),
                accepted,
                "a code {offset} steps away"
            );
        }
    }

    #[rstest]
    #[case("")]
    #[case("12345")]
    #[case("1234567")]
    #[case("12a456")]
    #[case("１２３４５６")]
    fn what_is_not_six_digits_is_never_right(#[case] code: &str) {
        assert_eq!(rfc_secret().verify(code, 59), None);
    }

    #[test]
    fn a_space_in_the_middle_is_allowed() {
        assert_eq!(rfc_secret().verify("287 082", 59), Some(1));
    }

    #[test]
    fn base32_round_trips_and_matches_the_rfc() {
        // RFC 4648 §10, unpadded.
        assert_eq!(base32_encode(b"foobar"), "MZXW6YTBOI");
        let secret = TotpSecret::generate();
        let encoded = secret.to_base32();
        assert_eq!(encoded.len(), 32);
        assert_eq!(
            TotpSecret::from_base32(&encoded).map(|s| s.0),
            Some(secret.0)
        );
        assert!(TotpSecret::from_base32("not base32!").is_none());
    }

    #[test]
    fn the_link_names_the_issuer_and_account_safely() {
        let uri = TotpSecret(b"foobar".to_vec()).uri("My Files", "alice");
        assert_eq!(
            uri,
            "otpauth://totp/My%20Files:alice?secret=MZXW6YTBOI&issuer=My%20Files\
             &algorithm=SHA1&digits=6&period=30"
        );
    }

    #[test]
    fn recovery_codes_are_distinct_and_survive_being_copied_by_hand() {
        let codes = generate_recovery_codes();
        assert_eq!(codes.len(), RECOVERY_CODE_COUNT);
        let unique: std::collections::HashSet<_> = codes.iter().collect();
        assert_eq!(unique.len(), RECOVERY_CODE_COUNT);
        for code in &codes {
            assert_eq!(code.len(), RECOVERY_CODE_LEN + 1);
            assert!(!looks_like_totp(code));
            let typed = code.to_uppercase().replace('-', " ");
            assert_eq!(
                normalise_recovery_code(&typed),
                normalise_recovery_code(code)
            );
        }
        assert_eq!(normalise_recovery_code("O1-IL"), "0111");
    }
}
