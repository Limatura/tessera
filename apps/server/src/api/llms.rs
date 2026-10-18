//! `/llms.txt`: this instance, described for a language model.
//!
//! An agent asked to hand someone a file needs three things it cannot guess:
//! the address, the request that uploads, and what this instance allows. The
//! page answers them in the plain Markdown the llms.txt convention asks for,
//! written for this instance rather than for Tessera in general, from the same
//! figures `/capabilities` gives a visitor who is not signed in. Nothing here
//! is more than an anonymous caller could already learn.

use std::fmt::Write as _;

use axum::{
    extract::State,
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
};

use crate::{
    api::{capabilities::Capabilities, preview::origin, shares::max_file_bytes},
    state::AppState,
};

/// The page, for this instance as a stranger sees it.
pub async fn llms_txt(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let config = state.config();
    let site = origin(&config, &headers).unwrap_or_default();
    let title = Some(config.site.title.trim())
        .filter(|title| !title.is_empty())
        .unwrap_or("Tessera")
        .to_owned();
    drop(config);
    let capabilities = Capabilities::from_state(&state);
    // A failure to read the limit is no reason to withhold the rest; the page
    // then says the server will answer for itself.
    let max_bytes = max_file_bytes(&state, None).await.ok();
    (
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        page(&title, &site, &capabilities, max_bytes),
    )
        .into_response()
}

fn page(title: &str, site: &str, capabilities: &Capabilities, max_bytes: Option<u64>) -> String {
    let mut out = format!(
        "# {title}\n\n\
         > A file-sharing service running Tessera. Upload a file or a note and get a \
         link back; the share is deleted when it expires or has been downloaded as many \
         times as allowed.\n\n"
    );

    out.push_str("## Limits on this instance\n\n");
    out.push_str(if capabilities.anonymous_upload {
        "- Uploading needs no account.\n"
    } else {
        "- Uploading needs an account: send an API token as `Authorization: Bearer tsk_...`. \
         A person creates one under their account on the site.\n"
    });
    let largest = max_bytes.map_or_else(
        || "- A file that is too large is refused with `413`.\n".to_owned(),
        |bytes| format!("- Largest file: {}.\n", human_bytes(bytes)),
    );
    out.push_str(&largest);
    let lifetime = match (
        capabilities.default_expiry_hours,
        capabilities.max_expiry_hours,
    ) {
        (Some(default), Some(max)) => format!(
            "- Shares last {} unless asked otherwise, and {} at most.\n",
            hours(default),
            hours(max)
        ),
        (Some(default), None) => {
            format!("- Shares last {} unless asked otherwise.\n", hours(default))
        }
        (None, Some(max)) => format!("- Shares last {} at most.\n", hours(max)),
        (None, None) => "- Shares last until deleted unless asked otherwise.\n".to_owned(),
    };
    out.push_str(&lifetime);
    if capabilities.encryption {
        out.push_str(
            "- End-to-end encryption is available through the `tess` command-line client.\n",
        );
    }

    write!(
        out,
        "\n## Upload a file\n\n\
         ```sh\n\
         curl -T report.pdf {site}/\n\
         echo 'a note' | curl -T - {site}/\n\
         ```\n\n\
         The answer is the link on one line. Options go in headers:\n\n\
         - `X-Share-Expires: 30m`, `12h`, `1d` or `2w`\n\
         - `X-Share-Max-Downloads: 3` (`1` for burn after reading)\n\
         - `X-Share-Password: ...`, which the person downloading then needs\n\
         - `Accept: application/json` for the share as JSON instead of a link\n\n\
         An error is one line, `error: <message>`, with the HTTP status: `413` too \
         large, `507` out of room, `429` too many requests (wait for `Retry-After`), \
         `401` an account is needed.\n\n\
         The `tess` client does the same, and also resumes interrupted uploads, sends \
         whole directories and encrypts: `tess send report.pdf --server {site}`.\n"
    )
    .ok();

    write!(
        out,
        "\n## Read a share\n\n\
         A link looks like `{site}/s/CODE`; that page is for people. To fetch the \
         content:\n\n\
         - `curl -fLOJ {site}/s/CODE/download`: the file, or several files as a zip.\n\
         - `curl -fL {site}/s/CODE/raw`: a note's text.\n\
         - Add `-H 'X-Share-Password: ...'` for a share with a password.\n\n\
         Every download uses up one of a limited share's downloads. A link with \
         `#...` after the code is encrypted and opens only in a browser or with \
         `tess get`; the server cannot read it.\n"
    )
    .ok();

    write!(
        out,
        "\n## Reference\n\n\
         - [OpenAPI document]({site}/openapi.json): every endpoint, including the \
         resumable upload API for large files.\n\
         - [What this instance allows]({site}/api/v1/capabilities), as JSON.\n"
    )
    .ok();
    out
}

/// Whole hours as people say them: `24 hours` is `1 day`.
fn hours(hours: u64) -> String {
    match hours {
        1 => "1 hour".to_owned(),
        h if h % 24 == 0 && h / 24 == 1 => "1 day".to_owned(),
        h if h % 24 == 0 => format!("{} days", h / 24),
        h => format!("{h} hours"),
    }
}

/// Bytes in the binary units uploads are usually limited in.
fn human_bytes(bytes: u64) -> String {
    const UNITS: [(u64, &str); 3] = [(1 << 30, "GiB"), (1 << 20, "MiB"), (1 << 10, "KiB")];
    for (size, unit) in UNITS {
        if bytes >= size {
            let whole = bytes / size;
            let tenths = (bytes % size) * 10 / size;
            return if tenths == 0 {
                format!("{whole} {unit}")
            } else {
                format!("{whole}.{tenths} {unit}")
            };
        }
    }
    format!("{bytes} bytes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_and_sizes_read_naturally() {
        assert_eq!(hours(24), "1 day");
        assert_eq!(hours(168), "7 days");
        assert_eq!(hours(36), "36 hours");
        assert_eq!(human_bytes(100 * 1024 * 1024), "100 MiB");
        assert_eq!(human_bytes(1536), "1.5 KiB");
    }
}
