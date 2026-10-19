//! The ticket, moving: its stub torn off when something is sent, a small one
//! in transit while something waits, and light passing over the large one.
//!
//! Each is drawn from how long it has run, so a frame that comes late shows
//! where the motion has got to rather than slowing it down.

use std::time::Duration;

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

use crate::logo::{Ink, Picture};

/// A small ticket laid flat, 28 pixels by 12. The stub is the orange, its
/// edge with the rest zigzagging a pixel either way as a tear does.
const SMALL: [&str; 12] = [
    ".cccccccccccccccccccooooooo.",
    "cccccccccccccccccccooooooooo",
    "ccccccccccccccccccccoooooooo",
    "cccccdddccdddcddcccooooooooo",
    "cccccdddccdddcddccccoooooooo",
    "ccccdddccdddcddccccooooooooo",
    "ccccdddccdddcddcccccoooooooo",
    "cccdddccdddcddcccccooooooooo",
    "cccdddccdddcddccccccoooooooo",
    "cccccccccccccccccccooooooooo",
    "ccccccccccccccccccccoooooooo",
    ".ccccccccccccccccccoooooooo.",
];

/// How far the stub falls when torn: pixels across and down, and how many
/// more across its bottom goes than its top, so it lands leaning.
const TEAR: (usize, usize, usize) = (5, 3, 2);

/// Pixels, each an ink or nothing, drawn two to a cell.
struct Canvas {
    width: usize,
    height: usize,
    pixels: Vec<Option<Color>>,
}

impl Canvas {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![None; width * height],
        }
    }

    /// Paints `rows`, each pixel where `place` puts it, if anywhere.
    fn paint(
        &mut self,
        rows: &[&str],
        place: impl Fn(usize, usize, Ink) -> (usize, usize),
        colour: impl Fn(Ink) -> Color,
    ) {
        for (y, row) in rows.iter().enumerate() {
            for (x, pixel) in row.bytes().enumerate() {
                let Some(ink) = ink_of(pixel) else {
                    continue;
                };
                let (x, y) = place(x, y, ink);
                if x < self.width
                    && y < self.height
                    && let Some(cell) = self.pixels.get_mut(y * self.width + x)
                {
                    *cell = Some(colour(ink));
                }
            }
        }
    }

    fn at(&self, x: usize, y: usize) -> Option<Color> {
        if x < self.width && y < self.height {
            self.pixels.get(y * self.width + x).copied().flatten()
        } else {
            None
        }
    }

    fn lines(&self) -> Vec<Line<'static>> {
        (0..self.height)
            .step_by(2)
            .map(|y| {
                Line::from(
                    (0..self.width)
                        .map(|x| half_block(self.at(x, y), self.at(x, y + 1)))
                        .collect::<Vec<_>>(),
                )
            })
            .collect()
    }
}

const fn ink_of(pixel: u8) -> Option<Ink> {
    match pixel {
        b'c' => Some(Ink::Paper),
        b'o' => Some(Ink::Stub),
        b'd' => Some(Ink::Print),
        _ => None,
    }
}

/// A cell holding an upper and a lower pixel, either of them maybe empty.
fn half_block(upper: Option<Color>, lower: Option<Color>) -> Span<'static> {
    match (upper, lower) {
        (None, None) => Span::raw(" "),
        (Some(upper), None) => Span::styled("▀", Style::new().fg(upper)),
        (None, Some(lower)) => Span::styled("▄", Style::new().fg(lower)),
        (Some(upper), Some(lower)) => Span::styled("▀", Style::new().fg(upper).bg(lower)),
    }
}

/// An ink in the colours this terminal has.
pub(crate) const fn colour(ink: Ink, truecolor: bool) -> Color {
    if truecolor {
        let (red, green, blue) = ink.rgb();
        Color::Rgb(red, green, blue)
    } else {
        Color::Indexed(ink.indexed())
    }
}

/// An ink with light on it.
const fn lit(ink: Ink, truecolor: bool) -> Color {
    match (ink, truecolor) {
        (Ink::Paper, true) => Color::Rgb(0xff, 0xff, 0xff),
        (Ink::Stub, true) => Color::Rgb(0xf2, 0x86, 0x66),
        (Ink::Print, true) => Color::Rgb(0x5c, 0x52, 0x4e),
        (Ink::Paper, false) => Color::Indexed(231),
        (Ink::Stub, false) => Color::Indexed(209),
        (Ink::Print, false) => Color::Indexed(240),
    }
}

/// How far along `elapsed` is of a motion that waits `delay` and then takes
/// `length`, from 0 to 1, slowing towards the end.
fn eased(elapsed: Duration, delay: Duration, length: Duration) -> f32 {
    let done = elapsed.saturating_sub(delay).as_secs_f32() / length.as_secs_f32();
    let done = done.clamp(0.0, 1.0);
    1.0 - (1.0 - done) * (1.0 - done)
}

/// How long [`torn`] moves for; after that it holds still.
pub(crate) const TEARING: Duration = Duration::from_millis(700);

