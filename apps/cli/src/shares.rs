//! `tess ls` and `tess rm`: the signed-in account's own shares.

use std::fmt::Write as _;

use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    Result,
    client::{Client, OwnShare},
    style::{DIM, STRONG},
};

/// Every share the account owns, newest first, across as many pages as the
/// server splits them into.
///
/// # Errors
///
/// When there is no token, or the server refuses.
pub async fn all_mine(client: &Client) -> Result<Vec<OwnShare>> {
    let mut shares = Vec::new();
    let mut offset: u64 = 0;
    loop {
        let page = client.list_shares(offset).await?;
        let got = u64::try_from(page.data.len()).unwrap_or(u64::MAX);
        offset = offset.saturating_add(got);
        shares.extend(page.data);
        // An empty page also ends it, so a total that moved under the loop
        // cannot keep it asking.
        if got == 0 || offset >= page.meta.total {
            return Ok(shares);
        }
    }
}

/// What a share is called in the list: its title, else its first file, with
/// how many more there are.
pub(crate) fn name(share: &OwnShare) -> String {
    if share.kind.as_deref() == Some("link") {
        return "(link)".to_owned();
    }
    if share.encryption.is_some() {
        // The name is sealed with a key the server never had, and neither does
        // this list: it lives in the link.
        return "(encrypted)".to_owned();
    }
    if let Some(title) = share.title.as_deref().filter(|title| !title.is_empty()) {
        return title.to_owned();
    }
    // A folder sent whole, from the web page or `tess send <dir>`: every name
    // starts with the folder's, and the folder is what the person sent.
    if share.files.len() > 1
        && let Some((folder, _)) = share
            .files
            .first()
            .and_then(|file| file.filename.as_deref())
            .and_then(|first| first.split_once('/'))
        && share.files.iter().all(|file| {
            file.filename
                .as_deref()
                .and_then(|name| name.strip_prefix(folder))
                .is_some_and(|rest| rest.starts_with('/'))
        })
    {
        return format!("{folder}/ ({} files)", share.files.len());
    }
    let mut names = share
        .files
        .iter()
        .filter_map(|file| file.filename.as_deref());
    match (names.next(), share.files.len()) {
        (None, 0) => "(note)".to_owned(),
        (None, _) => "(file)".to_owned(),
        (Some(first), 1) => first.to_owned(),
        (Some(first), count) => format!("{first} (+{})", count - 1),
    }
}

/// How long until `expires_at`, coarsely: `3d`, `5h`, `12m`, `expired`.
pub(crate) fn expires(expires_at: Option<&str>, now: OffsetDateTime) -> String {
    let Some(at) = expires_at.and_then(|at| OffsetDateTime::parse(at, &Rfc3339).ok()) else {
        return if expires_at.is_some() { "?" } else { "never" }.to_owned();
    };
    let left = at - now;
    if left.is_negative() {
        "expired".to_owned()
    } else if left.whole_hours() >= 24 {
        // Rounded, not cut: a share made to last three days says `3d` for
        // most of its first day, not `2d` from its first minute.
        format!("{}d", (left.whole_hours() + 12) / 24)
    } else if left.whole_hours() > 0 {
        format!("{}h", left.whole_hours())
    } else {
        format!("{}m", left.whole_minutes().max(1))
    }
}

/// The shares as aligned columns, with a header, styled for a terminal.
/// Nothing when there are none.
#[must_use]
pub fn table(shares: &[OwnShare], now: OffsetDateTime) -> String {
    if shares.is_empty() {
        return String::new();
    }
    let rows: Vec<[String; 4]> = shares
        .iter()
        .map(|share| {
            [
                share.code.clone(),
                expires(share.expires_at.as_deref(), now),
                share
                    .uses_remaining
                    .map_or_else(|| "-".to_owned(), |left| left.to_string()),
                name(share),
            ]
        })
        .collect();
    let header = ["CODE", "EXPIRES", "LEFT", "NAME"].map(str::to_owned);
    let width = |column: usize| {
        std::iter::once(&header)
            .chain(&rows)
            .map(|row| row.get(column).map_or(0, |cell| cell.chars().count()))
            .max()
            .unwrap_or(0)
    };
    let widths = [width(0), width(1), width(2)];

    let mut out = String::new();
    for (index, row) in std::iter::once(&header).chain(&rows).enumerate() {
        let [code, expires, left, name] = row;
        // Padded first and styled after, so the escapes do not count as width.
        let line = format!(
            "{code:<w0$}  {expires:<w1$}  {left:>w2$}  {name}",
            w0 = widths[0],
            w1 = widths[1],
            w2 = widths[2],
        );
        // Writing to a `String` cannot fail.
        if index == 0 {
            writeln!(out, "{DIM}{line}{DIM:#}").ok();
        } else {
            let (code, rest) = line.split_at(widths[0].min(line.len()));
            writeln!(out, "{STRONG}{code}{STRONG:#}{rest}").ok();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;
    use crate::client::OwnFile;

    fn share(code: &str) -> OwnShare {
        OwnShare {
            code: code.to_owned(),
            ..OwnShare::default()
        }
    }

    #[test]
    fn expiry_is_coarse_and_relative() {
        let now = datetime!(2026-10-18 12:00 UTC);
        assert_eq!(expires(None, now), "never");
        assert_eq!(expires(Some("2026-10-21T13:00:00Z"), now), "3d");
        assert_eq!(expires(Some("2026-10-21T11:59:00Z"), now), "3d");
        assert_eq!(expires(Some("2026-10-18T17:30:00Z"), now), "5h");
        assert_eq!(expires(Some("2026-10-18T12:00:20Z"), now), "1m");
        assert_eq!(expires(Some("2026-10-18T11:00:00Z"), now), "expired");
    }

    #[test]
    fn a_share_is_named_by_what_can_be_read_of_it() {
        let mut files = share("A");
        files.files = vec![
            OwnFile {
                filename: Some("report.pdf".to_owned()),
            },
            OwnFile {
                filename: Some("notes.txt".to_owned()),
            },
        ];
        assert_eq!(name(&files), "report.pdf (+1)");
        assert_eq!(name(&share("B")), "(note)");
        let mut folder = share("C");
        folder.files = ["photos/a.jpg", "photos/2024/b.jpg"]
            .map(|name| OwnFile {
                filename: Some(name.to_owned()),
            })
            .to_vec();
        assert_eq!(name(&folder), "photos/ (2 files)");
        let mut sealed = files;
        sealed.encryption = Some("link".to_owned());
        assert_eq!(name(&sealed), "(encrypted)");
    }

    #[test]
    fn the_columns_line_up() {
        let mut long = share("P8D72PSF");
        long.uses_remaining = Some(12);
        let out = table(&[long, share("AB")], datetime!(2026-10-18 12:00 UTC));
        assert_eq!(
            anstream::adapter::strip_str(&out).to_string(),
            "CODE      EXPIRES  LEFT  NAME\n\
             P8D72PSF  never      12  (note)\n\
             AB        never       -  (note)\n"
        );
    }
}
