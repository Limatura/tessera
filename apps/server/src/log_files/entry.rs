//! One line of the log file, where it sits, and which lines a reader wants.

use core::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use time::{Date, format_description::BorrowedFormatItem, macros::format_description};
use utoipa::ToSchema;

use crate::config::LogLevel;

/// How a day is written in a file name and an entry's `id`.
pub(crate) const DAY: &[BorrowedFormatItem<'_>] = format_description!("[year]-[month]-[day]");

/// One line of a log file, as written and as served.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Entry {
    /// Where the line starts: its day and byte offset in that day's file.
    /// Stable, so it doubles as a cursor.
    #[schema(example = "2026-10-19:48213")]
    pub id: String,
    /// When it was logged, RFC 3339 in UTC.
    #[schema(example = "2026-10-19T03:12:09.381Z")]
    pub time: String,
    /// How severe it is.
    pub level: LogLevel,
    /// The module that logged it.
    #[schema(example = "tessera_server::scan")]
    pub target: String,
    /// What happened.
    pub message: String,
    /// The line's own fields, and those of the spans it was logged inside,
    /// such as the request's `method`, `path` and `request_id`.
    #[schema(value_type = Object)]
    pub fields: Map<String, Value>,
}

/// A place in the log: the start of one line in one day's file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    /// Which file.
    pub date: Date,
    /// How far into it, in bytes.
    pub offset: u64,
}

impl fmt::Display for Cursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let day = self.date.format(DAY).map_err(|_| fmt::Error)?;
        write!(formatter, "{day}:{}", self.offset)
    }
}

impl FromStr for Cursor {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (day, offset) = value.split_once(':').ok_or(())?;
        Ok(Self {
            date: Date::parse(day, DAY).map_err(|_| ())?,
            offset: offset.parse().map_err(|_| ())?,
        })
    }
}

/// Which lines a reader asked for.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// The least severe level wanted. `None` takes every level.
    pub level: Option<LogLevel>,
    /// Text the message or a field value must contain, ignoring case.
    text: Option<String>,
}

impl Filter {
    /// Lines at `level` or above that mention `text`. Blank text matches all.
    #[must_use]
    pub fn new(level: Option<LogLevel>, text: Option<&str>) -> Self {
        Self {
            level,
            text: text
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_lowercase),
        }
    }

    /// Whether `entry` is one of them.
    #[must_use]
    pub fn matches(&self, entry: &Entry) -> bool {
        if self.level.is_some_and(|least| entry.level < least) {
            return false;
        }
        let Some(text) = &self.text else {
            return true;
        };
        entry.message.to_lowercase().contains(text)
            || entry.fields.values().any(|value| match value {
                Value::String(value) => value.to_lowercase().contains(text),
                other => other.to_string().to_lowercase().contains(text),
            })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use time::macros::date;

    use super::{Cursor, Entry, Filter};
    use crate::config::LogLevel;

    fn entry(level: LogLevel, message: &str, fields: &serde_json::Value) -> Entry {
        Entry {
            id: "2026-10-19:0".to_owned(),
            time: "2026-10-19T00:00:00Z".to_owned(),
            level,
            target: "tessera_server".to_owned(),
            message: message.to_owned(),
            fields: fields.as_object().cloned().unwrap_or_default(),
        }
    }

    #[test]
    fn a_cursor_survives_a_round_trip() {
        let cursor = Cursor {
            date: date!(2026 - 10 - 09),
            offset: 48_213,
        };
        assert_eq!(cursor.to_string(), "2026-10-09:48213");
        assert_eq!("2026-10-09:48213".parse(), Ok(cursor));
    }

    #[test]
    fn a_malformed_cursor_is_refused() {
        for value in [
            "",
            "2026-10-09",
            "2026-13-01:5",
            "2026-10-09:-1",
            "../etc:1",
        ] {
            assert!(value.parse::<Cursor>().is_err(), "{value}");
        }
    }

    #[test]
    fn the_level_is_a_minimum() {
        let filter = Filter::new(Some(LogLevel::Warn), None);
        assert!(!filter.matches(&entry(LogLevel::Info, "", &json!({}))));
        assert!(filter.matches(&entry(LogLevel::Warn, "", &json!({}))));
        assert!(filter.matches(&entry(LogLevel::Error, "", &json!({}))));
    }

    #[test]
    fn text_is_looked_for_in_the_message_and_field_values_ignoring_case() {
        let filter = Filter::new(None, Some("  ClamD "));
        assert!(filter.matches(&entry(LogLevel::Info, "clamd is down", &json!({}))));
        assert!(filter.matches(&entry(LogLevel::Info, "", &json!({ "backend": "CLAMD" }))));
        assert!(!filter.matches(&entry(LogLevel::Info, "", &json!({ "clamd": 1 }))));

        let numbers = Filter::new(None, Some("413"));
        assert!(numbers.matches(&entry(LogLevel::Info, "", &json!({ "status": 413 }))));
    }

    #[test]
    fn blank_text_matches_everything() {
        assert!(Filter::new(None, Some("   ")).matches(&entry(LogLevel::Trace, "", &json!({}))));
    }
}
