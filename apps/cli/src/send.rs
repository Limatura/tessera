//! Sending files, or a note, and handing back the link.

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use tessera_crypto::{CHUNK_LEN, HEADER_LEN, NONCE_LEN, Salt, Sealer, ShareKey};
use tokio::io::AsyncReadExt as _;

use crate::{
    Error, Result,
    client::{Client, Share, Upload},
    link,
    names::{self, SealedName},
};

/// How many times one part is retried before the upload gives up.
const ATTEMPTS: u32 = 5;

/// What to send.
#[derive(Debug)]
pub enum Content {
    /// Files, each under its own name, in one share.
    Files(Vec<PathBuf>),
    /// A note.
    Note(Vec<u8>),
}

/// How the share should be made.
#[derive(Debug, Default)]
pub struct Options {
    /// Seal the content before it leaves this machine.
    pub encrypt: bool,
    /// A password: the share's, or with `encrypt`, the one its key comes from.
    pub password: Option<String>,
    /// How long it should last.
    pub expires_in_hours: Option<u64>,
    /// How many times it may be retrieved.
    pub max_uses: Option<u32>,
}

/// What a finished send gives back.
#[derive(Debug)]
pub struct Sent {
    /// The link to hand out: with its key after `#` when the key lives there.
    pub link: String,
    /// The retrieval code alone.
    pub code: String,
}

/// How far a transfer has got: a name, bytes done, bytes in all.
pub type Progress<'a> = &'a mut dyn FnMut(&str, u64, u64);

/// How a share is sealed, with what the sealing needs.
enum Sealing {
    Plain,
    Link(ShareKey),
    Password { key: ShareKey, salt: String },
}

impl Sealing {
    const fn key(&self) -> Option<&ShareKey> {
        match self {
            Self::Plain => None,
            Self::Link(key) | Self::Password { key, .. } => Some(key),
        }
    }

    /// What a request making the share says about sealing.
    fn query(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::Plain => Vec::new(),
            Self::Link(_) => vec![("encryption", "link".to_owned())],
            Self::Password { salt, .. } => vec![
                ("encryption", "password".to_owned()),
                ("key_salt", salt.clone()),
            ],
        }
    }
}

/// Makes a share of `content` and returns its link.
///
/// # Errors
///
/// When a file cannot be read, the server refuses, or the network fails more
/// often than a retry covers.
pub async fn send(
    client: &Client,
    content: Content,
    options: Options,
    progress: Progress<'_>,
) -> Result<Sent> {
    let sealing = sealing(&options).await?;
    let share = match content {
        Content::Note(body) => send_note(client, &sealing, body).await?,
        Content::Files(paths) => send_files(client, &sealing, &paths, progress).await?,
    };

    // The password a server checks: the one typed, or for a key that came from
    // it, the token derived from the key.
    let password = match &sealing {
        Sealing::Password { key, .. } => Some(key.access_token()),
        Sealing::Link(_) | Sealing::Plain => options.password.clone(),
    };
    let mut settings = serde_json::Map::new();
    if let Some(hours) = options.expires_in_hours {
        settings.insert("expires_in_hours".to_owned(), hours.into());
    }
    if let Some(uses) = options.max_uses {
        settings.insert("max_uses".to_owned(), uses.into());
    }
    if password.is_some() || !settings.is_empty() {
        client
            .update_share(
                &share.code,
                share.management_token.as_deref(),
                password.as_deref(),
                &settings.into(),
            )
            .await?;
    }

    let key = match &sealing {
        Sealing::Link(key) => Some(key),
        Sealing::Password { .. } | Sealing::Plain => None,
    };
    Ok(Sent {
        link: link::to_share(client.server(), &share.code, key),
        code: share.code,
    })
}

