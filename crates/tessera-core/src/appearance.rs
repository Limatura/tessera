//! A chosen look, as three names.
//!
//! # Why the colours are not here
//!
//! Every theme and every accent is a block of CSS in `packages/ui`, selected by
//! an attribute on `<html>`. This crate, the database and the API only ever
//! hold the name of one: `rose` is a string whose meaning lives in a stylesheet
//! that goes through review like any other code.
//!
//! That split is deliberate rather than incidental. Colours are a design
//! decision and belong in the design, versioned with it. And a value that never
//! becomes anything but one of the names below cannot carry a stylesheet in it,
//! which matters because these end up as attributes on an HTML element.
//!
//! Adding a colour is therefore two changes that have to land together: a
//! variant here and the CSS that answers to it.

use serde::{Deserialize, Serialize};

/// Light, dark, or whichever the device asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Always the light palette.
    Light,
    /// Always the dark one.
    Dark,
    /// Whatever `prefers-color-scheme` says, re-read when it changes.
    System,
}

impl Mode {
    /// The name, as it is stored and as it appears in the markup.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }
}

/// The neutral palette: backgrounds, text, borders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// The one the design ships with.
    Default,
    /// Cooler and flatter.
    Graphite,
    /// Warm, paper-like.
    Sepia,
    /// Warm paper with an olive cast, after the Ghibli-flavoured `matsu` theme.
    Matsu,
    /// Deep blue-violet, a night sky.
    Cosmic,
    /// Cool neutral grey, quieter than `Graphite`.
    Slate,
    /// Aged print: low contrast, tea-stained.
    Vintage,
}

impl Theme {
    /// The name, as it is stored and as it appears in the markup.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Graphite => "graphite",
            Self::Sepia => "sepia",
            Self::Matsu => "matsu",
            Self::Cosmic => "cosmic",
            Self::Slate => "slate",
            Self::Vintage => "vintage",
        }
    }
}

/// The one hue that carries emphasis: buttons, links, focus rings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Accent {
    /// The default.
    Violet,
    /// Red-leaning pink.
    Rose,
    /// Warm orange.
    Orange,
    /// Yellow-leaning orange.
    Amber,
    /// Green.
    Green,
    /// Blue-leaning green.
    Teal,
    /// Blue.
    Blue,
}

impl Accent {
    /// The name, as it is stored and as it appears in the markup.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Violet => "violet",
            Self::Rose => "rose",
            Self::Orange => "orange",
            Self::Amber => "amber",
            Self::Green => "green",
            Self::Teal => "teal",
            Self::Blue => "blue",
        }
    }
}

/// One person's answer to the three questions, with room to have answered none.
///
/// # Why every field is optional
///
/// Somebody who has picked an accent and left the rest alone should keep
/// following the instance's default for the rest, including when the operator
/// changes it. Storing the default alongside their one choice would freeze the
/// other two at whatever they happened to be on the day they clicked, and there
/// would be no way to tell that from having chosen them.
///
/// So absent means "no opinion", and an empty [`Appearance`] is the shape
/// somebody who has never opened the setting is described by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Appearance {
    /// Light, dark or the device's own answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<Mode>,
    /// The neutral palette.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<Theme>,
    /// The emphasis hue.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<Accent>,
}

/// Read from a JSON object and from nothing else.
///
/// # Why this is not derived
///
/// A derived struct deserializer accepts a sequence as well as a map, taking
/// the elements as fields in declaration order. So `[null, null]` arrives here
/// as a valid [`Appearance`] with everything unset, even though the contract
/// says this field is an object, and a fuzzer comparing the two found exactly
/// that.
///
/// It is worth refusing rather than documenting. Positional appearance is not
/// a thing anybody means to send, and the shape that is stored and the shape
/// the API promises should not be two different answers.
impl<'de> Deserialize<'de> for Appearance {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(AppearanceVisitor)
    }
}

struct AppearanceVisitor;

impl<'de> serde::de::Visitor<'de> for AppearanceVisitor {
    type Value = Appearance;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("an object with any of `mode`, `theme` and `accent`")
    }

    fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<Appearance, M::Error> {
        use serde::de::Error as _;

        let mut appearance = Appearance::default();
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "mode" => appearance.mode = map.next_value()?,
                "theme" => appearance.theme = map.next_value()?,
                "accent" => appearance.accent = map.next_value()?,
                // Named rather than ignored. A misspelt key is a preference
                // that silently did not apply, and the person who sent it is a
                // browser that cannot notice.
                other => {
                    return Err(M::Error::unknown_field(other, &["mode", "theme", "accent"]));
                }
            }
        }
        Ok(appearance)
    }
}

