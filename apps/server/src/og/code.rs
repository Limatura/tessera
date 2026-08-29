//! Colouring the few lines of a text share that fit on its card.
//!
//! # Why the markup is built here instead of by syntect
//!
//! `syntect::html` emits a `<pre>` and relies on the newlines inside it. takumi
//! lays out with flexbox, where a newline in a text run is whitespace and
//! nothing else, so a `<pre>` renders as one very long line running off the
//! card. The lines have to be boxes, which means building the markup from the
//! styled ranges instead of the HTML syntect offers.
//!
//! # What it costs
//!
//! The first card in a given language pays for compiling that syntax's regular
//! expressions. Measured on the release profile: 405 ms for TypeScript, then
//! 1.3 ms for every card after it. Once per language per process, and what
//! waits for it is a crawler, so it is not worth a cache. See the module docs
//! above for the same argument about the cards themselves.

use std::sync::OnceLock;

use syntect::{
    easy::HighlightLines,
    highlighting::{Style, Theme},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

/// Lines shown on the card.
///
/// Seven, not the nine the design draws: nine at a readable size overflows 630
/// pixels once the header and the footer have taken theirs, and what overflows
/// is the retrieval code, the one thing the card must show.
const LINES: usize = 7;
/// Characters kept from any one line. A minified file is one line of fifty
/// thousand characters, and shaping all of it to draw the first eighty is time
/// spent on pixels that never render.
const COLUMNS: usize = 96;

/// The syntaxes and the theme, loaded once.
///
/// Loading is 6 ms and the result is several megabytes of parsed definitions;
/// doing it per request would dominate the render it is part of.
fn assets() -> &'static (SyntaxSet, Theme) {
    static ASSETS: OnceLock<(SyntaxSet, Theme)> = OnceLock::new();
    ASSETS.get_or_init(|| {
        let syntaxes = two_face::syntax::extra_newlines();
        // Nord: a dark theme whose greys sit close to the card's own ground, so
        // the code block reads as part of the card, not a screenshot pasted
        // onto it.
        let theme = two_face::theme::extra()
            .get(two_face::theme::EmbeddedThemeName::Nord)
            .clone();
        (syntaxes, theme)
    })
}

/// One line of the snippet, as coloured runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// How many characters of indentation the line had.
    ///
    /// Carried as a number, not left in the text, because a layout engine may
    /// collapse runs of whitespace, and on a card the difference
    /// between two spaces and four is the difference between code and a wall.
    /// The card draws it as padding.
    pub indent: usize,
    /// `(colour, text)`, in order, with the indentation removed. The colour is
    /// `#rrggbb`.
    pub runs: Vec<(String, String)>,
}

/// The snippet a card draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    /// The coloured lines. At most seven; see the constant this module keeps
    /// and the reason beside it.
    pub lines: Vec<Line>,
    /// Whether the text carried on past what is shown.
    pub truncated: bool,
    /// What the colouring believes this is, for the card to name.
    pub language: String,
}

/// Colours the first few lines of `text`.
///
/// `hint` is the uploader's language, a hint and not a promise. An unknown one
/// falls back to plain text instead of refusing, because a card without colour
/// beats no card.
#[must_use]
pub fn highlight(text: &str, hint: Option<&str>) -> Snippet {
    let (syntaxes, theme) = assets();
    let syntax = hint
        .and_then(|hint| {
            let hint = hint.trim().to_ascii_lowercase();
            syntaxes
                .find_syntax_by_token(&hint)
                .or_else(|| syntaxes.find_syntax_by_name(&hint))
        })
        .unwrap_or_else(|| syntaxes.find_syntax_plain_text());

    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut lines = Vec::with_capacity(LINES);
    let mut seen = 0_usize;

    for raw in LinesWithEndings::from(text) {
        seen += 1;
        if lines.len() >= LINES {
            break;
        }
        let clipped = clip(raw, COLUMNS);
        // A line that will not parse is drawn without colour, not dropped.
        // Losing the fourth line of a snippet silently would be worse
        // than showing it plain.
        let mut runs = highlighter
            .highlight_line(&clipped, syntaxes)
            .map_or_else(|_| vec![(quiet(), clipped.clone())], |ranges| runs(&ranges));
        let indent = take_indent(&mut runs);
        lines.push(Line { indent, runs });
    }

    Snippet {
        lines,
        truncated: seen > LINES,
        language: syntax.name.clone(),
    }
}

/// Keeps the first `columns` characters, and the newline off the end.
fn clip(line: &str, columns: usize) -> String {
    let body = line.trim_end_matches(['\n', '\r']);
    match body.char_indices().nth(columns) {
        Some((at, _)) => format!("{}…", body.get(..at).unwrap_or(body)),
        None => body.to_owned(),
    }
}

