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

use std::collections::HashSet;

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

/// Makes one name safe to put in an archive.
///
/// # Why this exists
///
/// The name came from whoever uploaded the file. An entry called `../../etc/x`
/// is a path traversal against whatever unpacks it, and plenty of tools still
/// honour one. Separators go, and so does anything that would leave the name
/// empty.
fn sanitise(raw: Option<&str>, position: usize) -> String {
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

/// The names a share's files take inside an archive, all of them different.
///
/// # Why the whole list at once
///
/// Two files in one share can have the same name — somebody dragging a photo
/// from each of two folders is the ordinary way it happens, not a contrived
/// one. An archive holding two entries called `photo.jpg` unpacks to one file:
/// the second overwrites the first, and nothing reports it. Measured before
/// this existed: two files in, one file out, no error anywhere.
///
/// So a repeat gets a number before its extension, the way a desktop does it.
/// Doing that needs to see the other names, which one-name-at-a-time cannot.
#[must_use]
pub fn safe_names(raw: &[Option<&str>]) -> Vec<String> {
    let mut taken: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(raw.len());
    for (position, name) in raw.iter().enumerate() {
        let base = sanitise(*name, position);
        let mut candidate = base.clone();
        let mut attempt = 1u32;
        while !taken.insert(candidate.to_lowercase()) {
            // Folded before comparing: `Photo.jpg` and `photo.jpg` are two files
            // on Linux and one on Windows and macOS, and the archive has to
            // survive being unpacked on any of them.
            candidate = numbered(&base, attempt);
            attempt = attempt.saturating_add(1);
        }
        out.push(candidate);
    }
    out
}

/// `photo.jpg` and 1 becomes `photo (1).jpg`.
fn numbered(base: &str, attempt: u32) -> String {
    match base.rsplit_once('.') {
        // A leading dot is the whole name of a dotfile, not an extension.
        Some((stem, extension)) if !stem.is_empty() => format!("{stem} ({attempt}).{extension}"),
        _ => format!("{base} ({attempt})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name from an uploader is a name an attacker chose.
    #[test]
    fn a_name_cannot_climb_out_of_the_archive() {
        assert_eq!(safe_names(&[Some("../../etc/passwd")]), ["etcpasswd"]);
        assert_eq!(safe_names(&[Some("a/b/c.txt")]), ["abc.txt"]);
        assert_eq!(safe_names(&[Some("...")]), ["file-1"]);
        assert_eq!(
            safe_names(&[None, None, None]),
            ["file-1", "file-2", "file-3"]
        );
        assert_eq!(safe_names(&[Some("holiday.jpg")]), ["holiday.jpg"]);
    }

    /// Two files with one name unpack to one file, silently. Measured before
    /// this existed: two in, one out, no error.
    #[test]
    fn a_repeated_name_is_made_different() {
        assert_eq!(
            safe_names(&[Some("photo.jpg"), Some("photo.jpg"), Some("photo.jpg")]),
            ["photo.jpg", "photo (1).jpg", "photo (2).jpg"]
        );
    }

    /// `Photo.jpg` and `photo.jpg` are two files on Linux and one on Windows and
    /// macOS. The archive has to survive being unpacked on any of them.
    #[test]
    fn a_repeat_that_differs_only_in_case_still_counts() {
        assert_eq!(
            safe_names(&[Some("Photo.JPG"), Some("photo.jpg")]),
            ["Photo.JPG", "photo (1).jpg"]
        );
    }

    /// A dotfile's leading dot is its name, not an extension.
    #[test]
    fn a_dotfile_keeps_its_shape() {
        // The leading dot is stripped as a traversal guard, so this is really a
        // check that the numbering does not produce `. (1)gitignore`.
        assert_eq!(
            safe_names(&[Some(".gitignore"), Some(".gitignore")]),
            ["gitignore", "gitignore (1)"]
        );
    }

    /// Names with no extension get the number at the end.
    #[test]
    fn a_name_without_an_extension_is_numbered_at_the_end() {
        assert_eq!(
            safe_names(&[Some("README"), Some("README")]),
            ["README", "README (1)"]
        );
    }
}
