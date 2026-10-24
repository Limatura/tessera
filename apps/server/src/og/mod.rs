//! The picture a chat client shows for a pasted share link.
//!
//! # Why the server draws it
//!
//! What unfurls a link does not run JavaScript, so the picture has to exist as
//! bytes at a URL. That is why this is in Rust and not in the app:
//! see [`crate::api::preview`], which puts the tags pointing at it into the
//! page.
//!
//! # Why it is drawn per request and not cached
//!
//! Caching each card in the object store was the original plan, keyed by the
//! layout version, the font configuration and the share's own fields. Measured,
//! a card costs about **three milliseconds**, so the cache would be a store to
//! write, a key to get right and an invalidation to get wrong in exchange for
//! three milliseconds. The route already loads the share row to build the tags,
//! and drawing is the cheaper half of what it then does.
//!
//! It also buys something: the fields that move, such as how long is left and
//! how many retrievals remain, are correct every time they are looked at,
//! because there is no older copy to serve.
//!
//! # What it must not do
//!
//! The rules are [`crate::api::preview`]'s, enforced by sharing its decision
//! instead of repeating it. [`Card`] says what may be drawn; this module only
//! draws it. A locked share reaches here as [`Card::Locked`], carrying no file
//! name, no size and no file count, so there is nothing here to leak.
//!
//! Spending a retrieval is prevented the same way: the route reads the share
//! through [`preview::lookup`](crate::api::preview::lookup), which does not
//! count.
//!
//! # Why the layouts are HTML
//!
//! There will be a layout editor eventually, and one that edits the same source
//! the server renders cannot drift from it. A node tree built in Rust would
//! have meant inventing a format later and maintaining two implementations: the
//! preview in the browser and the real thing here.

pub mod cache;
pub mod code;
pub mod layout;

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
/// Its own type, not a variant of [`crate::error::Error`], because none
/// of these depend on the request. A card that will not draw means the layout
/// or the embedded fonts are wrong, and the caller's only sensible response is
/// to serve the page without a picture.
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
/// Decoding is the expensive part of drawing a card, several times the cost of
/// the drawing itself, so this is built at startup and shared. Which faces and
/// why is recorded in `assets/fonts/README.md`.
pub struct Cards {
    fonts: Fonts,
}

impl std::fmt::Debug for Cards {
    /// `Fonts` is a decode cache with nothing printable in it, and the faces it
    /// holds are a compile-time constant, so the count is the only fact worth
    /// showing.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cards")
            .field("faces", &FACES.len())
            .finish_non_exhaustive()
    }
}

/// The embedded fonts, in lookup order.
///
/// Latin first, so a mostly-ASCII name is set in the font designed for it;
/// each fallback covers what the ones before it do not.
///
/// Named as well as embedded, because `GET /fonts/{name}` also serves
/// [`FACES`]. An editor rendering the same card in a browser needs the same
/// fonts in the same order, or its wrapping and glyphs differ from what this
/// server produces, which is exactly what the operator is checking.
pub const FACES: [(&str, &[u8]); 6] = [
    (
        "DejaVuSans.woff2",
        include_bytes!("../../assets/fonts/DejaVuSans.woff2"),
    ),
    (
        "DejaVuSans-Bold.woff2",
        include_bytes!("../../assets/fonts/DejaVuSans-Bold.woff2"),
    ),
    (
        "DroidSansFallback.woff2",
        include_bytes!("../../assets/fonts/DroidSansFallback.woff2"),
    ),
    (
        "NotoSansKR-hangul.woff2",
        include_bytes!("../../assets/fonts/NotoSansKR-hangul.woff2"),
    ),
    (
        "NotoEmoji.woff2",
        include_bytes!("../../assets/fonts/NotoEmoji.woff2"),
    ),
    // Code is set in a monospace face, and not only because it looks right:
    // indentation on a card is drawn as padding measured in character widths,
    // a number only when every character is the same width.
    (
        "DejaVuSansMono.woff2",
        include_bytes!("../../assets/fonts/DejaVuSansMono.woff2"),
    ),
];