fn runs(ranges: &[(Style, &str)]) -> Vec<(String, String)> {
    ranges
        .iter()
        .filter(|(_, text)| !text.is_empty())
        .map(|(style, text)| {
            let colour = format!(
                "#{:02x}{:02x}{:02x}",
                style.foreground.r, style.foreground.g, style.foreground.b
            );
            ((colour), (*text).to_owned())
        })
        .collect()
}

/// Removes the leading whitespace and says how wide it was.
///
/// Tabs count as four. A guess, and the only one available: what a tab
/// is worth is the reader's setting and a card has no reader to ask.
fn take_indent(runs: &mut Vec<(String, String)>) -> usize {
    let mut indent = 0;
    while let Some((_, text)) = runs.first_mut() {
        // Counted by character, not sliced at a byte offset. A run can start
        // with anything, and a boundary computed from two lengths is
        // one refactor away from landing inside a character.
        let mut width = 0;
        let mut leading = 0;
        for character in text.chars() {
            match character {
                ' ' => width += 1,
                '\t' => width += 4,
                _ => break,
            }
            leading += character.len_utf8();
        }
        indent += width;

        let rest = text.get(leading..).unwrap_or_default().to_owned();
        if rest.is_empty() {
            runs.remove(0);
        } else {
            *text = rest;
            break;
        }
    }
    indent
}

/// The colour a line falls back to when it will not parse.
fn quiet() -> String {
    "#c0c5ce".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Indentation survives as a number, because a layout engine may not let it
    /// survive as whitespace.
    #[test]
    fn indentation_is_measured_and_taken_out_of_the_text() {
        let snippet = highlight("if (x) {\n    return 1;\n}\n", Some("ts"));

        assert_eq!(snippet.lines[0].indent, 0);
        assert_eq!(snippet.lines[1].indent, 4);
        let second: String = snippet.lines[1]
            .runs
            .iter()
            .map(|(_, text)| text.as_str())
            .collect();
        assert!(
            second.starts_with("return"),
            "the indentation is still in the text: {second:?}"
        );
    }

    /// A tab is worth four: a guess, and the only one available.
    #[test]
    fn a_tab_counts_as_four() {
        let snippet = highlight("x\n\t\ty\n", None);

        assert_eq!(snippet.lines[1].indent, 8);
    }

    #[test]
    fn a_known_language_is_coloured() {
        let snippet = highlight("const x: number = 1;\n", Some("ts"));

        assert_eq!(snippet.language, "TypeScript");
        let colours: Vec<&str> = snippet.lines[0]
            .runs
            .iter()
            .map(|(colour, _)| colour.as_str())
            .collect();
        assert!(
            colours
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                > 1,
            "everything came out one colour: {colours:?}"
        );
    }

    /// An unknown hint is not an error. The uploader typed it; a card without
    /// colour beats a card that would not draw.
    #[test]
    fn an_unknown_language_falls_back_to_plain_text() {
        let snippet = highlight("hello\n", Some("not-a-language"));

        assert_eq!(snippet.language, "Plain Text");
        assert_eq!(snippet.lines.len(), 1);
    }

    /// The count is load-bearing, not taste. Any more and the retrieval code
    /// falls off the bottom of the card.
    #[test]
    fn only_the_first_lines_are_kept() {
        let mut text = String::new();
        for i in 0..40 {
            use std::fmt::Write as _;
            let _ = writeln!(text, "line {i}");
        }
        let snippet = highlight(&text, None);

        assert_eq!(snippet.lines.len(), LINES);
        assert!(snippet.truncated);
    }

    /// A minified file is one enormous line, and shaping all of it to draw the
    /// first eighty characters is work that never reaches the card.
    #[test]
    fn a_very_long_line_is_clipped() {
        let snippet = highlight(&"x".repeat(10_000), None);

        let drawn: usize = snippet.lines[0]
            .runs
            .iter()
            .map(|(_, text)| text.chars().count())
            .sum();
        assert!(drawn <= COLUMNS + 1, "kept {drawn} characters");
    }

    /// Multi-byte characters must not be cut in half, so `clip` slices by
    /// character index.
    #[test]
    fn clipping_does_not_split_a_character() {
        let snippet = highlight(&"字".repeat(500), None);

        let text: String = snippet.lines[0]
            .runs
            .iter()
            .map(|(_, text)| text.as_str())
            .collect();
        assert!(text.chars().all(|c| c == '字' || c == '…'), "{text}");
    }
}
