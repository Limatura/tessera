//! Handing a link on: as a QR code in the terminal, or onto the clipboard.

use std::{
    io::Write as _,
    process::{Command, Stdio},
};

use base64ct::{Base64, Encoding as _};
use qrcode::{QrCode, render::unicode::Dense1x2};

use crate::{Error, Result};

/// `text` as a QR code drawn with block characters, two rows of modules to a
/// line, with its quiet zone.
///
/// Drawn for a dark terminal, the usual kind: the dark modules are left as the
/// background and the light ones are drawn, so a phone sees dark on light.
///
/// # Errors
///
/// When `text` is too long for any QR code, which no link is.
pub fn qr(text: &str) -> Result<String> {
    let code = QrCode::new(text.as_bytes())
        .map_err(|error| Error::Usage(format!("cannot draw that as a QR code: {error}")))?;
    Ok(code
        .render::<Dense1x2>()
        .dark_color(Dense1x2::Light)
        .light_color(Dense1x2::Dark)
        .quiet_zone(true)
        .build())
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

    #[test]
    fn a_link_draws_as_a_square_of_blocks() {
        let drawn = qr("https://share.example/s/P8D72PSF").expect("a code");
        let lines: Vec<&str> = drawn.lines().collect();
        let width = lines.first().map_or(0, |line| line.chars().count());
        assert!(width > 20, "{drawn}");
        assert!(lines.iter().all(|line| line.chars().count() == width));
        // Two rows of modules to a line, give or take the odd last row.
        assert!(lines.len().abs_diff(width / 2) <= 1, "{drawn}");
    }
}
