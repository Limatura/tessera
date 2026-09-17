//! What an encrypted share says about itself, sealed.
//!
//! The server stores `sealed_name` and `sealed_details` without looking inside,
//! so what goes inside is an agreement between clients. It is JSON, and
//! `docs/encryption.md` spells out the fields; the browser writes the same.

use std::path::{Component, Path, PathBuf};

use base64ct::{Base64UrlUnpadded, Encoding as _};
use serde::{Deserialize, Serialize};
use tessera_crypto::ShareKey;

use crate::{Error, Result};

/// A file's name and media type, as `sealed_name` holds them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedName {
    /// What the file was called, possibly with the folders it sat in, separated
    /// by `/`.
    pub name: String,
    /// Its media type, when the uploader knew one.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
}

/// A share's title and a note's language, as `sealed_details` holds them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedDetails {
    /// The display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// A note's syntax-highlighting hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// Seals a description into what the API takes.
///
/// # Errors
///
/// Only if randomness is unavailable.
pub fn seal<T: Serialize>(key: &ShareKey, value: &T) -> Result<String> {
    let json = serde_json::to_vec(value)
        .map_err(|error| Error::Protocol(format!("could not describe the file: {error}")))?;
    Ok(Base64UrlUnpadded::encode_string(&tessera_crypto::seal(
        key,
        random()?,
        &json,
    )))
}

/// Opens a description the server handed back.
///
/// # Errors
///
/// When it does not open with this key, or is not the JSON clients write.
pub fn open<T: for<'de> Deserialize<'de>>(key: &ShareKey, sealed: &str) -> Result<T> {
    let bytes = Base64UrlUnpadded::decode_vec(sealed)
        .map_err(|_| Error::Protocol("a sealed description is not base64".to_owned()))?;
    let json = tessera_crypto::open(key, &bytes)?;
    serde_json::from_slice(&json)
        .map_err(|error| Error::Protocol(format!("a sealed description is not ours: {error}")))
}

/// Fresh bytes from the operating system.
///
/// # Errors
///
/// When it has none to give, which on anything this runs on means something is
/// badly wrong.
pub fn random<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes)
        .map_err(|error| Error::Io(std::io::Error::other(error.to_string())))?;
    Ok(bytes)
}

/// Where a file with this name goes under `root`, or `None` if nothing is left
/// of the name once it is made safe.
///
/// The name came from whoever uploaded the share. Only its ordinary parts are
/// kept, so `../../.bashrc` lands as `.bashrc` inside `root` and `/etc/passwd`
/// as `etc/passwd`, never outside it.
#[must_use]
pub fn place(root: &Path, name: &str) -> Option<PathBuf> {
    let relative: PathBuf = name
        .split(['/', '\\'])
        .flat_map(|part| Path::new(part).components())
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part),
            _ => None,
        })
        .collect();
    (relative.components().next().is_some()).then(|| root.join(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_description_round_trips() {
        let key = ShareKey::from_random([5; 32]);
        let name = SealedName {
            name: "報告/final.pdf".to_owned(),
            media_type: Some("application/pdf".to_owned()),
        };
        let sealed = seal(&key, &name).unwrap();
        assert!(
            sealed
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        );
        assert_eq!(open::<SealedName>(&key, &sealed).unwrap(), name);
    }

    /// The field is `type` on the wire, the word the browser uses.
    #[test]
    fn the_media_type_is_called_type() {
        let json = serde_json::to_string(&SealedName {
            name: "a".to_owned(),
            media_type: Some("text/plain".to_owned()),
        })
        .unwrap();
        assert_eq!(json, r#"{"name":"a","type":"text/plain"}"#);
    }

    #[test]
    fn a_name_cannot_climb_out() {
        let root = Path::new("/downloads");
        assert_eq!(place(root, "../../.bashrc").unwrap(), root.join(".bashrc"));
        assert_eq!(place(root, "/etc/passwd").unwrap(), root.join("etc/passwd"));
        assert_eq!(place(root, "a\\..\\b.txt").unwrap(), root.join("a/b.txt"));
        assert_eq!(
            place(root, "photos/cat.jpg").unwrap(),
            root.join("photos/cat.jpg")
        );
        assert!(place(root, "../..").is_none());
        assert!(place(root, "").is_none());
    }
}
