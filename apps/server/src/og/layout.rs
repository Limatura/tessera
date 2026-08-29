//! The operator's own card design, and what it is allowed to contain.
//!
//! # Whose design this is
//!
//! One layout for the instance, set by whoever runs it, not one per share.
//! Per-share customisation would be a knob on every upload form for
//! something almost never wanted, and the shares table's `og_config` column
//! stays unused by that decision, not by oversight.
//!
//! # What the checking is for
//!
//! Not a security boundary. The only person who can set this already runs the
//! process; a layout is not a way for them to reach anything they could not
//! reach by editing the configuration file, the other way to set it.
//!
//! What it is for is telling them **when they save**, not when a link is
//! pasted into a channel. A `<script>` or an `<img src="https://…">`
//! does not fail loudly at render time: takumi has no HTTP client, so an
//! external image is quietly nothing, and an unknown element is quietly
//! dropped. Left to be discovered, the first sign of a mistake is a card that
//! looks wrong on a stranger's screen.
//!
//! So the rules are narrow, and they refuse things that would merely be useless
//! as firmly as things that would be wrong.
//!
//! # Why it is checked twice
//!
//! Once when it is saved, and again before it is drawn. The rules can tighten,
//! and a layout in the database was written under the rules of the day it was
//! written. A layout that no longer passes falls back to the built-in one with
//! a warning: a card that is not the operator's design still tells the reader
//! what the share is, and an error page tells them nothing.

use html5ever::{
    Attribute, ParseOpts, QualName, local_name, ns, parse_fragment, tendril::TendrilSink,
    tree_builder::TreeBuilderOpts,
};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

/// Elements a layout may use.
///
/// Boxes and text, and nothing else. `img` and `svg` are absent because both
/// name a resource, takumi does not fetch resources, and a layout
/// referencing one draws a hole. When there is a reason to put a logo on a
/// card, the answer is somewhere to upload one, not a URL in a string.
const ELEMENTS: [&str; 2] = ["div", "span"];

/// Attributes an element may carry.
///
/// `style` alone. `class` and `tw` would work, since takumi reads both, but
/// they name rules living somewhere this layout cannot see, so a layout using
/// them renders differently here and in whatever previewed it.
const ATTRIBUTES: [&str; 1] = ["style"];

/// The most markup a layout may be.
///
/// A card is 1200 by 630 and has room for a few dozen elements. Eight kilobytes
/// is far more than that and far less than something that would take a
/// noticeable time to parse.
pub const MAX_BYTES: usize = 8 * 1024;

/// How deeply elements may nest.
///
/// Flexbox layouts are shallow. Twelve is past anything a card needs and short
/// of a depth that costs anything to walk.
pub const MAX_DEPTH: usize = 12;

/// Why a layout was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    /// Longer than [`MAX_BYTES`].
    #[error("the layout is {0} bytes; the most allowed is {MAX_BYTES}")]
    TooLong(usize),
    /// Nested deeper than [`MAX_DEPTH`].
    #[error("the layout nests more than {MAX_DEPTH} deep")]
    TooDeep,
    /// An element that is not on the list.
    #[error("`<{0}>` is not allowed here; a layout may use `div` and `span`")]
    Element(String),
    /// An attribute that is not on the list.
    #[error("the `{0}` attribute is not allowed here; a layout may use `style`")]
    Attribute(String),
    /// Something that would name a resource outside this server.
    #[error("a layout cannot reference anything outside this server, and this one has `{0}`")]
    External(String),
    /// A placeholder this server will not fill in.
    #[error("`{{{{{0}}}}}` is not a placeholder this server knows")]
    Placeholder(String),
}

/// Checks that `html` is a layout this server will draw.
///
/// # Errors
///
/// Returns the first [`Refused`] that applies, not all of them. The checks are
/// cheap to re-run, and a list of problems that stops
/// being true after the first fix is worse company than one problem at a time.
pub fn check(html: &str) -> Result<(), Refused> {
    if html.len() > MAX_BYTES {
        return Err(Refused::TooLong(html.len()));
    }
    // Before parsing: `url(` lives inside a style attribute's value, where the
    // walk below sees it as an opaque string, and `@import` inside a `<style>`
    // element that html5ever keeps as text.
    let lowered = html.to_ascii_lowercase();
    for marker in ["url(", "@import", "javascript:", "expression("] {
        if lowered.contains(marker) {
            return Err(Refused::External(marker.to_owned()));
        }
    }
    for name in placeholders(html) {
        if !PLACEHOLDERS.contains(&name.as_str()) {
            return Err(Refused::Placeholder(name));
        }
    }

    let dom = parse_fragment(
        RcDom::default(),
        ParseOpts {
            tree_builder: TreeBuilderOpts {
                drop_doctype: true,
                ..TreeBuilderOpts::default()
            },
            ..ParseOpts::default()
        },
        QualName::new(None, ns!(html), local_name!("div")),
        Vec::new(),
        // Scripting off: with it on the parser treats a `<noscript>` body as
        // text, and this wants to see every element for what it is.
        false,
    )
    .one(html);

    walk(&dom.document, 0)
}

