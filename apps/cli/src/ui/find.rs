//! Finding a file anywhere under a folder by a few letters of its path, the
//! way fzf does: the walk runs on its own thread, and what it has found so far
//! is matched as it arrives.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
};

use nucleo_matcher::{
    Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};

/// How many paths a walk collects before it stops: enough for any project,
/// and a bound when the folder turns out to be `/`.
const LIMIT: usize = 200_000;
/// How many matches are kept in order, best first.
const SHOWN: usize = 1_000;
/// How many paths travel from the walk together.
const BATCH: usize = 256;

/// A path found by the walk.
#[derive(Debug, Clone)]
pub(crate) struct Found {
    /// Below the folder searched, `/`-separated.
    pub(crate) path: String,
    pub(crate) is_dir: bool,
}

enum Message {
    Batch(Vec<Found>),
    Capped,
}

/// A search under one folder.
pub(crate) struct Finder {
    root: PathBuf,
    query: String,
    found: Vec<Found>,
    shown: Vec<usize>,
    cursor: usize,
    incoming: Option<Receiver<Message>>,
    stop: Arc<AtomicBool>,
    capped: bool,
    matcher: Matcher,
}

impl std::fmt::Debug for Finder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Finder")
            .field("root", &self.root)
            .field("query", &self.query)
            .field("found", &self.found.len())
            .finish_non_exhaustive()
    }
}

impl Finder {
    /// Starts walking `root`, skipping what `.gitignore` files skip, and
    /// dotfiles unless `show_hidden`.
    pub(crate) fn start(root: PathBuf, show_hidden: bool) -> Self {
        let (send, incoming) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let walk = WalkJob {
            root: root.clone(),
            show_hidden,
            stop: Arc::clone(&stop),
        };
        // A thread that cannot start leaves an empty search, which says so.
        std::thread::Builder::new()
            .name("find".to_owned())
            .spawn(move || walk.run(&send))
            .ok();
        Self {
            root,
            query: String::new(),
            found: Vec::new(),
            shown: Vec::new(),
            cursor: 0,
            incoming: Some(incoming),
            stop,
            capped: false,
            matcher: Matcher::new(nucleo_matcher::Config::DEFAULT.match_paths()),
        }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    /// Whether the walk is still going.
    pub(crate) const fn walking(&self) -> bool {
        self.incoming.is_some()
    }

    /// Whether the walk stopped at its limit, before the end.
    pub(crate) const fn capped(&self) -> bool {
        self.capped
    }

    pub(crate) const fn total(&self) -> usize {
        self.found.len()
    }

    /// The matches, best first.
    pub(crate) fn shown(&self) -> impl Iterator<Item = &Found> {
        self.shown.iter().filter_map(|index| self.found.get(*index))
    }

    /// How many matches there are.
    pub(crate) const fn matches(&self) -> usize {
        self.shown.len()
    }

    pub(crate) const fn cursor(&self) -> usize {
        self.cursor
    }

    pub(crate) fn current(&self) -> Option<&Found> {
        self.shown
            .get(self.cursor)
            .and_then(|index| self.found.get(*index))
    }

    pub(crate) fn move_by(&mut self, by: isize) {
        let last = self.shown.len().saturating_sub(1);
        self.cursor = self.cursor.saturating_add_signed(by).min(last);
    }

    pub(crate) fn move_to(&mut self, row: usize) {
        self.cursor = row.min(self.shown.len().saturating_sub(1));
    }

    pub(crate) fn set_query(&mut self, query: String) {
        self.query = query;
        self.cursor = 0;
        self.rematch();
    }

    /// Where a match is on disk.
    pub(crate) fn absolute(&self, found: &Found) -> PathBuf {
        self.root.join(&found.path)
    }

    /// Takes what the walk has found since last asked. Says whether anything
    /// changed.
    pub(crate) fn poll(&mut self) -> bool {
        let Some(incoming) = &self.incoming else {
            return false;
        };
        let mut changed = false;
        loop {
            match incoming.try_recv() {
                Ok(Message::Batch(batch)) => {
                    self.found.extend(batch);
                    changed = true;
                }
                Ok(Message::Capped) => self.capped = true,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.incoming = None;
                    changed = true;
                    break;
                }
            }
        }
        if changed {
            self.rematch();
        }
        changed
    }

