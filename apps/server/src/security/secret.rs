//! Sealing the settings that are stored and read back, so the database alone
//! cannot open them.
//!
//! # Why anything is encrypted here at all
//!
//! Every other secret this software holds is either a hash (passwords, tokens)
//! or lives outside the database (`database.url`). A backend's URL was the
//! first thing that had to be both *recoverable*, since the
//! process needs the key to talk to the bucket, and stored, since the point is
//! that an operator can add one from the panel without a restart.
//!
//! It is no longer the only one: an identity provider's client secret, two
//! OAuth ones, a relay's credentials and a scanner's all arrive the same way
//! and are sealed with the same key. That is why the key is
//! `security.secret_key` and not, as it was, `storage.secret_key` — the name
//! sent people looking for a bucket setting.
//!
//! Recoverable and stored means a database dump is a credential leak, unless
//! the thing that opens it lives somewhere the dump does not. So the key is
//! kept apart from the rows: in the environment when an operator sets one, and
//! otherwise in a file of its own, `secret.key` under `server.data_dir`,
//! written on the first start. See [`Sealer::resolve`].
//!
//! The file is the convenience and the environment the stronger choice. A
//! backup that copies the whole data directory copies the key with the
//! database, and an operator who cares about that sets the variable instead.
//!
//! # What this is not
//!
//! Not a secrets manager, and not protection against a reader of the running
//! process, since anything that can read `/proc` can read the key. It moves the
//! blast radius of "the database file leaked" from "and now they have
//! your buckets" to "and now they have some ciphertext", the realistic
//! failure and the one an operator can do nothing about after the fact.

use aws_lc_rs::aead::{AES_256_GCM, Aad, NONCE_LEN, Nonce, RandomizedNonceKey};

/// Anything that can go wrong sealing or opening a credential.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// No key is configured, so nothing can be stored.
    ///
    /// Refused, not fallen back to plaintext. An instance that quietly
    /// stored recoverable credentials in a table would do the one thing this
    /// module exists to prevent, and that surfaces only when the dump leaks.
    #[error(
        "security.secret_key is not set; add one to the environment before configuring a backend"
    )]
    NoKey,

    /// The key is not 32 bytes of hex.
    #[error("security.secret_key must be 64 hexadecimal characters (32 bytes)")]
    BadKey,

    /// The key file could not be read, written or understood.
    #[error("could not use the key file `{path}`: {source}")]
    KeyFile {
        /// The file.
        path: String,
        /// What went wrong with it.
        source: std::io::Error,
    },

    /// The stored value will not open with this key.
    ///
    /// Almost always a key that changed, and the message says so because the
    /// other reading, corrupted rows, sends an operator to the wrong place.
    #[error("a stored credential will not open with this key; was security.secret_key changed?")]
    WillNotOpen,
}

/// The key a process seals credentials with, when it has one.
#[derive(Clone)]
pub struct Sealer(Option<[u8; 32]>);

impl core::fmt::Debug for Sealer {
    /// Prints whether there is a key, never the key.
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(if self.0.is_some() {
            "Sealer(set)"
        } else {
            "Sealer(unset)"
        })
    }
}

impl Sealer {
    /// Reads the key from its configured hex form. Empty means no key.
    ///
    /// # Errors
    ///
    /// [`SecretError::BadKey`] if it is not 64 hexadecimal characters.
    pub fn new(hex: &str) -> Result<Self, SecretError> {
        let hex = hex.trim();
        if hex.is_empty() {
            return Ok(Self(None));
        }
        if hex.len() != 64 {
            return Err(SecretError::BadKey);
        }
        let mut key = [0_u8; 32];
        for (index, byte) in key.iter_mut().enumerate() {
            let at = index * 2;
            *byte = u8::from_str_radix(hex.get(at..at + 2).ok_or(SecretError::BadKey)?, 16)
                .map_err(|_| SecretError::BadKey)?;
        }
        Ok(Self(Some(key)))
    }

