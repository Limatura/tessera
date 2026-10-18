//! How the command looks in a terminal.
//!
//! A few styles, used the same way everywhere: bold for the thing to act on (a
//! code, a link), dim for what is only context, green for done, red for
//! failed. Written through `anstream`, which drops the styling when the output
//! is not a terminal or `NO_COLOR` is set, so a pipe or a log gets plain text.

use anstyle::{AnsiColor, Style};

/// The value the person is meant to look at.
pub const STRONG: Style = Style::new().bold();
/// Labels and context.
pub const DIM: Style = Style::new().dimmed();
/// A link.
pub const LINK: Style = Style::new()
    .fg_color(Some(anstyle::Color::Ansi(AnsiColor::Cyan)))
    .underline();
/// The mark in front of something that worked.
pub const OK: Style = Style::new()
    .fg_color(Some(anstyle::Color::Ansi(AnsiColor::Green)))
    .bold();
/// The word in front of something that failed.
pub const ERROR: Style = Style::new()
    .fg_color(Some(anstyle::Color::Ansi(AnsiColor::Red)))
    .bold();

/// Frames of the spinner shown while waiting.
pub const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Bytes in the units people read: `512 B`, `1.2 MB`, `3.0 GB`.
#[must_use]
pub fn bytes(count: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut unit = 0;
    let mut whole = count;
    let mut tenths = 0;
    while whole >= 1000 && unit + 1 < UNITS.len() {
        tenths = (whole % 1000) / 100;
        whole /= 1000;
        unit += 1;
    }
    let name = UNITS.get(unit).copied().unwrap_or("B");
    if unit == 0 {
        format!("{whole} {name}")
    } else {
        format!("{whole}.{tenths} {name}")
    }
}

/// A bar `width` cells wide, filled to `done` of `total`.
#[must_use]
pub fn bar(done: u64, total: u64, width: usize) -> (String, String) {
    let width64 = u64::try_from(width).unwrap_or(u64::MAX);
    let filled = done
        .min(total)
        .saturating_mul(width64)
        .checked_div(total)
        .map_or(width, |cells| usize::try_from(cells).unwrap_or(width));
    ("█".repeat(filled), "░".repeat(width.saturating_sub(filled)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_read_the_way_people_say_them() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1_234_567), "1.2 MB");
        assert_eq!(bytes(3_000_000_000), "3.0 GB");
    }

    #[test]
    fn a_bar_fills_in_proportion() {
        assert_eq!(bar(0, 100, 4), (String::new(), "░░░░".to_owned()));
        assert_eq!(bar(50, 100, 4), ("██".to_owned(), "░░".to_owned()));
        assert_eq!(bar(100, 100, 4), ("████".to_owned(), String::new()));
    }
}
