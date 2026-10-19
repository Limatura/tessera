//! One folder at a time: what is in it, what is marked, where the cursor is.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// One thing in a folder.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    /// Its name in the folder.
    pub(crate) name: String,
    /// Where it is.
    pub(crate) path: PathBuf,
    /// A folder, or a link to one.
    pub(crate) is_dir: bool,
    /// Bytes, for a file.
    pub(crate) size: Option<u64>,
    /// When it last changed.
    pub(crate) modified: Option<SystemTime>,
}

impl Entry {
    fn hidden(&self) -> bool {
        self.name.starts_with('.')
    }
}

/// A row of the list: the way up, or an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Row {
    /// `..`
    Parent,
    /// The entry at this index.
    Entry(usize),
}

/// What a marked path is, for the count under the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Marked {
    File(u64),
    Folder,
}

/// The folder being looked at.
#[derive(Debug)]
pub(crate) struct Browser {
    dir: PathBuf,
    entries: Vec<Entry>,
    rows: Vec<Row>,
    cursor: usize,
    filter: String,
    show_hidden: bool,
    marked: BTreeMap<PathBuf, Marked>,
    /// Folders gone back from and forward to, as a browser keeps them: each
    /// with the name the cursor was on, to put it there again.
    back: Vec<(PathBuf, Option<String>)>,
    forward: Vec<(PathBuf, Option<String>)>,
    /// Why the folder could not be read, when it could not.
    pub(crate) error: Option<String>,
}

impl Browser {
    /// Looks at `dir`.
    pub(crate) fn new(dir: PathBuf, show_hidden: bool) -> Self {
        let mut browser = Self {
            dir,
            entries: Vec::new(),
            rows: Vec::new(),
            cursor: 0,
            filter: String::new(),
            show_hidden,
            marked: BTreeMap::new(),
            back: Vec::new(),
            forward: Vec::new(),
            error: None,
        };
        browser.reload();
        browser
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    pub(crate) fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub(crate) fn entry(&self, row: Row) -> Option<&Entry> {
        match row {
            Row::Parent => None,
            Row::Entry(index) => self.entries.get(index),
        }
    }

    pub(crate) const fn cursor(&self) -> usize {
        self.cursor
    }

    pub(crate) fn current(&self) -> Option<Row> {
        self.rows.get(self.cursor).copied()
    }

    pub(crate) fn filter(&self) -> &str {
        &self.filter
    }

    pub(crate) const fn show_hidden(&self) -> bool {
        self.show_hidden
    }

    /// Reads the folder again, keeping the cursor on the same name.
    pub(crate) fn reload(&mut self) {
        let keep = self
            .current()
            .and_then(|row| self.entry(row))
            .map(|entry| entry.name.clone());
        match read(&self.dir) {
            Ok(entries) => {
                self.entries = entries;
                self.error = None;
            }
            Err(error) => {
                self.entries = Vec::new();
                self.error = Some(error.to_string());
            }
        }
        self.refilter();
        if let Some(name) = keep {
            self.select(&name);
        }
    }

    /// Goes into `dir`, remembering where it came from for going back.
    pub(crate) fn enter(&mut self, dir: PathBuf) {
        if dir != self.dir {
            let here = self.here();
            self.back.push(here);
            self.forward.clear();
        }
        self.show(dir, None);
    }

    /// Goes to the folder before, as a browser's back button does.
    pub(crate) fn back(&mut self) -> bool {
        let Some((dir, name)) = self.back.pop() else {
            return false;
        };
        let here = self.here();
        self.forward.push(here);
        self.show(dir, name.as_deref());
        true
    }

    /// Undoes going back.
    pub(crate) fn forward(&mut self) -> bool {
        let Some((dir, name)) = self.forward.pop() else {
            return false;
        };
        let here = self.here();
        self.back.push(here);
        self.show(dir, name.as_deref());
        true
    }

    fn here(&self) -> (PathBuf, Option<String>) {
        let name = self
            .current()
            .and_then(|row| self.entry(row))
            .map(|entry| entry.name.clone());
        (self.dir.clone(), name)
    }

    fn show(&mut self, dir: PathBuf, name: Option<&str>) {
        self.dir = dir;
        self.filter.clear();
        self.cursor = 0;
        self.reload();
        if let Some(name) = name {
            self.select(name);
        }
    }

    /// Goes up one folder, with the cursor on the one just left.
    pub(crate) fn up(&mut self) {
        let Some(parent) = self.dir.parent().map(Path::to_path_buf) else {
            return;
        };
        let left = self
            .dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        self.enter(parent);
        if let Some(name) = left {
            self.select(&name);
        }
    }

    /// Puts the cursor on the entry called `name`, if it is listed.
    pub(crate) fn select(&mut self, name: &str) {
        if let Some(at) = self
            .rows
            .iter()
            .position(|row| self.entry(*row).is_some_and(|entry| entry.name == name))
        {
            self.cursor = at;
        }
    }

    pub(crate) fn set_filter(&mut self, filter: String) {
        self.filter = filter;
        self.cursor = 0;
        self.refilter();
    }

    pub(crate) fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        let keep = self
            .current()
            .and_then(|row| self.entry(row))
            .map(|entry| entry.name.clone());
        self.refilter();
        if let Some(name) = keep {
            self.select(&name);
        }
    }

