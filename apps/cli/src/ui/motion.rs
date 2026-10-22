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

/// A small ticket laid flat, 30 pixels by 14, drawn as the logo is: rounded
/// corners, a bite out of either end, the brand's slanted bars, and between
/// paper and stub a line of holes for it to tear along.
const SMALL: [&str; 14] = [
    ".ccccccccccccccccccccoooooooo.",
    "ccccccccccccccccccccc.oooooooo",
    "ccccccccccccccccccccc.oooooooo",
    "cccccccccccccccccccccooooooooo",
    "ccccccdddccdddccdddccooooooooo",
    ".cccccdddccdddccdddcc.ooooooo.",
    "..cccdddccdddccdddccc.oooooo..",
    "..cccdddccdddccdddcccooooooo..",
    ".cccdddccdddccdddccccoooooooo.",
    "ccccdddccdddccdddcccc.oooooooo",
    "ccccccccccccccccccccc.oooooooo",
    "cccccccccccccccccccccooooooooo",
    "cccccccccccccccccccccooooooooo",
    ".cccccccccccccccccccc.ooooooo.",
];

/// How far the stub is pulled when torn: pixels across and down.
const TEAR: (usize, usize) = (5, 3);

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

    fn set(&mut self, x: usize, y: usize, colour: Color) {
        if x < self.width
            && let Some(cell) = self.pixels.get_mut(y * self.width + x)
        {
            *cell = Some(colour);
        }
    }

    /// Empties the pixels in `columns` across `rows`.
    fn clear(&mut self, columns: std::ops::Range<usize>, rows: std::ops::Range<usize>) {
        for y in rows {
            for x in columns.clone() {
                if x < self.width
                    && let Some(cell) = self.pixels.get_mut(y * self.width + x)
                {
                    *cell = None;
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

/// When the stub has been pulled clear of the ticket and starts to fall.
const PULLED: Duration = Duration::from_millis(700);

/// How long [`torn`] moves for; after that it holds still, the stub gone.
pub(crate) const TEARING: Duration = Duration::from_millis(2400);

/// Where the stub starts on the ticket, in pixels from its left.
const PERFORATION: usize = 21;

/// The room the stub falls through, in pixels: wide enough for it to swing,
/// and as tall as a QR code beside it, so the dialog does not grow for it.
const FALL_ROOM: (usize, usize) = (46, 26);

/// The small ticket, `elapsed` into having its stub torn off: whole for a
/// moment, then the stub pulled away to the right and down, and let go. It
/// comes down as paper does, swinging from side to side and tipping as it
/// swings, turning over to show its back, until it is out of sight.
pub(crate) fn torn(elapsed: Duration, truecolor: bool) -> Vec<Line<'static>> {
    const WHOLE: Duration = Duration::from_millis(250);
    let pulled = eased(elapsed, WHOLE, PULLED.saturating_sub(WHOLE));
    let falling = (elapsed.saturating_sub(PULLED).as_secs_f32()
        / TEARING.saturating_sub(PULLED).as_secs_f32())
    .clamp(0.0, 1.0);
    let mut canvas = Canvas::new(FALL_ROOM.0, FALL_ROOM.1);
    canvas.paint(&SMALL, |x, y, _| (x, y), |ink| colour(ink, truecolor));
    // The paint above put the stub where it was; take it off again and draw
    // it where it has got to.
    canvas.clear(PERFORATION..FALL_ROOM.0, 0..SMALL.len());
    let flight = Flight::at(pulled, falling);
    let front = colour(Ink::Stub, truecolor);
    let back = if truecolor {
        Color::Rgb(0xa3, 0x36, 0x1b)
    } else {
        Color::Indexed(130)
    };
    flight.draw(&mut canvas, if flight.turned { back } else { front });
    canvas.lines()
}

/// Where the stub is and how it lies, part of the way through its fall.
struct Flight {
    /// Its middle, in pixels.
    centre: (f32, f32),
    /// How far it has tipped, in radians, clockwise.
    tilt: f32,
    /// How wide it looks, turning over: 1 flat, less edge on.
    breadth: f32,
    /// Whether its back is to the viewer.
    turned: bool,
}

impl Flight {
    /// `pulled` and `falling` each from 0 to 1.
    fn at(pulled: f32, falling: f32) -> Self {
        use std::f32::consts::TAU;
        let (width, height) = stub_size();
        #[expect(
            clippy::cast_precision_loss,
            clippy::as_conversions,
            reason = "pixel counts of a few dozen"
        )]
        let (middle, perforation, room, (pull_across, pull_down)) = (
            ((width as f32 - 1.0) / 2.0, (height as f32 - 1.0) / 2.0),
            PERFORATION as f32,
            FALL_ROOM.1 as f32,
            (TEAR.0 as f32, TEAR.1 as f32),
        );
        let start = (
            perforation + middle.0 + pull_across * pulled,
            middle.1 + pull_down * pulled,
        );
        // A swing and a half on the way down, growing from nothing as it
        // leaves the ticket, and tipping with the swing as a pendulum does.
        let swing = (TAU * 1.5 * falling).sin() * (falling * 4.0).min(1.0);
        // Three quarters of a turn: edge on, its back, edge on again.
        let turning = (TAU * 0.75 * falling).cos();
        // Out of sight below by the end, slow to start and then steady.
        let drop = (room + 10.0 - start.1) * (0.35 * falling * falling + 0.65 * falling);
        Self {
            centre: (start.0 + 4.0 * falling + 5.0 * swing, start.1 + drop),
            tilt: -0.49 * swing,
            breadth: turning.abs().max(0.25),
            turned: turning < 0.0,
        }
    }

    /// The stub, as it lies, onto `canvas` in `ink`.
    fn draw(&self, canvas: &mut Canvas, ink: Color) {
        let (width, height) = stub_size();
        #[expect(
            clippy::cast_precision_loss,
            clippy::as_conversions,
            reason = "pixel counts of a few dozen"
        )]
        let middle = ((width as f32 - 1.0) / 2.0, (height as f32 - 1.0) / 2.0);
        let (sin, cos) = self.tilt.sin_cos();
        for y in 0..canvas.height {
            for x in 0..canvas.width {
                // Back from the screen to the stub: undo the tilt, then the
                // turn.
                #[expect(
                    clippy::cast_precision_loss,
                    clippy::as_conversions,
                    reason = "pixel counts of a few dozen"
                )]
                let (across, down) = (x as f32 - self.centre.0, y as f32 - self.centre.1);
                let along = (across * cos + down * sin) / self.breadth + middle.0;
                let up = (down * cos - across * sin) + middle.1;
                if stub_has(along.round(), up.round()) {
                    canvas.set(x, y, ink);
                }
            }
        }
    }
}