fn walk(handle: &Handle, depth: usize) -> Result<(), Refused> {
    if depth > MAX_DEPTH {
        return Err(Refused::TooDeep);
    }
    let mut descend = depth;
    match &handle.data {
        NodeData::Element { name, attrs, .. } => {
            let tag = name.local.to_string();
            // The fragment parser wraps everything in the context element it was
            // given, ours and not the operator's.
            if tag != "html" {
                if !ELEMENTS.contains(&tag.as_str()) {
                    return Err(Refused::Element(tag));
                }
                check_attributes(&attrs.borrow())?;
                descend = depth + 1;
            }
        }
        NodeData::Text { .. } | NodeData::Document => {}
        // A comment costs nothing and says nothing; anything else is a construct
        // this has no opinion about and should not silently accept.
        NodeData::Comment { .. } => return Ok(()),
        NodeData::Doctype { .. } | NodeData::ProcessingInstruction { .. } => {
            return Err(Refused::Element("!doctype".to_owned()));
        }
    }

    for child in handle.children.borrow().iter() {
        walk(child, descend)?;
    }
    Ok(())
}

fn check_attributes(attrs: &[Attribute]) -> Result<(), Refused> {
    for attr in attrs {
        let name = attr.name.local.to_string();
        if !ATTRIBUTES.contains(&name.as_str()) {
            return Err(Refused::Attribute(name));
        }
    }
    Ok(())
}

/// The names a layout may ask for. What each is worth is decided by
/// [`Card`](crate::api::preview::Card), not here.
pub const PLACEHOLDERS: [&str; 7] = [
    "title", "label", "size", "code", "expires", "footer", "language",
];

/// Every `{{name}}` in `html`, in the order they appear.
fn placeholders(html: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = html;
    while let Some(open) = rest.find("{{") {
        let after = rest.get(open + 2..).unwrap_or_default();
        let Some(close) = after.find("}}") else {
            break;
        };
        found.push(after.get(..close).unwrap_or_default().trim().to_owned());
        rest = after.get(close + 2..).unwrap_or_default();
    }
    found
}

/// Puts `values` into `html`, escaping each as it goes in.
///
/// A name with no value becomes nothing. That is how a locked share's layout
/// comes out saying nothing about its contents: the values come from the same
/// decision the built-in layout reads, so a placeholder the share may not
/// answer is simply not answered.
#[must_use]
pub fn fill(html: &str, values: &std::collections::BTreeMap<&str, String>) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(open) = rest.find("{{") {
        out.push_str(rest.get(..open).unwrap_or_default());
        let after = rest.get(open + 2..).unwrap_or_default();
        let Some(close) = after.find("}}") else {
            out.push_str("{{");
            rest = after;
            continue;
        };
        let name = after.get(..close).unwrap_or_default().trim();
        if let Some(value) = values.get(name) {
            out.push_str(&super::escape(value));
        }
        rest = after.get(close + 2..).unwrap_or_default();
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn a_plain_layout_is_accepted() {
        assert_eq!(
            check(r#"<div style="display:flex"><span>{{title}}</span></div>"#),
            Ok(())
        );
    }

    #[test]
    fn an_element_off_the_list_is_refused() {
        assert!(matches!(
            check("<div><script>alert(1)</script></div>"),
            Err(Refused::Element(tag)) if tag == "script"
        ));
        assert!(matches!(
            check(r#"<img src="https://example.test/logo.png">"#),
            Err(Refused::Element(tag)) if tag == "img"
        ));
    }

    #[test]
    fn an_attribute_off_the_list_is_refused() {
        assert!(matches!(
            check(r#"<div onclick="steal()"></div>"#),
            Err(Refused::Attribute(name)) if name == "onclick"
        ));
    }

    /// takumi has no HTTP client, so an external reference is not a way out of
    /// this process but a hole in the picture. Refusing it at save time is the
    /// difference between an error message and a card that looks broken to
    /// everyone but its author.
    #[test]
    fn anything_pointing_outside_is_refused() {
        for layout in [
            r#"<div style="background:url(https://example.test/x.png)"></div>"#,
            r#"<div style="background:URL('x')"></div>"#,
            "<div>@import url(x)</div>",
        ] {
            assert!(
                matches!(check(layout), Err(Refused::External(_))),
                "accepted {layout}"
            );
        }
    }

    #[test]
    fn an_unknown_placeholder_is_refused() {
        assert!(matches!(
            check("<div>{{password}}</div>"),
            Err(Refused::Placeholder(name)) if name == "password"
        ));
    }

    #[test]
    fn something_enormous_is_refused() {
        let long = format!("<div>{}</div>", "x".repeat(MAX_BYTES));
        assert!(matches!(check(&long), Err(Refused::TooLong(_))));
    }

    #[test]
    fn something_deeply_nested_is_refused() {
        let deep = "<div>".repeat(MAX_DEPTH + 4) + &"</div>".repeat(MAX_DEPTH + 4);
        assert!(matches!(check(&deep), Err(Refused::TooDeep)));
    }

    #[test]
    fn placeholders_are_filled_and_escaped() {
        let mut values = BTreeMap::new();
        values.insert("title", "<script>".to_owned());

        assert_eq!(
            fill("<div>{{title}}</div>", &values),
            "<div>&lt;script&gt;</div>"
        );
    }

    /// The one that matters: a placeholder the share may not answer becomes
    /// nothing, not the value it would have had.
    #[test]
    fn a_placeholder_with_no_value_becomes_nothing() {
        let values = BTreeMap::new();

        assert_eq!(fill("<div>{{size}}!</div>", &values), "<div>!</div>");
    }

    /// An unclosed placeholder is text, not the start of a swallowed document.
    #[test]
    fn an_unclosed_placeholder_is_left_alone() {
        let values = BTreeMap::new();

        assert_eq!(fill("<div>{{oops</div>", &values), "<div>{{oops</div>");
    }
}