    /// The key this instance seals with: `configured` when it is set, else
    /// the one in `secret.key` under `data_dir`, made there if there is none.
    ///
    /// # Why a key is made rather than asked for
    ///
    /// Without one, nothing sealed can be stored, and that is every backend
    /// and every credential the panel takes. Asking for a variable before the
    /// first start is one more thing to get wrong, for a value nobody needs
    /// to know. The environment still wins, for an operator who wants the key
    /// out of the data directory.
    ///
    /// Created with `create_new`, so two processes starting at once (the
    /// server and a command run beside it) end up with the one that got there
    /// first, not one each.
    ///
    /// # Errors
    ///
    /// [`SecretError::BadKey`] for a configured key that is not 64 hexadecimal
    /// characters, [`SecretError::KeyFile`] when the file cannot be read or
    /// written, or holds something that is not a key.
    pub fn resolve(configured: &str, data_dir: &std::path::Path) -> Result<Self, SecretError> {
        if !configured.trim().is_empty() {
            return Self::new(configured);
        }
        let path = data_dir.join(KEY_FILE);
        let failed = |source| SecretError::KeyFile {
            path: path.display().to_string(),
            source,
        };
        loop {
            match std::fs::read_to_string(&path) {
                Ok(stored) => {
                    return match Self::new(&stored) {
                        Ok(sealer) if sealer.is_set() => Ok(sealer),
                        _ => Err(failed(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "not 64 hexadecimal characters",
                        ))),
                    };
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(failed(error)),
            }
            std::fs::create_dir_all(data_dir).map_err(failed)?;
            match create_key_file(&path) {
                Ok(hex) => {
                    tracing::info!(
                        path = %path.display(),
                        "made a key for sealing stored credentials; back it up, or set \
                         TESSERA_SECURITY__SECRET_KEY to keep it out of the data directory"
                    );
                    return Self::new(&hex);
                }
                // Another process made it between the read and the write. Its
                // key is the one to use, so read again.
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(failed(error)),
            }
        }
    }

    /// [`Sealer::resolve`] for an instance's configuration: the server and
    /// every command that opens the database read the key the same way, so
    /// what one seals the other opens.
    ///
    /// # Errors
    ///
    /// As [`Sealer::resolve`].
    pub fn for_config(config: &crate::config::Config) -> Result<Self, SecretError> {
        Self::resolve(
            &config.security.secret_key,
            std::path::Path::new(&config.server.data_dir),
        )
    }

    /// Whether this process can store credentials at all.
    #[must_use]
    pub const fn is_set(&self) -> bool {
        self.0.is_some()
    }

    /// Seals `plain`, returning the nonce and ciphertext as hex.
    ///
    /// A fresh nonce per call, chosen by the library and not by us. Reusing
    /// one under the same key is what breaks GCM, and the failure is silent.
    ///
    /// # Errors
    ///
    /// [`SecretError::NoKey`] when none is configured.
    pub fn seal(&self, plain: &str) -> Result<String, SecretError> {
        let key = self.key()?;
        let mut buffer = plain.as_bytes().to_vec();
        let nonce = key
            .seal_in_place_append_tag(Aad::empty(), &mut buffer)
            .map_err(|_| SecretError::WillNotOpen)?;

        let mut sealed = Vec::with_capacity(NONCE_LEN + buffer.len());
        sealed.extend_from_slice(nonce.as_ref());
        sealed.extend_from_slice(&buffer);
        Ok(sealed.iter().fold(String::new(), |mut hex, byte| {
            use core::fmt::Write as _;
            // `write!` into a `String` cannot fail, and swallowing the result
            // is what keeps this an infallible fold.
            let _ = write!(hex, "{byte:02x}");
            hex
        }))
    }

    /// Opens what [`Sealer::seal`] produced.
    ///
    /// # Errors
    ///
    /// [`SecretError::NoKey`] when none is configured, [`SecretError::WillNotOpen`]
    /// when the value does not belong to this key.
    pub fn open(&self, sealed: &str) -> Result<String, SecretError> {
        let key = self.key()?;
        let bytes = decode_hex(sealed)?;
        let (nonce, body) = bytes
            .split_at_checked(NONCE_LEN)
            .ok_or(SecretError::WillNotOpen)?;
        let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| SecretError::WillNotOpen)?;

        let mut buffer = body.to_vec();
        let opened = key
            .open_in_place(
                Nonce::assume_unique_for_key(nonce),
                Aad::empty(),
                &mut buffer,
            )
            .map_err(|_| SecretError::WillNotOpen)?;
        String::from_utf8(opened.to_vec()).map_err(|_| SecretError::WillNotOpen)
    }

    fn key(&self) -> Result<RandomizedNonceKey, SecretError> {
        let raw = self.0.as_ref().ok_or(SecretError::NoKey)?;
        RandomizedNonceKey::new(&AES_256_GCM, raw).map_err(|_| SecretError::BadKey)
    }
}

/// The name of the file [`Sealer::resolve`] keeps a made key in.
pub const KEY_FILE: &str = "secret.key";

