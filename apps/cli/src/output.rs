//! Handing a link on: as a QR code in the terminal, or onto the clipboard.

use std::{
    io::Write as _,
    process::{Command, Stdio},
};

use base64ct::{Base64, Encoding as _};
use qrcode::{QrCode, render::unicode::Dense1x2};

use crate::{Error, Result};

/// Black on white, set explicitly: a terminal's own colours are dark on light
/// or light on dark depending on its theme, and a phone only reads dark on
/// light. Named colours rather than RGB, which every terminal understands.
const BLACK_ON_WHITE: &str = "\x1b[30;47m";

/// Back to the terminal's own colours.
const RESET: &str = "\x1b[0m";

/// `text` as a QR code drawn with block characters, two rows of modules to a
/// line, with its quiet zone, in black on white whatever the terminal's theme,
/// set in from the left edge like the rest of the output.
///
/// # Errors
///
/// When `text` is too long for any QR code, which no link is.
pub fn qr(text: &str) -> Result<String> {
    let code = QrCode::new(text.as_bytes())
        .map_err(|error| Error::Usage(format!("cannot draw that as a QR code: {error}")))?;
    let drawn = code.render::<Dense1x2>().quiet_zone(true).build();
    let mut out = String::with_capacity(drawn.len() * 2);
    for line in drawn.lines() {
        out.push_str("  ");
        out.push_str(BLACK_ON_WHITE);
        out.push_str(line);
        out.push_str(RESET);
        out.push('\n');
    }
    Ok(out)
}

/// How [`copy`] got the text onto a clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Copied {
    /// A clipboard program on this machine took it.
    Here,
    /// It was sent to the terminal as an OSC 52 sequence, for the clipboard
    /// of the machine the terminal runs on. That is the one that matters over
    /// SSH, but not every terminal accepts it, and nothing says whether this
    /// one did.
    ToTerminal,
}

/// Puts `text` on the clipboard: through a clipboard program when this
/// machine has a desktop, otherwise through the terminal.
///
/// # Errors
///
/// When there is neither a clipboard program nor a terminal to ask.
pub fn copy(text: &str) -> Result<Copied> {
    if clipboard_programs()
        .into_iter()
        .any(|(program, args)| pipe_to(program, args, text))
    {
        return Ok(Copied::Here);
    }
    let mut stderr = std::io::stderr();
    if !std::io::IsTerminal::is_terminal(&stderr) {
        return Err(Error::Usage(
            "no clipboard here, and no terminal to pass the link to".to_owned(),
        ));
    }
    write!(
        stderr,
        "\x1b]52;c;{}\x07",
        Base64::encode_string(text.as_bytes())
    )?;
    stderr.flush()?;
    Ok(Copied::ToTerminal)
}

/// The programs that may take a clipboard on this machine, in the order to
/// try them. None on a Linux without a display, where they would fail anyway.
fn clipboard_programs() -> Vec<(&'static str, &'static [&'static str])> {
    if cfg!(target_os = "macos") {
        vec![("pbcopy", &[])]
    } else if cfg!(windows) {
        vec![("clip", &[])]
    } else {
        let mut programs: Vec<(&'static str, &'static [&'static str])> = Vec::new();
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            programs.push(("wl-copy", &[]));
        }
        if std::env::var_os("DISPLAY").is_some() {
            programs.push(("xclip", &["-selection", "clipboard"]));
            programs.push(("xsel", &["--clipboard", "--input"]));
        }
        programs
    }
}

/// Runs `program` with `text` on its standard input, and says whether it took
/// it.
fn pipe_to(program: &str, args: &[&str], text: &str) -> bool {
    let Ok(mut child) = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let written = child
        .stdin
        .take()
        .is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok());
    child.wait().is_ok_and(|status| status.success()) && written
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a camera sees: each character as its two modules, with the block
    /// in the foreground colour (black) and the rest in the background
    /// (white), blown up so the decoder has pixels to work with.
    fn photograph(drawn: &str) -> (usize, usize, Vec<u8>) {
        const SCALE: usize = 4;
        let rows: Vec<Vec<char>> = drawn
            .lines()
            .map(|line| {
                line.trim_start_matches(' ')
                    .strip_prefix(BLACK_ON_WHITE)
                    .and_then(|line| line.strip_suffix(RESET))
                    .expect("every line is coloured")
                    .chars()
                    .collect()
            })
            .collect();
        let width = rows.first().map_or(0, Vec::len);
        let height = rows.len() * 2;
        let mut pixels = vec![255_u8; width * SCALE * height * SCALE];
        for (row, line) in rows.iter().enumerate() {
            for (column, block) in line.iter().enumerate() {
                let (top, bottom) = match block {
                    '\u{2588}' => (true, true),
                    '\u{2580}' => (true, false),
                    '\u{2584}' => (false, true),
                    _ => (false, false),
                };
                for (half, dark) in [(0, top), (1, bottom)] {
                    if !dark {
                        continue;
                    }
                    for dy in 0..SCALE {
                        let y = (row * 2 + half) * SCALE + dy;
                        for dx in 0..SCALE {
                            if let Some(pixel) =
                                pixels.get_mut(y * width * SCALE + column * SCALE + dx)
                            {
                                *pixel = 0;
                            }
                        }
                    }
                }
            }
        }
        (width * SCALE, height * SCALE, pixels)
    }

    #[test]
    fn a_scanner_reads_the_link_back() {
        let link = "https://share.example/device?code=WDJB-MJHT";
        let (width, height, pixels) = photograph(&qr(link).expect("a code"));
        let mut image = rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| {
            pixels.get(y * width + x).copied().unwrap_or(255)
        });
        let grids = image.detect_grids();
        assert_eq!(grids.len(), 1);
        let (_, content) = grids.first().expect("a grid").decode().expect("decoded");
        assert_eq!(content, link);
    }
}
