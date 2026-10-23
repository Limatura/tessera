//! The file types an operator would rather this instance did not share.
//!
//! `share.blocked_extensions` lists them by extension. A file falls under one
//! by its name, or by its first bytes when they say it is a program: renaming
//! `setup.exe` to `setup.jpg` changes the name and not the bytes.
//!
//! Only programs are told from their bytes, because they are what gets a
//! domain flagged and because their first bytes say so plainly. Most other
//! formats need more than the start of the file to be told apart: an old Word
//! document and a Windows installer begin the same way, and so do an Android
//! app and any zip.
//!
//! What the bytes said is recorded with the file, so a rule added later is
//! kept on download without reading the file back.
//!
//! Files in an end-to-end encrypted share have neither a readable name nor
//! readable bytes, and go through.

use object_store::{ObjectStoreExt as _, path::Path as ObjectPath};

use crate::{
    db::repository::share::{Content, NewFile, StorageLocation},
    error::{Error, ErrorCode, Result},
    state::AppState,
};

/// How much of the start of a file [`detect`] looks at.
const HEAD_BYTES: u64 = 8;

/// What a file's first bytes say it is, when they say it is a program.
///
/// `exe` for a Windows program (DLLs and screensavers included, which begin
/// the same way), `elf` for a Linux one, `macho` for a macOS one.
#[must_use]
pub fn detect(head: &[u8]) -> Option<&'static str> {
    match head {
        [b'M', b'Z', ..] => Some("exe"),
        [0x7F, b'E', b'L', b'F', ..] => Some("elf"),
        [0xFE, 0xED, 0xFA, 0xCE | 0xCF, ..] | [0xCE | 0xCF, 0xFA, 0xED, 0xFE, ..] => Some("macho"),
        // A universal binary, which compiled Java begins with too. Java follows
        // with its version, 45 or more; a universal binary with how many
        // architectures it holds, which is a handful.
        [0xCA, 0xFE, 0xBA, 0xBE, a, b, c, d, ..] if u32::from_be_bytes([*a, *b, *c, *d]) < 45 => {
            Some("macho")
        }
        _ => None,
    }
}

/// The entry of `blocked` a file falls under, if any.
///
/// Entries are matched without their case or a leading dot, so `.EXE` lists
/// `exe`. An entry of two parts, like `tar.gz`, matches a name ending in both.
fn listed<'a>(
    blocked: &'a [String],
    filename: Option<&str>,
    detected: Option<&str>,
) -> Option<&'a str> {
    // The last component: a file uploaded from a folder can carry its path.
    let name = filename
        .and_then(|name| name.rsplit(['/', '\\']).next())
        .map(str::to_lowercase);
    blocked
        .iter()
        .map(|entry| entry.trim().trim_start_matches('.'))
        .filter(|entry| !entry.is_empty())
        .find(|entry| {
            let entry = entry.to_lowercase();
            detected == Some(entry.as_str())
                || name.as_deref().is_some_and(|name| {
                    name.strip_suffix(entry.as_str())
                        .is_some_and(|rest| rest.ends_with('.'))
                })
        })
}

fn refused(extension: &str) -> Error {
    Error::new(ErrorCode::FileTypeBlocked).with_detail("extension", extension)
}

/// Refuses a file by its name, before any of it is sent.
///
/// # Errors
///
/// [`ErrorCode::FileTypeBlocked`] when its name ends in a listed extension.
pub fn check_name(state: &AppState, filename: Option<&str>) -> Result<()> {
    let config = state.config();
    match listed(&config.share.blocked_extensions, filename, None) {
        Some(extension) => Err(refused(extension)),
        None => Ok(()),
    }
}

/// Looks at a file that has arrived, before it is recorded, and notes what
/// its first bytes say it is.
///
/// A file of a listed type has its bytes deleted and is refused. Encrypted
/// files are not looked at.
///
/// # Errors
///
/// [`ErrorCode::FileTypeBlocked`] when it is of a listed type, or the store's
/// error when its start cannot be read.
pub async fn admit(state: &AppState, mut file: NewFile) -> Result<NewFile> {
    if file.sealed_name.is_some() {
        return Ok(file);
    }
    let detected = match first_object(&file) {
        Some((backend, key)) if file.size_bytes > 0 => {
            let store = state.backend(backend).ok_or(Error::Unavailable)?;
            let head = store
                .get_range(&ObjectPath::from(key), 0..HEAD_BYTES.min(file.size_bytes))
                .await?;
            detect(&head)
        }
        _ => None,
    };
    let config = state.config();
    if let Some(extension) = listed(
        &config.share.blocked_extensions,
        file.filename.as_deref(),
        detected,
    ) {
        discard(state, &file).await;
        return Err(refused(extension));
    }
    file.detected_type = detected.map(str::to_owned);
    Ok(file)
}