    /// Moves the cursor by `by` rows, stopping at either end.
    pub(crate) fn move_by(&mut self, by: isize) {
        let last = self.rows.len().saturating_sub(1);
        self.cursor = self.cursor.saturating_add_signed(by).min(last);
    }

    pub(crate) fn move_to(&mut self, row: usize) {
        self.cursor = row.min(self.rows.len().saturating_sub(1));
    }

    pub(crate) fn is_marked(&self, path: &Path) -> bool {
        self.marked.contains_key(path)
    }

    /// Marks the row, or unmarks it.
    pub(crate) fn toggle_mark(&mut self, row: Row) {
        let Some(entry) = self.entry(row) else {
            return;
        };
        let path = entry.path.clone();
        let kind = if entry.is_dir {
            Marked::Folder
        } else {
            Marked::File(entry.size.unwrap_or(0))
        };
        if self.marked.remove(&path).is_none() {
            self.marked.insert(path, kind);
        }
    }

    /// Marks every entry listed; when they all are already, unmarks them.
    pub(crate) fn toggle_all(&mut self) {
        let listed: Vec<Row> = self
            .rows
            .iter()
            .copied()
            .filter(|row| *row != Row::Parent)
            .collect();
        let all = listed.iter().all(|row| {
            self.entry(*row)
                .is_some_and(|entry| self.marked.contains_key(&entry.path))
        });
        for row in listed {
            let marked = self
                .entry(row)
                .is_some_and(|entry| self.marked.contains_key(&entry.path));
            if marked == all {
                self.toggle_mark(row);
            }
        }
    }

    /// Marks a path found elsewhere, by search.
    pub(crate) fn toggle_path(&mut self, path: &Path, is_dir: bool) {
        if self.marked.remove(path).is_none() {
            let kind = if is_dir {
                Marked::Folder
            } else {
                Marked::File(std::fs::metadata(path).map_or(0, |meta| meta.len()))
            };
            self.marked.insert(path.to_path_buf(), kind);
        }
    }

    pub(crate) fn clear_marks(&mut self) {
        self.marked.clear();
    }

    /// Everything marked, in path order.
    pub(crate) fn marked(&self) -> impl Iterator<Item = (&PathBuf, Marked)> {
        self.marked.iter().map(|(path, kind)| (path, *kind))
    }

    pub(crate) fn marked_count(&self) -> usize {
        self.marked.len()
    }

    fn refilter(&mut self) {
        let needle = self.filter.to_lowercase();
        self.rows = Vec::with_capacity(self.entries.len() + 1);
        if needle.is_empty() && self.dir.parent().is_some() {
            self.rows.push(Row::Parent);
        }
        self.rows.extend(
            self.entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| self.show_hidden || !entry.hidden())
                .filter(|(_, entry)| {
                    needle.is_empty() || entry.name.to_lowercase().contains(&needle)
                })
                .map(|(index, _)| Row::Entry(index)),
        );
        self.move_by(0);
    }
}

