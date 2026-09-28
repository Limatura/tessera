//! Link previews for `/s/{code}`.
//!
//! # Why the server does this and not the page
//!
//! Whatever unfurls a link (Discord, Slack, a mail client) fetches the URL and
//! reads the HTML without running JavaScript, so a single-page app can put
//! whatever it likes in `document.head` and the preview still shows what the
//! shipped `index.html` said. The tags have to be in the bytes the server
//! sends.
//!
//! That does not make this server-side rendering. The page is the same embedded
//! `index.html` as everywhere else; a handful of `<meta>` elements are inserted
//! into its `<head>` on the way out, and the app takes over as usual.
//!
//! # What it must not do
//!
//! - **Spend a retrieval.** An unfurler follows the link the moment it is pasted, before any person
//!   clicks. Counting that would destroy a one-time share between sending it and reading it.
//! - **Describe a locked share.** A preview is public by construction, shown to everyone in the
//!   channel. If a password-protected share announced its file name and size there, the password
//!   would be decorative.

mod words;

pub use words::{Words, words};

use crate::{
    config::Language,
    db::repository::share::{Availability, Content, Share, ShareKind},
    state::AppState,
};

/// What the picture may draw, decided in the same place as the words.
///
/// Both the tags and [`crate::og`] read this instead of each deciding what is
/// safe to say. A locked share whose description gives nothing away while its
/// picture names the file is exactly the failure the rule prevents, and two
/// implementations of one rule is how that happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Card {
    /// Available. Everything about it may be shown.
    Open {
        /// The eyebrow above the headline.
        label: &'static str,
        /// Short facts beside it: the size, and a file count when there are
        /// several.
        chips: Vec<String>,
        /// The line along the bottom: when it goes, and how many goes are left.
        ///
        /// Not the size, which the chips already show.
        footer: String,
        /// When it goes, alone, for a layout that places it apart.
        expires: Option<String>,
        /// The first lines of a text share, coloured. `None` for a file, and
        /// for a note whose body was not read.
        #[cfg(feature = "og-images")]
        snippet: Option<crate::og::code::Snippet>,
    },
    /// End-to-end encrypted. The server can say nothing about the content but
    /// its size, and says that since a download would reveal it anyway.
    Sealed {
        /// The size, and a file count when there are several.
        chips: Vec<String>,
        /// When it expires, and how many downloads are left.
        footer: String,
    },
    /// Password-protected. Only a title the uploader typed themselves appears.
    Locked {
        /// Whether there was a title. Without one the headline is our own
        /// stand-in sentence, and the card draws it quietly to say so.
        named: bool,
    },
    /// Expired, exhausted, or switched off.
    Gone,
}

/// The tags to insert, already escaped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    title: String,
    description: String,
    /// What the card's own words are written in.
    language: Language,
    /// What the picture is allowed to draw. See [`Card`].
    pub card: Card,
}

impl Preview {
    /// What a share should say about itself to a channel full of people, in
    /// `language`.
    #[must_use]
    pub fn of(
        share: &Share,
        content: &Content,
        now: time::OffsetDateTime,
        language: Language,
    ) -> Self {
        let said = words(language);
        let facts = Facts::of(share, content, now, said);
        let (title, description, card) = match share.availability_at(now) {
            Availability::Expired | Availability::Exhausted | Availability::Disabled => (
                said.gone_title.to_owned(),
                said.gone_description.to_owned(),
                Card::Gone,
            ),
            // A title is the one thing a locked share may say. The uploader
            // typed it as a label for other people; the file name came off
            // their disk, and they never agreed to publish that.
            Availability::Available if share.needs_password() => (
                share
                    .title
                    .clone()
                    .unwrap_or_else(|| said.locked_title.to_owned()),
                said.locked_description.to_owned(),
                Card::Locked {
                    named: share.title.is_some(),
                },
            ),
            // After the password check, so a locked encrypted share reveals no
            // more than any other locked share, size included.
            Availability::Available if share.encryption.is_some() => (
                said.sealed_title.to_owned(),
                facts.description(),
                Card::Sealed {
                    chips: chips(share, content, said),
                    footer: facts.footer(),
                },
            ),
            Availability::Available => (
                share.title.clone().unwrap_or_else(|| match share.kind {
                    ShareKind::File => match content.files() {
                        [only] => only
                            .filename
                            .clone()
                            .unwrap_or_else(|| said.shared_file.to_owned()),
                        // A count, not the first name. Naming one of five
                        // discloses one of five.
                        many => (said.shared_files)(many.len()),
                    },
                    ShareKind::Text => said.shared_note.to_owned(),
                }),
                facts.description(),
                Card::Open {
                    label: match share.kind {
                        ShareKind::File if content.files().len() > 1 => said.label_folder,
                        ShareKind::File => said.label_file,
                        ShareKind::Text => said.label_note,
                    },
                    chips: chips(share, content, said),
                    footer: facts.footer(),
                    expires: facts.expires,
                    #[cfg(feature = "og-images")]
                    snippet: None,
                },
            ),
        };
        Self {
            title,
            description,
            language,
            card,
        }
    }

