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
/// Boxes, text, and the drawable part of SVG.
///
/// # What is missing, and why
///
/// `img` names a resource and takumi has no HTTP client, so it draws a hole.
///
/// Inside SVG the same rule sorts the rest. `image` is `img` again: usvg is
/// given a parser with its string href resolver disabled, exactly so that
/// untrusted markup cannot read a local file, and the element therefore draws
/// nothing. `text` and `tspan` draw nothing either, because usvg is compiled
/// without its `text` feature. `foreignObject` is a way back into HTML from
/// inside a subtree that has already been handed to a different renderer.
///
/// `use` is the odd one out: it works. It is absent because `href` is the only
/// attribute that can point outside the document, and an allowlist with no
/// href-shaped attribute in it is a simpler thing to be sure of than one with
/// a rule about which values are local.
///
/// `filter` and the `fe*` elements are absent for now, and not because they
/// fail: they are the expensive ones, and `/{code}/og.png` is public and
/// unthrottled. That cost is already reachable through `style` and wants
/// answering once for both, rather than being half-avoided here.
const ELEMENTS: &[&str] = &[
    "div",
    "span",
    "svg",
    "g",
    "defs",
    "path",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "linearGradient",
    "radialGradient",
    "stop",
    "clipPath",
];

/// Attributes an element may carry.
///
/// `style` and `tw`, plus what the SVG shapes above need to describe
/// themselves.
///
/// `tw` is takumi's own Tailwind implementation, not a stylesheet: it reads
/// the utilities and turns them into styles itself, so a layout using it
/// carries everything it needs. `class` is still absent for the reason `tw`
/// used to be lumped in with: it names rules living somewhere this layout
/// cannot see, and there is no stylesheet here to find them in.
///
/// The list is not per element. A `div` carrying `cx` is meaningless rather
/// than harmful, and takumi ignores it; a table of which attribute belongs to
/// which element would be a great deal of upkeep to refuse something nobody
/// writes by accident.
///
/// The spellings are the ones the parser produces. Inside `<svg>` an HTML
/// parser is in foreign-content mode and restores the camel case of the names
/// it knows, so `viewBox` arrives as `viewBox` and not `viewbox`. There is a
/// test for that, because it is the sort of thing that would otherwise be
/// discovered by a card silently losing its coordinate system.
const ATTRIBUTES: &[&str] = &[
    "style",
    "tw",
    // Shared geometry.
    "id",
    "width",
    "height",
    "x",
    "y",
    "transform",
    "opacity",
    // `svg` itself.
    "viewBox",
    "xmlns",
    "preserveAspectRatio",
    // Shapes.
    "d",
    "points",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "x1",
    "y1",
    "x2",
    "y2",
    "pathLength",
    // Paint.
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-width",
    "stroke-opacity",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-dasharray",
    // Gradients and clipping.
    "offset",
    "stop-color",
    "stop-opacity",
    "gradientUnits",
    "gradientTransform",
    "spreadMethod",
    "fx",
    "fy",
    "fr",
    "clip-path",
    "clipPathUnits",
    "clip-rule",
];

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
    /// A `tw` utility takumi's Tailwind implementation does not recognise.
    #[error("`{0}` is not a utility class this renderer knows")]
    Utility(String),
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
    for marker in ["@import", "javascript:", "expression("] {
        if lowered.contains(marker) {
            return Err(Refused::External(marker.to_owned()));
        }
    }
    // `url(` is how a gradient or a clip path names another element of the
    // same layout, `fill="url(#brand)"`, so refusing all of them would refuse
    // the reason `linearGradient` is on the list above. A fragment points
    // inside this document and can reach nothing; anything else is a resource
    // takumi has no way to fetch and would draw as a hole.
    if let Some(external) = external_reference(&lowered) {
        return Err(Refused::External(external));
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
        if name == "tw" {
            check_utilities(&attr.value)?;
        }
    }
    Ok(())
}

/// Refuses a `tw` utility takumi's Tailwind implementation does not know.
///
/// This is the whole reason the module exists, in its clearest form. takumi
/// parses the attribute with `filter_map`: a utility it does not recognise is
/// dropped without a word, so `tw="flex supports-[display:grid]:grid"` lays
/// out as `flex` alone and nothing anywhere says why. Its implementation is
/// its own, covering a subset of Tailwind, which is exactly the situation in
/// which somebody writes a class that is real in Tailwind and silent here.
///
/// Asking it, rather than keeping a list, means the answer is whatever the
/// renderer that will draw the card actually accepts.
fn check_utilities(value: &str) -> Result<(), Refused> {
    use std::str::FromStr as _;

    use takumi::prelude::TailwindValues;

    // takumi exposes no way to ask how many utilities it understood, so the
    // question is put as a comparison: a utility it dropped leaves the same
    // value as parsing nothing at all.
    let Ok(nothing) = TailwindValues::from_str("") else {
        // The empty string is not a parse it can fail. If it ever does, this
        // check has no baseline to compare against and must not start
        // refusing every layout on the instance.
        return Ok(());
    };
    for utility in value.split_whitespace() {
        let understood = TailwindValues::from_str(utility).is_ok_and(|parsed| parsed != nothing);
        if !understood {
            return Err(Refused::Utility(utility.to_owned()));
        }
    }
    Ok(())
}

