//! Packing several files into one download.
//!
//! # Why a share with several files is packed at all
//!
//! Handing somebody five files means five downloads and five things to keep
//! track of. One is what a person asked for when they followed one link.
//!
//! # Why nothing is compressed
//!
//! What goes through here is what somebody uploaded — images, video, archives,
//! documents that are already compressed. Deflating those spends processor time
//! to make them slightly larger. The rare case that would compress well is text,
//! and a text share is not an archive.
//!
//! Storing rather than compressing also means no compression library, which is
//! the difference between pure Rust and a C dependency for every format here.
//!
//! # Why the formats are written here rather than taken from crates
//!
//! The size has to be known before the first byte goes out — a share with five
//! files is a download somebody watches, and a response with no
//! `Content-Length` has no progress bar and no estimated time.
//!
//! That size has to match the bytes exactly. If a crate wrote the bytes and this
//! predicted the length, the two would have to agree to the byte through every
//! version of that crate — one added extra field and the transfer truncates.
//! Written here, the prediction and the bytes come from the same code, and one
//! test pins them together.
//!
//! Every format is checked against an outside tool: `zipfile` and `tarfile` from
//! Python's standard library, and 7-Zip itself for the third.

pub mod sevenz;
pub mod tar;
pub mod zip;

use serde::Deserialize;

/// Which container a caller asked for.
///
/// # Why the choice is the recipient's
///
/// They are the one who has to open it. What a machine can unpack varies by
/// operating system and by what somebody installed, and the uploader does not
/// know who they sent the link to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// The default. The only one Windows, macOS and Linux all open without
    /// anything installed.
    #[default]
    Zip,
    /// No size ceiling, names kept as bytes, and already on every Unix.
    Tar,
    /// For somebody who asked for it.
    #[serde(rename = "7z")]
    SevenZ,
}

impl Format {
    /// The extension to put on the download.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::Tar => "tar",
            Self::SevenZ => "7z",
        }
    }

    /// What to send as `Content-Type`.
    ///
    /// None of these is `application/octet-stream`: a browser that knows the
    /// type offers the right thing to open it with.
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Zip => "application/zip",
            Self::Tar => "application/x-tar",
            Self::SevenZ => "application/x-7z-compressed",
        }
    }

    /// The exact number of bytes this format produces for these entries.
    #[must_use]
    pub fn archive_size(self, names_and_sizes: &[(String, u64)]) -> u64 {
        match self {
            Self::Zip => zip::archive_size(names_and_sizes),
            Self::Tar => tar::archive_size(names_and_sizes),
            Self::SevenZ => sevenz::archive_size(names_and_sizes),
        }
    }
}

/// Refuses an archive a format cannot describe.
///
/// # Errors
///
/// Returns [`crate::error::Error`] when zip's four-gigabyte ceiling would be
/// passed. tar and 7z have no such limit, so they are never refused here.
pub fn check_size(format: Format, total: u64) -> crate::error::Result<()> {
    if format == Format::Zip && total > zip::MAX_ARCHIVE_SIZE {
        return Err(crate::error::Error::bad_request(
            "this share is too large for a zip; ask for it as tar or 7z instead",
        ));
    }
    Ok(())
}

/// Makes a name safe to put in an archive.
///
/// # Why this exists
///
/// The name came from whoever uploaded the file. An entry called `../../etc/x`
/// is a path traversal against whatever unpacks it, and plenty of tools still
/// honour one. Separators go, and so does anything that would leave the name
/// empty.
#[must_use]
pub fn safe_name(raw: Option<&str>, position: usize) -> String {
    let cleaned: String = raw
        .unwrap_or("")
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | '\0'))
        .collect();
    let trimmed = cleaned.trim().trim_start_matches('.');
    if trimmed.is_empty() {
        format!("file-{}", position + 1)
    } else {
        trimmed.to_owned()
    }
}
