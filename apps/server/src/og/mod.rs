//! The picture a chat client shows when somebody pastes a share link.
//!
//! # Why the server draws it
//!
//! What unfurls a link does not run JavaScript, so the picture has to exist as
//! bytes at a URL. That is the whole reason this is in Rust and not in the app:
//! see [`crate::api::preview`], which puts the tags pointing at it into the
//! page.
//!
//! # Why it is drawn per request and not cached
//!
//! [#12] planned to render each card once and keep it in the object store,
//! keyed by the layout version, the font configuration and the share's own
//! fields. Measured, a card costs about **three milliseconds** — so the cache
//! would be a store to write, a key to get right and an invalidation to get
//! wrong, in exchange for three milliseconds. The route already loads the share
//! row to build the tags; drawing is the cheaper half of what it then does.
//!
//! It also buys something: the fields that move — how long is left, how many
//! retrievals remain — are correct every time they are looked at, because there
//! is no older copy to serve.
//!
//! # What it must not do
//!
//! The rules are [`crate::api::preview`]'s, and they are enforced by sharing
//! its decision rather than repeating it. [`Card`] says what may be drawn; this
//! module only draws it. In particular a locked share reaches here as
//! [`Card::Locked`], which carries no file name, no size and no file count —
//! there is nothing here to leak, because the leak was prevented one layer up.
//!
//! Spending a retrieval is prevented the same way: the route reads the share
//! through [`preview::lookup`](crate::api::preview::lookup), which does not
//! count.
//!
//! # Why the layouts are HTML
//!
//! [#12] wants an editor eventually, and an editor that edits the same source
//! the server renders cannot drift from it. A node tree built in Rust would
//! have meant inventing a format later and maintaining two implementations of
//! it — the preview in the browser and the real thing here.
//!
//! [#12]: https://github.com/Limatura/tessera/issues/12

pub mod code;

use std::result::Result;

use takumi::{from_html, prelude::*, render, write_image};

use crate::api::preview::{Card, Preview};

/// The size every unfurler expects. 1200×630 is what Open Graph documents and
/// what Slack, Discord and Twitter all crop against.
pub const WIDTH: u32 = 1200;
/// See [`WIDTH`].
pub const HEIGHT: u32 = 630;

/// Something went wrong drawing a card.
///
/// Its own type rather than a variant of [`crate::error::Error`]: none of these
/// depend on the request. A card that will not draw means the layout or the
/// embedded fonts are wrong, which is this module's problem, and the caller's
/// only sensible response is to serve the page without a picture.
#[derive(Debug, thiserror::Error)]
pub enum CardError {
    /// An embedded face would not decode. A build problem.
    #[error("an embedded font will not decode: {0}")]
    Font(String),
    /// The layout would not parse.
    #[error("the card layout will not parse: {0}")]
    Layout(String),
    /// The image would not draw or encode.
    #[error("the card will not render: {0}")]
    Render(String),
}

/// The faces, decoded once.
///
/// Decoding is the expensive part of drawing a card — several times the cost of
/// the drawing itself — so this is built at startup and shared. Which faces and
/// why is recorded in `assets/fonts/README.md`.
pub struct Cards {
    fonts: Fonts,
}

impl std::fmt::Debug for Cards {
    /// `Fonts` is a decode cache with nothing printable in it, and the faces it
    /// holds are a compile-time constant — so the count is the only fact worth
    /// showing.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cards")
            .field("faces", &FACES.len())
            .finish_non_exhaustive()
    }
}

/// The embedded faces, in the order they should be consulted.
///
/// Latin first so that a name which is mostly ASCII is set in the face designed
/// for it; the fallbacks behind it, each covering what the ones before it do
/// not.
const FACES: [&[u8]; 6] = [
    include_bytes!("../../assets/fonts/DejaVuSans.woff2"),
    include_bytes!("../../assets/fonts/DejaVuSans-Bold.woff2"),
    include_bytes!("../../assets/fonts/DroidSansFallback.woff2"),
    include_bytes!("../../assets/fonts/NotoSansKR-hangul.woff2"),
    include_bytes!("../../assets/fonts/NotoEmoji.woff2"),
    // Code is set in a monospace face, and not only because it looks right:
    // indentation on a card is drawn as padding measured in character widths,
    // which is a number only when every character is the same width.
    include_bytes!("../../assets/fonts/DejaVuSansMono.woff2"),
];