impl Appearance {
    /// Whether nothing has been chosen.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.mode.is_none() && self.theme.is_none() && self.accent.is_none()
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::{Accent, Appearance, Mode, Theme};

    /// The names are the contract with the stylesheet and with the stored rows,
    /// so serialising must produce exactly what `as_str` says and nothing
    /// prettier.
    #[rstest]
    #[case(Mode::System, "system")]
    #[case(Mode::Light, "light")]
    #[case(Mode::Dark, "dark")]
    fn a_mode_serialises_as_its_name(#[case] mode: Mode, #[case] expected: &str) {
        assert_eq!(mode.as_str(), expected);
        assert_eq!(
            serde_json::to_string(&mode).expect("a plain enum serialises"),
            format!("\"{expected}\"")
        );
    }

    #[rstest]
    #[case(Theme::Default, "default")]
    #[case(Theme::Graphite, "graphite")]
    #[case(Theme::Sepia, "sepia")]
    fn a_theme_serialises_as_its_name(#[case] theme: Theme, #[case] expected: &str) {
        assert_eq!(theme.as_str(), expected);
        assert_eq!(
            serde_json::to_string(&theme).expect("a plain enum serialises"),
            format!("\"{expected}\"")
        );
    }

    #[rstest]
    #[case(Accent::Violet, "violet")]
    #[case(Accent::Rose, "rose")]
    #[case(Accent::Orange, "orange")]
    #[case(Accent::Amber, "amber")]
    #[case(Accent::Green, "green")]
    #[case(Accent::Teal, "teal")]
    #[case(Accent::Blue, "blue")]
    fn an_accent_serialises_as_its_name(#[case] accent: Accent, #[case] expected: &str) {
        assert_eq!(accent.as_str(), expected);
        assert_eq!(
            serde_json::to_string(&accent).expect("a plain enum serialises"),
            format!("\"{expected}\"")
        );
    }

    /// The reason these are enums and not strings: what ends up in an HTML
    /// attribute cannot be anything the caller felt like sending.
    #[rstest]
    #[case::unknown_accent(r#"{"accent":"chartreuse"}"#)]
    #[case::markup(r#"{"theme":"default\" onload=\"alert(1)"}"#)]
    #[case::wrong_shape(r#"{"mode":["dark"]}"#)]
    #[case::unknown_field(r#"{"colour":"rose"}"#)]
    fn a_value_outside_the_lists_is_refused(#[case] json: &str) {
        assert!(serde_json::from_str::<Appearance>(json).is_err());
    }

    /// A derived struct deserializer takes a sequence as its fields in order,
    /// so `[null, null]` would have been a valid appearance with everything
    /// unset. The contract says this is an object, and the two answers have to
    /// agree.
    #[rstest]
    #[case::positional(r"[null, null]")]
    #[case::empty_sequence(r"[]")]
    #[case::filled_sequence(r#"["dark", "sepia", "rose"]"#)]
    #[case::not_a_structure(r#""dark""#)]
    #[case::a_number(r"1")]
    fn anything_that_is_not_an_object_is_refused(#[case] json: &str) {
        assert!(
            serde_json::from_str::<Appearance>(json).is_err(),
            "accepted {json}"
        );
    }

    /// Somebody who has chosen nothing is described by an empty object, not by
    /// a copy of the instance's default.
    #[test]
    fn having_chosen_nothing_serialises_as_an_empty_object() {
        let none = Appearance::default();
        assert!(none.is_empty());
        assert_eq!(
            serde_json::to_string(&none).expect("a plain struct serialises"),
            "{}"
        );
    }

    /// One choice is stored on its own; the other two stay absent so they keep
    /// following whatever the operator sets.
    #[test]
    fn one_choice_leaves_the_others_absent() {
        let only_accent: Appearance =
            serde_json::from_str(r#"{"accent":"teal"}"#).expect("a known accent");
        assert_eq!(only_accent.accent, Some(Accent::Teal));
        assert_eq!(only_accent.mode, None);
        assert_eq!(only_accent.theme, None);
        assert_eq!(
            serde_json::to_string(&only_accent).expect("a plain struct serialises"),
            r#"{"accent":"teal"}"#
        );
    }
}
