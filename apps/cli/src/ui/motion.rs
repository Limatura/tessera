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

/// The gusts that worry at the stub before it goes: when each comes, in
/// milliseconds, and how hard, from 0 to 1. The last tears it off.
const GUSTS: [(u64, f32); 3] = [(300, 0.35), (750, 0.6), (1200, 0.9)];

/// How long a gust takes.
const GUST: Duration = Duration::from_millis(400);

/// When the stub lets go: at the height of the last gust.
const LET_GO: Duration = Duration::from_millis(1400);

/// How long [`torn`] moves for; after that it holds still, the stub and the
/// leaves on the ground.
pub(crate) const TEARING: Duration = Duration::from_millis(3300);

/// Where the stub starts on the ticket, in pixels from its left.
const PERFORATION: usize = 21;

/// How tall the room is that the stub falls through, in pixels: as tall as a
/// QR code beside it, so the dialog does not grow for it. The ground is its
/// last row.
const FALL_HEIGHT: usize = 26;

/// The narrowest room [`torn`] draws, in pixels: the ticket and some air.
pub(crate) const FALL_WIDTH: usize = 40;

/// How far through its fall the stub reaches the ground.
const LANDS: f32 = 0.78;

/// A leaf blown past.
struct Leaf {
    /// When it sets off and how long it takes, in milliseconds.
    start: u64,
    length: u64,
    /// The pixel row it starts on.
    top: f32,
    /// How far across the room it gets, from 0 to 1.
    reach: f32,
    /// How many times it sways on the way, and by how many pixels.
    sways: f32,
    size: f32,
    /// Its colour, in 24 bits and in 256.
    rgb: (u8, u8, u8),
    indexed: u8,
}

const LEAVES: [Leaf; 3] = [
    Leaf {
        start: 250,
        length: 1900,
        top: 1.0,
        reach: 0.92,
        sways: 1.3,
        size: 3.0,
        rgb: (0xe0, 0x9a, 0x3c),
        indexed: 179,
    },
    Leaf {
        start: 700,
        length: 1800,
        top: 5.0,
        reach: 0.25,
        sways: 2.0,
        size: 2.0,
        rgb: (0xb5, 0x4a, 0x22),
        indexed: 130,
    },
    Leaf {
        start: 1150,
        length: 1750,
        top: 2.0,
        reach: 0.82,
        sways: 1.6,
        size: 3.5,
        rgb: (0x8a, 0x5a, 0x2b),
        indexed: 94,
    },
];

/// A leaf, slanting, in pixels from its top left.
const LEAF: [(usize, usize); 4] = [(0, 0), (1, 0), (1, 1), (2, 1)];

/// The small ticket in a room `width` pixels wide, `elapsed` into an autumn
/// wind blowing down from the right. Gust after gust the stub flaps on its
/// perforation, trembling in between, until the last tears it off. It comes
/// down as paper does, carried down and to the left, swinging and tipping,
/// turning over to show its back, and lands flat on the ground under the
/// ticket, where the leaves each gust brought settle too.
pub(crate) fn torn(elapsed: Duration, width: usize, truecolor: bool) -> Vec<Line<'static>> {
    let width = width.max(FALL_WIDTH);
    let shade = |rgb: (u8, u8, u8), indexed: u8| {
        if truecolor {
            Color::Rgb(rgb.0, rgb.1, rgb.2)
        } else {
            Color::Indexed(indexed)
        }
    };
    let mut canvas = Canvas::new(width, FALL_HEIGHT);
    let ground = FALL_HEIGHT - 1;
    for x in 0..width {
        canvas.set(x, ground, shade((0x3b, 0x32, 0x2e), 236));
    }
    for (index, leaf) in LEAVES.iter().enumerate() {
        if let Some((x, y)) = leaf_at(elapsed, index, width) {
            for (across, down) in LEAF {
                if let (Some(x), Some(y)) = (whole(x), whole(y)) {
                    canvas.set(x + across, y + down, shade(leaf.rgb, leaf.indexed));
                }
            }
        }
    }
    // The ticket in front of the leaves.
    canvas.paint(&SMALL, |x, y, _| (x, y), |ink| colour(ink, truecolor));
    canvas.clear(PERFORATION..PERFORATION + stub_size().0, 0..SMALL.len());
    let flight = Flight::at(elapsed, float(ground));
    let ink = if flight.turned {
        shade((0xa3, 0x36, 0x1b), 130)
    } else {
        colour(Ink::Stub, truecolor)
    };
    flight.draw(&mut canvas, ink);
    canvas.lines()
}