impl Cards {
    /// Decodes the embedded faces.
    ///
    /// # Errors
    ///
    /// Returns [`CardError::Font`] if a face will not decode, which can only
    /// mean the embedded bytes are wrong — a build problem, not a runtime one.
    pub fn new() -> Result<Self, CardError> {
        let mut fonts = Fonts::default();
        for face in FACES {
            fonts
                .register(FontResource::new(face))
                .map_err(|error| CardError::Font(error.to_string()))?;
        }
        Ok(Self { fonts })
    }

    /// Draws `preview` for the share reached by `code`, as a PNG.
    ///
    /// # Errors
    ///
    /// Returns [`CardError`] if the layout will not parse or the image will not
    /// encode. Neither depends on the share, so both mean this module is broken
    /// rather than the request.
    pub fn draw(&self, preview: &Preview, code: &str) -> Result<Vec<u8>, CardError> {
        let markup = layout(preview, code);
        let node = from_html(&markup, FromHtmlOptions::default())
            .map_err(|error| CardError::Layout(error.to_string()))?;
        let options = RenderOptions::builder()
            .viewport(Viewport::new((WIDTH, HEIGHT)))
            .node(node)
            .fonts(&self.fonts)
            .build();
        let image = render(options).map_err(|error| CardError::Render(error.to_string()))?;

        let mut out = Vec::new();
        write_image(&image, &mut out, OutputFormat::Png)
            .map_err(|error| CardError::Render(error.to_string()))?;
        Ok(out)
    }
}

/// The card's palette, kept together so the three states cannot drift apart.
mod ink {
    /// The ground.
    pub(super) const GROUND: &str = "#0a0605";
    /// The one accent, on everything that is ours rather than the share's.
    pub(super) const ACCENT: &str = "#d94f2b";
    /// Body text.
    pub(super) const TEXT: &str = "#ffffff";
    /// The footer line, and anything the reader does not need to read first.
    pub(super) const QUIET: &str = "#b4a9a5";
    /// A share that is gone, or a headline standing in for one that is absent.
    pub(super) const FADED: &str = "#6b615e";
    /// The chips behind the size and the file count.
    pub(super) const CHIP: &str = "#1c1614";
}

fn layout(preview: &Preview, code: &str) -> String {
    let code = escape(code);
    let headline = escape(preview.title());
    let (label, label_ink, headline_ink, body, footer) = match &preview.card {
        Card::Open {
            label,
            chips,
            footer,
            // A note with one took the branch above; this arm is the file card.
            snippet: _,
        } => (
            *label,
            ink::ACCENT,
            ink::TEXT,
            chip_row(chips),
            escape(footer),
        ),
        Card::Locked { named } => (
            "PASSWORD PROTECTED",
            ink::ACCENT,
            // An absent title is drawn quietly: it is our stand-in sentence, not
            // something the uploader wrote.
            if *named { ink::TEXT } else { ink::FADED },
            String::new(),
            "Enter the password to see what this is".to_owned(),
        ),
        Card::Gone => (
            "NO LONGER AVAILABLE",
            ink::FADED,
            ink::FADED,
            String::new(),
            "It expired, or it has already been collected".to_owned(),
        ),
    };
    // A note with a snippet gets its own arrangement: the code is the picture,
    // so it takes the middle and the headline shrinks to a line above it. Fitted
    // into the shared shell instead, it overflows — and what a 630-pixel card
    // drops off the bottom is the retrieval code.
    if let Card::Open {
        label,
        snippet: Some(snippet),
        footer,
        ..
    } = &preview.card
    {
        return note_layout(label, &headline, snippet, &escape(footer), &code);
    }

    let mark = match &preview.card {
        Card::Open { .. } => document_mark(),
        Card::Locked { .. } => lock_mark(),
        Card::Gone => empty_mark(),
    };

    format!(
        r#"<div style="width:{WIDTH}px;height:{HEIGHT}px;background:{ground};display:flex;flex-direction:column;justify-content:space-between;padding:64px;color:{text};font-family:DejaVu Sans">
  <div style="display:flex;align-items:center;gap:16px">
    <div style="width:40px;height:24px;background:{accent}"></div>
    <div style="font-size:24px;font-weight:700">Tessera</div>
  </div>
  <div style="display:flex;align-items:center;gap:40px">
    {mark}
    <div style="display:flex;flex-direction:column;gap:12px">
      <div style="font-size:20px;font-weight:700;color:{label_ink};letter-spacing:2px">{label}</div>
      <div style="font-size:52px;font-weight:700;color:{headline_ink}">{headline}</div>
      {body}
    </div>
  </div>
  <div style="display:flex;align-items:flex-end;justify-content:space-between">
    <div style="font-size:22px;color:{quiet}">{footer}</div>
    <div style="border:2px dashed {accent};border-radius:12px;padding:20px 32px;font-size:36px;font-weight:700;letter-spacing:4px">{code}</div>
  </div>
</div>"#,
        ground = ink::GROUND,
        text = ink::TEXT,
        accent = ink::ACCENT,
        quiet = ink::QUIET,
    )
}