impl Cards {
    /// Decodes the embedded faces.
    ///
    /// # Errors
    ///
    /// Returns [`CardError::Font`] if a face will not decode, which can only
    /// mean the embedded bytes are wrong: a build problem, not a runtime one.
    pub fn new() -> Result<Self, CardError> {
        let mut fonts = Fonts::default();
        for (_, face) in FACES {
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
    /// and not the request.
    pub fn draw(&self, preview: &Preview, code: &str) -> Result<Vec<u8>, CardError> {
        self.draw_with(preview, code, "")
    }

    /// The same, with the operator's own layout if they have set one.
    ///
    /// A layout that no longer passes its own rules falls back to the built-in
    /// card and says so in the log. The rules can tighten after a layout was
    /// stored, and a card that is not the operator's design still tells the
    /// reader what the share is. An error on a stranger's screen tells them
    /// nothing.
    ///
    /// # Errors
    ///
    /// As [`Cards::draw`].
    pub fn draw_with(
        &self,
        preview: &Preview,
        code: &str,
        custom: &str,
    ) -> Result<Vec<u8>, CardError> {
        let markup = if custom.trim().is_empty() {
            layout(preview, code)
        } else if let Err(refused) = layout::check(custom) {
            tracing::warn!(%refused, "the configured card layout is no longer allowed; drawing the built-in one");
            layout(preview, code)
        } else {
            layout::fill(custom, &placeholder_values(preview, code))
        };
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

/// What each placeholder is worth for this share.
///
/// Read off [`Card`], the same thing the built-in layout reads. A locked share
/// has no `size` and no `label` to give, so a layout asking for them gets
/// nothing: the discretion is in the value, not in the layout's manners.
fn placeholder_values<'a>(
    preview: &'a Preview,
    code: &'a str,
) -> std::collections::BTreeMap<&'static str, String> {
    let said = preview.words();
    let mut values = std::collections::BTreeMap::new();
    values.insert("code", code.to_owned());
    values.insert("title", preview.title().to_owned());

    match &preview.card {
        Card::Open {
            label,
            chips,
            footer,
            expires,
            snippet,
        } => {
            values.insert("label", (*label).to_owned());
            values.insert("size", chips.join(" · "));
            values.insert("footer", footer.clone());
            if let Some(when) = expires {
                values.insert("expires", when.clone());
            }
            if let Some(snippet) = snippet {
                values.insert("language", snippet.language.clone());
            }
        }
        Card::Sealed { chips, footer } => {
            values.insert("label", said.label_sealed.to_owned());
            values.insert("size", chips.join(" · "));
            values.insert("footer", footer.clone());
        }
        Card::Locked { .. } => {
            values.insert("label", said.label_locked.to_owned());
            values.insert("footer", said.locked_footer.to_owned());
        }
        Card::Gone => {
            values.insert("label", said.label_gone.to_owned());
            values.insert("footer", said.gone_footer.to_owned());
        }
    }
    values
}

/// What each placeholder is worth, reachable from the integration tests.
///
/// The rule it carries, that a locked share has no size to give, is worth
/// testing from outside this crate, where a test cannot reach a private
/// function and asserting on drawn pixels would test the renderer instead.
#[must_use]
pub fn placeholder_values_for_test(
    preview: &Preview,
    code: &str,
) -> std::collections::BTreeMap<&'static str, String> {
    placeholder_values(preview, code)
}

/// The card's palette, kept together so the three states cannot drift apart.
mod ink {
    /// The ground.
    pub(super) const GROUND: &str = "#0a0605";
    /// The one accent, on everything that is ours and not the share's.
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
    let said = preview.words();
    let (label, label_ink, headline_ink, body, footer) = match &preview.card {
        Card::Open {
            label,
            chips,
            footer,
            expires: _,
            // A note with one took the branch above; this arm is the file card.
            snippet: _,
        } => (
            *label,
            ink::ACCENT,
            ink::TEXT,
            chip_row(chips),
            escape(footer),
        ),
        Card::Sealed { chips, footer } => (
            said.label_sealed,
            ink::ACCENT,
            // The headline is our own sentence, not the uploader's, so it is
            // drawn like a missing title.
            ink::FADED,
            chip_row(chips),
            escape(footer),
        ),
        Card::Locked { named } => (
            said.label_locked,
            ink::ACCENT,
            // An absent title is drawn quietly: it is our stand-in sentence, not
            // something the uploader wrote.
            if *named { ink::TEXT } else { ink::FADED },
            String::new(),
            escape(said.locked_footer),
        ),
        Card::Gone => (
            said.label_gone,
            ink::FADED,
            ink::FADED,
            String::new(),
            escape(said.gone_footer),
        ),
    };
    // A note with a snippet gets its own arrangement: the code is the picture,
    // so it takes the middle and the headline shrinks to a line above it.
    // Fitted into the shared shell it overflows, and what a 630-pixel card
    // drops off the bottom is the retrieval code.
    if let Card::Open {
        label,
        snippet: Some(snippet),
        footer,
        ..
    } = &preview.card
    {
        return note_layout(
            label,
            &headline,
            snippet,
            &escape(footer),
            &code,
            said.and_more,
        );
    }