/// Where leaf `index` is `elapsed` in, in a room `width` pixels wide: its
/// top left, in pixels, coming in past the right edge and blown to the left.
/// None before it sets off.
fn leaf_at(elapsed: Duration, index: usize, width: usize) -> Option<(f32, f32)> {
    use std::f32::consts::TAU;
    let leaf = LEAVES.get(index)?;
    let (top, reach, sways, size) = (leaf.top, leaf.reach, leaf.sways, leaf.size);
    let start = Duration::from_millis(leaf.start);
    if elapsed < start {
        return None;
    }
    let done = fraction(elapsed, start, Duration::from_millis(leaf.length));
    let ground = float(FALL_HEIGHT - 1) - 1.0;
    let still = 1.0 - done;
    let x = float(width) + 1.0 - float(width) * reach * (1.0 - still.powf(1.6))
        + size * (TAU * sways * done + float(index) * 1.7).sin() * still;
    let y = top
        + (ground - top) * (0.45 * done * done + 0.55 * done)
        + 0.8 * (TAU * sways * 2.0 * done).cos() * still;
    Some((x, y.min(ground)))
}

/// Where the stub is and how it lies, part of the way through its fall.
struct Flight {
    /// Its middle, in pixels.
    centre: (f32, f32),
    /// How far it has tipped, in radians, clockwise.
    tilt: f32,
    /// How wide it looks, turning over: 1 flat on, less edge on.
    breadth: f32,
    /// Whether its back is to the viewer.
    turned: bool,
}

impl Flight {
    /// The stub `elapsed` into the wind, over a ground at row `ground`.
    fn at(elapsed: Duration, ground: f32) -> Self {
        use std::f32::consts::{FRAC_PI_2, PI, TAU};
        let (width, height) = stub_size();
        let middle = ((float(width) - 1.0) / 2.0, (float(height) - 1.0) / 2.0);
        // Held at its perforation, it flaps: each gust folds it back towards
        // the viewer and bends it down, and between them it trembles.
        let gusting: f32 = GUSTS
            .iter()
            .map(|&(at, strength)| {
                let into = fraction(elapsed, Duration::from_millis(at), GUST);
                strength * (PI * into).sin()
            })
            .sum();
        let trembling = 0.04 * (elapsed.as_secs_f32() * TAU * 9.0).sin();
        let hinged = |flap: f32, shake: f32| {
            let breadth = (1.0 - 0.75 * flap).max(0.25);
            let tilt = 0.3 * flap + shake;
            // The edge along the perforation stays where it is.
            let reach = (middle.0 + 0.5) * breadth;
            let hinge = (float(PERFORATION) - 0.5, middle.1);
            (
                (hinge.0 + reach * tilt.cos(), hinge.1 + reach * tilt.sin()),
                tilt,
                breadth,
            )
        };
        if elapsed < LET_GO {
            let (centre, tilt, breadth) = hinged(gusting, trembling);
            return Self {
                centre,
                tilt,
                breadth,
                turned: false,
            };
        }
        // Let go where the last gust left it, and carried off.
        let (start, tilt_then, breadth_then) = hinged(gusting, 0.0);
        let falling = fraction(elapsed, LET_GO, TEARING.saturating_sub(LET_GO));
        let air = (falling / LANDS).min(1.0);
        let settled = smooth((falling - LANDS) / (1.0 - LANDS));
        // Swinging less as it nears the ground, tipping with the swing as a
        // pendulum does, and blown down and to the left.
        let swing = (TAU * 1.25 * air).sin() * (air * 4.0).min(1.0) * (1.0 - air);
        let rest = ground - 1.0;
        // Down first, clear of the ticket, then off to the left.
        let x = start.0 - 5.0 * air * air + 3.0 * swing - 1.5 * settled;
        let y = start.1 + (rest - start.1) * (0.3 * air * air + 0.7 * air);
        // Turning on from how far the gust had folded it: its back to the
        // viewer partway down.
        let turning = (TAU * air + breadth_then.clamp(-1.0, 1.0).acos()).cos();
        // Lying down as it lands: tipped onto its side, seen as a strip.
        let lying = if falling > LANDS {
            1.0
        } else {
            smooth((air - 0.8) / 0.2)
        };
        let flying = 1.0 - lying;
        let letting = (air * 5.0).min(1.0);
        Self {
            centre: (x, y),
            tilt: (tilt_then * (1.0 - letting) - 0.5 * swing) * flying + FRAC_PI_2 * lying,
            breadth: turning.abs().max(0.25) * flying + 0.22 * lying,
            turned: turning < 0.0 && lying < 0.5,
        }
    }

