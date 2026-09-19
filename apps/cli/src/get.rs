//! Fetching a share: every file into a directory, or a note's text.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use futures_util::StreamExt as _;
use tessera_crypto::{HEADER_LEN, Opener, Salt, ShareKey};
use tokio::io::AsyncWriteExt as _;
use url::Url;

use crate::{
    Error, Result,
    client::{Client, Described},
    names::{self, SealedName},
    send::Progress,
};

/// How many times one file's download is resumed before giving up.
const ATTEMPTS: u32 = 5;

/// What was fetched.
#[derive(Debug, Default)]
pub struct Fetched {
    /// Where each file was written, in the share's order.
    pub files: Vec<PathBuf>,
    /// A note's text, which is not written anywhere.
    pub note: Option<Vec<u8>>,
}

/// Fetches the share `code`, opening it with `key` or a key from `password`.
///
/// Spends one retrieval, however many files it holds.
///
/// # Errors
///
/// [`Error::PasswordRequired`] when it has a password and none was given, and
/// otherwise when the share cannot be had or a file cannot be written.
pub async fn get(
    client: &Client,
    code: &str,
    key: Option<ShareKey>,
    password: Option<&str>,
    output: &Path,
    progress: Progress<'_>,
) -> Result<Fetched> {
    let first = client.describe(code, None).await?;
    let (key, presented) = keys(&first, key, password).await?;
    if first.has_password && presented.is_none() {
        return Err(Error::PasswordRequired);
    }
    let described = if first.unlocked {
        first
    } else {
        client.describe(code, presented.as_deref()).await?
    };
    let ticket = unlock_when_scanned(client, code, presented.as_deref()).await?;

    if described.kind == "text" {
        // `/raw` refuses a sealed note, and the bytes are the same either way.
        let route = if key.is_some() { "download" } else { "raw" };
        let mut url = client.url(&format!("/s/{code}/{route}"))?;
        url.query_pairs_mut().append_pair("t", &ticket);
        let bytes = client.download(url, 0).await?.bytes().await?;
        let note = match &key {
            Some(key) => tessera_crypto::open(key, &bytes)?,
            None => bytes.to_vec(),
        };
        return Ok(Fetched {
            files: Vec::new(),
            note: Some(note),
        });
    }

    let mut fetched = Fetched::default();
    for (position, file) in described.files.iter().enumerate() {
        let name = match (&key, &file.sealed_name, &file.filename) {
            (Some(key), Some(sealed), _) => names::open::<SealedName>(key, sealed)?.name,
            (None, _, Some(name)) => name.clone(),
            _ => format!("{code}-{position}"),
        };
        let path = names::place(output, &name)
            .unwrap_or_else(|| output.join(format!("{code}-{position}")));
        if tokio::fs::try_exists(&path).await? {
            return Err(Error::Usage(format!(
                "`{}` already exists; choose another directory with --output",
                path.display()
            )));
        }
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut url = client.url(&format!("/s/{code}/download"))?;
        url.query_pairs_mut()
            .append_pair("file", &position.to_string())
            .append_pair("t", &ticket);
        let incoming = Incoming::new(key.as_ref(), file.size_bytes);
        fetch_file(client, url, incoming, &path, &name, progress).await?;
        fetched.files.push(path);
    }
    Ok(fetched)
}

/// How long to wait for an instance's malware scanner before giving up.
const SCAN_PATIENCE: Duration = Duration::from_mins(5);

