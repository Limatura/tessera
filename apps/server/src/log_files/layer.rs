//! The `tracing` layer that hands each line to the writer.
//!
//! It runs on whatever thread logged, often in the middle of a request, so it
//! does as little as it can: copy the line's fields and queue them. A full
//! queue drops the line instead of waiting. The file is a convenience; the
//! request is not.

use std::{
    cell::Cell,
    sync::{Arc, OnceLock, mpsc::SyncSender},
};

use arc_swap::ArcSwap;
use parking_lot::Mutex;
use serde_json::{Map, Value};
use time::OffsetDateTime;
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
    span,
};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};

use super::{Logs, writer};
use crate::config::{Config, LogLevel};

/// How many lines may wait for the writer before new ones are dropped.
const QUEUE: usize = 4096;

/// How many lines logged before the writer starts are kept for it. Start-up
/// logs a few dozen; this only stops a broken start from growing without end.
const EARLY: usize = 1024;

thread_local! {
    /// Set on the writer's own thread. What it logs about failing to write goes
    /// to stdout only, or a full disk would be reported into the file that
    /// cannot be written, and each report would queue another.
    pub(super) static WRITER: Cell<bool> = const { Cell::new(false) };
}

/// A line on its way to the writer.
#[derive(Debug)]
pub(super) struct Record {
    pub(super) time: OffsetDateTime,
    pub(super) level: LogLevel,
    pub(super) target: String,
    pub(super) message: String,
    pub(super) fields: Map<String, Value>,
}

/// Copies every line into the log file once [`FileLayer::attach`] has run.
///
/// Installed with the subscriber, before the configuration that says where
/// the file goes has been read from the database. Lines logged before then
/// are kept and written when it attaches.
#[derive(Debug, Clone, Default)]
pub struct FileLayer {
    slot: Arc<Slot>,
}

#[derive(Debug, Default)]
struct Slot {
    sink: OnceLock<Sink>,
    early: Mutex<Vec<Record>>,
}

#[derive(Debug)]
struct Sink {
    queue: SyncSender<Record>,
    config: Arc<ArcSwap<Config>>,
}

impl FileLayer {
    /// A layer with nowhere to write yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts writing into `logs`, reading `logs.min_level` from `config` for
    /// every line so the panel can change it.
    ///
    /// A second call does nothing: one process has one log.
    ///
    /// # Errors
    ///
    /// When the writer thread cannot be started.
    pub fn attach(&self, logs: &Logs, config: Arc<ArcSwap<Config>>) -> std::io::Result<()> {
        if self.slot.sink.get().is_some() {
            return Ok(());
        }
        let (queue, lines) = std::sync::mpsc::sync_channel(QUEUE);
        writer::spawn(logs.clone(), lines)?;
        if self.slot.sink.set(Sink { queue, config }).is_err() {
            return Ok(());
        }
        let early = std::mem::take(&mut *self.slot.early.lock());
        for record in early {
            self.queue(record);
        }
        Ok(())
    }

    fn wants(&self, level: LogLevel) -> bool {
        self.slot
            .sink
            .get()
            .is_none_or(|sink| level >= sink.config.load().logs.min_level)
    }

    fn queue(&self, record: Record) {
        if let Some(sink) = self.slot.sink.get() {
            // Full or gone: either way the line is dropped, not waited for.
            drop(sink.queue.try_send(record));
            return;
        }
        let mut early = self.slot.early.lock();
        if early.len() < EARLY {
            early.push(record);
        }
    }
}

/// A span's fields, kept in its extensions so each line inside it can carry
/// them.
#[derive(Debug, Default)]
struct SpanFields(Map<String, Value>);

impl<S> Layer<S> for FileLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(
        &self,
        attributes: &span::Attributes<'_>,
        id: &span::Id,
        context: Context<'_, S>,
    ) {
        let Some(span) = context.span(id) else {
            return;
        };
        let mut fields = SpanFields::default();
        attributes.record(&mut Fields {
            fields: &mut fields.0,
            message: None,
        });
        span.extensions_mut().insert(fields);
    }

    fn on_record(&self, id: &span::Id, values: &span::Record<'_>, context: Context<'_, S>) {
        let Some(span) = context.span(id) else {
            return;
        };
        if let Some(fields) = span.extensions_mut().get_mut::<SpanFields>() {
            values.record(&mut Fields {
                fields: &mut fields.0,
                message: None,
            });
        }
    }

    fn on_event(&self, event: &Event<'_>, context: Context<'_, S>) {
        if WRITER.with(Cell::get) {
            return;
        }
        let level = LogLevel::from(*event.metadata().level());
        if !self.wants(level) {
            return;
        }

        let mut fields = Map::new();
        if let Some(scope) = context.event_scope(event) {
            for span in scope.from_root() {
                if let Some(own) = span.extensions().get::<SpanFields>() {
                    fields.extend(
                        own.0
                            .iter()
                            .map(|(name, value)| (name.clone(), value.clone())),
                    );
                }
            }
        }
        let mut message = String::new();
        event.record(&mut Fields {
            fields: &mut fields,
            message: Some(&mut message),
        });

        self.queue(Record {
            time: OffsetDateTime::now_utc(),
            level,
            target: event.metadata().target().to_owned(),
            message,
            fields,
        });
    }
}