/// The first `url(...)` in `lowered` that does not point inside this document.
///
/// Whitespace is allowed after the parenthesis and the value may be quoted,
/// because CSS allows both and a check that missed them would be a check
/// anybody could step around by accident.
fn external_reference(lowered: &str) -> Option<String> {
    let mut rest = lowered;
    while let Some(open) = rest.find("url(") {
        let after = rest.get(open + "url(".len()..)?;
        let value = after.trim_start().trim_start_matches(['"', '\'']);
        if !value.starts_with('#') {
            let end = after.find(')').unwrap_or(after.len()).min(40);
            return Some(format!("url({}", after.get(..end).unwrap_or_default()));
        }
        rest = after;
    }
    None
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

    /// A logo is the reason `svg` is here: shapes, a gradient and a clip path,
    /// all of it inline, none of it naming anything this server has to fetch.
    #[test]
    fn an_inline_drawing_is_accepted() {
        let layout = concat!(
            r##"<div style="display:flex"><svg viewBox="0 0 100 100" width="100" height="100">"##,
            r##"<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="0">"##,
            r##"<stop offset="0" stop-color="red"/><stop offset="1" stop-color="blue"/>"##,
            r##"</linearGradient><clipPath id="c"><circle cx="50" cy="50" r="40"/></clipPath></defs>"##,
            r##"<rect width="100" height="100" fill="url(#g)" clip-path="url(#c)"/>"##,
            r##"<path d="M0 0 L100 100" stroke="white" stroke-width="2" fill="none"/>"##,
            r##"</svg><span>{{title}}</span></div>"##,
        );
        assert_eq!(check(layout), Ok(()));
    }

    /// An HTML parser inside `<svg>` is in foreign-content mode and restores
    /// the camel case of the names it knows. The allowlist is spelled to match
    /// what it produces, so this is really asking whether that is still true:
    /// if `viewBox` ever started arriving as `viewbox`, every drawing on every
    /// instance would lose its coordinate system and the refusal here would be
    /// the only warning anybody got.
    #[test]
    fn the_parser_keeps_the_camel_case_of_svg_names() {
        assert_eq!(
            check(r#"<svg viewBox="0 0 1 1" gradientUnits="userSpaceOnUse"></svg>"#),
            Ok(())
        );
    }

    /// Inside SVG, `url(#id)` is how one element names another in the same
    /// layout. Refusing every `url(` would refuse the reason gradients are on
    /// the list.
    #[test]
    fn a_reference_to_this_document_is_allowed() {
        assert_eq!(
            check(r#"<div style="display:flex"><svg><circle r="1" fill="url(#g)"/></svg></div>"#),
            Ok(())
        );
        for layout in [
            r#"<svg><circle r="1" fill="url(/local.png)"/></svg>"#,
            r#"<svg><circle r="1" fill="url( https://example.test/x.svg#g)"/></svg>"#,
        ] {
            assert!(
                matches!(check(layout), Err(Refused::External(_))),
                "accepted {layout}"
            );
        }
    }

    /// The elements that would draw nothing, each for its own reason: no
    /// resolver, no text feature, no way back into HTML. All refused as firmly
    /// as the ones that would be wrong, because a card that silently lost a
    /// piece is discovered by a stranger.
    #[test]
    fn svg_that_would_draw_nothing_is_refused() {
        for (layout, expected) in [
            (r#"<svg><image href="/etc/hostname"/></svg>"#, "image"),
            (r"<svg><text>hi</text></svg>", "text"),
            (
                r"<svg><foreignObject></foreignObject></svg>",
                "foreignObject",
            ),
            (r##"<svg><use href="#a"/></svg>"##, "use"),
            (r#"<svg><filter id="f"></filter></svg>"#, "filter"),
        ] {
            assert!(
                matches!(check(layout), Err(Refused::Element(ref tag)) if tag == expected),
                "accepted {layout}"
            );
        }
    }

    /// takumi reads `tw` itself, so a layout using it carries everything it
    /// needs and renders the same wherever that renderer runs.
    #[test]
    fn utility_classes_are_accepted() {
        assert_eq!(
            check(
                r#"<div tw="flex w-full items-center bg-red-500"><span tw="text-white">{{title}}</span></div>"#
            ),
            Ok(())
        );
    }

    /// The reason `tw` is checked and not merely allowed. takumi drops a
    /// utility it does not know without a word, and its Tailwind is its own
    /// subset, so a class that is real in Tailwind can be silent here.
    /// `supports-[…]` is exactly that: valid Tailwind v4, nothing to takumi.
    #[test]
    fn a_utility_this_renderer_does_not_know_is_refused() {
        for (layout, expected) in [
            (r#"<div tw="not-a-real-class"></div>"#, "not-a-real-class"),
            (
                r#"<div tw="flex supports-[display:grid]:grid"></div>"#,
                "supports-[display:grid]:grid",
            ),
        ] {
            assert!(
                matches!(check(layout), Err(Refused::Utility(ref name)) if name == expected),
                "accepted {layout}"
            );
        }
    }

    /// `class` stays off the list. takumi reads it, but it names rules living
    /// in a stylesheet that does not exist here, so it can only ever be a
    /// layout that looks right to whoever wrote it.
    #[test]
    fn a_class_name_is_still_refused() {
        assert!(matches!(
            check(r#"<div class="flex"></div>"#),
            Err(Refused::Attribute(name)) if name == "class"
        ));
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
