//! Selecting text with the mouse while the view has it.
//!
//! A terminal selects text itself only while no program has asked for the
//! mouse. This view asks, so it selects instead: a drag marks cells on the
//! screen it drew, and letting go copies what they say.

use ratatui::{
    buffer::Buffer,
    layout::Position,
    style::{Modifier, Style},
};
use unicode_width::UnicodeWidthStr as _;

/// A drag from `anchor` to `head`, in reading order between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Selection {
    pub(crate) anchor: Position,
    pub(crate) head: Position,
}

impl Selection {
    fn ordered(self) -> (Position, Position) {
        let key = |at: Position| (at.y, at.x);
        if key(self.anchor) <= key(self.head) {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// The cells it covers on one row of `buffer`, as columns from and to,
    /// both included.
    fn columns(self, y: u16, buffer: &Buffer) -> Option<(u16, u16)> {
        let (start, end) = self.ordered();
        if y < start.y || y > end.y {
            return None;
        }
        let area = buffer.area;
        let from = if y == start.y { start.x } else { area.left() };
        let to = if y == end.y {
            end.x
        } else {
            area.right().saturating_sub(1)
        };
        Some((from, to))
    }

    /// Shows it, by reversing the colours of the cells it covers.
    pub(crate) fn highlight(self, buffer: &mut Buffer) {
        let (start, end) = self.ordered();
        for y in start.y..=end.y {
            let Some((from, to)) = self.columns(y, buffer) else {
                continue;
            };
            for x in from..=to {
                if let Some(cell) = buffer.cell_mut(Position::new(x, y)) {
                    cell.set_style(Style::new().add_modifier(Modifier::REVERSED));
                }
            }
        }
    }

    /// What it covers, a line to a row, without the frames around it.
    pub(crate) fn text(self, buffer: &Buffer) -> String {
        let (start, end) = self.ordered();
        let mut lines = Vec::new();
        for y in start.y..=end.y {
            let Some((from, to)) = self.columns(y, buffer) else {
                continue;
            };
            let mut line = String::new();
            // A wide character fills its own cell and blanks the next.
            let mut covered = 0;
            for x in buffer.area.left()..=to {
                let Some(cell) = buffer.cell(Position::new(x, y)) else {
                    break;
                };
                if covered > 0 {
                    covered -= 1;
                    continue;
                }
                let symbol = cell.symbol();
                covered = symbol.width().saturating_sub(1);
                if x >= from {
                    line.push_str(symbol);
                }
            }
            lines.push(
                line.trim_matches(|c: char| c.is_whitespace() || is_frame(c))
                    .to_owned(),
            );
        }
        lines.join("\n")
    }
}

/// The lines boxes are drawn with.
const fn is_frame(c: char) -> bool {
    matches!(
        c,
        '│' | '─' | '┌' | '┐' | '└' | '┘' | '├' | '┤' | '╭' | '╮' | '╰' | '╯'
    )
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::*;

    fn screen(lines: &[&str]) -> Buffer {
        let width = lines.iter().map(|line| line.width()).max().unwrap_or(0);
        let mut buffer = Buffer::empty(Rect::new(
            0,
            0,
            u16::try_from(width).expect("narrow"),
            u16::try_from(lines.len()).expect("short"),
        ));
        for (y, line) in lines.iter().enumerate() {
            buffer.set_string(0, u16::try_from(y).expect("short"), line, Style::new());
        }
        buffer
    }

    fn select(buffer: &Buffer, anchor: (u16, u16), head: (u16, u16)) -> String {
        Selection {
            anchor: Position::new(anchor.0, anchor.1),
            head: Position::new(head.0, head.1),
        }
        .text(buffer)
    }

    #[test]
    fn a_drag_on_one_row_copies_what_it_covers() {
        let buffer = screen(&["│ https://share.example/s/AB12 │"]);
        assert_eq!(
            select(&buffer, (2, 0), (29, 0)),
            "https://share.example/s/AB12"
        );
        // Backwards is the same drag.
        assert_eq!(
            select(&buffer, (29, 0), (2, 0)),
            "https://share.example/s/AB12"
        );
    }

    #[test]
    fn a_drag_over_rows_reads_like_a_terminal_selection() {
        let buffer = screen(&["│ first line   │", "│ second line  │", "│ third        │"]);
        assert_eq!(select(&buffer, (8, 0), (7, 2)), "line\nsecond line\nthird");
    }

    #[test]
    fn wide_characters_are_copied_once() {
        let buffer = screen(&["報告.pdf  12 KB"]);
        assert_eq!(select(&buffer, (0, 0), (7, 0)), "報告.pdf");
        assert_eq!(select(&buffer, (2, 0), (3, 0)), "告");
    }
}
