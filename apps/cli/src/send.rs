//! Sending files, or a note, and handing back the link.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use tessera_crypto::{CHUNK_LEN, HEADER_LEN, NONCE_LEN, Salt, Sealer, ShareKey};
use tokio::io::AsyncReadExt as _;

use crate::{
    Error, Result,
    client::{Client, Share, Upload},
    link,
    names::{self, SealedName},
    resume::{Current, Fingerprint, Job, State, StoredSealing},
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
    /// A note with what the share page shows for it: a title, and a
    /// language for its highlighting, both kept only when it is not sealed.
    Text {
        /// What it says.
        body: Vec<u8>,
        /// Its name on the share page.
        title: Option<String>,
        /// A language as the site names them: `rust`, `python`...
        language: Option<String>,
    },
    /// A link that the share sends its visitors on to. Never sealed: the
    /// instance has to read it to redirect.
    Link(String),
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
    /// Where to record how far a send of files has got, so running it again
    /// after an interruption carries on. `None` keeps nothing.
    pub resume: Option<PathBuf>,
    /// Paths under a directory being sent to leave out of it.
    pub leave_out: Vec<PathBuf>,
}

/// What a finished send gives back.
#[derive(Debug)]
pub struct Sent {
    /// The link to hand out: with its key after `#` when the key lives there.
    pub link: String,
    /// The retrieval code alone.
    pub code: String,
    /// Empty files found inside a directory and left out, since a share cannot
    /// hold an empty file unless it is sealed.
    pub skipped_empty: usize,
}

/// One file to send, and the name it goes under.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Where it is on this machine.
    pub path: PathBuf,
    /// Its name in the share: the file's own name, or for a file found inside
    /// a directory, the directory's name and the path below it, `/`-separated,
    /// the way the web page names a folder's files.
    pub name: String,
}

/// The files `paths` names: each file as it is, and each directory as every
/// file under it but those in `leave_out` or under them, in name order.
///
/// Symbolic links to directories are not followed, so a link back up the tree
/// cannot make the walk endless. With `skip_empty`, empty files found inside a
/// directory are left out and counted; an empty file named on its own is still
/// an error later, since that one was asked for.
///
/// # Errors
///
/// When a path cannot be read, or a directory holds nothing to send.
pub fn expand(
    paths: &[PathBuf],
    leave_out: &[PathBuf],
    skip_empty: bool,
) -> Result<(Vec<Entry>, usize)> {
    let mut entries = Vec::new();
    let mut skipped = 0;
    for path in paths {
        if !std::fs::metadata(path)?.is_dir() {
            entries.push(Entry {
                path: path.clone(),
                name: file_name(path),
            });
            continue;
        }
        let root = file_name(&std::fs::canonicalize(path)?);
        let mut found = Vec::new();
        walk(path, &root, leave_out, &mut found)?;
        found.sort_by(|a, b| a.name.cmp(&b.name));
        let before = found.len();
        if skip_empty {
            found.retain(|entry| std::fs::metadata(&entry.path).is_ok_and(|meta| meta.len() > 0));
        }
        skipped += before - found.len();
        if found.is_empty() {
            return Err(Error::Usage(format!(
                "`{}` has no files to send",
                path.display()
            )));
        }
        entries.extend(found);
    }
    Ok((entries, skipped))
}

