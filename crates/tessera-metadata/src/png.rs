//! PNG, rebuilt without its metadata chunks.
//!
//! A PNG is a signature and a run of chunks, each a length, a type, the data
//! and a checksum. Text chunks can hold anything (XMP with a location among
//! them), `tIME` says when the file was last changed, and `eXIf` is Exif. Every
//! other chunk stays as it is.

use crate::{Error, exif};

const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// Chunks that carry metadata and nothing a decoder needs.
const METADATA: [&[u8; 4]; 5] = [b"tEXt", b"zTXt", b"iTXt", b"tIME", b"eXIf"];

const MALFORMED: Error = Error::Malformed("PNG");

pub(crate) fn is(head: &[u8]) -> bool {
    head.starts_with(SIGNATURE)
}

pub(crate) fn strip(image: &[u8]) -> Result<Option<Vec<u8>>, Error> {
    let mut out = SIGNATURE.to_vec();
    let mut at = SIGNATURE.len();
    loop {
        let length = image
            .get(at..at.checked_add(4).ok_or(MALFORMED)?)
            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
            .map(u32::from_be_bytes)
            .and_then(|length| usize::try_from(length).ok())
            .ok_or(MALFORMED)?;
        let end = at
            .checked_add(12)
            .and_then(|end| end.checked_add(length))
            .filter(|&end| end <= image.len())
            .ok_or(MALFORMED)?;
        let chunk = image.get(at..end).ok_or(MALFORMED)?;
        let kind = chunk.get(4..8).ok_or(MALFORMED)?;
        if kind == b"eXIf" {
            let data = chunk.get(8..8 + length).ok_or(MALFORMED)?;
            if let Some(orientation) = exif::orientation(data) {
                push(&mut out, *b"eXIf", &exif::only_orientation(orientation));
            }
        } else if !METADATA.iter().any(|metadata| kind == metadata.as_slice()) {
            out.extend_from_slice(chunk);
        }
        at = end;
        if kind == b"IEND" {
            break;
        }
    }
    // Anything after the end is not part of the image.
    Ok((out != image).then_some(out))
}

/// Appends a chunk, with its length and checksum.
fn push(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    let length = u32::try_from(data.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&length.to_be_bytes());
    let mut crc = crc32fast::Hasher::new();
    crc.update(&kind);
    crc.update(data);
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc.finalize().to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real two-by-two PNG with metadata chunks added after its header.
    fn sample(orientation: u16) -> Vec<u8> {
        let mut encoded = Vec::new();
        image::RgbImage::from_pixel(2, 2, image::Rgb([200, 30, 30]))
            .write_to(
                &mut std::io::Cursor::new(&mut encoded),
                image::ImageFormat::Png,
            )
            .unwrap_or_default();
        // Signature and IHDR, which is always first.
        let (head, rest) = encoded.split_at(8 + 25);
        let mut image = head.to_vec();
        push(&mut image, *b"tEXt", b"Comment\0taken by someone");
        push(
            &mut image,
            *b"iTXt",
            b"XML:com.adobe.xmp\0\0\0\0\0<x:xmpmeta/>",
        );
        push(&mut image, *b"tIME", &[7, 234, 10, 22, 12, 0, 0]);
        push(&mut image, *b"eXIf", &exif::only_orientation(orientation));
        image.extend_from_slice(rest);
        image
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    #[test]
    fn metadata_chunks_go_and_the_picture_stays() {
        let image = sample(1);
        let clean = strip(&image).unwrap_or_default().unwrap_or_default();
        for gone in [&b"someone"[..], b"xmpmeta", b"tIME", b"eXIf"] {
            assert!(!contains(&clean, gone), "{}", String::from_utf8_lossy(gone));
        }
        let before = image::load_from_memory(&image).map(|i| i.to_rgb8());
        let after = image::load_from_memory(&clean).map(|i| i.to_rgb8());
        assert!(after.is_ok(), "{after:?}");
        assert_eq!(before.ok(), after.ok());
    }

    #[test]
    fn the_orientation_is_kept_in_a_fresh_exif_chunk() {
        let clean = strip(&sample(8)).unwrap_or_default().unwrap_or_default();
        assert!(contains(&clean, b"eXIf"));
        assert!(contains(&clean, &exif::only_orientation(8)));
        assert!(image::load_from_memory(&clean).is_ok());
    }

    #[test]
    fn a_clean_file_is_left_alone() {
        let clean = strip(&sample(1)).unwrap_or_default().unwrap_or_default();
        assert_eq!(strip(&clean), Ok(None));
    }

    #[test]
    fn a_chunk_running_past_the_end_is_malformed() {
        let mut image = sample(1);
        image.truncate(40);
        assert_eq!(strip(&image), Err(MALFORMED));
    }
}
