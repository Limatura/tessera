//! The thread that appends each line to its day's file and announces it.
//!
//! A thread of its own, not a task: the layer feeding it runs inside and
//! outside the runtime alike, and a slow disk should stall nothing but this.

use std::{
    fs::{File, OpenOptions},
    io::{self, BufWriter, Write as _},
    sync::{Arc, mpsc::Receiver},
};

use time::{Date, OffsetDateTime, format_description::well_known::Rfc3339};

use super::{
    Logs,
    entry::{Cursor, Entry},
    layer::{Record, WRITER},
};

/// Starts the writer. It runs until every sender is gone, which for the
/// process's own log is when the process exits.
pub(super) fn spawn(logs: Logs, lines: Receiver<Record>) -> io::Result<()> {
    std::thread::Builder::new()
        .name("log-writer".to_owned())
        .spawn(move || {
            WRITER.with(|writer| writer.set(true));
            Writer {
                logs,
                open: None,
                failing: false,
            }
            .run(&lines);
        })?;
    Ok(())
}

struct Writer {
    logs: Logs,
    open: Option<Open>,
    failing: bool,
}

/// The file being appended to, and where the next line starts.
struct Open {
    date: Date,
    file: BufWriter<File>,
    end: u64,
}

impl Writer {
    /// Writes whatever is queued, flushes, then announces it, so a reader told
    /// of a line can already find it in the file.
    fn run(&mut self, lines: &Receiver<Record>) {
        while let Ok(first) = lines.recv() {
            let mut written = Vec::new();
            self.write(first, &mut written);
            while let Ok(next) = lines.try_recv() {
                self.write(next, &mut written);
            }
            if let Some(open) = &mut self.open
                && let Err(error) = open.file.flush()
            {
                self.open = None;
                self.fail(&error);
            }
            for entry in written {
                self.logs.publish(entry);
            }
        }
    }

    fn write(&mut self, record: Record, written: &mut Vec<Arc<Entry>>) {
        let date = record.time.date();
        let open = match self.file_for(date) {
            Ok(open) => open,
            Err(error) => {
                self.fail(&error);
                return;
            }
        };
        let entry = Entry {
            id: Cursor {
                date,
                offset: open.end,
            }
            .to_string(),
            time: timestamp(record.time),
            level: record.level,
            target: record.target,
            message: record.message,
            fields: record.fields,
        };
        let Ok(mut line) = serde_json::to_vec(&entry) else {
            return;
        };
        line.push(b'\n');
        match open.file.write_all(&line) {
            Ok(()) => {
                open.end = open
                    .end
                    .saturating_add(u64::try_from(line.len()).unwrap_or(u64::MAX));
                written.push(Arc::new(entry));
                self.recover();
            }
            Err(error) => {
                // Part of the line may have gone out, so the offset is no
                // longer known. Reopening reads it from the file again.
                self.open = None;
                self.fail(&error);
            }
        }
    }

    /// The file for `date`, opening it when the day has changed.
    fn file_for(&mut self, date: Date) -> io::Result<&mut Open> {
        if self.open.as_ref().is_none_or(|open| open.date != date) {
            if let Some(mut previous) = self.open.take() {
                drop(previous.file.flush());
            }
            let file = open_appending(&self.logs, date)?;
            let end = file.metadata()?.len();
            self.open = Some(Open {
                date,
                file: BufWriter::new(file),
                end,
            });
        }
        self.open
            .as_mut()
            .ok_or_else(|| io::Error::other("the log file was not opened"))
    }

    /// Says once, on stdout, that lines are being lost.
    fn fail(&mut self, error: &io::Error) {
        if !self.failing {
            self.failing = true;
            tracing::warn!(
                %error,
                directory = %self.logs.dir().display(),
                "could not write the log file; its lines are dropped until it can be written"
            );
        }
    }

    fn recover(&mut self) {
        if self.failing {
            self.failing = false;
            tracing::info!("writing the log file again");
        }
    }
}

/// Opens a day's file for appending, creating it and its directory readable by
/// this user only: the log names addresses, accounts and shares.
fn open_appending(logs: &Logs, date: Date) -> io::Result<File> {
    let mut directory = std::fs::DirBuilder::new();
    directory.recursive(true);
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
        directory.mode(0o700);
        options.mode(0o600);
    }
    directory.create(logs.dir())?;
    options.open(logs.file_for(date))
}

/// RFC 3339 in UTC, to the millisecond.
fn timestamp(time: OffsetDateTime) -> String {
    let time = time
        .replace_nanosecond(u32::from(time.millisecond()).saturating_mul(1_000_000))
        .unwrap_or(time);
    time.format(&Rfc3339).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::timestamp;

    #[test]
    fn times_are_written_to_the_millisecond() {
        assert_eq!(
            timestamp(datetime!(2026-10-19 03:12:09.381_234 UTC)),
            "2026-10-19T03:12:09.381Z"
        );
    }
}
