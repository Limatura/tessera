//! QR codes in the view: as a picture where the terminal draws pictures, so
//! its modules come out square, and otherwise in half blocks.
//!
//! Dark on light either way, as on the site, since a reader may not take a
//! code the other way round. The finder squares in three corners take the
//! accent, darkened until a reader still counts them as dark.

use image::{DynamicImage, Rgb, RgbImage};
use ratatui::{
    Frame,
    layout::{Rect, Size},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use ratatui_image::{
    Image, Resize,
    picker::{Picker, ProtocolType},
    protocol::Protocol,
};

use crate::{logo::Ink, output};

/// How bright a finder square may be, as luma out of 255. The paper is
/// near 240, and a reader splits dark from light about halfway.
const DARKEST_LUMA: u32 = 100;

/// A QR code made ready to draw, and the cells it takes.
pub(crate) struct Drawn {
    art: Art,
    pub(crate) width: u16,
    pub(crate) height: u16,
}

enum Art {
    /// Drawn afresh for each frame, as far as it has printed.
    Blocks {
        matrix: output::Matrix,
        finder: (u8, u8, u8),
        head: (u8, u8, u8),
        truecolor: bool,
    },
    /// One picture for each step of the printing, the last the code whole.
    Picture(Vec<Protocol>),
}

/// How many steps a picture is printed in.
const STEPS: usize = 12;

/// How far a code has printed: its first `rows` rows of modules, and on the
/// row after them, while there is one, the print head, in its colour.
#[derive(Clone, Copy)]
struct Printing {
    rows: usize,
    head: Option<(u8, u8, u8)>,
}

impl Printing {
    /// `part` of the way through a code `side` modules square, from 0 to 1.
    fn of(side: usize, part: f32, head: (u8, u8, u8)) -> Self {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            clippy::as_conversions,
            reason = "a row count, from a fraction between 0 and 1"
        )]
        let rows = ((side as f32) * part.clamp(0.0, 1.0)).ceil() as usize;
        Self {
            rows,
            head: (rows < side).then_some(head),
        }
    }

    /// What module row `y` shows.
    const fn row(self, y: usize) -> Row {
        match self.head {
            _ if y < self.rows => Row::Printed,
            Some(head) if y == self.rows => Row::Head(head),
            _ => Row::Blank,
        }
    }
}

/// A row of modules, as far as the printing has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    /// Printed: as the code says.
    Printed,
    /// Under the print head: a line in its colour.
    Head((u8, u8, u8)),
    /// Not printed yet: paper.
    Blank,
}

impl Drawn {
    /// `link` as a QR code: through `picker` when the terminal draws
    /// pictures, otherwise in half blocks, in 24-bit colour or not.
    pub(crate) fn new(
        link: &str,
        accent: Color,
        picker: Option<&Picker>,
        truecolor: bool,
    ) -> Option<Self> {
        let matrix = output::Matrix::new(link).ok()?;
        let head = rgb_of(accent);
        let finder = darkened(head);
        if let Some(picker) =
            picker.filter(|picker| picker.protocol_type() != ProtocolType::Halfblocks)
            && let Some(drawn) = picture(&matrix, (finder, head), picker)
        {
            return Some(drawn);
        }
        let side = u16::try_from(matrix.side()).ok()?;
        Some(Self {
            art: Art::Blocks {
                matrix,
                finder,
                head,
                truecolor,
            },
            width: side,
            height: side.div_ceil(2),
        })
    }

    /// The code at `at`, `printed` of the way through coming out of a ticket
    /// machine, from 0 to 1: the paper first, then the modules a row at a
    /// time under a print head in the accent.
    pub(crate) fn render(&self, frame: &mut Frame<'_>, at: Rect, printed: f32) {
        let area = Rect {
            width: self.width.min(at.width),
            height: self.height.min(at.height),
            ..at
        };
        match &self.art {
            Art::Blocks {
                matrix,
                finder,
                head,
                truecolor,
            } => {
                let printing = Printing::of(matrix.side(), printed, *head);
                frame.render_widget(
                    Paragraph::new(blocks(matrix, *finder, *truecolor, printing)),
                    area,
                );
            }
            Art::Picture(steps) => {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    clippy::cast_precision_loss,
                    clippy::as_conversions,
                    reason = "a step, from a fraction between 0 and 1"
                )]
                let step = ((STEPS as f32) * printed.clamp(0.0, 1.0)).floor() as usize;
                if let Some(protocol) = steps.get(step.min(STEPS)) {
                    frame.render_widget(Image::new(protocol), area);
                }
            }
        }
    }
}

