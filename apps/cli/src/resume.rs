//! Picking an interrupted `tess send` up where it stopped.
//!
//! While files are being sent, how far the send has got is written to a small
//! file under `$XDG_STATE_HOME/tessera/uploads/`: which files are done, the
//! share they went into, and the upload session of the one in flight. Running
//! the same command again, the same files to the same instance, reads it and
//! carries on. The server keeps an unfinished upload for
//! `upload_session_ttl_hours`, 24 by default; past that, or if a file changed,
//! the send starts over.
//!
//! # What the file holds
//!
//! For an encrypted share, the key and the nonce of the file in flight: the
//! bytes already on the server were sealed with them, and the rest has to be
//! sealed the same way to line up. That is a secret on disk, so the file is
//! readable by its owner only and removed as soon as the send finishes. The key
//! would end up in the link printed at the end anyway.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tessera_crypto::{NONCE_LEN, ShareKey};
use url::Url;

use crate::{
    Error, Result,
    client::{Share, Upload},
    config,
};

/// Bumped when the shape changes, so an older file is ignored, not misread.
const VERSION: u32 = 1;

/// What a file looked like when its send started. A file whose length or
/// modification time differ is not the one the server has part of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Fingerprint {
    pub(crate) len: u64,
    pub(crate) modified: Option<std::time::SystemTime>,
}

impl Fingerprint {
    pub(crate) fn of(metadata: &std::fs::Metadata) -> Self {
        Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        }
    }
}

/// How the share is sealed, in a form that can be written down.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum StoredSealing {
    Plain,
    Link { key: String },
    Password { key: String, salt: String },
}

/// The upload of one file, in flight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Current {
    pub(crate) upload: Upload,
    pub(crate) nonce: [u8; NONCE_LEN],
}

/// How far a send has got.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct State {
    version: u32,
    pub(crate) fingerprints: Vec<Fingerprint>,
    pub(crate) sealing: StoredSealing,
    /// The share, once the first file has made it.
    pub(crate) share: Option<Share>,
    /// How many files, from the start, are finished.
    pub(crate) done: usize,
    pub(crate) current: Option<Current>,
}

impl State {
    pub(crate) const fn new(fingerprints: Vec<Fingerprint>, sealing: StoredSealing) -> Self {
        Self {
            version: VERSION,
            fingerprints,
            sealing,
            share: None,
            done: 0,
            current: None,
        }
    }
}

/// Where one send keeps its state.
#[derive(Debug, Clone)]
pub(crate) struct Job {
    path: PathBuf,
}

impl Job {
    /// The state file for sending `paths` to `server`, with the same choices
    /// about sealing: a plain send and an encrypted one of the same files are
    /// different sends.
    pub(crate) fn for_send(
        dir: &Path,
        server: &Url,
        paths: &[PathBuf],
        encrypt: bool,
        password: bool,
    ) -> Result<Self> {
        let mut identity = format!(
            "{}\n{encrypt}\n{password}\n",
            server.origin().ascii_serialization()
        );
        for path in paths {
            let path = std::fs::canonicalize(path)?;
            identity.push_str(&path.to_string_lossy());
            identity.push('\n');
        }
        Ok(Self {
            path: dir.join(format!("{:016x}.json", fnv1a(identity.as_bytes()))),
        })
    }

    /// The saved state, if there is one this version can read.
    pub(crate) fn load(&self) -> Option<State> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        serde_json::from_str::<State>(&text)
            .ok()
            .filter(|state| state.version == VERSION)
    }

    /// Writes `state`, readable by its owner only.
    pub(crate) fn save(&self, state: &State) -> Result<()> {
        let text = serde_json::to_string(state)
            .map_err(|error| Error::Protocol(format!("cannot record the upload: {error}")))?;
        config::write_private(&self.path, text.as_bytes())
    }

    /// Forgets the send, now that it is finished.
    pub(crate) fn remove(&self) {
        // Nothing to do about a file that will not go: the next send of the
        // same files would find it, see the share is complete, and finish.
        std::fs::remove_file(&self.path).ok();
    }
}

/// The directory for upload state.
///
/// # Errors
///
/// When there is no home directory to put it in.
pub fn default_dir() -> Result<PathBuf> {
    use etcetera::BaseStrategy as _;

    let strategy = etcetera::choose_base_strategy().map_err(|error| Error::Config {
        path: PathBuf::from("uploads"),
        message: format!("no directory to keep upload state in: {error}"),
    })?;
    let base = strategy.state_dir().unwrap_or_else(|| strategy.cache_dir());
    Ok(base.join("tessera").join("uploads"))
}

impl StoredSealing {
    pub(crate) fn link(key: &ShareKey) -> Self {
        Self::Link { key: key.encode() }
    }

    pub(crate) fn password(key: &ShareKey, salt: &str) -> Self {
        Self::Password {
            key: key.encode(),
            salt: salt.to_owned(),
        }
    }
}

/// FNV-1a, 64 bits: stable across builds, unlike the standard library's
/// hasher, so a state file is found again by the next version.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_send_finds_the_same_file() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("a.txt");
        std::fs::write(&file, b"hello").expect("written");
        let server = Url::parse("https://share.example").expect("a url");
        let paths = [file];
        let one = Job::for_send(dir.path(), &server, &paths, false, false).expect("a job");
        let again = Job::for_send(dir.path(), &server, &paths, false, false).expect("a job");
        let sealed = Job::for_send(dir.path(), &server, &paths, true, false).expect("a job");
        assert_eq!(one.path, again.path);
        assert_ne!(one.path, sealed.path);
    }

    #[test]
    fn state_reads_back() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let job = Job {
            path: dir.path().join("uploads").join("job.json"),
        };
        let mut state = State::new(
            vec![Fingerprint {
                len: 5,
                modified: None,
            }],
            StoredSealing::Plain,
        );
        state.done = 1;
        job.save(&state).expect("saved");
        let read = job.load().expect("read back");
        assert_eq!(read.done, 1);
        assert_eq!(read.fingerprints, state.fingerprints);
        job.remove();
        assert!(job.load().is_none());
    }
}
