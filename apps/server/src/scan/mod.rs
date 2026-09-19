//! Handing uploads to a malware scanner.
//!
//! The scanner is somebody else's program, reached over the network: ClamAV's
//! daemon, or anything that speaks the small HTTP contract in [`http`]. See
//! [`crate::config::ScanConfig`] for why it is not built in, and
//! `docs/scanning.md` for the whole arrangement.
//!
//! # The order of events
//!
//! A file is recorded as `pending` the moment it is stored. [`worker`] reads it
//! back and sends it to the scanner; until the verdict is written, its share
//! is not handed out. So a scan never happens inside the request that
//! uploaded the file, which matters twice over: a large file takes minutes,
//! and a file sent straight to a bucket never passed through this process at
//! all.

pub mod clamd;
pub mod http;
pub mod worker;

use bytes::Bytes;
use tokio_stream::Stream;

use crate::{
    config::ScanConfig,
    db::repository::{
        scan::{Scan, Skip},
        share::Content,
    },
    error::{Error, Result},
    state::AppState,
};

/// How long a share held for scanning tells a client to wait before asking
/// again. Most files are scanned in well under a second.
pub const RETRY_AFTER_SECS: u64 = 3;

/// What a scanner said about one stream of bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing found.
    Clean,
    /// A threat, by the scanner's name for it.
    Infected(String),
    /// More than the scanner will take.
    TooLarge,
}

/// Why no verdict came back.
///
/// Every cause is treated alike: the file is handed out and asked about again
/// later. Which one it was only matters to whoever reads the log.
#[derive(Debug, thiserror::Error)]
pub enum Unreachable {
    /// Connecting, sending or reading failed.
    #[error("could not talk to the scanner: {0}")]
    Io(#[from] std::io::Error),
    /// The HTTP request failed.
    #[error("could not talk to the scanner: {0}")]
    Http(#[from] reqwest::Error),
    /// The scanner answered something this does not understand.
    #[error("the scanner said something unexpected: {0}")]
    Answer(String),
    /// It took longer than [`worker::SCAN_TIMEOUT`].
    #[error("the scanner did not answer in time")]
    Timeout,
    /// The file could not be read back from the store to be sent.
    #[error("the file could not be read from the store: {0}")]
    Store(String),
}

/// Where a new file or note starts: `None` when no scanner is configured, so
/// nothing is held for one; `skipped` when the content is sealed and there is
/// nothing a scanner could read; `pending` otherwise.
#[must_use]
pub fn initial(state: &AppState, encrypted: bool) -> Option<Scan> {
    if !state.config().scan.is_enabled() {
        return None;
    }
    Some(if encrypted {
        Scan::Skipped(Skip::Encrypted)
    } else {
        Scan::Pending
    })
}

/// Refuses to hand out content still waiting for a verdict.
///
/// A share recorded while scanning was on and still pending when it was
/// switched off is not held: nothing is left to deliver the verdict.
///
/// # Errors
///
/// [`Error::Scanning`] when it is held.
pub fn release(state: &AppState, content: &Content) -> Result<()> {
    if state.config().scan.is_enabled() && content.awaiting_scan() {
        return Err(Error::Scanning {
            retry_after_secs: RETRY_AFTER_SECS,
        });
    }
    Ok(())
}

/// Sends `body`, `length` bytes long, to the scanner `config` names.
///
/// # Errors
///
/// [`Unreachable`] when no verdict came back, for whatever reason.
pub async fn scan<S, E>(config: &ScanConfig, body: S, length: u64) -> Result<Verdict, Unreachable>
where
    S: Stream<Item = Result<Bytes, E>> + Send + Unpin + 'static,
    E: std::fmt::Display + Send + 'static,
{
    let url = url::Url::parse(&config.url)
        .map_err(|_| Unreachable::Answer(format!("`{}` is not a URL", config.url)))?;
    let work = async {
        if url.scheme() == "clamd" {
            let host = url.host_str().unwrap_or_default();
            let port = url.port().unwrap_or(clamd::DEFAULT_PORT);
            clamd::scan(&format!("{host}:{port}"), body).await
        } else {
            http::scan(url, &config.authorization, body, length).await
        }
    };
    tokio::time::timeout(worker::SCAN_TIMEOUT, work)
        .await
        .map_err(|_| Unreachable::Timeout)?
}