/// One pixel to a cell's width per module, so the code is as wide as in half
/// blocks but square, and as many rows as that takes, on paper: a picture
/// for each step of its printing.
fn picture(
    matrix: &output::Matrix,
    (finder, head): ((u8, u8, u8), (u8, u8, u8)),
    picker: &Picker,
) -> Option<Drawn> {
    let font = picker.font_size();
    let mut cells = Size::default();
    let mut steps = Vec::with_capacity(STEPS + 1);
    for step in 0..=STEPS {
        #[expect(
            clippy::cast_precision_loss,
            clippy::as_conversions,
            reason = "a step of a dozen"
        )]
        let part = step as f32 / STEPS as f32;
        let printing = Printing::of(matrix.side(), part, head);
        let (image, size) = painted(matrix, finder, (font.width, font.height), printing)?;
        cells = size;
        steps.push(
            picker
                .new_protocol(DynamicImage::ImageRgb8(image), size, Resize::Fit(None))
                .ok()?,
        );
    }
    Some(Drawn {
        art: Art::Picture(steps),
        width: cells.width,
        height: cells.height,
    })
}

/// The code painted for cells `font` pixels across and down, and how many
/// cells the painting covers.
fn painted(
    matrix: &output::Matrix,
    finder: (u8, u8, u8),
    font: (u16, u16),
    printing: Printing,
) -> Option<(RgbImage, Size)> {
    let (cell_width, cell_height) = (u32::from(font.0), u32::from(font.1));
    if cell_width < 2 || cell_height < 2 {
        return None;
    }
    let side = u32::try_from(matrix.side()).ok()?;
    let module = cell_width;
    let width = side;
    let height = (side * module).div_ceil(cell_height);
    let pixels_down = height * cell_height;
    // Centred down the rows, which rarely come out even.
    let top = (pixels_down - side * module) / 2;
    let paper = rgb(Ink::Paper.rgb());
    let print = rgb(Ink::Print.rgb());
    let finder = rgb(finder);
    let image = RgbImage::from_fn(width * cell_width, pixels_down, |x, y| {
        let Some(y) = y.checked_sub(top) else {
            return paper;
        };
        let at = |pixel: u32| usize::try_from(pixel / module).unwrap_or(usize::MAX);
        let (x, y) = (at(x), at(y));
        if let Row::Head(head) = printing.row(y) {
            rgb(head)
        } else if printing.row(y) == Row::Blank || !matrix.is_dark(x, y) {
            paper
        } else if matrix.is_finder(x, y) {
            finder
        } else {
            print
        }
    });
    let cells = Size::new(u16::try_from(width).ok()?, u16::try_from(height).ok()?);
    Some((image, cells))
}

/// Two modules to a cell, one above the other.
fn blocks(
    matrix: &output::Matrix,
    finder: (u8, u8, u8),
    truecolor: bool,
    printing: Printing,
) -> Vec<Line<'static>> {
    let paint = |colour: (u8, u8, u8)| {
        if truecolor {
            Color::Rgb(colour.0, colour.1, colour.2)
        } else {
            Color::Indexed(nearest_indexed(colour))
        }
    };
    let paper = paint(Ink::Paper.rgb());
    let dark = paint(Ink::Print.rgb());
    let finder = paint(finder);
    let mut lines = Vec::new();
    for y in (0..matrix.side()).step_by(2) {
        // A row not yet printed, or the print head on one, half a cell.
        let (upper_row, lower_row) = (printing.row(y), printing.row(y + 1));
        if lower_row != Row::Printed {
            let module = |row: Row, x: usize, y: usize| match row {
                Row::Head(head) => Some(paint(head)),
                Row::Blank => None,
                Row::Printed => matrix
                    .is_dark(x, y)
                    .then(|| if matrix.is_finder(x, y) { finder } else { dark }),
            };
            let spans = (0..matrix.side())
                .map(
                    |x| match (module(upper_row, x, y), module(lower_row, x, y + 1)) {
                        (None, None) => Span::styled(" ", Style::new().bg(paper)),
                        (Some(ink), None) => Span::styled("▀", Style::new().fg(ink).bg(paper)),
                        (None, Some(ink)) => Span::styled("▄", Style::new().fg(ink).bg(paper)),
                        (Some(top), Some(bottom)) => {
                            Span::styled("▀", Style::new().fg(top).bg(bottom))
                        }
                    },
                )
                .collect::<Vec<_>>();
            lines.push(Line::from(spans));
            continue;
        }
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut run = String::new();
        let mut ink = dark;
        for x in 0..matrix.side() {
            let (upper, lower) = (matrix.is_dark(x, y), matrix.is_dark(x, y + 1));
            // A cell never holds a finder square and another dark module:
            // a light row or column parts them.
            let here = if !upper && !lower {
                ink
            } else if (upper && matrix.is_finder(x, y)) || (lower && matrix.is_finder(x, y + 1)) {
                finder
            } else {
                dark
            };
            if here != ink && !run.is_empty() {
                spans.push(Span::styled(
                    std::mem::take(&mut run),
                    Style::new().fg(ink).bg(paper),
                ));
            }
            ink = here;
            run.push(output::block(upper, lower));
        }
        spans.push(Span::styled(run, Style::new().fg(ink).bg(paper)));
        lines.push(Line::from(spans));
    }
    lines
}

const fn rgb((red, green, blue): (u8, u8, u8)) -> Rgb<u8> {
    Rgb([red, green, blue])
}