    /// The phrases the card's own words come from.
    #[must_use]
    pub const fn words(&self) -> &'static Words {
        words(self.language)
    }

    /// Attaches the first lines of a note's body, coloured.
    ///
    /// A no-op unless the share is [`Card::Open`], and that is why it lives
    /// here. The caller reads a body without knowing whether the
    /// share may describe itself, and that is already decided here. A locked
    /// note keeps its code to itself with no caller having to remember.
    #[cfg(feature = "og-images")]
    #[must_use]
    pub fn with_snippet(mut self, body: &str, language: Option<&str>) -> Self {
        if let Card::Open { snippet, .. } = &mut self.card {
            *snippet = Some(crate::og::code::highlight(body, language));
        }
        self
    }

    /// A digest of everything this preview would draw.
    ///
    /// # Why
    ///
    /// It is an entity tag computable *without rendering the card*. An
    /// unfurler returning to a link it has seen sends the tag it holds, and
    /// answering costs a lookup and a digest instead of a render.
    ///
    /// # Why a counting-down card still has a stable one
    ///
    /// The countdown is already coarse. `expires_in` rounds the way people
    /// speak, precisely to avoid a different sentence every time a crawler
    /// looks. So a share renders the same bytes while the sentence holds, and
    /// the digest changes exactly when the image would.
    ///
    /// Everything drawn goes in, and nothing else. A raw expiry would give
    /// every request a different tag for an identical image, which is the
    /// mistake this avoids.
    #[must_use]
    pub fn fingerprint(&self) -> [u8; 32] {
        use sha2::{Digest as _, Sha256};

        let mut digest = Sha256::new();
        // Lengths as well as contents. Without them `["ab", "c"]` and
        // `["a", "bc"]` are the same byte stream, and two different cards
        // would share a tag.
        let mut field = |bytes: &[u8]| {
            digest.update(bytes.len().to_le_bytes());
            digest.update(bytes);
        };
        field(self.title.as_bytes());
        field(self.description.as_bytes());
        // The card's own words, which the title and description do not always
        // carry: a locked share's title is the uploader's in every language.
        field(self.words().label_locked.as_bytes());

        match &self.card {
            Card::Open {
                label,
                chips,
                footer,
                // Part of `footer`, already counted.
                expires: _,
                #[cfg(feature = "og-images")]
                snippet,
            } => {
                field(b"open");
                field(label.as_bytes());
                field(footer.as_bytes());
                field(&chips.len().to_le_bytes());
                for chip in chips {
                    field(chip.as_bytes());
                }
                #[cfg(feature = "og-images")]
                if let Some(snippet) = snippet {
                    field(b"snippet");
                    field(snippet.language.as_bytes());
                    field(&[u8::from(snippet.truncated)]);
                    field(&snippet.lines.len().to_le_bytes());
                    for line in &snippet.lines {
                        field(&line.indent.to_le_bytes());
                        for (colour, text) in &line.runs {
                            field(colour.as_bytes());
                            field(text.as_bytes());
                        }
                    }
                }
            }
            Card::Sealed { chips, footer } => {
                field(b"sealed");
                field(footer.as_bytes());
                field(&chips.len().to_le_bytes());
                for chip in chips {
                    field(chip.as_bytes());
                }
            }
            Card::Locked { named } => {
                field(b"locked");
                field(&[u8::from(*named)]);
            }
            Card::Gone => field(b"gone"),
        }

        digest.finalize().into()
    }

    /// The headline: a title the uploader typed, or what stands in for one.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The tags, ready to splice into a `<head>`.
    ///
    /// `origin` is where this instance is reachable from outside, without a
    /// trailing slash; see [`origin`]. Without one the picture is left out
    /// instead of given a relative path, since `og:image` is read by a crawler
    /// on another machine where that resolves to nothing.
    #[must_use]
    pub fn to_head_tags(&self, origin: Option<&str>, code: &str, site: &str) -> String {
        let title = escape(&self.title);
        let description = escape(&self.description);
        let site_name = escape(site);

        let picture = origin.map_or_else(String::new, |origin| {
            let image = escape(&format!("{origin}/s/{code}/og.png"));
            let page = escape(&format!("{origin}/s/{code}"));
            let width = CARD_WIDTH;
            let height = CARD_HEIGHT;
            format!(
                "<meta property=\"og:url\" content=\"{page}\">\
                 <meta property=\"og:image\" content=\"{image}\">\
                 <meta property=\"og:image:width\" content=\"{width}\">\
                 <meta property=\"og:image:height\" content=\"{height}\">\
                 <meta property=\"og:image:type\" content=\"image/png\">\
                 <meta property=\"og:image:alt\" content=\"{title}. {description}\">\
                 <meta name=\"twitter:image\" content=\"{image}\">"
            )
        });

        // `summary_large_image` only when there is one: the small card is what a
        // client falls back to, and asking for the large one with no picture
        // gets a large blank.
        let twitter_card = if origin.is_some() {
            "summary_large_image"
        } else {
            "summary"
        };

        format!(
            "<meta property=\"og:title\" content=\"{title}\">\
             <meta property=\"og:description\" content=\"{description}\">\
             <meta property=\"og:type\" content=\"website\">\
             <meta property=\"og:site_name\" content=\"{site_name}\">\
             {picture}\
             <meta name=\"twitter:card\" content=\"{twitter_card}\">\
             <meta name=\"twitter:title\" content=\"{title}\">\
             <meta name=\"twitter:description\" content=\"{description}\">\
             <meta name=\"description\" content=\"{description}\">"
        )
    }
}