fn chip_row(chips: &[String]) -> String {
    if chips.is_empty() {
        return String::new();
    }
    let mut boxes = String::new();
    for chip in chips {
        use std::fmt::Write as _;
        // A push rather than `map().collect()`: clippy would rather this than a
        // `format!` per item, and the loop reads the same.
        let _ = write!(
            boxes,
            r#"<div style="background:{chip_ink};border-radius:999px;padding:10px 20px;font-size:22px">{text}</div>"#,
            chip_ink = ink::CHIP,
            text = escape(chip),
        );
    }
    format!(r#"<div style="display:flex;gap:12px">{boxes}</div>"#)
}

/// The card a text share gets: its own code, large, with everything else small.
fn note_layout(
    label: &str,
    headline: &str,
    snippet: &code::Snippet,
    footer: &str,
    code_text: &str,
) -> String {
    let language = escape(&snippet.language);
    format!(
        r#"<div style="width:{WIDTH}px;height:{HEIGHT}px;background:{ground};display:flex;flex-direction:column;justify-content:space-between;padding:48px 56px;color:{text};font-family:DejaVu Sans">
  <div style="display:flex;align-items:center;justify-content:space-between">
    <div style="display:flex;align-items:center;gap:16px">
      <div style="width:40px;height:24px;background:{accent}"></div>
      <div style="font-size:24px;font-weight:700">Tessera</div>
    </div>
    <div style="font-size:20px;color:{quiet}">{language}</div>
  </div>
  <div style="display:flex;flex-direction:column;gap:10px">
    <div style="font-size:18px;font-weight:700;color:{accent};letter-spacing:2px">{label}</div>
    <div style="font-size:32px;font-weight:700">{headline}</div>
  </div>
  {block}
  <div style="display:flex;align-items:center;justify-content:space-between">
    <div style="font-size:20px;color:{quiet}">{footer}</div>
    <div style="border:2px dashed {accent};border-radius:10px;padding:12px 20px;font-size:26px;font-weight:700;letter-spacing:3px">{code_text}</div>
  </div>
</div>"#,
        ground = ink::GROUND,
        text = ink::TEXT,
        accent = ink::ACCENT,
        quiet = ink::QUIET,
        block = code_block(snippet),
    )
}

/// The first lines of a note, coloured, as a block of line boxes.
///
/// One `div` per line because takumi lays out with flexbox, where the newlines
/// inside a `<pre>` are whitespace and the whole snippet would come out as a
/// single line running off the card.
fn code_block(snippet: &code::Snippet) -> String {
    use std::fmt::Write as _;

    let mut lines = String::new();
    for line in &snippet.lines {
        let mut runs = String::new();
        for (colour, text) in &line.runs {
            // Spaces become non-breaking ones: indentation is made of spaces,
            // and a layout engine that collapses them turns nine lines of code
            // into a left-aligned wall.
            let text = escape(text).replace(' ', "\u{a0}");
            let _ = write!(runs, r#"<span style="color:{colour}">{text}</span>"#);
        }
        if runs.is_empty() {
            // A blank line still needs a box, or the lines below it move up.
            r#"<span style="color:#00000000">.</span>"#.clone_into(&mut runs);
        }
        // Indentation as padding, at the width of one monospace character at
        // this size. `ch` units would say it better, but the layout engine
        // takes pixels.
        let pad = line.indent * 13;
        let _ = write!(
            lines,
            r#"<div style="height:30px;display:flex;padding-left:{pad}px">{runs}</div>"#
        );
    }

    let more = if snippet.truncated {
        format!(
            r#"<div style="font-size:20px;color:{faded};margin-top:8px">…and more</div>"#,
            faded = ink::FADED,
        )
    } else {
        String::new()
    };

    format!(
        r#"<div style="display:flex;flex-direction:column;background:{chip};border-radius:12px;padding:20px 24px;font-size:21px;font-family:DejaVu Sans Mono">{lines}{more}</div>"#,
        chip = ink::CHIP,
    )
}

/// The three marks are drawn from boxes rather than an icon font, so that the
/// card needs nothing the binary does not already carry.
fn document_mark() -> String {
    format!(
        r#"<div style="width:88px;height:88px;border:2px solid {accent};border-radius:16px"></div>"#,
        accent = ink::ACCENT,
    )
}

fn lock_mark() -> String {
    format!(
        r#"<div style="width:88px;height:88px;border:2px solid {accent};border-radius:16px;display:flex;align-items:center;justify-content:center">
  <div style="display:flex;flex-direction:column;align-items:center">
    <div style="width:22px;height:14px;border:3px solid {accent};border-bottom-width:0;border-top-left-radius:11px;border-top-right-radius:11px"></div>
    <div style="width:34px;height:24px;background:{accent};border-radius:4px"></div>
  </div>
</div>"#,
        accent = ink::ACCENT,
    )
}

fn empty_mark() -> String {
    format!(
        r#"<div style="width:88px;height:88px;border:2px solid {faded};border-radius:16px"></div>"#,
        faded = ink::FADED,
    )
}

/// Escapes text going into the layout.
///
/// The same hole as the one [`crate::api::preview`] escapes for, one parser
/// along: the title and file name came from whoever made the share, and they
/// are being spliced into HTML that `takumi` will parse. An unescaped `"` ends
/// the style attribute and the rest of the name becomes markup — which on a
/// picture is a broken card rather than script execution, but a name is not
/// allowed to restructure the layout either.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::preview::{Preview, testing};

    /// Writes the cards out so a person can look at them. Ignored by default —
    /// this checks nothing, it produces something to judge.
    #[test]
    #[ignore = "draws the cards to /tmp for a person to look at"]
    fn draw_the_cards() {
        use crate::db::repository::share::{Content, FilePart, ShareKind, StorageLocation};

        let cards = Cards::new().expect("the fonts");
        let content = Content::Files(vec![FilePart {
            segments: Vec::new(),
            filename: Some("Q3-2026-finance-and-operations-review.pdf".to_owned()),
            content_type: None,
            size_bytes: 26_000_000,
            storage: StorageLocation::Inline,
        }]);
        let now = time::OffsetDateTime::now_utc();

        let base = testing::share(ShareKind::Text);
        let note = Preview::of(&base, &content, now).with_snippet(
            "export function withAuth(handler: Handler) {\n  return async (req: Request) => {\n    const token = req.headers.get(\"authorization\");\n    if (!token) return new Response(\"Unauthorized\", { status: 401 });\n\n    const session = await verifySession(token);\n    return handler(req, session);\n  };\n}\n\nexport default withAuth;\n",
            Some("ts"),
        );

        for (name, preview) in [
            ("note", note),
            (
                "file",
                Preview::of(&testing::share(ShareKind::File), &content, now),
            ),
        ] {
            let png = cards.draw(&preview, "K3P9XQ2M").expect("a card");
            std::fs::write(format!("/tmp/card-{name}.png"), png).expect("write");
            println!("wrote /tmp/card-{name}.png");
        }
    }
}
