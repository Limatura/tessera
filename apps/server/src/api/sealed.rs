//! Checking what an upload says about end-to-end encryption before trusting it.
//!
//! The server cannot tell ciphertext from anything else, so there is little to
//! check about the content. It can check that a request is coherent: that it
//! does not name a file in the clear on a share it calls encrypted, which
//! would put what encryption hides into a column, and that what it returns to
//! clients is usable.

use crate::{
    db::repository::share::Encryption,
    error::{Error, Result},
    state::AppState,
};

/// Longest sealed description accepted, in characters.
///
/// A 255-byte file name seals to 288 bytes and encodes to 384. This leaves room
/// for a media type and a long path too, and stops the column being used as
/// file storage.
const MAX_SEALED: usize = 4096;

/// How long a salt is once encoded: 16 bytes of unpadded base64.
const SALT_CHARS: usize = 22;

/// How a request asks for its share to be sealed, if it does.
///
/// # Errors
///
/// `400` when the instance does not accept encrypted shares, or the two
/// parameters do not describe a way a client could hold a key.
pub(crate) fn requested(
    state: &AppState,
    encryption: Option<&str>,
    key_salt: Option<String>,
) -> Result<Option<Encryption>> {
    let Some(encryption) = encryption else {
        if key_salt.is_some() {
            return Err(Error::bad_request(
                "`key_salt` is for `encryption=password`; send both or neither",
            ));
        }
        return Ok(None);
    };
    // A malformed-request error, not a 403: the caller is not refused for who
    // they are, and `/capabilities` already said so.
    if !state.config().features.encryption {
        return Err(Error::bad_request(
            "this instance does not accept end-to-end encrypted shares",
        ));
    }
    match (encryption, key_salt) {
        ("link", None) => Ok(Some(Encryption::Link)),
        ("link", Some(_)) => Err(Error::bad_request(
            "a key carried in the link has no salt; drop `key_salt`",
        )),
        ("password", Some(salt)) if is_base64url(&salt) && salt.len() == SALT_CHARS => {
            Ok(Some(Encryption::Password { salt }))
        }
        ("password", _) => Err(Error::bad_request(
            "`encryption=password` needs `key_salt`: 16 bytes as unpadded URL-safe base64",
        )),
        _ => Err(Error::bad_request(
            "`encryption` must be `link` or `password`",
        )),
    }
}

/// A sealed description, if it is one.
///
/// # Errors
///
/// `400` for anything but unpadded URL-safe base64 of a sensible length.
pub(crate) fn checked(value: Option<String>, field: &'static str) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_empty() || value.len() > MAX_SEALED || !is_base64url(&value) {
        return Err(Error::bad_request(format!(
            "`{field}` must be unpadded URL-safe base64, at most {MAX_SEALED} characters"
        )));
    }
    Ok(Some(value))
}

/// Refuses a request that describes its content the wrong way for its share.
///
/// On an encrypted share, a plain description is what encryption keeps off the
/// server, so it is refused instead of stored and never shown. On a plain
/// share, a sealed one could never be opened.
///
/// # Errors
///
/// `400` naming the first field that does not belong.
pub(crate) fn coherent(
    encrypted: bool,
    plain: &[(&'static str, bool)],
    sealed: &[(&'static str, bool)],
) -> Result<()> {
    let (wrong, why) = if encrypted {
        (plain, "an encrypted share takes its descriptions sealed")
    } else {
        (sealed, "only an encrypted share takes sealed descriptions")
    };
    match wrong.iter().find(|(_, given)| *given) {
        Some((field, _)) => Err(Error::bad_request(format!("`{field}`: {why}"))),
        None => Ok(()),
    }
}

fn is_base64url(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::padded("AAAA==")]
    #[case::standard_alphabet("ab+/")]
    #[case::empty("")]
    fn only_unpadded_url_safe_base64_is_sealed(#[case] value: &str) {
        assert!(checked(Some(value.to_owned()), "sealed_name").is_err());
    }

    #[test]
    fn a_sealed_description_passes_through() {
        assert_eq!(
            checked(Some("q-_9".to_owned()), "sealed_name").unwrap(),
            Some("q-_9".to_owned())
        );
    }

    #[test]
    fn a_plain_name_on_an_encrypted_share_is_named_in_the_refusal() {
        let refusal = coherent(true, &[("filename", true)], &[("sealed_name", false)])
            .unwrap_err()
            .to_string();
        assert!(refusal.contains("filename"), "got: {refusal}");
        assert!(coherent(false, &[("filename", true)], &[("sealed_name", false)]).is_ok());
        assert!(coherent(false, &[], &[("sealed_name", true)]).is_err());
    }
}
