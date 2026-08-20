//! The 7z half of packing a share.
//!
//! # Why this one is the awkward one
//!
//! 7z keeps its table of contents at the *end*, and the thirty-two bytes at the
//! start point at it — offset, size and checksum. So the first bytes out of the
//! door describe something that has not been written yet.
//!
//! That is survivable because the format makes per-file checksums optional. Left
//! out, the end header depends only on the names and the sizes, both of which
//! are known before anything is read. So it can be computed up front, written
//! into the signature, and the entries streamed after it.
//!
//! The alternative would be reading every file twice — once for a checksum,
//! once to send — which doubles the work of every download to fill in a field
//! nothing here checks.
//!
//! # What is implemented
//!
//! Store only: one folder per file, each with a single Copy coder. No
//! compression, no encryption, no per-file checksums, no attributes. Empty files
//! go through the format's own empty-stream bit vectors rather than being given
//! a zero-length stream, which is what 7-Zip itself does.

use bytes::{BufMut as _, Bytes, BytesMut};

/// The six bytes every 7z file starts with.
const SIGNATURE: [u8; 6] = [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];

/// Where the packed streams begin, and what `NextHeaderOffset` is relative to.
const SIGNATURE_HEADER: u64 = 32;

// Property identifiers, as the format names them.
const K_END: u8 = 0x00;
const K_HEADER: u8 = 0x01;
const K_MAIN_STREAMS_INFO: u8 = 0x04;
const K_FILES_INFO: u8 = 0x05;
const K_PACK_INFO: u8 = 0x06;
const K_UNPACK_INFO: u8 = 0x07;
const K_FOLDER: u8 = 0x0B;
const K_CODERS_UNPACK_SIZE: u8 = 0x0C;
const K_SIZE: u8 = 0x09;
const K_EMPTY_STREAM: u8 = 0x0E;
const K_EMPTY_FILE: u8 = 0x0F;
const K_NAME: u8 = 0x11;

/// Writes a number the way 7z keeps them.
///
/// The first byte's high bits say how many more follow, and the rest are little
/// endian. Small numbers — which is nearly all of them — take one byte.
fn put_number(out: &mut BytesMut, value: u64) {
    let mut first: u8 = 0;
    let mut mask: u8 = 0x80;
    let mut extra = 0usize;
    for i in 0..8usize {
        if value < (1u64 << (7 * (i + 1))) {
            first |= u8::try_from((value >> (8 * i)) & 0xFF).unwrap_or(0);
            extra = i;
            break;
        }
        first |= mask;
        mask >>= 1;
        extra = 8;
    }
    out.put_u8(first);
    let mut rest = value;
    for _ in 0..extra {
        out.put_u8(u8::try_from(rest & 0xFF).unwrap_or(0));
        rest >>= 8;
    }
}

/// A bit per item, most significant first within each byte.
fn put_bits(out: &mut BytesMut, bits: &[bool]) {
    let mut byte = 0u8;
    let mut filled = 0u8;
    for bit in bits {
        byte |= u8::from(*bit) << (7 - filled);
        filled += 1;
        if filled == 8 {
            out.put_u8(byte);
            byte = 0;
            filled = 0;
        }
    }
    if filled > 0 {
        out.put_u8(byte);
    }
}

/// The end header describing these entries.
///
/// Deterministic: the same names and sizes always produce the same bytes, which
/// is what lets [`archive_size`] be exact.
#[must_use]
pub fn header(names_and_sizes: &[(String, u64)]) -> Bytes {
    let with_content: Vec<&(String, u64)> = names_and_sizes
        .iter()
        .filter(|(_, size)| *size > 0)
        .collect();
    let mut out = BytesMut::new();
    out.put_u8(K_HEADER);

    if !with_content.is_empty() {
        out.put_u8(K_MAIN_STREAMS_INFO);

        out.put_u8(K_PACK_INFO);
        put_number(&mut out, 0); // the streams start where the signature ends
        put_number(&mut out, u64::try_from(with_content.len()).unwrap_or(0));
        out.put_u8(K_SIZE);
        for (_, size) in &with_content {
            put_number(&mut out, *size);
        }
        out.put_u8(K_END);

        out.put_u8(K_UNPACK_INFO);
        out.put_u8(K_FOLDER);
        put_number(&mut out, u64::try_from(with_content.len()).unwrap_or(0));
        out.put_u8(0); // the folders are here, not in a stream
        for _ in &with_content {
            put_number(&mut out, 1); // one coder
            // Flags: the id is one byte long, no attributes, not complex.
            out.put_u8(0x01);
            out.put_u8(0x00); // Copy
        }
        out.put_u8(K_CODERS_UNPACK_SIZE);
        for (_, size) in &with_content {
            put_number(&mut out, *size);
        }
        // No `kCRC`: leaving it out is what makes this header computable before
        // a byte has been read. See the module note.
        out.put_u8(K_END);

        out.put_u8(K_END); // main streams info
    }

    out.put_u8(K_FILES_INFO);
    put_number(&mut out, u64::try_from(names_and_sizes.len()).unwrap_or(0));

    let empty: Vec<bool> = names_and_sizes.iter().map(|(_, size)| *size == 0).collect();
    if empty.iter().any(|is_empty| *is_empty) {
        let mut bits = BytesMut::new();
        put_bits(&mut bits, &empty);
        out.put_u8(K_EMPTY_STREAM);
        put_number(&mut out, u64::try_from(bits.len()).unwrap_or(0));
        out.put(bits.freeze());

        // Of the ones with no stream, which are files rather than directories.
        // All of them: nothing here makes a directory.
        let empties: Vec<bool> = empty.iter().filter(|e| **e).map(|_| true).collect();
        let mut bits = BytesMut::new();
        put_bits(&mut bits, &empties);
        out.put_u8(K_EMPTY_FILE);
        put_number(&mut out, u64::try_from(bits.len()).unwrap_or(0));
        out.put(bits.freeze());
    }

    let mut names = BytesMut::new();
    names.put_u8(0); // the names are here, not in a stream
    for (name, _) in names_and_sizes {
        for unit in name.encode_utf16() {
            names.put_u16_le(unit);
        }
        names.put_u16_le(0);
    }
    out.put_u8(K_NAME);
    put_number(&mut out, u64::try_from(names.len()).unwrap_or(0));
    out.put(names.freeze());

    out.put_u8(K_END); // files info
    out.put_u8(K_END); // header
    out.freeze()
}

/// The thirty-two bytes that open the file, pointing at the header.
#[must_use]
pub fn signature_header(packed_size: u64, header: &[u8]) -> Bytes {
    let mut start = BytesMut::with_capacity(20);
    start.put_u64_le(packed_size); // NextHeaderOffset, from the end of this
    start.put_u64_le(u64::try_from(header.len()).unwrap_or(0));
    start.put_u32_le(super::zip::crc32(header));

    let mut out = BytesMut::with_capacity(32);
    out.put_slice(&SIGNATURE);
    out.put_u8(0); // major
    out.put_u8(4); // minor
    out.put_u32_le(super::zip::crc32(&start));
    out.put(start.freeze());
    out.freeze()
}

/// The exact size these entries produce.
#[must_use]
pub fn archive_size(names_and_sizes: &[(String, u64)]) -> u64 {
    let packed: u64 = names_and_sizes.iter().map(|(_, size)| *size).sum();
    SIGNATURE_HEADER + packed + u64::try_from(header(names_and_sizes).len()).unwrap_or(0)
}
