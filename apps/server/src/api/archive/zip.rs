//! The zip half of packing a share.
//!
//! # Why this is written here rather than taken from a crate
//!
//! The size has to be known before the first byte goes out. A share with five
//! files is a download somebody watches, and a response with no
//! `Content-Length` has no progress bar and no estimated time — on exactly the
//! transfers where those matter most.
//!
//! Streaming zip crates cannot give that: they compress, so the size is not
//! known until the last byte. Storing without compression makes it arithmetic,
//! and then the entries can still be streamed one at a time without holding any
//! of them in memory.
//!
//! # Why storing rather than compressing
//!
//! What goes through here is what somebody uploaded — images, video, archives,
//! documents that are already compressed. Deflating those spends processor time
//! to make them slightly larger. The rare case that would compress well is text,
//! and a text share is not an archive.
// `as` rather than `u64::from`: the latter is not const yet, and this is a
// widening conversion of a constant — the case `as` cannot get wrong.
// The table is built in a `const fn`, where `From` and `TryFrom` are not
// available and every index is bounded by the loop that writes it. Both lints
// are right in general and cannot see either fact here.
#![expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    reason = "const-evaluated table with loop-bounded indices"
)]

//! # What is implemented
//!
//! The subset of PKWARE's format that stores: local headers, entry data, the
//! central directory, and the end record. No compression, no encryption, no
//! Zip64 — see [`MAX_ARCHIVE_SIZE`] for what that last one costs.

use std::io;

use bytes::{BufMut as _, Bytes, BytesMut};

/// Beyond this an archive needs Zip64, which this does not write.
///
/// Four gigabytes, because the classic format keeps sizes and offsets in 32
/// bits. An archive past it would be produced happily and open as nonsense, so
/// the size is checked before anything is sent and refused with an explanation
/// rather than a corrupt file.
pub const MAX_ARCHIVE_SIZE: u64 = u32::MAX as u64;

/// One file going into the archive.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The name it will have inside the archive.
    pub name: String,
    /// How many bytes it holds.
    pub size: u64,
    /// CRC-32 of its contents, which the format keeps in the header.
    ///
    /// Computed as the bytes stream past rather than up front — reading every
    /// file twice to fill in a checksum would double the work of the download.
    /// The header carries a zero and a flag saying "the real values follow the
    /// data", which is what the format provides for exactly this.
    pub crc: u32,
}

/// The bytes of a store-mode entry header, and what follows it.
#[must_use]
pub fn local_header(name: &str) -> Bytes {
    let name = name.as_bytes();
    let mut out = BytesMut::with_capacity(30 + name.len());
    out.put_u32_le(0x0403_4b50); // local file header
    out.put_u16_le(20); // version needed: 2.0
    // Bit 3: sizes and CRC come after the data, in a descriptor. Set because
    // both are unknown when this header goes out.
    out.put_u16_le(0b1000);
    out.put_u16_le(0); // stored
    out.put_u16_le(0); // modification time
    out.put_u16_le(0); // modification date
    out.put_u32_le(0); // crc, in the descriptor
    out.put_u32_le(0); // compressed size, in the descriptor
    out.put_u32_le(0); // uncompressed size, in the descriptor
    out.put_u16_le(u16::try_from(name.len()).unwrap_or(u16::MAX));
    out.put_u16_le(0); // no extra field
    out.put_slice(name);
    out.freeze()
}

/// The record that follows an entry's data, carrying what the header could not.
#[must_use]
pub fn data_descriptor(crc: u32, size: u64) -> Bytes {
    let mut out = BytesMut::with_capacity(16);
    out.put_u32_le(0x0807_4b50);
    out.put_u32_le(crc);
    let size = u32::try_from(size).unwrap_or(u32::MAX);
    out.put_u32_le(size); // compressed
    out.put_u32_le(size); // uncompressed
    out.freeze()
}

/// The central directory and the end record, which close the archive.
#[must_use]
pub fn central_directory(entries: &[(Entry, u64)]) -> Bytes {
    let mut out = BytesMut::new();
    let start = entries
        .iter()
        .map(|(entry, _)| entry_span(&entry.name, entry.size))
        .sum::<u64>();

    for (entry, offset) in entries {
        let name = entry.name.as_bytes();
        out.put_u32_le(0x0201_4b50); // central directory header
        out.put_u16_le(20); // version made by
        out.put_u16_le(20); // version needed
        out.put_u16_le(0b1000); // same flag as the local header
        out.put_u16_le(0); // stored
        out.put_u16_le(0);
        out.put_u16_le(0);
        out.put_u32_le(entry.crc);
        let size = u32::try_from(entry.size).unwrap_or(u32::MAX);
        out.put_u32_le(size);
        out.put_u32_le(size);
        out.put_u16_le(u16::try_from(name.len()).unwrap_or(u16::MAX));
        out.put_u16_le(0); // extra
        out.put_u16_le(0); // comment
        out.put_u16_le(0); // disk number
        out.put_u16_le(0); // internal attributes
        out.put_u32_le(0); // external attributes
        out.put_u32_le(u32::try_from(*offset).unwrap_or(u32::MAX));
        out.put_slice(name);
    }

    let directory_size = out.len();
    let count = u16::try_from(entries.len()).unwrap_or(u16::MAX);
    out.put_u32_le(0x0605_4b50); // end of central directory
    out.put_u16_le(0); // this disk
    out.put_u16_le(0); // disk with the directory
    out.put_u16_le(count);
    out.put_u16_le(count);
    out.put_u32_le(u32::try_from(directory_size).unwrap_or(u32::MAX));
    out.put_u32_le(u32::try_from(start).unwrap_or(u32::MAX));
    out.put_u16_le(0); // no comment
    out.freeze()
}

