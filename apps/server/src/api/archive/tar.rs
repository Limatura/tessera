//! The tar half of packing a share.
//!
//! # Why tar as well as zip
//!
//! It has no four-gigabyte ceiling, it keeps names as bytes and not in a
//! code page, and on Linux and macOS it is what a person already has a command
//! for. Nothing is compressed here either; see [`super`] for why.
//!
//! # What is implemented
//!
//! The USTAR shape: a 512-byte header per entry, the data padded up to the next
//! 512, and two empty blocks to finish. Long names go through a GNU `L`
//! extension record, because USTAR's own name field stops at 100 bytes and a
//! share's files are named by whoever uploaded them.

// Fixed-size arrays with indices bounded by constants a few lines above. The
// lint is right in general and wrong for a header whose every field is at a
// literal offset the format defines, where writing each one through `get_mut` would
// bury the layout under unwraps.
#![expect(
    clippy::indexing_slicing,
    reason = "fixed-layout header, offsets defined by the format"
)]

use bytes::{BufMut as _, Bytes, BytesMut};

/// Every tar structure is a multiple of this.
const BLOCK: u64 = 512;

/// The longest name USTAR's own field holds.
const SHORT_NAME: usize = 100;

/// Rounds up to the next block.
const fn padded(size: u64) -> u64 {
    size.div_ceil(BLOCK) * BLOCK
}

/// The header for one entry, and the long-name record before it when the name
/// does not fit.
#[must_use]
pub fn header(name: &str, size: u64) -> Bytes {
    let mut out = BytesMut::new();
    if name.len() > SHORT_NAME {
        // GNU's long name: a record whose *data* is the real name, typed `L`,
        // followed by the entry it belongs to. Readers that do not know the type
        // skip it and see a file called `././@LongLink`: ugly, but not
        // wrong.
        let bytes = name.as_bytes();
        let length = u64::try_from(bytes.len()).unwrap_or(0);
        out.put(block("././@LongLink", length, b'L'));
        let mut data = BytesMut::from(bytes);
        data.resize(usize::try_from(padded(length)).unwrap_or(0), 0);
        out.put(data.freeze());
    }
    out.put(block(name, size, b'0'));
    out.freeze()
}

/// One 512-byte header.
fn block(name: &str, size: u64, kind: u8) -> Bytes {
    let mut header = [0u8; 512];
    // `get`, not a slice. A name longer than the field is truncated
    // here and carried in full by the long-name record above, and a panic on
    // the boundary would be a crash triggered by a filename.
    let truncated = name
        .as_bytes()
        .get(..name.len().min(SHORT_NAME))
        .unwrap_or(b"");
    header[..truncated.len()].copy_from_slice(truncated);

    // Mode, owner and group are fixed. This is content uploaded over HTTP, so
    // there is no real answer, and inventing one would put a stranger's
    // idea of ownership on a file the recipient unpacks.
    write_octal(&mut header[100..108], 0o644);
    write_octal(&mut header[108..116], 0);
    write_octal(&mut header[116..124], 0);
    write_octal(&mut header[124..136], size);
    // A fixed timestamp, not "now", so the archive of a given share is
    // the same bytes every time. That is what makes the size predictable and a
    // retry identical.
    write_octal(&mut header[136..148], 0);
    header[156] = kind;
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");

    // The checksum is computed with its own field read as spaces, then written
    // into it, per the format's own circular definition.
    header[148..156].fill(b' ');
    let sum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
    write_octal(&mut header[148..155], u64::from(sum));
    header[155] = b' ';

    Bytes::copy_from_slice(&header)
}

/// Writes a number the way tar keeps them: octal, zero-padded, NUL-terminated.
fn write_octal(field: &mut [u8], value: u64) {
    let width = field.len().saturating_sub(1);
    let text = format!("{value:o}");
    let text = if text.len() > width {
        // Wider than the field: out of range for this shape, and zeros are a
        // visibly wrong instead of silently truncated.
        "0".repeat(width)
    } else {
        format!("{text:0>width$}")
    };
    field[..width].copy_from_slice(text.as_bytes());
    if let Some(last) = field.last_mut() {
        *last = 0;
    }
}

/// The padding that follows an entry's data.
#[must_use]
pub fn padding(size: u64) -> Bytes {
    let needed = padded(size).saturating_sub(size);
    Bytes::from(vec![0u8; usize::try_from(needed).unwrap_or(0)])
}

/// The two empty blocks that end an archive.
#[must_use]
pub fn trailer() -> Bytes {
    Bytes::from(vec![0u8; usize::try_from(BLOCK * 2).unwrap_or(1024)])
}

/// The exact size these entries produce.
#[must_use]
pub fn archive_size(names_and_sizes: &[(String, u64)]) -> u64 {
    let entries: u64 = names_and_sizes
        .iter()
        .map(|(name, size)| {
            let long = if name.len() > SHORT_NAME {
                BLOCK + padded(u64::try_from(name.len()).unwrap_or(0))
            } else {
                0
            };
            long + BLOCK + padded(*size)
        })
        .sum();
    entries + BLOCK * 2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, data) in files {
            out.extend_from_slice(&header(name, u64::try_from(data.len()).unwrap_or(0)));
            out.extend_from_slice(data);
            out.extend_from_slice(&padding(u64::try_from(data.len()).unwrap_or(0)));
        }
        out.extend_from_slice(&trailer());
        out
    }

    /// The size goes out as `Content-Length` before the first byte, so one byte
    /// wrong truncates or hangs the transfer.
    #[test]
    fn the_predicted_size_is_the_size() {
        let files: &[(&str, &[u8])] = &[
            ("one.txt", b"hello"),
            ("two.bin", &[7u8; 1024]),
            ("empty", b""),
            (
                "a name long enough to need the extension record, which is a hundred characters or more in total",
                b"x",
            ),
        ];
        let built = build(files);
        let predicted = archive_size(
            &files
                .iter()
                .map(|(name, data)| ((*name).to_owned(), u64::try_from(data.len()).unwrap_or(0)))
                .collect::<Vec<_>>(),
        );
        assert_eq!(predicted, u64::try_from(built.len()).unwrap_or(0));
    }
}