/// The short facts that sit beside the headline on the picture.
fn chips(share: &Share, content: &Content, said: &Words) -> Vec<String> {
    let mut chips = vec![human_size(content.size_bytes())];
    if share.kind == ShareKind::File {
        let count = content.files().len();
        if count > 1 {
            chips.push((said.files)(count));
        }
    }
    chips
}

/// What can be said about a share besides its name, each part on its own.
///
/// Apart because the card and the description want different subsets: the
/// card shows the size as a chip, so its footer leaves it out.
struct Facts {
    size: String,
    expires: Option<String>,
    left: Option<String>,
}

impl Facts {
    fn of(share: &Share, content: &Content, now: time::OffsetDateTime, said: &Words) -> Self {
        Self {
            size: match share.kind {
                ShareKind::File => human_size(content.size_bytes()),
                ShareKind::Text => (said.text_of)(&human_size(content.size_bytes())),
            },
            expires: share
                .expires_at
                .and_then(|when| expires_in(when, now, said)),
            left: share
                .use_limit
                .map(|limit| (said.left)(limit.saturating_sub(share.use_count))),
        }
    }

    /// Everything, for the description a chat client shows under the title.
    fn description(&self) -> String {
        std::iter::once(&self.size)
            .chain(&self.expires)
            .chain(&self.left)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// Everything but the size, for the card's footer.
    fn footer(&self) -> String {
        self.expires
            .iter()
            .chain(&self.left)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// How long is left, rounded the way a person would say it.
///
/// Coarse by design. A countdown to the second would be a different sentence
/// every time a crawler looked, and this is a caption, not a timer.
fn expires_in(
    when: time::OffsetDateTime,
    now: time::OffsetDateTime,
    said: &Words,
) -> Option<String> {
    let left = when - now;
    if left <= time::Duration::ZERO {
        return None;
    }
    let days = left.whole_days();
    if days >= 1 {
        return Some(if days == 1 {
            said.expires_tomorrow.to_owned()
        } else {
            (said.expires_in_days)(days)
        });
    }
    let hours = left.whole_hours();
    Some(if hours > 1 {
        (said.expires_in_hours)(hours)
    } else {
        said.expires_within_hour.to_owned()
    })
}

/// A size the way a person would say it.
///
/// Integer arithmetic throughout. This is a caption, and a float would trade a
/// lossy conversion for precision that "2.0 KiB" does not need.
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    const STEP: u64 = 1024;

    let mut scale: u64 = 1;
    let mut unit = 0;
    while bytes / scale >= STEP && unit + 1 < UNITS.len() {
        scale = scale.saturating_mul(STEP);
        unit += 1;
    }

    let name = UNITS.get(unit).copied().unwrap_or("B");
    if unit == 0 {
        return format!("{bytes} {name}");
    }
    let whole = bytes / scale;
    // One decimal place, rounded, without leaving integer arithmetic.
    let tenths = (bytes % scale).saturating_mul(10).saturating_add(scale / 2) / scale;
    let (whole, tenths) = if tenths >= 10 {
        (whole.saturating_add(1), 0)
    } else {
        (whole, tenths)
    };
    format!("{whole}.{tenths} {name}")
}

/// Escapes text for an HTML attribute.
///
/// The title and file name came from whoever made the share and are spliced
/// into a `content="..."` attribute on a page served from this origin. Without
/// this, `"><script>` closes the attribute, closes the tag and runs: a
/// cross-site scripting hole reachable by anyone who can upload.
///
/// Both quote characters are escaped, not only the one the template uses, so
/// changing the template cannot reopen the hole.
pub(crate) fn escape(value: &str) -> String {
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

/// Splices `tags` into the document.
///
/// Preferably just inside `<head>`, so ours come before anything the bundle
/// ships: where a crawler sees the same property twice, it keeps the first.
///
/// A document without a `<head>` still gets them, after the doctype or at the
/// very start. An HTML parser hoists leading `<meta>` elements into the head it
/// creates, and an unfurler reads bytes and not a tree, so both work. The
/// first version gave up when there was no `<head>`, which left the shipped
/// placeholder page silently without previews, and any change to the bundle's
/// template could do the same without a word.
#[must_use]
pub fn splice_into_document(html: &str, tags: &str) -> String {
    let at = insertion_point(html);
    let mut out = String::with_capacity(html.len() + tags.len());
    out.push_str(html.get(..at).unwrap_or_default());
    out.push_str(tags);
    out.push_str(html.get(at..).unwrap_or_default());
    out
}

/// Byte offset to insert at: after `<head …>`, else after the doctype, else 0.
fn insertion_point(html: &str) -> usize {
    let lowered = html.to_ascii_lowercase();

    if let Some(open) = lowered.find("<head")
        && let Some(end) = lowered.get(open..).and_then(|rest| rest.find('>'))
    {
        return open + end + 1;
    }
    if let Some(open) = lowered.find("<!doctype")
        && let Some(end) = lowered.get(open..).and_then(|rest| rest.find('>'))
    {
        return open + end + 1;
    }
    0
}

/// The card's dimensions, repeated here so the tags can state them without
/// depending on a module that may not be compiled in.
const CARD_WIDTH: u32 = 1200;
/// See [`CARD_WIDTH`].
const CARD_HEIGHT: u32 = 630;

/// Where this instance is reachable from outside, with no trailing slash.
///
/// The configured value wins. It exists because deriving this is guesswork in
/// exactly the deployments this is for: a reverse proxy that rewrites `Host`, a
/// container that only hears `localhost:8080`. When set, nothing about the
/// request changes the answer, so a forged `Host` header cannot make our page
/// advertise a picture from elsewhere.
///
/// Falling back to the request still beats emitting nothing, since the common
/// case of one container behind a proxy that passes `Host` through is right
/// with no configuration.
#[must_use]
pub fn origin(config: &crate::config::Config, headers: &axum::http::HeaderMap) -> Option<String> {
    let configured = config.server.public_url.trim().trim_end_matches('/');
    if !configured.is_empty() {
        return Some(configured.to_owned());
    }

    let host = headers.get(axum::http::header::HOST)?.to_str().ok()?;
    if host.is_empty() {
        return None;
    }
    // A proxy that terminates TLS says so here; without it, assume what we are
    // actually speaking.
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .map_or("http", |value| {
            if value.eq_ignore_ascii_case("https") {
                "https"
            } else {
                "http"
            }
        });
    Some(format!("{scheme}://{host}"))
}

/// Fixtures other modules' tests need. Compiled only for tests.
#[cfg(test)]
pub mod testing {
    use tessera_core::{RetrievalCode, ShareId};
    use time::OffsetDateTime;

    use crate::db::repository::share::{Share, ShareKind};

    /// A share with nothing set: no password, and three days left.
    ///
    /// # Panics
    ///
    /// If the hard-coded retrieval code stops being valid, which would mean the
    /// code format changed and every fixture using it wants looking at.
    #[must_use]
    pub fn share(kind: ShareKind) -> Share {
        Share {
            role_id: None,
            id: ShareId::generate(),
            code: RetrievalCode::parse("K3P9XQ2M").expect("a valid code"),
            user_id: None,
            kind,
            password_hash: None,
            use_limit: Some(5),
            use_count: 0,
            expires_at: Some(OffsetDateTime::now_utc() + time::Duration::days(3)),
            disabled_at: None,
            title: None,
            encryption: None,
            sealed_details: None,
            og_config: None,
            management_token_hash: None,
            view_count: 0,
            copy_count: 0,
            bytes_served: 0,
            bytes_redirected: 0,
            last_accessed_at: None,
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
        }
    }
}

/// Reads the share a `/s/{code}` page is about, without touching its counters.
///
/// Not [`crate::api::gate::open`], which refuses expired shares and counts
/// misses against the lookup limit. Neither is right here. A
/// crawler following a dead link should get a page that explains itself, and
/// should not be able to exhaust a limit meant for people guessing.
pub async fn lookup(state: &AppState, code: &str) -> Option<(Share, Content)> {
    let code = tessera_core::RetrievalCode::parse(code).ok()?;
    let share = state.shares.find_by_code(&code).await.ok().flatten()?;
    let content = state.shares.content_of(&share).await.ok()?;
    Some((share, content))
}

#[cfg(test)]
mod tests {
    use tessera_core::{RetrievalCode, ShareId};
    use time::OffsetDateTime;

    use super::*;
    use crate::db::repository::share::{FilePart, StorageLocation};

    /// The content as the owning table describes it, where a preview reads the
    /// name and the size from.
    fn content() -> Content {
        Content::Files(vec![FilePart {
            segments: Vec::new(),
            filename: None,
            content_type: None,
            sealed_name: None,
            scan: None,
            size_bytes: 2048,
            storage: StorageLocation::Inline,
        }])
    }

    /// The same, with a name on the one file.
    fn named(filename: &str) -> Content {
        Content::Files(vec![FilePart {
            segments: Vec::new(),
            filename: Some(filename.to_owned()),
            content_type: None,
            sealed_name: None,
            scan: None,
            size_bytes: 2048,
            storage: StorageLocation::Inline,
        }])
    }

    /// Everything a preview produces about a locked share, as one haystack.
    fn everything_said(share: &Share, content: &Content) -> String {
        let preview = Preview::of(share, content, OffsetDateTime::now_utc(), Language::En);
        format!(
            "{} {} {:?}",
            preview.title, preview.description, preview.card
        )
    }

    /// The card's own words follow the instance's language; the uploader's
    /// title and file name are left as they are.
    #[test]
    fn a_preview_is_written_in_the_instance_language() {
        let preview = Preview::of(
            &testing::share(ShareKind::File),
            &named("報告.pdf"),
            OffsetDateTime::now_utc(),
            Language::ZhTw,
        );
        assert_eq!(preview.title, "報告.pdf");
        assert!(
            preview.description.contains("剩餘 5 次提取"),
            "{}",
            preview.description
        );
        let Card::Open {
            label,
            footer,
            expires,
            ..
        } = &preview.card
        else {
            panic!("an open card: {:?}", preview.card);
        };
        assert_eq!(*label, "分享的檔案");
        assert_eq!(expires.as_deref(), Some("2 天後到期"));
        assert_eq!(footer, "2 天後到期 · 剩餘 5 次提取");
    }

    /// The size is a chip on the card, so the footer does not say it again.
    /// The description has no chips and keeps it.
    #[test]
    fn the_card_says_the_size_once() {
        let preview = Preview::of(
            &testing::share(ShareKind::File),
            &content(),
            OffsetDateTime::now_utc(),
            Language::En,
        );
        let Card::Open { chips, footer, .. } = &preview.card else {
            panic!("an open card: {:?}", preview.card);
        };
        assert_eq!(chips, &["2.0 KiB"]);
        assert!(!footer.contains("KiB"), "{footer}");
        assert!(
            preview.description.starts_with("2.0 KiB · "),
            "{}",
            preview.description
        );
    }

    /// The rule this module exists for. A preview is shown to everyone in
    /// the channel, so a locked share must not name what is inside it.
    #[test]
    fn a_locked_share_never_names_its_file() {
        let mut share = share(ShareKind::File);
        share.password_hash = Some("argon2-something".to_owned());

        let said = everything_said(&share, &named("severance-agreement.pdf"));

        assert!(
            !said.contains("severance"),
            "a locked share leaked its file name: {said}"
        );
        assert!(
            !said.contains("2.0 KiB"),
            "a locked share leaked its size: {said}"
        );
    }

    /// The exception, and the reason it is safe: a title is something the
    /// uploader typed as a label for other people.
    #[test]
    fn a_locked_share_shows_a_title_the_uploader_typed() {
        let mut share = share(ShareKind::File);
        share.password_hash = Some("argon2-something".to_owned());
        share.title = Some("Board pack, October".to_owned());

        let said = everything_said(&share, &named("severance-agreement.pdf"));

        assert!(
            said.contains("Board pack, October"),
            "the title was dropped: {said}"
        );
        assert!(
            !said.contains("severance"),
            "a title is not permission to publish the file name: {said}"
        );
        assert!(said.contains("named: true"));
    }

    /// Expired or collected says nothing about what it was either. "The 24.8 MB
    /// file has been collected" still tells a channel plenty.
    #[test]
    fn a_share_that_is_gone_describes_nothing() {
        let mut share = share(ShareKind::File);
        share.expires_at = Some(OffsetDateTime::now_utc() - time::Duration::hours(1));
        share.title = Some("Board pack, October".to_owned());

        let said = everything_said(&share, &named("severance-agreement.pdf"));

        assert!(!said.contains("severance"), "{said}");
        assert!(!said.contains("Board pack"), "{said}");
        assert!(!said.contains("2.0 KiB"), "{said}");
    }

    /// A note's card carries its first lines; a locked note's does not.
    ///
    /// The caller reads the body without knowing whether the share may describe
    /// itself, so that one place decides. This is that place.
    #[cfg(feature = "og-images")]
    #[test]
    fn a_snippet_reaches_an_open_card_and_not_a_locked_one() {
        let text = "const secret = \"hunter2\";\n";

        let open = Preview::of(
            &share(ShareKind::Text),
            &content(),
            OffsetDateTime::now_utc(),
            Language::En,
        )
        .with_snippet(text, Some("ts"));
        assert!(
            matches!(
                &open.card,
                Card::Open {
                    snippet: Some(_),
                    ..
                }
            ),
            "an open note lost its snippet: {:?}",
            open.card
        );

        let mut locked = share(ShareKind::Text);
        locked.password_hash = Some("$argon2id$...".to_owned());
        let locked = Preview::of(&locked, &content(), OffsetDateTime::now_utc(), Language::En)
            .with_snippet(text, Some("ts"));
        assert!(
            matches!(locked.card, Card::Locked { .. }),
            "{:?}",
            locked.card
        );
        assert!(
            !format!("{:?}", locked.card).contains("hunter2"),
            "a locked note's body reached its card: {:?}",
            locked.card
        );
    }

    /// Without an origin there is no picture, so the small card is asked for
    /// instead: a large card with no image is a large blank.
    #[test]
    fn no_origin_means_no_image_tags() {
        let tags = Preview::of(
            &share(ShareKind::File),
            &content(),
            OffsetDateTime::now_utc(),
            Language::En,
        )
        .to_head_tags(None, "ABCD1234", "Tessera");

        assert!(!tags.contains("og:image"), "{tags}");
        assert!(tags.contains(r#"content="summary""#), "{tags}");
    }

    /// With one, the image is absolute, since a relative path resolves to
    /// nothing on the crawler's machine.
    #[test]
    fn the_image_url_is_absolute() {
        let tags = Preview::of(
            &share(ShareKind::File),
            &content(),
            OffsetDateTime::now_utc(),
            Language::En,
        )
        .to_head_tags(Some("https://share.example"), "ABCD1234", "Tessera");

        assert!(
            tags.contains(r#"content="https://share.example/s/ABCD1234/og.png""#),
            "{tags}"
        );
        assert!(tags.contains("summary_large_image"), "{tags}");
    }

    /// The configured value wins over anything a request claims, so a forged
    /// `Host` cannot make our page advertise a picture from elsewhere.
    #[test]
    fn a_configured_origin_beats_the_host_header() {
        let mut config = crate::config::Config::default();
        config.server.public_url = "https://share.example/".to_owned();
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::HOST,
            "evil.example".parse().expect("a host"),
        );

        assert_eq!(
            origin(&config, &headers).as_deref(),
            Some("https://share.example")
        );
    }

    /// And with nothing configured, the request is still better than nothing:
    /// one container behind one proxy is the common case and needs no setup.
    #[test]
    fn without_configuration_the_request_is_used() {
        let config = crate::config::Config::default();
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::HOST,
            "share.example".parse().expect("a host"),
        );
        headers.insert("x-forwarded-proto", "https".parse().expect("a scheme"));

        assert_eq!(
            origin(&config, &headers).as_deref(),
            Some("https://share.example")
        );
    }

    fn share(kind: ShareKind) -> Share {
        Share {
            role_id: None,
            id: ShareId::generate(),
            code: RetrievalCode::parse("ABCD1234").expect("a valid code"),
            user_id: None,
            kind,
            password_hash: None,
            use_limit: None,
            use_count: 0,
            expires_at: None,
            disabled_at: None,
            title: None,
            encryption: None,
            sealed_details: None,
            og_config: None,
            management_token_hash: None,
            view_count: 0,
            copy_count: 0,
            bytes_served: 0,
            bytes_redirected: 0,
            last_accessed_at: None,
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
        }
    }

    /// The whole reason this module exists: an unfurler reads the bytes, so the
    /// tags have to be in them.
    #[test]
    fn tags_land_inside_the_head() {
        let html = "<!doctype html><html><head><title>Tessera</title></head><body></body></html>";
        let spliced = splice_into_document(html, "<meta name=\"x\">");
        assert!(
            spliced.contains("<head><meta name=\"x\"><title>"),
            "{spliced}"
        );
    }

    #[test]
    fn a_head_with_attributes_is_still_found() {
        let html = "<html><HEAD lang=\"en\"><title>t</title></HEAD></html>";
        let spliced = splice_into_document(html, "<meta>");
        assert!(spliced.contains("lang=\"en\"><meta><title>"), "{spliced}");
    }

    /// Giving up when there is no `<head>` is how a preview stops working
    /// unnoticed. The shipped placeholder was a bare fragment, and the bundle's
    /// template is not ours to depend on.
    #[test]
    fn a_document_without_a_head_still_gets_its_tags() {
        assert_eq!(
            splice_into_document("<!doctype html><h1>hi</h1>", "<meta>"),
            "<!doctype html><meta><h1>hi</h1>"
        );
        assert_eq!(
            splice_into_document("<h1>hi</h1>", "<meta>"),
            "<meta><h1>hi</h1>"
        );
    }

    /// The title came from whoever made the share and is going into an
    /// attribute on a page served from this origin.
    #[test]
    fn a_title_cannot_break_out_of_its_attribute() {
        let mut share = share(ShareKind::Text);
        share.title = Some(r#""><script>alert(1)</script>"#.to_owned());

        let tags = Preview::of(&share, &content(), OffsetDateTime::now_utc(), Language::En)
            .to_head_tags(None, "ABCD1234", "Tessera");
        assert!(!tags.contains("<script"), "{tags}");
        assert!(tags.contains("&quot;&gt;&lt;script&gt;"), "{tags}");
    }

    /// A preview is shown to everyone in the channel. If it described the file,
    /// the password would be decorative.
    ///
    /// The title is the one deliberate exception: the uploader typed it as a
    /// label for other people, where the file name came off their disk and they
    /// never agreed to publish it. The cost is real, since a share titled "Q3
    /// salaries" announces that in the channel, so the default with no title
    /// says nothing.
    #[test]
    fn a_locked_share_describes_nothing_but_its_title() {
        let mut share = share(ShareKind::File);
        share.title = Some("Q3 salaries".to_owned());
        share.password_hash = Some("$argon2id$...".to_owned());
        let content = named("payroll.xlsx");

        let tags = Preview::of(&share, &content, OffsetDateTime::now_utc(), Language::En)
            .to_head_tags(None, "ABCD1234", "Tessera");
        assert!(!tags.contains("payroll"), "the file name leaked: {tags}");
        assert!(!tags.contains("2.0 KiB"), "the size leaked: {tags}");
        assert!(
            tags.contains("Q3 salaries"),
            "the title was dropped: {tags}"
        );
    }

    /// And with no title, nothing.
    #[test]
    fn a_locked_share_with_no_title_says_nothing_at_all() {
        let mut share = share(ShareKind::File);
        share.password_hash = Some("$argon2id$...".to_owned());

        let tags = Preview::of(
            &share,
            &named("payroll.xlsx"),
            OffsetDateTime::now_utc(),
            Language::En,
        )
        .to_head_tags(None, "ABCD1234", "Tessera");
        assert!(!tags.contains("payroll"), "{tags}");
        assert!(tags.contains("Password-protected"), "{tags}");
    }

    #[test]
    fn a_dead_share_says_so_rather_than_describing_itself() {
        let mut share = share(ShareKind::File);
        share.expires_at = Some(OffsetDateTime::now_utc() - time::Duration::hours(1));
        let content = named("gone.zip");

        let tags = Preview::of(&share, &content, OffsetDateTime::now_utc(), Language::En)
            .to_head_tags(None, "ABCD1234", "Tessera");
        assert!(!tags.contains("gone.zip"), "{tags}");
        assert!(tags.contains("Share unavailable"), "{tags}");
    }

    #[test]
    fn a_live_share_names_itself() {
        let mut share = share(ShareKind::File);
        share.use_limit = Some(3);
        let content = named("holiday.jpg");

        let preview = Preview::of(&share, &content, OffsetDateTime::now_utc(), Language::En);
        assert_eq!(preview.title, "holiday.jpg");
        assert_eq!(preview.description, "2.0 KiB · 3 retrievals left");
    }

    #[test]
    fn the_last_retrieval_reads_as_one_rather_than_a_number() {
        let mut share = share(ShareKind::Text);
        share.use_limit = Some(3);
        share.use_count = 2;
        assert!(
            Preview::of(&share, &content(), OffsetDateTime::now_utc(), Language::En)
                .description
                .contains("one retrieval left")
        );
    }

    #[test]
    fn sizes_read_the_way_a_person_would_say_them() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2048), "2.0 KiB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MiB");
    }
}