    fn rematch(&mut self) {
        let keep = self.current().map(|found| found.path.clone());
        if self.query.is_empty() {
            self.shown = (0..self.found.len().min(SHOWN)).collect();
        } else {
            let pattern = Pattern::parse(&self.query, CaseMatching::Smart, Normalization::Smart);
            let mut buffer = Vec::new();
            let mut scored: Vec<(u32, usize)> = self
                .found
                .iter()
                .enumerate()
                .filter_map(|(index, found)| {
                    let haystack = Utf32Str::new(&found.path, &mut buffer);
                    pattern
                        .score(haystack, &mut self.matcher)
                        .map(|score| (score, index))
                })
                .collect();
            // Best first; between equals, the shorter path, then walk order.
            let length = |index: usize| self.found.get(index).map_or(0, |found| found.path.len());
            scored.sort_by(|a, b| {
                b.0.cmp(&a.0)
                    .then_with(|| length(a.1).cmp(&length(b.1)))
                    .then_with(|| a.1.cmp(&b.1))
            });
            scored.truncate(SHOWN);
            self.shown = scored.into_iter().map(|(_, index)| index).collect();
        }
        // New paths arriving do not move the cursor off what it is on.
        self.cursor = keep
            .and_then(|path| {
                self.shown.iter().position(|index| {
                    self.found
                        .get(*index)
                        .is_some_and(|found| found.path == path)
                })
            })
            .unwrap_or(0);
    }
}

impl Drop for Finder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

struct WalkJob {
    root: PathBuf,
    show_hidden: bool,
    stop: Arc<AtomicBool>,
}

impl WalkJob {
    fn run(self, send: &mpsc::Sender<Message>) {
        let walk = ignore::WalkBuilder::new(&self.root)
            .hidden(!self.show_hidden)
            // `.gitignore` counts outside a repository too: a folder copied
            // out of one still has its `target` and `node_modules`.
            .require_git(false)
            .follow_links(false)
            .build();
        let mut batch = Vec::with_capacity(BATCH);
        let mut count = 0;
        for item in walk {
            if self.stop.load(Ordering::Relaxed) {
                return;
            }
            let Ok(item) = item else {
                continue;
            };
            if item.depth() == 0 {
                continue;
            }
            let Ok(below) = item.path().strip_prefix(&self.root) else {
                continue;
            };
            let path = below
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            batch.push(Found {
                path,
                is_dir: item.file_type().is_some_and(|kind| kind.is_dir()),
            });
            count += 1;
            if batch.len() == BATCH {
                let full = std::mem::replace(&mut batch, Vec::with_capacity(BATCH));
                if send.send(Message::Batch(full)).is_err() {
                    return;
                }
            }
            if count == LIMIT {
                send.send(Message::Capped).ok();
                break;
            }
        }
        send.send(Message::Batch(batch)).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finished(root: &Path, show_hidden: bool) -> Finder {
        let mut finder = Finder::start(root.to_path_buf(), show_hidden);
        while finder.walking() {
            finder.poll();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        finder
    }

    fn shown(finder: &Finder) -> Vec<&str> {
        finder.shown().map(|found| found.path.as_str()).collect()
    }

    #[test]
    fn a_few_letters_find_a_deep_file() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let deep = dir.path().join("apps").join("cli").join("src");
        std::fs::create_dir_all(&deep).expect("folders");
        std::fs::write(deep.join("send.rs"), "").expect("a file");
        std::fs::write(dir.path().join("sender.txt"), "").expect("a file");
        std::fs::write(dir.path().join("notes.md"), "").expect("a file");
        let mut finder = finished(dir.path(), false);
        assert_eq!(finder.total(), 6);
        finder.set_query("clisend".to_owned());
        assert_eq!(shown(&finder), ["apps/cli/src/send.rs"]);
        finder.set_query("send".to_owned());
        assert_eq!(shown(&finder).len(), 2);
        let current = finder.current().expect("a match");
        assert_eq!(finder.absolute(current), dir.path().join(&current.path),);
    }

    #[test]
    fn what_gitignore_skips_is_not_searched() {
        let dir = tempfile::tempdir().expect("a temp dir");
        std::fs::write(dir.path().join(".gitignore"), "target/\n").expect("a file");
        std::fs::create_dir_all(dir.path().join("target").join("debug")).expect("folders");
        std::fs::write(dir.path().join("target").join("debug").join("tess"), "").expect("a file");
        std::fs::write(dir.path().join("Cargo.toml"), "").expect("a file");
        std::fs::write(dir.path().join(".env"), "").expect("a file");
        let finder = finished(dir.path(), false);
        assert_eq!(shown(&finder), ["Cargo.toml"]);
        let finder = finished(dir.path(), true);
        let mut all = shown(&finder);
        all.sort_unstable();
        assert_eq!(all, [".env", ".gitignore", "Cargo.toml"]);
    }
}
