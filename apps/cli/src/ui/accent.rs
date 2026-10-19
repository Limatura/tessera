//! The site's accents, for a terminal.

use ratatui::style::Color;

use crate::{Error, Result, logo};

/// Each accent the site offers: its name, the lighter of its two shades on the
/// site (most terminals are dark), and the nearest of the 256 colours.
const ACCENTS: [(&str, (u8, u8, u8), u8); 7] = [
    ("violet", (0x9d, 0x72, 0xff), 135),
    ("rose", (0xf6, 0x5d, 0x6d), 203),
    ("orange", (0xca, 0x35, 0x00), 166),
    ("amber", (0xf1, 0xae, 0x2e), 214),
    ("green", (0x4b, 0xb7, 0x66), 71),
    ("teal", (0x31, 0xb5, 0xa5), 73),
    ("blue", (0x5b, 0x91, 0xff), 69),
];

/// The accent called `name`, drawn as this terminal can.
pub(crate) fn named(name: &str, truecolor: bool) -> Option<Color> {
    ACCENTS
        .iter()
        .find(|(known, _, _)| *known == name)
        .map(|(_, (red, green, blue), indexed)| {
            if truecolor {
                Color::Rgb(*red, *green, *blue)
            } else {
                Color::Indexed(*indexed)
            }
        })
}

/// The ticket's orange, until the site says otherwise.
pub(crate) const fn brand(truecolor: bool) -> Color {
    let ink = logo::Ink::Stub;
    if truecolor {
        let (red, green, blue) = ink.rgb();
        Color::Rgb(red, green, blue)
    } else {
        Color::Indexed(ink.indexed())
    }
}

/// Refuses an accent in the config file that the site does not have.
///
/// # Errors
///
/// When `name` is not one of them.
pub(crate) fn check(name: &str) -> Result<()> {
    if named(name, false).is_some() {
        return Ok(());
    }
    let names: Vec<&str> = ACCENTS.iter().map(|(known, _, _)| *known).collect();
    Err(Error::Usage(format!(
        "[ui] accent = \"{name}\" is not an accent; the accents are {}",
        names.join(", ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_site_accent_has_a_colour() {
        for name in ["violet", "rose", "orange", "amber", "green", "teal", "blue"] {
            assert!(named(name, true).is_some(), "{name}");
            assert!(check(name).is_ok());
        }
        assert_eq!(named("teal", false), Some(Color::Indexed(73)));
        let error = check("pink").expect_err("refused");
        assert!(error.to_string().contains("violet, rose"), "{error}");
    }
}
