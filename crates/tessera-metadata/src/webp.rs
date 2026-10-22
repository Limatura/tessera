//! WebP, rebuilt without its metadata chunks.
//!
//! A WebP is a RIFF file: a header with the total size, then chunks of a type,
//! a size and the data, padded to an even length. Metadata is in `EXIF` and
//! `XMP ` chunks, which the extended header (`VP8X`) announces with two flags.
//! Both chunks go and both flags are cleared, except that an orientation comes
//! back in an `EXIF` chunk of its own.

use crate::{Error, exif};

/// The `VP8X` flags for an `EXIF` and an `XMP ` chunk.
const EXIF_FLAG: u8 = 0x08;
const XMP_FLAG: u8 = 0x04;

const MALFORMED: Error = Error::Malformed("WebP");

pub(crate) fn is(head: &[u8]) -> bool {
    head.get(..4) == Some(b"RIFF") && head.get(8..12) == Some(b"WEBP")
}

pub(crate) fn strip(image: &[u8]) -> Result<Option<Vec<u8>>, Error> {
    let declared = image
        .get(4..8)
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .map(u32::from_le_bytes)
        .and_then(|size| usize::try_from(size).ok())
        .and_then(|size| size.checked_add(8))
        .filter(|&end| end <= image.len())
        .ok_or(MALFORMED)?;
    let mut out = image.get(..12).ok_or(MALFORMED)?.to_vec();
    // Where the `VP8X` flags landed, and the orientation to put back.
    let mut flags = None;
    let mut orientation = None;
    let mut at = 12;
    while at < declared {
        let size = image
            .get(at.checked_add(4).ok_or(MALFORMED)?..at.checked_add(8).ok_or(MALFORMED)?)
            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
            .map(u32::from_le_bytes)
            .and_then(|size| usize::try_from(size).ok())
            .ok_or(MALFORMED)?;
        let end = at
            .checked_add(8)
            .and_then(|end| end.checked_add(size))
            .and_then(|end| end.checked_add(size % 2))
            .filter(|&end| end <= declared)
            .ok_or(MALFORMED)?;
        let chunk = image.get(at..end).ok_or(MALFORMED)?;
        match chunk.get(..4) {
            Some(b"EXIF") => {
                // The format says the TIFF structure starts right away, but
                // some writers put a JPEG's header first.
                let data = chunk.get(8..8 + size).ok_or(MALFORMED)?;
                orientation = exif::orientation(data.strip_prefix(exif::HEADER).unwrap_or(data));
            }
            Some(b"XMP ") => {}
            Some(b"VP8X") => {
                flags = Some(out.len() + 8);
                out.extend_from_slice(chunk);
            }
            _ => out.extend_from_slice(chunk),
        }
        at = end;
    }
    if let Some(flags) = flags.and_then(|at| out.get_mut(at)) {
        *flags &= !(EXIF_FLAG | XMP_FLAG);
        if orientation.is_some() {
            *flags |= EXIF_FLAG;
        }
    }
    // Last, where the format puts metadata.
    if let Some(orientation) = orientation.filter(|_| flags.is_some()) {
        let tiff = exif::only_orientation(orientation);
        out.extend_from_slice(b"EXIF");
        out.extend_from_slice(
            &u32::try_from(tiff.len())
                .map_err(|_| MALFORMED)?
                .to_le_bytes(),
        );
        out.extend_from_slice(&tiff);
    }
    let size = u32::try_from(out.len() - 8).map_err(|_| MALFORMED)?;
    out.get_mut(4..8)
        .ok_or(MALFORMED)?
        .copy_from_slice(&size.to_le_bytes());
    Ok((out != image).then_some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(kind: [u8; 4], data: &[u8]) -> Vec<u8> {
        let mut bytes = kind.to_vec();
        bytes.extend(u32::try_from(data.len()).unwrap_or(u32::MAX).to_le_bytes());
        bytes.extend(data);
        if data.len() % 2 == 1 {
            bytes.push(0);
        }
        bytes
    }

    /// A real lossless WebP, wrapped in an extended header with metadata.
    fn sample() -> Vec<u8> {
        let mut encoded = Vec::new();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([30, 200, 30, 255]))
            .write_to(
                &mut std::io::Cursor::new(&mut encoded),
                image::ImageFormat::WebP,
            )
            .unwrap_or_default();
        let picture = encoded.get(12..).unwrap_or_default();
        // Flags (EXIF and XMP), reserved, then the canvas less one, as 24 bits.
        let header = [EXIF_FLAG | XMP_FLAG, 0, 0, 0, 1, 0, 0, 1, 0, 0];
        let mut body = b"WEBP".to_vec();
        body.extend(chunk(*b"VP8X", &header));
        body.extend(picture);
        let mut exif_data = exif::HEADER.to_vec();
        exif_data.extend(exif::only_orientation(6));
        exif_data.extend(b"GPS 25.03N 121.56E");
        body.extend(chunk(*b"EXIF", &exif_data));
        body.extend(chunk(*b"XMP ", b"<x:xmpmeta/>"));
        let mut image = b"RIFF".to_vec();
        image.extend(u32::try_from(body.len()).unwrap_or(u32::MAX).to_le_bytes());
        image.extend(body);
        image
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    #[test]
    fn metadata_chunks_and_their_flags_go() {
        let image = sample();
        let clean = strip(&image).unwrap_or_default().unwrap_or_default();
        assert!(!contains(&clean, b"GPS"));
        assert!(!contains(&clean, b"xmpmeta"));
        assert_eq!(clean.get(20).map(|flags| flags & XMP_FLAG), Some(0));
        let size = clean
            .get(4..8)
            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
            .map(u32::from_le_bytes);
        assert_eq!(
            size.and_then(|size| usize::try_from(size).ok()),
            Some(clean.len() - 8)
        );
        let before = image::load_from_memory(&image).map(|i| i.to_rgba8());
        let after = image::load_from_memory(&clean).map(|i| i.to_rgba8());
        assert!(after.is_ok(), "{after:?}");
        assert_eq!(before.ok(), after.ok());
    }

    #[test]
    fn the_orientation_comes_back_on_its_own() {
        let clean = strip(&sample()).unwrap_or_default().unwrap_or_default();
        assert_eq!(
            clean.get(20).map(|flags| flags & EXIF_FLAG),
            Some(EXIF_FLAG)
        );
        assert!(contains(&clean, &exif::only_orientation(6)));
        assert!(image::load_from_memory(&clean).is_ok());
    }

    #[test]
    fn a_clean_file_is_left_alone() {
        let clean = strip(&sample()).unwrap_or_default().unwrap_or_default();
        assert_eq!(strip(&clean), Ok(None));
    }

    #[test]
    fn a_size_past_the_end_is_malformed() {
        let mut image = sample();
        image.truncate(30);
        assert_eq!(strip(&image), Err(MALFORMED));
    }
}
