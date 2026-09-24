//! The sign-in page both fake providers show.
//!
//! Where a real provider asks for a password, this asks who to be. Submitting
//! it sends the browser back to Tessera with a code, the way a real one does.

use std::fmt::Write as _;

use axum::response::Html;

/// One field on the form.
pub(crate) struct Field<'a> {
    pub(crate) name: &'a str,
    pub(crate) label: &'a str,
    pub(crate) value: &'a str,
    pub(crate) hint: &'a str,
}

/// The form, posting back to `action` with every query parameter the
/// authorisation request carried kept as a hidden field.
pub(crate) fn sign_in(
    provider: &str,
    action: &str,
    carried: &[(String, String)],
    fields: &[Field<'_>],
) -> Html<String> {
    let mut hidden = String::new();
    for (name, value) in carried {
        write!(
            hidden,
            r#"<input type="hidden" name="{}" value="{}">"#,
            escape(name),
            escape(value)
        )
        .ok();
    }
    let mut inputs = String::new();
    for field in fields {
        write!(
            inputs,
            r#"<label>{label}<input name="{name}" value="{value}"><small>{hint}</small></label>"#,
            label = escape(field.label),
            name = escape(field.name),
            value = escape(field.value),
            hint = escape(field.hint),
        )
        .ok();
    }
    Html(format!(
        r#"<!doctype html>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{provider} (fake)</title>
<style>
  body {{ font: 15px/1.5 system-ui, sans-serif; max-width: 26rem; margin: 3rem auto; padding: 0 1rem; color: #1f2328; background: #f6f8fa; }}
  h1 {{ font-size: 1.2rem; margin-bottom: .25rem; }}
  p {{ color: #59636e; margin-top: 0; }}
  form {{ display: grid; gap: .9rem; background: #fff; border: 1px solid #d1d9e0; border-radius: 8px; padding: 1.25rem; }}
  label {{ display: grid; gap: .25rem; font-weight: 600; }}
  input {{ font: inherit; padding: .45rem .6rem; border: 1px solid #d1d9e0; border-radius: 6px; font-weight: 400; }}
  small {{ color: #59636e; font-weight: 400; }}
  button {{ font: inherit; font-weight: 600; padding: .55rem; border: 0; border-radius: 6px; background: #1f883d; color: #fff; cursor: pointer; }}
</style>
<h1>{provider} (fake)</h1>
<p>A stand-in from <code>tessera-dev</code>. Sign in as anybody.</p>
<form method="post" action="{action}">{hidden}{inputs}<button>Sign in</button></form>
"#,
        provider = escape(provider),
        action = escape(action),
    ))
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A fresh, unguessable-enough code for one exchange.
pub(crate) fn code() -> String {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).ok();
    bytes.iter().fold(String::new(), |mut out, byte| {
        write!(out, "{byte:02x}").ok();
        out
    })
}