fn walk(dir: &Path, prefix: &str, leave_out: &[PathBuf], into: &mut Vec<Entry>) -> Result<()> {
    for item in std::fs::read_dir(dir)? {
        let item = item?;
        let path = item.path();
        if leave_out.contains(&path) {
            continue;
        }
        let name = format!("{prefix}/{}", item.file_name().to_string_lossy());
        let kind = item.file_type()?;
        if kind.is_dir() {
            walk(&path, &name, leave_out, into)?;
        } else if kind.is_file() || (kind.is_symlink() && std::fs::metadata(&path)?.is_file()) {
            into.push(Entry { path, name });
        }
    }
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || "file".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
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
    fn stored(&self) -> StoredSealing {
        match self {
            Self::Plain => StoredSealing::Plain,
            Self::Link(key) => StoredSealing::link(key),
            Self::Password { key, salt } => StoredSealing::password(key, salt),
        }
    }

    fn restore(stored: &StoredSealing) -> Result<Self> {
        Ok(match stored {
            StoredSealing::Plain => Self::Plain,
            StoredSealing::Link { key } => Self::Link(ShareKey::parse(key)?),
            StoredSealing::Password { key, salt } => Self::Password {
                key: ShareKey::parse(key)?,
                salt: salt.clone(),
            },
        })
    }

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
    let mut skipped_empty = 0;
    let (share, sealing, job) = match content {
        Content::Note(body) => {
            let sealing = sealing(&options).await?;
            (
                send_note(client, &sealing, body, Vec::new()).await?,
                sealing,
                None,
            )
        }
        Content::Text {
            body,
            title,
            language,
        } => {
            let sealing = sealing(&options).await?;
            // Sealed, the title and language would need sealing as well;
            // until that is done here, a sealed note goes without them.
            let shown = if sealing.key().is_none() {
                [("title", title), ("language", language)]
                    .into_iter()
                    .filter_map(|(name, value)| value.map(|value| (name, value)))
                    .collect()
            } else {
                Vec::new()
            };
            (
                send_note(client, &sealing, body, shown).await?,
                sealing,
                None,
            )
        }
        Content::Link(target) => {
            if options.encrypt {
                return Err(Error::Usage(
                    "a link cannot be encrypted: the instance has to read it to send visitors on"
                        .to_owned(),
                ));
            }
            let query = [("kind", "link".to_owned())];
            let share = client.create_note(&query, target.into_bytes()).await?;
            (share, Sealing::Plain, None)
        }
        Content::Files(paths) => {
            let (entries, skipped) = expand(&paths, &options.leave_out, !options.encrypt)?;
            skipped_empty = skipped;
            send_files(client, &options, &entries, progress).await?
        }
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

    if let Some(job) = job {
        job.remove();
    }
    let key = match &sealing {
        Sealing::Link(key) => Some(key),
        Sealing::Password { .. } | Sealing::Plain => None,
    };
    Ok(Sent {
        link: link::to_share(client.server(), &share.code, key),
        code: share.code,
        skipped_empty,
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

async fn send_note(
    client: &Client,
    sealing: &Sealing,
    body: Vec<u8>,
    shown: Vec<(&'static str, String)>,
) -> Result<Share> {
    let body = match sealing.key() {
        Some(key) => tessera_crypto::seal(key, names::random()?, &body),
        None => body,
    };
    let mut query = sealing.query();
    query.extend(shown);
    client.create_note(&query, body).await
}

/// Whether sending `paths` now would carry on an earlier, interrupted send
/// rather than start a new one.
#[must_use]
pub fn will_resume(client: &Client, paths: &[PathBuf], options: &Options) -> bool {
    let Some(dir) = options.resume.as_deref() else {
        return false;
    };
    let Ok((entries, _)) = expand(paths, &options.leave_out, !options.encrypt) else {
        return false;
    };
    let paths: Vec<PathBuf> = entries.into_iter().map(|entry| entry.path).collect();
    let paths = paths.as_slice();
    let Ok(fingerprints) = fingerprints(paths) else {
        return false;
    };
    Job::for_send(
        dir,
        client.server(),
        paths,
        options.encrypt,
        options.password.is_some(),
    )
    .ok()
    .and_then(|job| job.load())
    .is_some_and(|state| state.fingerprints == fingerprints)
}

/// Each file as it is now, refusing anything that cannot be sent.
fn fingerprints(paths: &[PathBuf]) -> Result<Vec<Fingerprint>> {
    paths
        .iter()
        .map(|path| {
            let metadata = std::fs::metadata(path)?;
            if !metadata.is_file() {
                return Err(Error::Usage(format!(
                    "`{}` is not a file; send the files inside it, or an archive of it",
                    path.display()
                )));
            }
            Ok(Fingerprint::of(&metadata))
        })
        .collect()
}

/// Refuses to carry on an encrypted send under a different password than the
/// one it started with: the part already sent is sealed with the first, and
/// the link would open with neither.
async fn same_password(sealing: &Sealing, options: &Options) -> Result<()> {
    let (Sealing::Password { key, salt }, Some(password)) = (sealing, options.password.clone())
    else {
        return Ok(());
    };
    let salt = Salt::parse(salt)?;
    let again = tokio::task::spawn_blocking(move || ShareKey::from_password(&password, &salt))
        .await
        .map_err(|error| Error::Io(std::io::Error::other(error)))??;
    if again.encode() == key.encode() {
        Ok(())
    } else {
        Err(Error::Usage(
            "that is not the password the interrupted send of these files started with".to_owned(),
        ))
    }
}

async fn send_files(
    client: &Client,
    options: &Options,
    entries: &[Entry],
    progress: Progress<'_>,
) -> Result<(Share, Sealing, Option<Job>)> {
    if entries.is_empty() {
        return Err(Error::Usage("nothing to send".to_owned()));
    }
    let paths: Vec<PathBuf> = entries.iter().map(|entry| entry.path.clone()).collect();
    let paths = paths.as_slice();
    let fingerprints = fingerprints(paths)?;
    let job = options
        .resume
        .as_deref()
        .map(|dir| {
            Job::for_send(
                dir,
                client.server(),
                paths,
                options.encrypt,
                options.password.is_some(),
            )
        })
        .transpose()?;
    let earlier = job
        .as_ref()
        .and_then(Job::load)
        .filter(|state| state.fingerprints == fingerprints);
    let (sealing, mut state) = if let Some(state) = earlier {
        let sealing = Sealing::restore(&state.sealing)?;
        same_password(&sealing, options).await?;
        (sealing, state)
    } else {
        let sealing = sealing(options).await?;
        let state = State::new(fingerprints.clone(), sealing.stored());
        (sealing, state)
    };

    for (index, (entry, fingerprint)) in entries.iter().zip(&fingerprints).enumerate() {
        if index < state.done {
            continue;
        }
        let share = upload_file(
            client,
            &sealing,
            entry,
            fingerprint,
            &mut state,
            job.as_ref(),
            progress,
        )
        .await?;
        state.share.get_or_insert(share);
        state.done = index + 1;
        state.current = None;
        if let Some(job) = &job {
            job.save(&state)?;
        }
    }
    let share = state
        .share
        .ok_or_else(|| Error::Protocol("every file was sent and no share came of it".to_owned()))?;
    Ok((share, sealing, job))
}

/// Sends one file: into a new share, or into the one `state` already has.
/// Carries on the upload `state` records as in flight, when there is one.
#[expect(
    clippy::too_many_arguments,
    reason = "one file's worth of context, each piece used once"
)]
async fn upload_file(
    client: &Client,
    sealing: &Sealing,
    entry: &Entry,
    fingerprint: &Fingerprint,
    state: &mut State,
    job: Option<&Job>,
    progress: Progress<'_>,
) -> Result<Share> {
    let path = entry.path.as_path();
    let name = entry.name.clone();
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

    // An upload the server still has picks up at what it holds. One it has
    // dropped, past its time to live, is started again.
    if let Some(current) = state.current.clone() {
        match client.upload_offset(&current.upload).await {
            Ok(offset) => {
                return send_parts(
                    client,
                    sealing,
                    path,
                    fingerprint,
                    &name,
                    declared,
                    &current,
                    offset,
                    progress,
                )
                .await;
            }
            Err(Error::Server {
                status: 404 | 410, ..
            }) => state.current = None,
            Err(error) => return Err(error),
        }
    }

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
    let into = state.share.as_ref();
    match into {
        None => query.extend(sealing.query()),
        Some(share) => query.push(("code", share.code.clone())),
    }
    let management = into.and_then(|share| share.management_token.as_deref());
    // Signed in, the account already says whose share it is.
    let management = management.filter(|_| !client.signed_in());
    let upload = client.open_upload(&query, declared, management).await?;
    let current = Current {
        upload,
        nonce: names::random()?,
    };
    state.current = Some(current.clone());
    if let Some(job) = job {
        job.save(state)?;
    }
    send_parts(
        client,
        sealing,
        path,
        fingerprint,
        &name,
        declared,
        &current,
        0,
        progress,
    )
    .await
}

/// Sends the parts of an upload from `offset` on, retrying what the network
/// drops, until the server says the file is whole.
#[expect(
    clippy::too_many_arguments,
    reason = "one upload's worth of context, each piece used once"
)]
async fn send_parts(
    client: &Client,
    sealing: &Sealing,
    path: &Path,
    fingerprint: &Fingerprint,
    name: &str,
    declared: u64,
    current: &Current,
    mut offset: u64,
    progress: Progress<'_>,
) -> Result<Share> {
    let Current { upload, nonce } = current;
    let nonce = *nonce;
    let part = usize::try_from(upload.part_size).unwrap_or(usize::MAX);
    let mut outgoing = Outgoing::open(path, sealing.key(), nonce, fingerprint).await?;
    outgoing.skip(offset).await?;
    progress(name, offset, declared);
    let mut attempt = 0;
    loop {
        let bytes = outgoing.take(part).await?;
        let sent = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        match client.send_part(upload, offset, bytes).await {
            Ok(Some(share)) => {
                progress(name, declared, declared);
                return Ok(share);
            }
            Ok(None) => {
                offset += sent;
                attempt = 0;
                progress(name, offset, declared);
            }
            Err(error) if attempt + 1 < ATTEMPTS && worth_retrying(&error) => {
                attempt += 1;
                tokio::time::sleep(Duration::from_secs(u64::from(attempt))).await;
                offset = resume_at(client, upload).await?;
                // Sealed again from the start: the same key, nonce and content
                // give the same bytes, so what the server already has lines up.
                outgoing = Outgoing::open(path, sealing.key(), nonce, fingerprint).await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_keeps_its_name_without_what_is_left_out() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let photos = dir.path().join("photos");
        std::fs::create_dir_all(photos.join("raw")).expect("folders");
        for name in ["a.jpg", "b.jpg", "raw/c.dng"] {
            std::fs::write(photos.join(name), "x").expect("a file");
        }
        let leave_out = [photos.join("b.jpg"), photos.join("raw")];
        let (entries, _) = expand(std::slice::from_ref(&photos), &leave_out, true).expect("files");
        let names: Vec<_> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["photos/a.jpg"]);
    }
}