/// Writes a new key to `path`, readable by this user alone, and returns it.
///
/// Fails with `AlreadyExists` rather than replacing one: a replaced key leaves
/// everything sealed under the old one unreadable.
fn create_key_file(path: &std::path::Path) -> std::io::Result<String> {
    use std::io::Write as _;

    let mut bytes = [0_u8; 32];
    aws_lc_rs::rand::fill(&mut bytes).map_err(|_| std::io::Error::other("no randomness"))?;
    let hex = bytes.iter().fold(String::new(), |mut hex, byte| {
        use core::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
        hex
    });

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    file.write_all(hex.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(hex)
}

fn decode_hex(hex: &str) -> Result<Vec<u8>, SecretError> {
    if !hex.len().is_multiple_of(2) {
        return Err(SecretError::WillNotOpen);
    }
    (0..hex.len() / 2)
        .map(|index| {
            let at = index * 2;
            u8::from_str_radix(hex.get(at..at + 2).ok_or(SecretError::WillNotOpen)?, 16)
                .map_err(|_| SecretError::WillNotOpen)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Sealer, SecretError};

    const KEY: &str = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
    const OTHER: &str = "ff02030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";

    #[test]
    fn a_credential_round_trips() {
        let key = Sealer::new(KEY).expect("a key");
        let stored = key
            .seal("s3://bucket?aws_secret_access_key=hunter2")
            .expect("seal");
        assert_eq!(
            key.open(&stored).expect("open"),
            "s3://bucket?aws_secret_access_key=hunter2"
        );
    }

    /// The stored form must not contain what it is protecting. A database dump
    /// is not supposed to be a credential leak.
    #[test]
    fn the_stored_form_does_not_contain_the_secret() {
        let key = Sealer::new(KEY).expect("a key");
        let stored = key
            .seal("s3://bucket?aws_secret_access_key=hunter2")
            .expect("seal");
        assert!(!stored.contains("hunter2"));
        assert!(!stored.contains("bucket"));
    }

    /// A fresh nonce per call. Sealing the same value twice must not produce
    /// the same bytes, or a dump leaks which backends share a URL.
    #[test]
    fn sealing_twice_differs() {
        let sealer = Sealer::new(KEY).expect("a key");
        let once = sealer.seal("s3://bucket").expect("seal");
        let twice = sealer.seal("s3://bucket").expect("seal");
        assert_ne!(once, twice);
        assert_eq!(
            sealer.open(&once).expect("open"),
            sealer.open(&twice).expect("open")
        );
    }

    /// The message an operator gets has to send them to the key, not the rows:
    /// a changed key looks exactly like corruption from the inside.
    #[test]
    fn another_key_will_not_open_it() {
        let sealed = Sealer::new(KEY)
            .expect("a key")
            .seal("s3://bucket")
            .expect("seal");
        assert!(matches!(
            Sealer::new(OTHER).expect("a key").open(&sealed),
            Err(SecretError::WillNotOpen)
        ));
    }

    /// Falling back to plaintext would do the one thing this module exists to
    /// prevent, and that surfaces only when the dump leaks.
    #[test]
    fn without_a_key_nothing_is_stored() {
        let sealer = Sealer::new("").expect("no key");
        assert!(!sealer.is_set());
        assert!(matches!(
            sealer.seal("s3://bucket"),
            Err(SecretError::NoKey)
        ));
    }

    #[test]
    fn a_key_of_the_wrong_length_is_refused() {
        assert!(matches!(Sealer::new("abcd"), Err(SecretError::BadKey)));
        assert!(matches!(
            Sealer::new(&"z".repeat(64)),
            Err(SecretError::BadKey)
        ));
    }

    /// With nothing configured, a key is made once and the same one read back
    /// after, or every restart would orphan what the last one sealed.
    #[test]
    fn a_made_key_is_kept_and_reused() {
        let dir = tempfile::tempdir().expect("a directory");
        let data = dir.path().join("nested");
        let sealed = Sealer::resolve("", &data)
            .expect("made")
            .seal("s3://bucket")
            .expect("seal");
        let again = Sealer::resolve("  ", &data).expect("read back");
        assert_eq!(again.open(&sealed).expect("open"), "s3://bucket");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(data.join(super::KEY_FILE))
                .expect("the file")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "nobody else may read the key");
        }
    }

    /// The environment wins, and does not touch the directory.
    #[test]
    fn a_configured_key_beats_the_file() {
        let dir = tempfile::tempdir().expect("a directory");
        let stored = Sealer::new(KEY)
            .expect("a key")
            .seal("s3://bucket")
            .expect("seal");
        let resolved = Sealer::resolve(KEY, dir.path()).expect("configured");
        assert_eq!(resolved.open(&stored).expect("open"), "s3://bucket");
        assert!(!dir.path().join(super::KEY_FILE).exists());
    }

    /// A damaged file is reported, not replaced: a new key would leave every
    /// sealed setting unreadable without a word.
    #[test]
    fn a_damaged_key_file_is_not_replaced() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join(super::KEY_FILE);
        std::fs::write(&path, "not a key").expect("write");
        assert!(matches!(
            Sealer::resolve("", dir.path()),
            Err(SecretError::KeyFile { .. })
        ));
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "not a key");
    }

    /// Whatever it prints, it must not be the key.
    #[test]
    fn debug_never_shows_the_key() {
        let shown = format!("{:?}", Sealer::new(KEY).expect("a key"));
        assert!(!shown.contains("0102"));
    }
}
