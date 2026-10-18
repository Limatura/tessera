//! Reading the files back: which days exist, and a page of lines, newest first.
//!
//! These block on the disk, so callers run them off the runtime.

use std::{
    fs::File,
    io::{self, Read as _, Seek as _, SeekFrom},
    ops::ControlFlow,
    path::{Path, PathBuf},
};

use time::{Date, OffsetDateTime, format_description::well_known::Rfc3339};

use super::entry::{Cursor, DAY, Entry, Filter};

/// How much one query may read before it stops and hands back a cursor. A
/// rare match in a large file is then found over several requests instead of
/// one that reads everything.
const BUDGET: u64 = 64 * 1024 * 1024;

/// How much is read at a time, walking backwards.
const CHUNK: u64 = 64 * 1024;

/// A line longer than this is skipped. Nothing logs one; this only keeps a
/// corrupt file from being read into memory whole.
const LONGEST_LINE: usize = 1024 * 1024;

/// One day's file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayFile {
    /// Which day it holds, in UTC.
    pub date: Date,
    /// How large it is.
    pub size_bytes: u64,
    path: PathBuf,
}

impl DayFile {
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

/// The days there are files for, newest first. No directory means no files.
///
/// # Errors
///
/// When the directory exists but cannot be listed.
pub(super) fn days(dir: &Path) -> io::Result<Vec<DayFile>> {
    let listing = match std::fs::read_dir(dir) {
        Ok(listing) => listing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut days = Vec::new();
    for item in listing {
        let item = item?;
        let name = item.file_name();
        let Some(date) = name
            .to_str()
            .and_then(|name| name.strip_suffix(".jsonl"))
            .and_then(|day| Date::parse(day, DAY).ok())
        else {
            continue;
        };
        let metadata = item.metadata()?;
        if metadata.is_file() {
            days.push(DayFile {
                date,
                size_bytes: metadata.len(),
                path: item.path(),
            });
        }
    }
    days.sort_by_key(|day| core::cmp::Reverse(day.date));
    Ok(days)
}

/// What to read.
#[derive(Debug, Clone, Default)]
pub struct Query {
    /// Nothing logged before this.
    pub since: Option<OffsetDateTime>,
    /// Nothing logged after this.
    pub until: Option<OffsetDateTime>,
    /// Which lines.
    pub filter: Filter,
    /// At most this many.
    pub limit: usize,
    /// Only lines before this one: where the previous page stopped.
    pub cursor: Option<Cursor>,
}

/// Lines, newest first, and where to carry on.
#[derive(Debug, Default)]
pub struct Page {
    /// What matched.
    pub entries: Vec<Entry>,
    /// Where the next page starts, or `None` when there is nothing older.
    pub next: Option<Cursor>,
}

/// One page of `query` from the files in `dir`.
///
/// # Errors
///
/// When a file cannot be listed or read.
pub(super) fn page(dir: &Path, query: &Query) -> io::Result<Page> {
    let mut search = Search {
        query,
        entries: Vec::new(),
        budget: BUDGET,
    };
    for day in days(dir)? {
        if query.cursor.is_some_and(|cursor| day.date > cursor.date)
            || query.until.is_some_and(|until| day.date > until.date())
        {
            continue;
        }
        if query.since.is_some_and(|since| day.date < since.date()) {
            break;
        }
        let end = match query.cursor {
            Some(cursor) if cursor.date == day.date => cursor.offset.min(day.size_bytes),
            _ => day.size_bytes,
        };
        let mut file = match File::open(&day.path) {
            Ok(file) => file,
            // Swept between listing and opening.
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        match search.day(&mut file, day.date, end)? {
            Stop::Exhausted => {}
            Stop::Done => break,
            Stop::Paused(cursor) => {
                return Ok(Page {
                    entries: search.entries,
                    next: Some(cursor),
                });
            }
        }
    }
    Ok(Page {
        entries: search.entries,
        next: None,
    })
}

/// Why reading a file stopped.
enum Stop {
    /// It had nothing more before `end`.
    Exhausted,
    /// Nothing older can match.
    Done,
    /// The page is full, or the budget spent; carry on from here.
    Paused(Cursor),
}

struct Search<'query> {
    query: &'query Query,
    entries: Vec<Entry>,
    budget: u64,
}

impl Search<'_> {
    fn day(&mut self, file: &mut File, date: Date, end: u64) -> io::Result<Stop> {
        let mut oldest = end;
        let mut done = false;
        let walked = backwards(file, end, &mut self.budget, &mut |start, line| {
            oldest = start;
            let Ok(entry) = serde_json::from_slice::<Entry>(line) else {
                return ControlFlow::Continue(());
            };
            let Ok(time) = OffsetDateTime::parse(&entry.time, &Rfc3339) else {
                return ControlFlow::Continue(());
            };
            if self.query.until.is_some_and(|until| time > until) {
                return ControlFlow::Continue(());
            }
            if self.query.since.is_some_and(|since| time < since) {
                done = true;
                return ControlFlow::Break(());
            }
            if self.query.filter.matches(&entry) {
                self.entries.push(entry);
                if self.entries.len() >= self.query.limit {
                    return ControlFlow::Break(());
                }
            }
            ControlFlow::Continue(())
        })?;
        let here = Cursor {
            date,
            offset: oldest,
        };
        Ok(match walked {
            Walk::Finished => Stop::Exhausted,
            Walk::Stopped if done => Stop::Done,
            // A full page, or the budget spent.
            Walk::Stopped | Walk::OutOfBudget => Stop::Paused(here),
        })
    }
}

/// How a backwards walk ended.
#[derive(Debug, PartialEq, Eq)]
enum Walk {
    /// Every line before `end` was seen.
    Finished,
    /// The callback asked to stop.
    Stopped,
    /// The budget ran out first.
    OutOfBudget,
}

/// Hands every complete line that ends at or before `end` to `each`, last
/// first, with the offset it starts at and without its newline.
///
/// A line with no newline yet is being written and is left out. `end` is
/// either a file's length or a line's start, so only the former can cut one.
fn backwards(
    file: &mut File,
    end: u64,
    budget: &mut u64,
    each: &mut dyn FnMut(u64, &[u8]) -> ControlFlow<()>,
) -> io::Result<Walk> {
    // `pending` holds the bytes from `start` that have been read but not yet
    // handed out. Once `aligned`, it ends just after a newline.
    let mut start = end;
    let mut pending: Vec<u8> = Vec::new();
    let mut aligned = false;
    loop {
        if start > 0 {
            if *budget == 0 {
                return Ok(Walk::OutOfBudget);
            }
            let size = CHUNK.min(start);
            start -= size;
            let mut chunk = vec![0; usize::try_from(size).unwrap_or(0)];
            file.seek(SeekFrom::Start(start))?;
            file.read_exact(&mut chunk)?;
            *budget = budget.saturating_sub(size);
            chunk.extend_from_slice(&pending);
            pending = chunk;
        }

        if !aligned {
            match pending.iter().rposition(|byte| *byte == b'\n') {
                Some(newline) => {
                    pending.truncate(newline + 1);
                    aligned = true;
                }
                None if start == 0 => return Ok(Walk::Finished),
                None => {
                    pending.clear();
                    continue;
                }
            }
        }

        // Every line whose start is now known, newest first.
        let mut line_end = pending.len();
        loop {
            let body_end = line_end - 1;
            let body = pending.get(..body_end).unwrap_or_default();
            match body.iter().rposition(|byte| *byte == b'\n') {
                Some(newline) => {
                    let line_start = newline + 1;
                    let offset = start + u64::try_from(line_start).unwrap_or(0);
                    let line = pending.get(line_start..body_end).unwrap_or_default();
                    if each(offset, line).is_break() {
                        return Ok(Walk::Stopped);
                    }
                    line_end = line_start;
                }
                None if start == 0 => {
                    return Ok(if each(0, body).is_break() {
                        Walk::Stopped
                    } else {
                        Walk::Finished
                    });
                }
                None => break,
            }
        }
        pending.truncate(line_end);
        if pending.len() > LONGEST_LINE {
            pending.clear();
            aligned = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Write as _, ops::ControlFlow};

    use time::macros::{date, datetime};

    use super::{Query, Walk, backwards, page};
    use crate::{
        config::LogLevel,
        log_files::entry::{Cursor, Entry, Filter},
    };

    fn lines_of(contents: &[u8], end: u64) -> (Vec<(u64, String)>, Walk) {
        let mut file = tempfile::tempfile().expect("temp file");
        file.write_all(contents).expect("write");
        let mut seen = Vec::new();
        let mut budget = u64::MAX;
        let walk = backwards(&mut file, end, &mut budget, &mut |start, line| {
            seen.push((start, String::from_utf8_lossy(line).into_owned()));
            ControlFlow::Continue(())
        })
        .expect("walk");
        (seen, walk)
    }

    #[test]
    fn lines_come_back_last_first_with_their_offsets() {
        let (seen, walk) = lines_of(b"one\ntwo\nthree\n", 14);
        assert_eq!(
            seen,
            [
                (8, "three".to_owned()),
                (4, "two".to_owned()),
                (0, "one".to_owned())
            ]
        );
        assert_eq!(walk, Walk::Finished);
    }

    #[test]
    fn a_line_still_being_written_is_left_out() {
        let (seen, _) = lines_of(b"one\ntwo\nthr", 11);
        assert_eq!(seen, [(4, "two".to_owned()), (0, "one".to_owned())]);
    }

    #[test]
    fn a_cursor_reads_only_what_comes_before_it() {
        let (seen, _) = lines_of(b"one\ntwo\nthree\n", 8);
        assert_eq!(seen, [(4, "two".to_owned()), (0, "one".to_owned())]);
    }

    #[test]
    fn lines_across_chunk_boundaries_come_back_whole() {
        let mut contents = Vec::new();
        let mut expected = Vec::new();
        for index in 0..20_000_u64 {
            let offset = u64::try_from(contents.len()).expect("fits");
            let line = format!("line number {index}");
            contents.extend_from_slice(line.as_bytes());
            contents.push(b'\n');
            expected.push((offset, line));
        }
        expected.reverse();
        let end = u64::try_from(contents.len()).expect("fits");
        let (seen, walk) = lines_of(&contents, end);
        assert_eq!(seen, expected);
        assert_eq!(walk, Walk::Finished);
    }

    #[test]
    fn an_empty_file_has_no_lines() {
        let (seen, walk) = lines_of(b"", 0);
        assert!(seen.is_empty());
        assert_eq!(walk, Walk::Finished);
    }

    fn write_day(dir: &std::path::Path, day: &str, entries: &[(&str, LogLevel, &str)]) {
        let mut contents = Vec::new();
        for (time, level, message) in entries {
            let entry = Entry {
                id: format!("{day}:{}", contents.len()),
                time: (*time).to_owned(),
                level: *level,
                target: "tessera_server".to_owned(),
                message: (*message).to_owned(),
                fields: serde_json::Map::new(),
            };
            contents.extend(serde_json::to_vec(&entry).expect("json"));
            contents.push(b'\n');
        }
        std::fs::write(dir.join(format!("{day}.jsonl")), contents).expect("write day");
    }

    fn messages(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.message.as_str()).collect()
    }

    fn sample() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp dir");
        write_day(
            dir.path(),
            "2026-10-18",
            &[
                ("2026-10-18T22:00:00Z", LogLevel::Info, "a"),
                ("2026-10-18T23:00:00Z", LogLevel::Warn, "b"),
            ],
        );
        write_day(
            dir.path(),
            "2026-10-19",
            &[
                ("2026-10-19T01:00:00Z", LogLevel::Info, "c"),
                ("2026-10-19T02:00:00Z", LogLevel::Error, "d"),
                ("2026-10-19T03:00:00Z", LogLevel::Info, "e"),
            ],
        );
        std::fs::write(dir.path().join("notes.txt"), "not a day").expect("write stray");
        dir
    }