/// The backend and key holding a file's first byte.
fn first_object(file: &NewFile) -> Option<(&str, &str)> {
    match file.segments.iter().find(|piece| piece.offset_bytes == 0) {
        Some(piece) => Some((&piece.backend, &piece.object_key)),
        None => match &file.storage {
            StorageLocation::Object { backend, key } => Some((backend, key)),
            StorageLocation::Inline | StorageLocation::Gone => None,
        },
    }
}

/// Deletes a refused file's bytes.
///
/// A failure is only logged: the reconciliation pass collects objects no share
/// references, so the bytes are kept longer, not for ever.
async fn discard(state: &AppState, file: &NewFile) {
    let mut objects: Vec<(&str, &str)> = file
        .segments
        .iter()
        .map(|piece| (piece.backend.as_str(), piece.object_key.as_str()))
        .collect();
    if objects.is_empty() {
        objects.extend(first_object(file));
    }
    for (backend, key) in objects {
        let Some(store) = state.backend(backend) else {
            continue;
        };
        if let Err(error) = store.delete(&ObjectPath::from(key)).await {
            tracing::warn!(%error, backend, key, "could not delete a refused upload");
        }
    }
}

/// Refuses to serve content holding a file of a listed type.
///
/// For a rule added after the file was shared, or a file shared before what
/// its bytes say was recorded, which only has its name to go by.
///
/// # Errors
///
/// [`ErrorCode::FileTypeBlocked`] when a file in it is of a listed type.
pub fn release(state: &AppState, content: &Content) -> Result<()> {
    let config = state.config();
    let blocked = &config.share.blocked_extensions;
    if blocked.is_empty() {
        return Ok(());
    }
    for file in content.files() {
        if let Some(extension) = listed(
            blocked,
            file.filename.as_deref(),
            file.detected_type.as_deref(),
        ) {
            return Err(refused(extension));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(entries: &[&str]) -> Vec<String> {
        entries.iter().map(|&entry| entry.to_owned()).collect()
    }

    #[test]
    fn programs_are_told_from_their_first_bytes() {
        assert_eq!(detect(b"MZ\x90\0\x03\0\0\0"), Some("exe"));
        assert_eq!(detect(b"\x7fELF\x02\x01\x01\0"), Some("elf"));
        assert_eq!(detect(&[0xCF, 0xFA, 0xED, 0xFE, 7, 0, 0, 1]), Some("macho"));
        assert_eq!(detect(&[0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 2]), Some("macho"));
        // Compiled Java, version 52.
        assert_eq!(detect(&[0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 52]), None);
        assert_eq!(detect(b"\x89PNG\r\n\x1a\n"), None);
        assert_eq!(detect(b"M"), None);
    }

    #[test]
    fn a_name_matches_without_case_or_a_leading_dot() {
        let blocked = list(&[".EXE", "apk", "tar.gz"]);
        assert_eq!(listed(&blocked, Some("Setup.exe"), None), Some("EXE"));
        assert_eq!(listed(&blocked, Some("photos/app.APK"), None), Some("apk"));
        assert_eq!(
            listed(&blocked, Some("backup.tar.gz"), None),
            Some("tar.gz")
        );
        assert_eq!(listed(&blocked, Some("notes.gz"), None), None);
        assert_eq!(listed(&blocked, Some("exe"), None), None);
        assert_eq!(listed(&blocked, Some("my-apk"), None), None);
        assert_eq!(listed(&blocked, None, None), None);
    }

    #[test]
    fn a_renamed_program_is_caught_by_what_its_bytes_say() {
        let blocked = list(&["exe"]);
        assert_eq!(listed(&blocked, Some("cat.jpg"), Some("exe")), Some("exe"));
        assert_eq!(listed(&blocked, Some("tool"), Some("elf")), None);
    }

    #[test]
    fn blank_entries_match_nothing() {
        let blocked = list(&["", " ", "."]);
        assert_eq!(listed(&blocked, Some("a."), None), None);
    }
}