    /// The stub, as it lies, onto `canvas` in `ink`.
    fn draw(&self, canvas: &mut Canvas, ink: Color) {
        let (width, height) = stub_size();
        let middle = ((float(width) - 1.0) / 2.0, (float(height) - 1.0) / 2.0);
        let (sin, cos) = self.tilt.sin_cos();
        for y in 0..canvas.height {
            for x in 0..canvas.width {
                // Back from the screen to the stub: undo the tilt, then the
                // turn.
                let (across, down) = (float(x) - self.centre.0, float(y) - self.centre.1);
                let along = (across * cos + down * sin) / self.breadth + middle.0;
                let up = (down * cos - across * sin) + middle.1;
                if stub_has(along.round(), up.round()) {
                    canvas.set(x, y, ink);
                }
            }
        }
    }
}

/// How far along `elapsed` is of a motion that waits `delay` and then takes
/// `length`, from 0 to 1, evenly.
fn fraction(elapsed: Duration, delay: Duration, length: Duration) -> f32 {
    (elapsed.saturating_sub(delay).as_secs_f32() / length.as_secs_f32()).clamp(0.0, 1.0)
}

/// From 0 to 1 as `x` goes from 0 to 1, starting and stopping gently.
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// A pixel count as a position to work with.
#[expect(
    clippy::cast_precision_loss,
    clippy::as_conversions,
    reason = "pixel counts of a few dozen"
)]
const fn float(count: usize) -> f32 {
    count as f32
}

/// A position as a pixel, when it lies on the canvas side of zero.
fn whole(position: f32) -> Option<usize> {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::as_conversions,
        reason = "a rounded, non-negative position of a few dozen pixels"
    )]
    let pixel = (position >= -0.5).then(|| position.round().max(0.0) as usize);
    pixel
}

/// The stub's width and height in pixels.
fn stub_size() -> (usize, usize) {
    let width = SMALL.first().map_or(0, |row| row.len());
    (width.saturating_sub(PERFORATION), SMALL.len())
}

/// Whether the stub has a pixel at `x`, `y` of its own, given as whole
/// numbers that may lie outside it.
fn stub_has(x: f32, y: f32) -> bool {
    let (Some(x), Some(y)) = (whole(x), whole(y)) else {
        return false;
    };
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
    const FROM: Duration = Duration::from_millis(550);
    const FOR: Duration = Duration::from_millis(750);
    (elapsed.saturating_sub(FROM).as_secs_f32() / FOR.as_secs_f32()).clamp(0.0, 1.0)
}

/// How long the dialog for something sent keeps moving: the tear and the
/// fall, the link typed out and the code printed, with a frame or two to spare so that the
/// last one drawn shows them finished.
pub(crate) const SENT_MOVES: Duration = Duration::from_millis(3450);

// Stopping before the stub has landed would leave it hanging in the air.
const _: () = assert!(SENT_MOVES.as_millis() > TEARING.as_millis());

/// The ticket machine, in pixels: its body, the edge the light catches, the
/// slot the ticket comes out of on the right, and its lamp.
const MACHINE: [&str; 18] = [
    ".eeeeeeeee",
    "ebbbbbbbbb",
    "ebllbbbbbs",
    "ebllbbbbbs",
    "ebbbbbbbbs",
    "ebbbbbbbbs",
    "ebbbbbbbbs",
    "ebbbbbbbbs",
    "ebbbbbbbbs",
    "ebbbbbbbbs",
    "ebbbbbbbbs",
    "ebbbbbbbbs",
    "ebbbbbbbbs",
    "ebbbbbbbbs",
    "ebbbbbbbbs",
    "ebbbbbbbbs",
    "ebbbbbbbbb",
    ".bbbbbbbb.",
];