/// The stub's width and height in pixels.
fn stub_size() -> (usize, usize) {
    let width = SMALL.first().map_or(0, |row| row.len());
    (width.saturating_sub(PERFORATION), SMALL.len())
}

/// Whether the stub has a pixel at `x`, `y` of its own, given as whole
/// numbers that may lie outside it.
fn stub_has(x: f32, y: f32) -> bool {
    if x < 0.0 || y < 0.0 {
        return false;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::as_conversions,
        reason = "whole, non-negative numbers of a few dozen"
    )]
    let (x, y) = (x as usize, y as usize);
    SMALL
        .get(y)
        .and_then(|row| row.as_bytes().get(PERFORATION + x))
        .is_some_and(|pixel| ink_of(*pixel) == Some(Ink::Stub))
}

/// How much of `text` has been typed out `elapsed` after the stub was torn:
/// none at first, then a character at a time, all of it before long.
pub(crate) fn typed(text: &str, elapsed: Duration) -> String {
    const FROM: Duration = Duration::from_millis(400);
    const FOR: Duration = Duration::from_millis(450);
    let done = elapsed.saturating_sub(FROM).as_secs_f32() / FOR.as_secs_f32();
    let count = text.chars().count();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        clippy::as_conversions,
        reason = "a character count, from a fraction between 0 and 1"
    )]
    let shown = (count as f32 * done.clamp(0.0, 1.0)).ceil() as usize;
    text.chars().take(shown).collect()
}

/// How much of a QR code has been printed `elapsed` after the stub was torn,
/// from 0 to 1, row by row as a ticket machine would.
pub(crate) fn printed(elapsed: Duration) -> f32 {
    const FROM: Duration = Duration::from_millis(600);
    const FOR: Duration = Duration::from_millis(550);
    (elapsed.saturating_sub(FROM).as_secs_f32() / FOR.as_secs_f32()).clamp(0.0, 1.0)
}

/// How long the dialog for something sent keeps moving: the tear, the link
/// typed out and the code printed, with a frame or two to spare so that the
/// last one drawn shows them finished.
pub(crate) const SENT_MOVES: Duration = Duration::from_millis(2550);

