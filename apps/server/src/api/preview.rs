//! Link previews for `/s/{code}`.
//!
//! # Why the server does this and not the page
//!
//! What unfurls a link — Discord, Slack, a mail client — fetches the URL and
//! reads the HTML. It does not run JavaScript, so a single-page app can put
//! whatever it likes in `document.head` and the preview will still show whatever
//! the shipped `index.html` said. The tags have to be in the bytes the server
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
//! - **Describe a locked share.** A preview is public by construction — it is shown to everyone in
//!   the channel. If a password-protected share announced its file name and size there, the
//!   password would be decorative.

use crate::{
    db::repository::share::{Availability, Share, ShareKind},
    state::AppState,
};

/// The tags to insert, already escaped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    title: String,
    description: String,
}

impl Preview {
    /// What a share should say about itself to a channel full of people.
    #[must_use]
    pub fn of(share: &Share, now: time::OffsetDateTime) -> Self {
        match share.availability_at(now) {
            Availability::Expired | Availability::Exhausted | Availability::Disabled => Self {
                title: "Share unavailable".to_owned(),
                description: "This share has expired or has already been collected.".to_owned(),
            },
            Availability::Available if share.needs_password() => Self {
                title: "Password-protected share".to_owned(),
                description: "Open the link and enter the password to see what this is.".to_owned(),
            },
            Availability::Available => Self {
                title: share.title.clone().unwrap_or_else(|| match share.kind {
                    ShareKind::File => share
                        .filename
                        .clone()
                        .unwrap_or_else(|| "Shared file".to_owned()),
                    ShareKind::Text => "Shared note".to_owned(),
                }),
                description: describe(share),
            },
        }
    }

    /// The tags, ready to splice into a `<head>`.
    #[must_use]
    pub fn to_head_tags(&self) -> String {
        let title = escape(&self.title);
        let description = escape(&self.description);
        format!(
            "<meta property=\"og:title\" content=\"{title}\">\
             <meta property=\"og:description\" content=\"{description}\">\
             <meta property=\"og:type\" content=\"website\">\
             <meta name=\"twitter:card\" content=\"summary\">\
             <meta name=\"description\" content=\"{description}\">"
        )
    }
}

fn describe(share: &Share) -> String {
    let mut parts = vec![match share.kind {
        ShareKind::File => human_size(share.size_bytes),
        ShareKind::Text => format!("{} of text", human_size(share.size_bytes)),
    }];
    if let Some(limit) = share.use_limit {
        let left = limit.saturating_sub(share.use_count);
        parts.push(if left == 1 {
            "one retrieval left".to_owned()
        } else {
            format!("{left} retrievals left")
        });
    }
    parts.join(" · ")
}

/// A size the way a person would say it.
///
/// Integer arithmetic throughout: this is a caption, and a float would trade a
/// lossy conversion for a precision nobody reading "2.0 KiB" wanted.
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
/// The title and file name came from whoever made the share, and they are being
/// spliced into a `content="..."` attribute on a page served from this origin.
/// Without this, `"><script>` closes the attribute, closes the tag, and runs —
/// which is a cross-site scripting hole reachable by anyone who can upload.
///
/// Both quote characters are escaped, not just the one the template happens to
/// use, so changing the template cannot reopen the hole.
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

/// Splices `tags` into the document.
///
/// Preferably just inside `<head>`, so that ours come before anything the bundle
/// ships — where a crawler sees the same property twice, the first is what it
/// keeps.
///
/// A document without a `<head>` still gets them, after the doctype or at the
/// very start. An HTML parser hoists leading `<meta>` elements into the head it
/// creates, and an unfurler reads the bytes rather than a tree, so both work.
/// Falling back matters more than it sounds: the first version of this gave up
/// when there was no `<head>`, which meant the shipped placeholder page — a bare
/// fragment — silently had no previews at all, and any future change to the
/// bundle's template could do the same without a word.
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

/// Reads the share a `/s/{code}` page is about, without touching its counters.
///
/// Deliberately not [`crate::api::gate::open`]: that refuses expired shares and
/// counts misses against the lookup limit, and neither is right here. A crawler
/// following a dead link should still get a page that explains itself, and a
/// crawler should not be able to exhaust a limit meant for people guessing.
pub async fn lookup(state: &AppState, code: &str) -> Option<Share> {
    let code = tessera_core::RetrievalCode::parse(code).ok()?;
    state.shares.find_by_code(&code).await.ok().flatten()
}

#[cfg(test)]
mod tests {
    use tessera_core::{RetrievalCode, ShareId};
    use time::OffsetDateTime;

    use super::*;
    use crate::db::repository::share::StorageLocation;

    fn share(kind: ShareKind) -> Share {
        Share {
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
            filename: None,
            content_type: None,
            language: None,
            size_bytes: 2048,
            storage: StorageLocation::Inline,
            og_config: None,
            management_token_hash: None,
            view_count: 0,
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

    /// Giving up when there is no `<head>` is how a preview stops working with
    /// nobody noticing: the shipped placeholder was a bare fragment, and the
    /// bundle's template is not ours to depend on.
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

        let tags = Preview::of(&share, OffsetDateTime::now_utc()).to_head_tags();
        assert!(!tags.contains("<script"), "{tags}");
        assert!(tags.contains("&quot;&gt;&lt;script&gt;"), "{tags}");
    }

    /// A preview is shown to everyone in the channel. If it described the file,
    /// the password would be decorative.
    #[test]
    fn a_locked_share_describes_nothing() {
        let mut share = share(ShareKind::File);
        share.filename = Some("payroll.xlsx".to_owned());
        share.title = Some("Q3 salaries".to_owned());
        share.password_hash = Some("$argon2id$...".to_owned());

        let tags = Preview::of(&share, OffsetDateTime::now_utc()).to_head_tags();
        assert!(!tags.contains("payroll"), "{tags}");
        assert!(!tags.contains("Q3"), "{tags}");
        assert!(tags.contains("Password-protected"), "{tags}");
    }

    #[test]
    fn a_dead_share_says_so_rather_than_describing_itself() {
        let mut share = share(ShareKind::File);
        share.filename = Some("gone.zip".to_owned());
        share.expires_at = Some(OffsetDateTime::now_utc() - time::Duration::hours(1));

        let tags = Preview::of(&share, OffsetDateTime::now_utc()).to_head_tags();
        assert!(!tags.contains("gone.zip"), "{tags}");
        assert!(tags.contains("Share unavailable"), "{tags}");
    }

    #[test]
    fn a_live_share_names_itself() {
        let mut share = share(ShareKind::File);
        share.filename = Some("holiday.jpg".to_owned());
        share.use_limit = Some(3);

        let preview = Preview::of(&share, OffsetDateTime::now_utc());
        assert_eq!(preview.title, "holiday.jpg");
        assert_eq!(preview.description, "2.0 KiB · 3 retrievals left");
    }

    #[test]
    fn the_last_retrieval_reads_as_one_rather_than_a_number() {
        let mut share = share(ShareKind::Text);
        share.use_limit = Some(3);
        share.use_count = 2;
        assert!(
            Preview::of(&share, OffsetDateTime::now_utc())
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
