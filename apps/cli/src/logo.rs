//! The ticket from the site's icon, drawn in a terminal.
//!
//! Traced from `apps/web/public/favicon.svg` at 40 by 20 pixels and laid flat:
//! tilted as on the site, it frays at this size. Two pixels go in a cell, one
//! above the other, as a half block in two colours.

/// The ticket, a character a pixel: `c` the paper, `o` the stub, `d` the ink of
/// the bars and the tear line, `.` nothing.
const TICKET: [&str; 20] = [
    ".ccccccccccccccccccccccccccccoooooooooo.",
    "ccccccccccccccccccccccccccccdooooooooooo",
    "ccccccccccccccccccccccccccccddoooooooooo",
    "ccccccccccccccccccccccccccccddoooooooooo",
    "cccccccccccccccccccccccccccccooooooooooo",
    "cccccccccddddccdddccdddccccccooooooooooo",
    ".ccccccccdddccddddccdddcccccddooooooooo.",
    "..cccccccdddccdddccddddcccccddoooooooo..",
    "...cccccdddcccdddccdddcccccccoooooooo...",
    "....ccccdddccdddcccdddcccccccooooooo....",
    "....cccddddccdddccdddcccccccddoooooo....",
    "...ccccdddccddddccdddcccccccddooooooo...",
    "..cccccdddccdddccddddccccccccooooooooo..",
    ".cccccdddcccdddccdddcccccccccoooooooooo.",
    "ccccccdddccdddcccdddccccccccddoooooooooo",
    "ccccccccccccccccccccccccccccddoooooooooo",
    "cccccccccccccccccccccccccccccooooooooooo",
    "cccccccccccccccccccccccccccccooooooooooo",
    "ccccccccccccccccccccccccccccddoooooooooo",
    ".cccccccccccccccccccccccccccddooooooooo.",
];

/// Cells across.
pub const WIDTH: usize = 40;
/// Cells down: two pixels to a cell.
pub const HEIGHT: usize = TICKET.len() / 2;

/// The colours the ticket is printed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    /// The ticket itself.
    Paper,
    /// The part torn off and handed over, in the site's orange.
    Stub,
    /// The bars and the tear line.
    Print,
}

impl Ink {
    const fn of(pixel: u8) -> Option<Self> {
        match pixel {
            b'c' => Some(Self::Paper),
            b'o' => Some(Self::Stub),
            b'd' => Some(Self::Print),
            _ => None,
        }
    }

    /// Red, green and blue, as on the site.
    #[must_use]
    pub const fn rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Paper => (0xf5, 0xef, 0xe9),
            Self::Stub => (0xd9, 0x4f, 0x2b),
            Self::Print => (0x0a, 0x06, 0x05),
        }
    }

    /// The nearest of the 256 colours every terminal has.
    #[must_use]
    pub const fn indexed(self) -> u8 {
        match self {
            Self::Paper => 255,
            Self::Stub => 166,
            Self::Print => 232,
        }
    }
}

/// Whether the terminal says it takes 24-bit colour. SSH does not pass the
/// variable on, so over SSH this is usually no, and the 256 colours stand in.
#[must_use]
pub fn truecolor() -> bool {
    matches!(
        std::env::var("COLORTERM").as_deref(),
        Ok("truecolor" | "24bit")
    )
}

/// The ticket, a row of cells at a time: what is above and below in each.
pub fn cells() -> impl Iterator<Item = Vec<(Option<Ink>, Option<Ink>)>> {
    TICKET.chunks(2).map(|pair| {
        let top = pair.first().map_or(&b""[..], |row| row.as_bytes());
        let bottom = pair.get(1).map_or(&b""[..], |row| row.as_bytes());
        (0..WIDTH)
            .map(|x| {
                (
                    top.get(x).copied().and_then(Ink::of),
                    bottom.get(x).copied().and_then(Ink::of),
                )
            })
            .collect()
    })
}

/// The ticket as lines of text with colour escapes, each set in by `indent`
/// spaces and ending with the colours reset.
#[must_use]
pub fn ansi(indent: usize, truecolor: bool) -> Vec<String> {
    let colour = |layer: u8, ink: Ink| {
        if truecolor {
            let (red, green, blue) = ink.rgb();
            format!("\x1b[{layer};2;{red};{green};{blue}m")
        } else {
            format!("\x1b[{layer};5;{}m", ink.indexed())
        }
    };
    cells()
        .map(|row| {
            let mut line = " ".repeat(indent);
            for cell in row {
                match cell {
                    (None, None) => line.push_str("\x1b[0m "),
                    (Some(top), None) => {
                        line.push_str("\x1b[0m");
                        line.push_str(&colour(38, top));
                        line.push('▀');
                    }
                    (None, Some(bottom)) => {
                        line.push_str("\x1b[0m");
                        line.push_str(&colour(38, bottom));
                        line.push('▄');
                    }
                    (Some(top), Some(bottom)) => {
                        line.push_str(&colour(38, top));
                        line.push_str(&colour(48, bottom));
                        line.push('▀');
                    }
                }
            }
            line.push_str("\x1b[0m");
            line
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_is_as_wide_as_the_ticket() {
        assert!(TICKET.iter().all(|row| row.len() == WIDTH));
        assert!(
            TICKET
                .iter()
                .flat_map(|row| row.bytes())
                .all(|pixel| b".cod".contains(&pixel))
        );
        assert_eq!(cells().count(), HEIGHT);
    }

    #[test]
    fn each_line_draws_a_cell_per_column_and_resets() {
        for line in ansi(2, false) {
            let drawn = anstream::adapter::strip_str(&line).to_string();
            assert_eq!(drawn.chars().count(), 2 + WIDTH, "{drawn}");
            assert!(line.ends_with("\x1b[0m"));
        }
        assert!(
            ansi(0, true)
                .iter()
                .any(|line| line.contains("38;2;217;79;43"))
        );
    }
}