/// The small ticket, `elapsed` into having its stub torn off: whole for a
/// moment, then the stub pulled away to the right, falling and leaning a
/// little, where it stays.
pub(crate) fn torn(elapsed: Duration, truecolor: bool) -> Vec<Line<'static>> {
    const WHOLE: Duration = Duration::from_millis(250);
    let done = eased(elapsed, WHOLE, TEARING.saturating_sub(WHOLE));
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        clippy::as_conversions,
        reason = "a few pixels, from a fraction between 0 and 1"
    )]
    let part = |far: usize| (far as f32 * done).round() as usize;
    let width = SMALL.first().map_or(0, |row| row.len());
    let height = SMALL.len();
    let mut canvas = Canvas::new(
        width + TEAR.0 + TEAR.2 + 1,
        (height + TEAR.1).next_multiple_of(2),
    );
    canvas.paint(
        &SMALL,
        |x, y, ink| {
            if ink == Ink::Stub {
                let lean = part(TEAR.2 * y / height.saturating_sub(1).max(1));
                (x + part(TEAR.0) + lean, y + part(TEAR.1))
            } else {
                (x, y)
            }
        },
        |ink| colour(ink, truecolor),
    );
    canvas.lines()
}

/// A ticket the size of a few letters crossing `track` cells from left to
/// right and coming round again, for something under way.
pub(crate) fn in_transit(elapsed: Duration, track: usize, truecolor: bool) -> Vec<Span<'static>> {
    const TICKET: [&str; 2] = ["cccco", "cccoo"];
    const CELLS_A_SECOND: f32 = 10.0;
    let lap = track + TICKET[0].len();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::as_conversions,
        reason = "a cell count, from seconds that only grow"
    )]
    let travelled = (elapsed.as_secs_f32() * CELLS_A_SECOND) as usize % lap;
    // Starting off the left edge, the canvas is wider than the track by a
    // ticket on that side, and only the track is shown.
    let mut canvas = Canvas::new(lap, 2);
    canvas.paint(
        &TICKET,
        |x, y, _| (x + travelled, y),
        |ink| colour(ink, truecolor),
    );
    canvas
        .lines()
        .into_iter()
        .next()
        .map(|line| line.spans.into_iter().skip(TICKET[0].len()).collect())
        .unwrap_or_default()
}

/// How often light passes over the large ticket, and how long it takes.
const SHINE_EVERY: Duration = Duration::from_secs(6);
const SHINE_FOR: Duration = Duration::from_millis(900);

/// `picture`, `elapsed` since it was put up: now and then a slanting band
/// of light crosses it from left to right.
pub(crate) fn shining(picture: Picture, elapsed: Duration, truecolor: bool) -> Vec<Line<'static>> {
    // The first pass a moment after it appears, then one every so often.
    let since = (elapsed + SHINE_EVERY).saturating_sub(Duration::from_millis(1200));
    let into =
        Duration::from_nanos(u64::try_from(since.as_nanos() % SHINE_EVERY.as_nanos()).unwrap_or(0));
    let reach = picture.width() + picture.pixel_rows();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        clippy::as_conversions,
        reason = "a pixel position, from a fraction between 0 and 1"
    )]
    let band = (into < SHINE_FOR).then(|| {
        let done = into.as_secs_f32() / SHINE_FOR.as_secs_f32();
        ((reach + 8) as f32 * done) as usize
    });
    picture
        .cells()
        .enumerate()
        .map(|(row, cells)| {
            Line::from(
                cells
                    .into_iter()
                    .enumerate()
                    .map(|(x, (upper, lower))| {
                        let paint = |ink: Option<Ink>, y: usize| {
                            ink.map(|ink| {
                                // Along the diagonal from the top left,
                                // against where the band has got to: the
                                // band itself runs from bottom left to top
                                // right, a pixel being about a cell wide.
                                let along = x + y + 4;
                                if band.is_some_and(|band| along.abs_diff(band) <= 2) {
                                    lit(ink, truecolor)
                                } else {
                                    colour(ink, truecolor)
                                }
                            })
                        };
                        half_block(paint(upper, row * 2), paint(lower, row * 2 + 1))
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

/// `picture` as it is, a line to a row of cells.
pub(crate) fn still(picture: Picture, truecolor: bool) -> Vec<Line<'static>> {
    shining(picture, Duration::ZERO, truecolor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logo;

    fn text(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn the_stub_starts_attached_and_ends_apart() {
        let whole = text(&torn(Duration::ZERO, true));
        // Whole, the room the stub falls into is empty.
        let room = TEAR.0 + TEAR.2;
        assert!(whole.iter().all(|row| row.ends_with(&" ".repeat(room))));
        // Torn, a gap of a few cells opens along the tear.
        let apart = text(&torn(TEARING * 2, true));
        let row: Vec<char> = apart.get(2).expect("a row").chars().collect();
        assert!(
            row.get(20..23)
                .is_some_and(|gap| gap.iter().all(|c| *c == ' ')),
            "{row:?}"
        );
        assert_eq!(torn(TEARING * 2, true), torn(TEARING * 5, true));
    }

    #[test]
    fn the_ticket_in_transit_keeps_to_its_track() {
        for tenth in 0..40 {
            let spans = in_transit(Duration::from_millis(tenth * 100), 8, true);
            assert_eq!(spans.len(), 8, "at {tenth}");
        }
    }

    #[test]
    fn still_is_the_picture_without_light() {
        let lines = still(logo::FLAT, true);
        assert_eq!(lines.len(), logo::FLAT.height());
        let lit = Color::Rgb(0xff, 0xff, 0xff);
        assert!(
            lines
                .iter()
                .flat_map(|line| &line.spans)
                .all(|span| span.style.fg != Some(lit))
        );
    }
}