/// The ticket machine printing a ticket as far as `done` says, from 0 to 1:
/// the ticket coming out of the slot stub first, the lamp blinking while it
/// prints. With no telling how far, the ticket goes out and back in.
pub(crate) fn printing(
    done: Option<f32>,
    elapsed: Duration,
    truecolor: bool,
) -> Vec<Line<'static>> {
    let shade = |rgb: (u8, u8, u8), indexed: u8| {
        if truecolor {
            Color::Rgb(rgb.0, rgb.1, rgb.2)
        } else {
            Color::Indexed(indexed)
        }
    };
    let ticket = SMALL.first().map_or(0, |row| row.len());
    let machine = MACHINE.first().map_or(0, |row| row.len());
    let out = done.unwrap_or_else(|| {
        // Out and back: a triangle, from 0 to 1 and down again.
        let lap = (elapsed.as_secs_f32() * 0.4).fract() * 2.0;
        if lap > 1.0 { 2.0 - lap } else { lap }
    });
    let shown = whole(float(ticket) * out.clamp(0.0, 1.0)).unwrap_or(0);
    let mut canvas = Canvas::new(machine + 1 + ticket, MACHINE.len());
    // The ticket first, so the machine hides what is still inside it.
    let top = (MACHINE.len() - SMALL.len()) / 2;
    canvas.paint(
        &SMALL,
        |x, y, _| {
            (machine + x + shown)
                .checked_sub(ticket)
                .map_or((usize::MAX, 0), |x| (x, y + top))
        },
        |ink| colour(ink, truecolor),
    );
    let lit = done.is_none_or(|done| done < 1.0) && (elapsed.as_millis() / 300).is_multiple_of(2);
    for (y, row) in MACHINE.iter().enumerate() {
        for (x, pixel) in row.bytes().enumerate() {
            let paint = match pixel {
                b'e' => shade((0x63, 0x66, 0x78), 242),
                b'b' => shade((0x45, 0x48, 0x57), 238),
                b's' => shade((0x0d, 0x0d, 0x12), 233),
                b'l' if lit => colour(Ink::Stub, truecolor),
                b'l' => shade((0x2a, 0x2b, 0x36), 236),
                _ => continue,
            };
            canvas.set(x, y, paint);
        }
    }
    canvas.lines()
}

/// A spinner's frame, `elapsed` in: for waiting on something with no telling
/// how long.
pub(crate) fn spinner(elapsed: Duration) -> &'static str {
    const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let frame = usize::try_from(elapsed.as_millis() / 100).unwrap_or(0) % FRAMES.len();
    FRAMES.get(frame).copied().unwrap_or(" ")
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

    /// The columns, from the left, where the stub shows in `lines`.
    fn stub_columns(lines: &[Line<'_>]) -> Vec<usize> {
        let stub = colour(Ink::Stub, true);
        lines
            .iter()
            .flat_map(|line| line.spans.iter().enumerate())
            .filter(|(_, span)| span.style.fg == Some(stub) || span.style.bg == Some(stub))
            .map(|(column, _)| column)
            .collect()
    }

    #[test]
    fn the_stub_holds_on_and_then_lands_under_the_ticket() {
        let whole = text(&torn(Duration::ZERO, FALL_WIDTH, true));
        // Whole, and nothing in the air yet; the last row holds the ground.
        let (_, air) = whole.split_last().expect("rows");
        assert!(air.iter().all(|row| row.ends_with(&" ".repeat(6))));
        // Through the gusts it stays by its perforation, flapping.
        for at in [500, 900, 1300] {
            let lines = torn(Duration::from_millis(at), FALL_WIDTH, true);
            let columns = stub_columns(&lines);
            assert!(!columns.is_empty(), "at {at}");
            assert!(
                columns.iter().all(|column| *column >= PERFORATION - 1),
                "at {at}"
            );
        }
        // Landed, it lies on the ground to the left, and stays.
        let landed = torn(TEARING, FALL_WIDTH, true);
        let (above, ground) = landed.split_at(landed.len() - 2);
        assert!(stub_columns(above).is_empty());
        assert!(
            stub_columns(ground)
                .iter()
                .any(|column| *column < PERFORATION)
        );
        assert_eq!(
            torn(TEARING * 2, FALL_WIDTH, true),
            torn(TEARING * 5, FALL_WIDTH, true)
        );
    }

    #[test]
    fn the_stub_turns_over_as_it_falls() {
        let back = Color::Rgb(0xa3, 0x36, 0x1b);
        let shows_back = (0..30)
            .map(|step| torn(LET_GO + Duration::from_millis(step * 60), FALL_WIDTH, true))
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
    fn the_machine_prints_the_ticket_as_far_as_the_transfer() {
        let paper = colour(Ink::Paper, true);
        let papers = |done| {
            printing(Some(done), Duration::ZERO, true)
                .iter()
                .flat_map(|line| line.spans.clone())
                .filter(|span| span.style.fg == Some(paper) || span.style.bg == Some(paper))
                .count()
        };
        // Nothing out yet, then more of it the further it has got.
        assert_eq!(papers(0.0), 0);
        assert!(papers(0.9) > papers(0.5));
        assert!(papers(0.5) > 0);
        // With no telling how far, it still draws.
        assert!(!printing(None, Duration::from_secs(1), true).is_empty());
    }

    #[test]
    fn the_spinner_turns() {
        assert_ne!(spinner(Duration::ZERO), spinner(Duration::from_millis(100)));
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