/// What is in `dir`: folders first, then files, each by name ignoring case.
fn read(dir: &Path) -> std::io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for item in std::fs::read_dir(dir)? {
        let Ok(item) = item else {
            continue;
        };
        let path = item.path();
        // Through a link, to what it points at; a broken link is listed as a
        // file that cannot be sent.
        let metadata = std::fs::metadata(&path).or_else(|_| item.metadata()).ok();
        let is_dir = metadata.as_ref().is_some_and(std::fs::Metadata::is_dir);
        entries.push(Entry {
            name: item.file_name().to_string_lossy().into_owned(),
            path,
            is_dir,
            size: metadata
                .as_ref()
                .filter(|_| !is_dir)
                .map(std::fs::Metadata::len),
            modified: metadata.and_then(|meta| meta.modified().ok()),
        });
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a temp dir");
        std::fs::create_dir(dir.path().join("src")).expect("a folder");
        std::fs::write(dir.path().join("src").join("main.rs"), "fn main() {}").expect("a file");
        std::fs::write(dir.path().join("README.md"), "hello").expect("a file");
        std::fs::write(dir.path().join("build.log"), "ok").expect("a file");
        std::fs::write(dir.path().join(".env"), "SECRET=1").expect("a file");
        dir
    }

    fn names(browser: &Browser) -> Vec<String> {
        browser
            .rows()
            .iter()
            .map(|row| {
                browser
                    .entry(*row)
                    .map_or_else(|| "..".to_owned(), |entry| entry.name.clone())
            })
            .collect()
    }

    #[test]
    fn folders_come_first_and_dotfiles_only_when_asked() {
        let dir = tree();
        let mut browser = Browser::new(dir.path().to_path_buf(), false);
        assert_eq!(names(&browser), ["..", "src", "build.log", "README.md"]);
        browser.toggle_hidden();
        assert_eq!(
            names(&browser),
            ["..", "src", ".env", "build.log", "README.md"]
        );
    }

    #[test]
    fn a_filter_keeps_matching_names_in_any_case() {
        let dir = tree();
        let mut browser = Browser::new(dir.path().to_path_buf(), false);
        browser.set_filter("read".to_owned());
        assert_eq!(names(&browser), ["README.md"]);
        browser.set_filter(String::new());
        assert_eq!(names(&browser).len(), 4);
    }

    #[test]
    fn going_up_lands_on_the_folder_left() {
        let dir = tree();
        let mut browser = Browser::new(dir.path().join("src"), false);
        assert_eq!(names(&browser), ["..", "main.rs"]);
        browser.up();
        assert_eq!(browser.dir(), dir.path());
        let current = browser.current().and_then(|row| browser.entry(row));
        assert_eq!(current.map(|entry| entry.name.as_str()), Some("src"));
    }

    #[test]
    fn back_and_forward_retrace_the_folders_visited() {
        let dir = tree();
        let root = dir.path().to_path_buf();
        let mut browser = Browser::new(root.clone(), false);
        browser.select("README.md");
        browser.enter(root.join("src"));
        browser.up();
        assert_eq!(browser.dir(), root);
        assert!(browser.back());
        assert_eq!(browser.dir(), root.join("src"));
        assert!(browser.back());
        assert_eq!(browser.dir(), root);
        let current = browser.current().and_then(|row| browser.entry(row));
        assert_eq!(current.map(|entry| entry.name.as_str()), Some("README.md"));
        assert!(!browser.back());
        assert!(browser.forward());
        assert_eq!(browser.dir(), root.join("src"));
        // Going somewhere new forgets the way forward.
        browser.enter(root);
        assert!(!browser.forward());
    }

    #[test]
    fn marks_outlive_leaving_the_folder() {
        let dir = tree();
        let mut browser = Browser::new(dir.path().to_path_buf(), false);
        browser.select("README.md");
        browser.toggle_mark(browser.current().expect("a row"));
        browser.enter(dir.path().join("src"));
        browser.toggle_all();
        let marked: Vec<_> = browser.marked().collect();
        assert_eq!(
            marked,
            [
                (&dir.path().join("README.md"), Marked::File(5)),
                (&dir.path().join("src").join("main.rs"), Marked::File(12)),
            ]
        );
        browser.toggle_all();
        assert_eq!(browser.marked_count(), 1);
    }

    #[test]
    fn the_cursor_stays_on_the_list() {
        let dir = tree();
        let mut browser = Browser::new(dir.path().to_path_buf(), false);
        browser.move_by(-5);
        assert_eq!(browser.cursor(), 0);
        browser.move_by(50);
        assert_eq!(browser.cursor(), 3);
        browser.set_filter("zzz".to_owned());
        assert_eq!(browser.current(), None);
    }
}