/// Whether a field's name says its value is a credential.
///
/// Nothing logs one today. This is the net for the day something does by
/// accident, so it errs towards hiding: a field merely named like a secret
/// loses its value, which costs a little detail and nothing else.
fn looks_secret(name: &str) -> bool {
    const NAMES: [&str; 8] = [
        "password",
        "passphrase",
        "secret",
        "token",
        "cookie",
        "authorization",
        "api_key",
        "credential",
    ];
    let name = name.to_ascii_lowercase();
    NAMES.iter().any(|secret| name.contains(secret))
}

/// Collects a line's fields as JSON, and its `message` as text.
struct Fields<'collect> {
    fields: &'collect mut Map<String, Value>,
    message: Option<&'collect mut String>,
}

impl Fields<'_> {
    fn insert(&mut self, field: &Field, value: Value) {
        let name = field.name();
        if name == "message"
            && let Some(message) = self.message.as_deref_mut()
        {
            *message = match value {
                Value::String(text) => text,
                other => other.to_string(),
            };
            return;
        }
        let value = if looks_secret(name) {
            Value::String("[redacted]".to_owned())
        } else {
            value
        };
        self.fields.insert(name.to_owned(), value);
    }
}

impl Visit for Fields<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn core::fmt::Debug) {
        self.insert(field, Value::String(format!("{value:?}")));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.insert(field, Value::String(value.to_owned()));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.insert(field, Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.insert(field, Value::from(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.insert(field, Value::from(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.insert(field, Value::from(value));
    }

    fn record_error(&mut self, field: &Field, value: &(dyn core::error::Error + 'static)) {
        self.insert(field, Value::String(value.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use arc_swap::ArcSwap;
    use tracing_subscriber::layer::SubscriberExt as _;

    use super::{FileLayer, looks_secret};
    use crate::{
        config::{Config, LogLevel},
        log_files::{Entry, Logs},
    };

    /// A layer writing under a fresh directory, and what it writes.
    fn attached(min_level: LogLevel) -> (FileLayer, Logs, Arc<ArcSwap<Config>>, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp dir");
        let logs = Logs::in_data_dir(&dir.path().display().to_string());
        let mut config = Config::default();
        config.logs.min_level = min_level;
        let config = Arc::new(ArcSwap::from_pointee(config));
        let layer = FileLayer::new();
        layer.attach(&logs, Arc::clone(&config)).expect("attach");
        (layer, logs, config, dir)
    }

    /// Every line written within a second, in order.
    fn written(mut lines: tokio::sync::broadcast::Receiver<Arc<Entry>>) -> Vec<Entry> {
        let mut entries = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while std::time::Instant::now() < deadline {
            match lines.try_recv() {
                Ok(entry) => entries.push((*entry).clone()),
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        entries
    }

    #[test]
    fn a_line_reaches_the_file_with_its_span_and_without_its_secrets() {
        let (layer, logs, _config, _dir) = attached(LogLevel::Trace);
        let lines = logs.subscribe();
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("http", method = "GET", path = "/s/{code}");
            let _inside = span.enter();
            tracing::info!(password = "hunter2", size = 3_u64, "hello");
        });

        let entries = written(lines);
        let [entry] = entries.as_slice() else {
            panic!("one line, got {entries:?}");
        };
        assert_eq!(entry.message, "hello");
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.fields["method"], "GET");
        assert_eq!(entry.fields["path"], "/s/{code}");
        assert_eq!(entry.fields["size"], 3);
        assert_eq!(entry.fields["password"], "[redacted]");

        let day = std::fs::read_dir(logs.dir())
            .expect("directory")
            .next()
            .expect("a file")
            .expect("entry");
        let contents = std::fs::read_to_string(day.path()).expect("read");
        assert!(!contents.contains("hunter2"), "{contents}");
        assert!(entry.id.ends_with(":0"), "{}", entry.id);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = day.metadata().expect("metadata").permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    /// Changed in the panel, applied to the next line.
    #[test]
    fn the_least_level_is_read_for_every_line() {
        let (layer, logs, config, _dir) = attached(LogLevel::Warn);
        let lines = logs.subscribe();
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("dropped");
            tracing::warn!("kept");
            let mut lower = Config::clone(&config.load());
            lower.logs.min_level = LogLevel::Info;
            config.store(Arc::new(lower));
            tracing::info!("kept too");
        });

        let messages: Vec<String> = written(lines)
            .into_iter()
            .map(|entry| entry.message)
            .collect();
        assert_eq!(messages, ["kept", "kept too"]);
    }

    #[test]
    fn lines_logged_before_attaching_are_written_once_it_does() {
        let dir = tempfile::tempdir().expect("temp dir");
        let logs = Logs::in_data_dir(&dir.path().display().to_string());
        let layer = FileLayer::new();
        let subscriber = tracing_subscriber::registry().with(layer.clone());
        tracing::subscriber::with_default(subscriber, || tracing::info!("starting"));

        let lines = logs.subscribe();
        layer
            .attach(&logs, Arc::new(ArcSwap::from_pointee(Config::default())))
            .expect("attach");
        let messages: Vec<String> = written(lines)
            .into_iter()
            .map(|entry| entry.message)
            .collect();
        assert_eq!(messages, ["starting"]);
    }

    #[test]
    fn credentials_are_recognised_by_name() {
        for name in [
            "password",
            "api_token",
            "Authorization",
            "session_cookie",
            "client_secret",
        ] {
            assert!(looks_secret(name), "{name}");
        }
        for name in ["path", "method", "request_id", "passkey", "size_bytes"] {
            assert!(!looks_secret(name), "{name}");
        }
    }
}