async fn sealing(options: &Options) -> Result<Sealing> {
    if !options.encrypt {
        return Ok(Sealing::Plain);
    }
    let Some(password) = options.password.clone() else {
        return Ok(Sealing::Link(ShareKey::from_random(names::random()?)));
    };
    if password.chars().count() < tessera_crypto::MIN_PASSWORD_CHARS {
        return Err(Error::Usage(format!(
            "a password the key comes from must be at least {} characters: anyone holding the \
             stored share can guess at it offline",
            tessera_crypto::MIN_PASSWORD_CHARS
        )));
    }
    let salt = Salt::from_random(names::random()?);
    // Slow by design (Argon2id), so off the thread that drives the network.
    let key = tokio::task::spawn_blocking(move || ShareKey::from_password(&password, &salt))
        .await
        .map_err(|error| Error::Io(std::io::Error::other(error)))??;
    Ok(Sealing::Password {
        key,
        salt: salt.encode(),
    })
}

async fn send_note(client: &Client, sealing: &Sealing, body: Vec<u8>) -> Result<Share> {
    let body = match sealing.key() {
        Some(key) => tessera_crypto::seal(key, names::random()?, &body),
        None => body,
    };
    client.create_note(&sealing.query(), body).await
}

async fn send_files(
    client: &Client,
    sealing: &Sealing,
    paths: &[PathBuf],
    progress: Progress<'_>,
) -> Result<Share> {
    let (first, rest) = paths
        .split_first()
        .ok_or_else(|| Error::Usage("nothing to send".to_owned()))?;
    let share = upload_file(client, sealing, first, None, progress).await?;
    for path in rest {
        upload_file(client, sealing, path, Some(&share), progress).await?;
    }
    Ok(share)
}

/// Sends one file: into a new share, or into `into`.
async fn upload_file(
    client: &Client,
    sealing: &Sealing,
    path: &Path,
    into: Option<&Share>,
    progress: Progress<'_>,
) -> Result<Share> {
    let metadata = tokio::fs::metadata(path).await?;
    if !metadata.is_file() {
        return Err(Error::Usage(format!(
            "`{}` is not a file; send the files inside it, or an archive of it",
            path.display()
        )));
    }
    let name = path.file_name().map_or_else(
        || "file".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let fingerprint = Fingerprint::of(&metadata);
    let declared = match sealing.key() {
        Some(_) => tessera_crypto::sealed_len(fingerprint.len)
            .ok_or_else(|| Error::Usage(format!("`{name}` is too large to seal")))?,
        None if fingerprint.len == 0 => {
            return Err(Error::Usage(format!(
                "`{name}` is empty; there is nothing to send"
            )));
        }
        None => fingerprint.len,
    };

    let mut query = match sealing.key() {
        Some(key) => vec![(
            "sealed_name",
            names::seal(
                key,
                &SealedName {
                    name: name.clone(),
                    media_type: None,
                },
            )?,
        )],
        None => vec![("filename", name.clone())],
    };
    match into {
        None => query.extend(sealing.query()),
        Some(share) => query.push(("code", share.code.clone())),
    }
    let management = into.and_then(|share| share.management_token.as_deref());
    // Signed in, the account already says whose share it is.
    let management = management.filter(|_| !client.signed_in());
    let upload = client.open_upload(&query, declared, management).await?;

    let nonce = names::random()?;
    let part = usize::try_from(upload.part_size).unwrap_or(usize::MAX);
    let mut outgoing = Outgoing::open(path, sealing.key(), nonce, &fingerprint).await?;
    let mut offset = 0;
    let mut attempt = 0;
    loop {
        let bytes = outgoing.take(part).await?;
        let sent = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        match client.send_part(&upload, offset, bytes).await {
            Ok(Some(share)) => {
                progress(&name, declared, declared);
                return Ok(share);
            }
            Ok(None) => {
                offset += sent;
                attempt = 0;
                progress(&name, offset, declared);
            }
            Err(error) if attempt + 1 < ATTEMPTS && worth_retrying(&error) => {
                attempt += 1;
                tokio::time::sleep(Duration::from_secs(u64::from(attempt))).await;
                offset = resume_at(client, &upload).await?;
                // Sealed again from the start: the same key, nonce and content
                // give the same bytes, so what the server already has lines up.
                outgoing = Outgoing::open(path, sealing.key(), nonce, &fingerprint).await?;
                outgoing.skip(offset).await?;
            }
            Err(error) => return Err(error),
        }
    }
}

async fn resume_at(client: &Client, upload: &Upload) -> Result<u64> {
    client.upload_offset(upload).await
}

const fn worth_retrying(error: &Error) -> bool {
    match error {
        Error::Http(_) => true,
        // 409 is an offset that no longer matches, which asking again settles.
        Error::Server { status, .. } => *status >= 500 || *status == 409,
        _ => false,
    }
}

/// What a file looked like when its upload started.
///
/// Resending part of a sealed file means sealing it again under the same
/// nonce. If the file changed in between, two different contents would share
/// one nonce, which a stream cipher cannot survive, so a changed file stops
/// the upload instead.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Fingerprint {
    len: u64,
    modified: Option<SystemTime>,
}

