//! The server's log, kept on disk so the panel can read it.
//!
//! stdout is still the log: `telemetry.*` decides what goes there, and a
//! deployment collects it however it likes. Every line that reaches stdout
//! also passes through [`FileLayer`], which queues it for a writer thread.
//! The writer appends it to `<data_dir>/logs/YYYY-MM-DD.jsonl` (UTC) and
//! announces it to whoever is watching live.
//!
//! The file never costs the server anything it needs. A full queue drops lines
//! instead of making a request wait, and a write that fails is reported once
//! on stdout, then retried with each line.
//!
//! What a line may carry is decided where it is logged. Two nets catch what
//! slips through: a field named like a credential loses its value (see
//! [`FileLayer`]), and a request is recorded by the route it matched, not the path
//! it asked for, so retrieval codes stay out (see `api::make_span`).

mod entry;
mod layer;
mod read;
mod writer;

use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) use entry::DAY;
pub use entry::{Cursor, Entry, Filter};
pub use layer::FileLayer;
pub use read::{DayFile, Page, Query};
use time::Date;
use tokio::sync::broadcast;

use crate::topic::Topic;

/// How many lines a live reader may fall behind before it misses some.
const LIVE_BUFFER: usize = 1024;

/// Where the files are, and the lines being written to them.
#[derive(Debug, Clone)]
pub struct Logs {
    dir: Arc<PathBuf>,
    live: Topic<Arc<Entry>>,
}

impl Logs {
    /// The files under `logs/` in `data_dir`. Nothing is created until the
    /// first line is written.
    #[must_use]
    pub fn in_data_dir(data_dir: &str) -> Self {
        Self {
            dir: Arc::new(Path::new(data_dir).join("logs")),
            live: Topic::with_capacity(LIVE_BUFFER),
        }
    }

    /// The directory the files are in.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where a day's file is, whether or not it exists.
    ///
    /// Built from a [`Date`] and nothing else, so a request naming a day can
    /// never name some other file.
    #[must_use]
    pub fn file_for(&self, date: Date) -> PathBuf {
        let day = date.format(DAY).unwrap_or_default();
        self.dir.join(format!("{day}.jsonl"))
    }

    /// Every line written from now on.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Entry>> {
        self.live.subscribe()
    }

    fn publish(&self, entry: Arc<Entry>) {
        self.live.publish(entry);
    }

    /// The days there are files for, newest first.
    ///
    /// # Errors
    ///
    /// When the directory cannot be listed.
    pub async fn days(&self) -> io::Result<Vec<DayFile>> {
        let dir = Arc::clone(&self.dir);
        blocking(move || read::days(&dir)).await
    }

    /// One page of lines, newest first.
    ///
    /// # Errors
    ///
    /// When a file cannot be listed or read.
    pub async fn page(&self, query: Query) -> io::Result<Page> {
        let dir = Arc::clone(&self.dir);
        blocking(move || read::page(&dir, &query)).await
    }

    /// Deletes the files for days before `first_kept`, and says how many went.
    ///
    /// # Errors
    ///
    /// When the directory cannot be listed. A file that will not delete is
    /// skipped and tried again next time.
    pub async fn remove_before(&self, first_kept: Date) -> io::Result<usize> {
        let dir = Arc::clone(&self.dir);
        blocking(move || {
            let mut removed = 0;
            for day in read::days(&dir)? {
                if day.date < first_kept && std::fs::remove_file(day.path()).is_ok() {
                    removed += 1;
                }
            }
            Ok(removed)
        })
        .await
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> io::Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(io::Error::other)?
}