    #[test]
    fn pages_run_newest_first_across_days_and_end_with_no_cursor() {
        let dir = sample();
        let mut query = Query {
            limit: 2,
            ..Query::default()
        };
        let mut seen = Vec::new();
        loop {
            let page = page(dir.path(), &query).expect("page");
            seen.extend(page.entries.iter().map(|entry| entry.message.clone()));
            match page.next {
                Some(next) => query.cursor = Some(next),
                None => break,
            }
        }
        assert_eq!(seen, ["e", "d", "c", "b", "a"]);
    }

    #[test]
    fn the_cursor_is_the_id_of_the_last_line_handed_out() {
        let dir = sample();
        let page = page(
            dir.path(),
            &Query {
                limit: 2,
                ..Query::default()
            },
        )
        .expect("page");
        let last = page.entries.last().expect("an entry");
        assert_eq!(
            page.next.map(|next| next.to_string()),
            Some(last.id.clone())
        );
    }

    #[test]
    fn level_and_time_narrow_the_page() {
        let dir = sample();
        let warnings = page(
            dir.path(),
            &Query {
                limit: 10,
                filter: Filter::new(Some(LogLevel::Warn), None),
                ..Query::default()
            },
        )
        .expect("page");
        assert_eq!(messages(&warnings.entries), ["d", "b"]);

        let window = page(
            dir.path(),
            &Query {
                limit: 10,
                since: Some(datetime!(2026-10-18 23:00 UTC)),
                until: Some(datetime!(2026-10-19 02:00 UTC)),
                ..Query::default()
            },
        )
        .expect("page");
        assert_eq!(messages(&window.entries), ["d", "c", "b"]);
        assert_eq!(window.next, None);
    }

    #[test]
    fn a_cursor_on_an_older_day_skips_the_newer_ones() {
        let dir = sample();
        let rest = page(
            dir.path(),
            &Query {
                limit: 10,
                cursor: Some(Cursor {
                    date: date!(2026 - 10 - 18),
                    offset: u64::MAX,
                }),
                ..Query::default()
            },
        )
        .expect("page");
        assert_eq!(messages(&rest.entries), ["b", "a"]);
    }

    #[test]
    fn days_are_listed_newest_first_and_strays_ignored() {
        let dir = sample();
        let days = super::days(dir.path()).expect("days");
        let dates: Vec<_> = days.iter().map(|day| day.date).collect();
        assert_eq!(dates, [date!(2026 - 10 - 19), date!(2026 - 10 - 18)]);
        assert!(
            super::days(&dir.path().join("missing"))
                .expect("days")
                .is_empty()
        );
    }
}