    let mark = match &preview.card {
        Card::Open { .. } => mark(ink::ACCENT, icon::DOCUMENT),
        Card::Sealed { .. } => mark(ink::ACCENT, icon::SEALED),
        Card::Locked { .. } => mark(ink::ACCENT, icon::LOCK),
        Card::Gone => mark(ink::FADED, icon::GONE),
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
        // A push, not `map().collect()`. Clippy prefers this to a `format!`
        // per item, and the loop reads the same.
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
    and_more: &str,
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
        block = code_block(snippet, and_more),
    )
}

/// The first lines of a note, coloured, as a block of line boxes.
///
/// One `div` per line because takumi lays out with flexbox, where the newlines
/// inside a `<pre>` are whitespace and the whole snippet would come out as a
/// single line running off the card.
fn code_block(snippet: &code::Snippet, and_more: &str) -> String {
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
            r#"<div style="font-size:20px;color:{faded};margin-top:8px">{and_more}</div>"#,
            faded = ink::FADED,
            and_more = escape(and_more),
        )
    } else {
        String::new()
    };

    format!(
        r#"<div style="display:flex;flex-direction:column;background:{chip};border-radius:12px;padding:20px 24px;font-size:21px;font-family:DejaVu Sans Mono">{lines}{more}</div>"#,
        chip = ink::CHIP,
    )
}

/// The icon beside the headline, framed.
///
/// Inline SVG, not an icon font or an image: takumi rasterises the subtree
/// itself, so the card still needs nothing outside the binary.
fn mark(colour: &str, shapes: &str) -> String {
    format!(
        r#"<div style="width:88px;height:88px;flex-shrink:0;border:2px solid {colour};border-radius:16px;display:flex;align-items:center;justify-content:center">
  <svg width="44" height="44" viewBox="0 0 24 24" fill="none" stroke="{colour}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{shapes}</svg>
</div>"#
    )
}

/// The shapes inside each mark, on a 24-unit grid.
///
/// Lucide's, the set the web app uses, so the card and the page it links to
/// look consistent.
mod icon {
    /// `file-text`.
    pub(super) const DOCUMENT: &str = r#"<path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z"/><path d="M14 2v5a1 1 0 0 0 1 1h5"/><path d="M10 9H8"/><path d="M16 13H8"/><path d="M16 17H8"/>"#;
    /// `lock`.
    pub(super) const LOCK: &str = r#"<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>"#;
    /// `shield-check`. Not the padlock: a sealed share has no password, and
    /// its promise is that this server cannot read it.
    pub(super) const SEALED: &str = r#"<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/><path d="m9 12 2 2 4-4"/>"#;
    /// `file-x`.
    pub(super) const GONE: &str = r#"<path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z"/><path d="M14 2v5a1 1 0 0 0 1 1h5"/><path d="m14.5 12.5-5 5"/><path d="m9.5 12.5 5 5"/>"#;
}

/// Escapes text going into the layout.
///
/// The same hole [`crate::api::preview`] escapes for, one parser along: the
/// title and file name came from whoever made the share and are spliced into
/// HTML that `takumi` will parse. An unescaped `"` ends the style attribute and
/// the rest of the name becomes markup. On a picture that is a broken card
/// and not script execution, but a name may not restructure the layout.
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

    /// Writes the cards out so a person can look at them. Ignored by default,
    /// since it checks nothing and only produces something to judge.
    #[test]
    #[ignore = "draws the cards to /tmp for a person to look at"]
    fn draw_the_cards() {
        use crate::db::repository::share::{
            Content, Encryption, FilePart, ShareKind, StorageLocation,
        };

        let cards = Cards::new().expect("the fonts");
        let content = Content::Files(vec![FilePart {
            segments: Vec::new(),
            filename: Some("Q3-2026-finance-and-operations-review.pdf".to_owned()),
            content_type: None,
            detected_type: None,
            sealed_name: None,
            scan: None,
            size_bytes: 26_000_000,
            storage: StorageLocation::Inline,
        }]);
        let now = time::OffsetDateTime::now_utc();

        let base = testing::share(ShareKind::Text);
        let note = Preview::of(&base, &content, now, crate::config::Language::En).with_snippet(
            "export function withAuth(handler: Handler) {\n  return async (req: Request) => {\n    const token = req.headers.get(\"authorization\");\n    if (!token) return new Response(\"Unauthorized\", { status: 401 });\n\n    const session = await verifySession(token);\n    return handler(req, session);\n  };\n}\n\nexport default withAuth;\n",
            Some("ts"),
        );

        let file = testing::share(ShareKind::File);
        let locked = crate::db::repository::share::Share {
            password_hash: Some("x".to_owned()),
            ..testing::share(ShareKind::File)
        };
        let sealed = crate::db::repository::share::Share {
            encryption: Some(Encryption::Link),
            ..testing::share(ShareKind::File)
        };
        let gone = crate::db::repository::share::Share {
            disabled_at: Some(now),
            ..testing::share(ShareKind::File)
        };
        let of = |share| Preview::of(share, &content, now, crate::config::Language::En);

        for (name, preview) in [
            ("note", note),
            ("file", of(&file)),
            ("locked", of(&locked)),
            ("sealed", of(&sealed)),
            ("gone", of(&gone)),
        ] {
            let png = cards.draw(&preview, "K3P9XQ2M").expect("a card");
            std::fs::write(format!("/tmp/card-{name}.png"), png).expect("write");
            println!("wrote /tmp/card-{name}.png");
        }
    }
}