/// How many bytes one entry occupies before the central directory.
fn entry_span(name: &str, size: u64) -> u64 {
    30 + u64::try_from(name.len()).unwrap_or(0) + size + 16
}

/// The exact size of the archive these entries produce.
///
/// Exact because nothing is compressed: every entry contributes its header, its
/// name, its bytes and a sixteen-byte descriptor, and the directory at the end
/// is a fixed shape per entry. This is what lets the response carry a
/// `Content-Length`.
#[must_use]
pub fn archive_size(names_and_sizes: &[(String, u64)]) -> u64 {
    let entries: u64 = names_and_sizes
        .iter()
        .map(|(name, size)| entry_span(name, *size))
        .sum();
    let directory: u64 = names_and_sizes
        .iter()
        .map(|(name, _)| 46 + u64::try_from(name.len()).unwrap_or(0))
        .sum();
    entries + directory + 22
}

/// CRC-32 over a whole slice.
///
/// The 7z signature header needs one over bytes it already holds, where the
/// streaming shape below would be ceremony.
#[must_use]
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = Crc32::default();
    crc.update(bytes);
    crc.finish()
}

/// CRC-32 as the format wants it, fed a chunk at a time.
///
/// Written here rather than taken from a crate because it is a table and a loop,
/// and the alternative is a dependency whose only use is this.
#[derive(Debug, Clone, Copy)]
pub struct Crc32(u32);

impl Default for Crc32 {
    fn default() -> Self {
        Self(0xFFFF_FFFF)
    }
}

impl Crc32 {
    /// Adds a chunk.
    pub fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            let index = usize::from(u8::try_from(self.0 & 0xFF).unwrap_or(0) ^ byte);
            // The index is a byte, and the table has 256 entries. `get` rather
            // than an index because the lint cannot see that, and a checksum is
            // not worth an unwrap that could ever fire.
            self.0 = (self.0 >> 8) ^ TABLE.get(index).copied().unwrap_or(0);
        }
    }

    /// The value to write.
    #[must_use]
    pub const fn finish(self) -> u32 {
        self.0 ^ 0xFFFF_FFFF
    }
}

/// The standard CRC-32 table, built once.
static TABLE: [u32; 256] = build_table();

const fn build_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut value = i as u32; // `i` counts to 256; `From` is not const yet
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 == 1 {
                0xEDB8_8320 ^ (value >> 1)
            } else {
                value >> 1
            };
            bit += 1;
        }
        table[i] = value;
        i += 1;
    }
    table
}

/// Refuses an archive the classic format cannot describe.
///
/// # Errors
///
/// Returns [`io::Error`] when the total would need Zip64.
pub fn check_size(total: u64) -> io::Result<()> {
    if total > MAX_ARCHIVE_SIZE {
        return Err(io::Error::other(
            "this share is too large to send as one archive",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a whole archive in memory, the way the streaming path does it in
    /// pieces. The test that matters is the one that unpacks it.
    fn build(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut entries = Vec::new();
        for (name, data) in files {
            let offset = u64::try_from(out.len()).unwrap_or(0);
            out.extend_from_slice(&local_header(name));
            out.extend_from_slice(data);
            let mut crc = Crc32::default();
            crc.update(data);
            let crc = crc.finish();
            out.extend_from_slice(&data_descriptor(
                crc,
                u64::try_from(data.len()).unwrap_or(0),
            ));
            entries.push((
                Entry {
                    name: (*name).to_owned(),
                    size: u64::try_from(data.len()).unwrap_or(0),
                    crc,
                },
                offset,
            ));
        }
        out.extend_from_slice(&central_directory(&entries));
        out
    }

    /// The size has to be exact, because it goes out as `Content-Length` before
    /// the first byte. One byte wrong and the transfer hangs or truncates.
    #[test]
    fn the_predicted_size_is_the_size() {
        let files: &[(&str, &[u8])] = &[
            ("one.txt", b"hello"),
            ("two.bin", &[7u8; 1024]),
            ("a longer name.dat", b""),
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

    /// The known answer for the standard polynomial.
    #[test]
    fn the_checksum_matches_the_standard() {
        let mut crc = Crc32::default();
        crc.update(b"123456789");
        assert_eq!(crc.finish(), 0xCBF4_3926);
    }

    #[test]
    fn an_archive_past_four_gigabytes_is_refused() {
        assert!(check_size(MAX_ARCHIVE_SIZE).is_ok());
        assert!(check_size(MAX_ARCHIVE_SIZE + 1).is_err());
    }
}