/// The small ticket riding along `width` cells as far as `done` says, from 0
/// to 1, or back and forth when there is no telling how far. It bobs a pixel
/// as it goes.
pub(crate) fn riding(
    done: Option<f32>,
    elapsed: Duration,
    width: usize,
    truecolor: bool,
) -> Vec<Line<'static>> {
    const LAPS_A_SECOND: f32 = 0.4;
    let length = SMALL.first().map_or(0, |row| row.len());
    let travel = width.saturating_sub(length);
    let along = done.unwrap_or_else(|| {
        // Out and back: a triangle, from 0 to 1 and down again.
        let lap = (elapsed.as_secs_f32() * LAPS_A_SECOND).fract() * 2.0;
        if lap > 1.0 { 2.0 - lap } else { lap }
    });
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        clippy::as_conversions,
        reason = "a cell position, from a fraction between 0 and 1"
    )]
    let x = (travel as f32 * along.clamp(0.0, 1.0)).round() as usize;
    let moving = done.is_none_or(|done| done < 1.0);
    let bob = usize::from(moving && (elapsed.as_millis() / 160) % 2 == 1);
    let mut canvas = Canvas::new(width, (SMALL.len() + 1).next_multiple_of(2));
    canvas.paint(
        &SMALL,
        |column, row, _| (column + x, row + bob),
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

    fn has_stub(lines: &[Line<'_>]) -> bool {
        let stub = colour(Ink::Stub, true);
        lines
            .iter()
            .flat_map(|line| &line.spans)
            .any(|span| span.style.fg == Some(stub) || span.style.bg == Some(stub))
    }

    #[test]
    fn the_stub_starts_attached_and_ends_out_of_sight() {
        let whole = text(&torn(Duration::ZERO, true));
        // Whole, the room the stub is pulled into is empty.
        let room = TEAR.0 + 1;
        assert!(whole.iter().all(|row| row.ends_with(&" ".repeat(room))));
        // Pulled clear, a gap of a few cells opens along the tear.
        let apart = text(&torn(PULLED, true));
        let row: Vec<char> = apart.get(2).expect("a row").chars().collect();
        assert!(
            row.get(21..25)
                .is_some_and(|gap| gap.iter().all(|c| *c == ' ')),
            "{row:?}"
        );
        assert!(has_stub(&torn(PULLED, true)));
        // Fallen, it is gone, and the ticket holds still.
        assert!(!has_stub(&torn(TEARING, true)));
        assert_eq!(torn(TEARING * 2, true), torn(TEARING * 5, true));
    }

    #[test]
    fn the_stub_turns_over_as_it_falls() {
        let back = Color::Rgb(0xa3, 0x36, 0x1b);
        let shows_back = (0..30)
            .map(|step| torn(PULLED + Duration::from_millis(step * 60), true))
            .any(|lines| {
                lines
                    .iter()
                    .flat_map(|line| &line.spans)
                    .any(|span| span.style.fg == Some(back) || span.style.bg == Some(back))
            });
        assert!(shows_back);
    }

    #[test]
    fn the_link_is_typed_out_and_the_code_printed() {
        let link = "https://share.example.com/s/P8D72PSF";
        assert_eq!(typed(link, Duration::ZERO), "");
        let midway = typed(link, Duration::from_millis(600));
        assert!(!midway.is_empty() && midway.len() < link.len(), "{midway}");
        assert_eq!(typed(link, SENT_MOVES), link);
        assert!(printed(Duration::ZERO) <= 0.0);
        assert!(printed(SENT_MOVES) >= 1.0);
    }

    #[test]
    fn the_riding_ticket_goes_as_far_as_the_transfer() {
        let start = text(&riding(Some(0.0), Duration::ZERO, 60, true));
        let end = text(&riding(Some(1.0), Duration::ZERO, 60, true));
        assert!(start.iter().all(|row| row.chars().count() == 60));
        // At the start the ticket is on the left; at the end, on the right.
        assert!(start.iter().any(|row| !row.starts_with(' ')));
        assert!(
            end.iter()
                .all(|row| row.starts_with(&" ".repeat(60 - SMALL[0].len())))
        );
        // A screen too narrow for the ride still gets the ticket, clipped.
        assert!(!text(&riding(None, Duration::ZERO, 10, true)).is_empty());
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