/// Unlocks the share, waiting while the instance is still scanning it.
///
/// A share is held from download until its scan comes back, which the server
/// answers with `423` and a short `Retry-After`. Most scans take a second, so
/// waiting here saves a person running the command again.
async fn unlock_when_scanned(
    client: &Client,
    code: &str,
    password: Option<&str>,
) -> Result<String> {
    let started = std::time::Instant::now();
    loop {
        match client.unlock(code, password).await {
            Err(Error::Server { status: 423, .. }) if started.elapsed() < SCAN_PATIENCE => {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            other => return other,
        }
    }
}

/// The key to open the share with, and what to present as its password.
async fn keys(
    share: &Described,
    key: Option<ShareKey>,
    password: Option<&str>,
) -> Result<(Option<ShareKey>, Option<String>)> {
    match share.encryption.as_deref() {
        None => Ok((None, password.map(str::to_owned))),
        Some("link") => {
            let key = key.ok_or_else(|| {
                Error::Usage(
                    "this share is end-to-end encrypted and its key is the part of the link \
                     after `#`; ask for the whole link"
                        .to_owned(),
                )
            })?;
            Ok((Some(key), password.map(str::to_owned)))
        }
        Some("password") => {
            let password = password.ok_or(Error::PasswordRequired)?.to_owned();
            let salt = Salt::parse(share.key_salt.as_deref().unwrap_or_default())?;
            let key =
                tokio::task::spawn_blocking(move || ShareKey::from_password(&password, &salt))
                    .await
                    .map_err(|error| Error::Io(std::io::Error::other(error)))??;
            let token = key.access_token();
            Ok((Some(key), Some(token)))
        }
        Some(other) => Err(Error::Protocol(format!(
            "this share is sealed in a way this version does not know: `{other}`"
        ))),
    }
}

/// Writes one file to `path`, resuming where a dropped connection left off.
async fn fetch_file(
    client: &Client,
    url: Url,
    mut incoming: Incoming<'_>,
    path: &Path,
    name: &str,
    progress: Progress<'_>,
) -> Result<()> {
    let partial = path.with_file_name(format!(
        "{}.part",
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let mut out = tokio::fs::File::create(&partial).await?;
    let mut attempt = 0;
    loop {
        match incoming
            .pull(client, url.clone(), &mut out, name, progress)
            .await
        {
            Ok(()) => break,
            Err(error) if attempt + 1 < ATTEMPTS && matches!(error, Error::Http(_)) => {
                attempt += 1;
                tokio::time::sleep(Duration::from_secs(u64::from(attempt))).await;
            }
            Err(error) => return Err(error),
        }
    }
    out.flush().await?;
    drop(out);
    tokio::fs::rename(&partial, path).await?;
    Ok(())
}

/// A download in progress, opened as it arrives when it is sealed.
struct Incoming<'a> {
    key: Option<&'a ShareKey>,
    size: u64,
    opener: Option<Opener>,
    next: u64,
    written: u64,
    buffer: Vec<u8>,
}

impl<'a> Incoming<'a> {
    const fn new(key: Option<&'a ShareKey>, size: u64) -> Self {
        Self {
            key,
            size,
            opener: None,
            next: 0,
            written: 0,
            buffer: Vec::new(),
        }
    }

    /// Where in the stored object to carry on from.
    const fn resume_from(&self) -> u64 {
        match (&self.key, &self.opener) {
            (None, _) => self.written,
            (Some(_), None) => 0,
            (Some(_), Some(opener)) => match opener.locate(self.next) {
                Ok((start, _)) => start,
                // Every chunk is in; nothing is left to ask for.
                Err(_) => self.size,
            },
        }
    }

    async fn pull(
        &mut self,
        client: &Client,
        url: Url,
        out: &mut tokio::fs::File,
        name: &str,
        progress: Progress<'_>,
    ) -> Result<()> {
        let from = self.resume_from();
        self.buffer.clear();
        if from < self.size || self.size == 0 {
            let response = client.download(url, from).await?;
            if from > 0 && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                return Err(Error::Protocol(
                    "the server would not resume the download".to_owned(),
                ));
            }
            let mut body = response.bytes_stream();
            while let Some(piece) = body.next().await {
                let piece = piece?;
                match self.key {
                    None => {
                        out.write_all(&piece).await?;
                        self.written += u64::try_from(piece.len()).unwrap_or(0);
                    }
                    Some(key) => {
                        self.buffer.extend_from_slice(&piece);
                        self.open_what_arrived(key, out).await?;
                    }
                }
                progress(name, from.max(self.written), self.size);
            }
        }
        self.finish()
    }

    async fn open_what_arrived(&mut self, key: &ShareKey, out: &mut tokio::fs::File) -> Result<()> {
        if self.opener.is_none() {
            let Some(header) = self.buffer.get(..HEADER_LEN) else {
                return Ok(());
            };
            self.opener = Some(Opener::new(key, header, self.size)?);
            self.buffer.drain(..HEADER_LEN);
        }
        let Some(opener) = &self.opener else {
            return Ok(());
        };
        while self.next < opener.chunks() {
            let (_, len) = opener.locate(self.next)?;
            let len = usize::try_from(len).unwrap_or(usize::MAX);
            let Some(sealed) = self.buffer.get(..len) else {
                break;
            };
            let plain = opener.open(self.next, sealed)?;
            out.write_all(&plain).await?;
            self.written += u64::try_from(plain.len()).unwrap_or(0);
            self.buffer.drain(..len);
            self.next += 1;
        }
        Ok(())
    }

    fn finish(&self) -> Result<()> {
        let complete = match (&self.key, &self.opener) {
            (None, _) => self.written == self.size,
            (Some(_), Some(opener)) => self.next == opener.chunks() && self.buffer.is_empty(),
            (Some(_), None) => false,
        };
        if complete {
            Ok(())
        } else {
            Err(Error::Protocol("the download ended early".to_owned()))
        }
    }
}