/// The six levels of the 256-colour cube along each axis.
const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// A colour as red, green and blue: exact for 24-bit and the colour cube,
/// and the print ink for anything else, which no accent is.
fn rgb_of(colour: Color) -> (u8, u8, u8) {
    match colour {
        Color::Rgb(red, green, blue) => (red, green, blue),
        Color::Indexed(index @ 16..=231) => {
            let at = index - 16;
            let level = |step: u8| LEVELS.get(usize::from(step)).copied().unwrap_or(0);
            (level(at / 36), level(at / 6 % 6), level(at % 6))
        }
        _ => Ink::Print.rgb(),
    }
}

/// `colour`, darkened as far as it must be to read as dark.
fn darkened((red, green, blue): (u8, u8, u8)) -> (u8, u8, u8) {
    let luma = (299 * u32::from(red) + 587 * u32::from(green) + 114 * u32::from(blue)) / 1000;
    if luma <= DARKEST_LUMA {
        return (red, green, blue);
    }
    let scale =
        |channel: u8| u8::try_from(u32::from(channel) * DARKEST_LUMA / luma).unwrap_or(u8::MAX);
    (scale(red), scale(green), scale(blue))
}

/// The nearest colour of the 256-colour cube.
fn nearest_indexed((red, green, blue): (u8, u8, u8)) -> u8 {
    let step = |channel: u8| {
        LEVELS
            .iter()
            .enumerate()
            .min_by_key(|(_, level)| level.abs_diff(channel))
            .map_or(0, |(at, _)| u8::try_from(at).unwrap_or(0))
    };
    16 + 36 * step(red) + 6 * step(green) + step(blue)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_light_accent_is_darkened_and_a_dark_one_kept() {
        let amber = darkened((0xf1, 0xae, 0x2e));
        let luma =
            (299 * u32::from(amber.0) + 587 * u32::from(amber.1) + 114 * u32::from(amber.2)) / 1000;
        assert!(luma <= DARKEST_LUMA, "{amber:?}");
        assert!(amber.0 > amber.2, "still amber: {amber:?}");
        assert_eq!(darkened((0xca, 0x35, 0x00)), (0xca, 0x35, 0x00));
    }

    #[test]
    fn the_colour_cube_goes_both_ways() {
        assert_eq!(rgb_of(Color::Indexed(166)), (215, 95, 0));
        assert_eq!(nearest_indexed((215, 95, 0)), 166);
    }

    #[test]
    fn the_picture_reads_back_whatever_the_accent() {
        let link = "https://share.example/s/WDJB-MJHT";
        let matrix = output::Matrix::new(link).expect("a code");
        for accent in [(0xca, 0x35, 0x00), (0xf1, 0xae, 0x2e), (0x9d, 0x72, 0xff)] {
            let whole = Printing {
                rows: matrix.side(),
                head: None,
            };
            let (image, cells) =
                painted(&matrix, darkened(accent), (10, 26), whole).expect("painted");
            assert_eq!(cells.width, u16::try_from(matrix.side()).expect("small"));
            let (width, height) = (
                usize::try_from(image.width()).expect("small"),
                usize::try_from(image.height()).expect("small"),
            );
            let mut prepared =
                rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| {
                    let pixel = image.get_pixel(
                        u32::try_from(x).expect("small"),
                        u32::try_from(y).expect("small"),
                    );
                    let [red, green, blue] = pixel.0;
                    u8::try_from(
                        (299 * u32::from(red) + 587 * u32::from(green) + 114 * u32::from(blue))
                            / 1000,
                    )
                    .expect("a luma")
                });
            let grids = prepared.detect_grids();
            let (_, content) = grids.first().expect("a grid").decode().expect("decoded");
            assert_eq!(content, link, "{accent:?}");
        }
    }

    #[test]
    fn a_code_prints_from_the_top_under_its_head() {
        let matrix = output::Matrix::new("https://example.com/s/ABCD").expect("a code");
        let head = (0xd9, 0x4f, 0x2b);
        let printing = Printing::of(matrix.side(), 0.5, head);
        assert_eq!(printing.row(0), Row::Printed);
        assert_eq!(printing.row(printing.rows), Row::Head(head));
        assert_eq!(printing.row(matrix.side() - 1), Row::Blank);
        let done = Printing::of(matrix.side(), 1.0, head);
        assert_eq!(done.row(matrix.side() - 1), Row::Printed);
        assert!(done.head.is_none());
    }

    #[test]
    fn finder_squares_take_their_own_ink() {
        let drawn = Drawn::new(
            "https://example.com/s/ABCD",
            Color::Rgb(0, 0, 200),
            None,
            true,
        )
        .expect("a code");
        let Art::Blocks {
            matrix,
            finder,
            truecolor,
            ..
        } = &drawn.art
        else {
            panic!("half blocks without a picker");
        };
        let whole = Printing {
            rows: matrix.side(),
            head: None,
        };
        let lines = blocks(matrix, *finder, *truecolor, whole);
        // The second row of cells holds a finder square's top edge at the left.
        let first = lines
            .get(1)
            .and_then(|line| line.spans.iter().find(|span| span.content.trim() != ""))
            .expect("a span");
        assert_eq!(first.style.fg, Some(Color::Rgb(0, 0, 200)));
    }
}