impl Fingerprint {
    fn of(metadata: &std::fs::Metadata) -> Self {
        Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        }
    }
}

/// A file as the bytes an upload sends: sealed, or as it is.
struct Outgoing {
    file: tokio::fs::File,
    remaining: u64,
    sealer: Option<Sealer>,
    header: Option<[u8; HEADER_LEN]>,
    finished: bool,
    ready: Vec<u8>,
    cursor: usize,
}

impl Outgoing {
    async fn open(
        path: &Path,
        key: Option<&ShareKey>,
        nonce: [u8; NONCE_LEN],
        expected: &Fingerprint,
    ) -> Result<Self> {
        let file = tokio::fs::File::open(path).await?;
        if Fingerprint::of(&file.metadata().await?) != *expected {
            return Err(Error::Usage(format!(
                "`{}` changed while it was being sent",
                path.display()
            )));
        }
        let (sealer, header) = match key {
            Some(key) => {
                let (sealer, header) = Sealer::new(key, nonce);
                (Some(sealer), Some(header))
            }
            None => (None, None),
        };
        Ok(Self {
            file,
            remaining: expected.len,
            sealer,
            header,
            finished: false,
            ready: Vec::new(),
            cursor: 0,
        })
    }

    /// Up to `want` bytes; fewer only at the end.
    async fn take(&mut self, want: usize) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(want.min(64 * CHUNK_LEN));
        while out.len() < want {
            if let Some(ready) = self
                .ready
                .get(self.cursor..)
                .filter(|rest| !rest.is_empty())
            {
                let used = ready.len().min(want - out.len());
                out.extend_from_slice(ready.get(..used).unwrap_or_default());
                self.cursor += used;
                continue;
            }
            if !self.refill().await? {
                break;
            }
        }
        Ok(out)
    }

    async fn skip(&mut self, mut count: u64) -> Result<()> {
        while count > 0 {
            let step = usize::try_from(count.min(1 << 20)).unwrap_or(1 << 20);
            let taken = self.take(step).await?;
            if taken.is_empty() {
                return Err(Error::Protocol(
                    "the server has more of this file than there is".to_owned(),
                ));
            }
            count -= u64::try_from(taken.len()).unwrap_or(count);
        }
        Ok(())
    }

    /// Reads and, when sealing, seals the next piece. `false` at the end.
    async fn refill(&mut self) -> Result<bool> {
        self.cursor = 0;
        if let Some(header) = self.header.take() {
            self.ready = header.to_vec();
            return Ok(true);
        }
        if self.finished {
            return Ok(false);
        }
        let len = usize::try_from(self.remaining.min(u64::try_from(CHUNK_LEN).unwrap_or(0)))
            .unwrap_or(CHUNK_LEN);
        let mut plain = vec![0; len];
        self.file.read_exact(&mut plain).await?;
        self.remaining -= u64::try_from(len).unwrap_or(0);
        let last = self.remaining == 0;
        self.ready = match &mut self.sealer {
            Some(sealer) => sealer.seal(&plain, last)?,
            None => plain,
        };
        self.finished = last;
        Ok(!self.ready.is_empty() || self.sealer.is_some())
    }
}
