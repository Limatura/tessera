//! JPEG, cleaned in place.
//!
//! A JPEG is a run of segments, each a marker and, for most, a length. The
//! metadata lives in the application segments (`APPn`) and comments before the
//! image data. Every one of them is overwritten except the three a decoder
//! needs: `APP0` (JFIF), `APP2` (colour profile, and the index of any further
//! images) and `APP14` (Adobe's colour transform).

use crate::{Error, exif};

const MARK: u8 = 0xFF;
const SOI: u8 = 0xD8;
const EOI: u8 = 0xD9;
const SOS: u8 = 0xDA;
const COM: u8 = 0xFE;
const APP0: u8 = 0xE0;
const APP1: u8 = 0xE1;
const APP2: u8 = 0xE2;
const APP14: u8 = 0xEE;

const MALFORMED: Error = Error::Malformed("JPEG");

pub(crate) fn is(head: &[u8]) -> bool {
    head.starts_with(&[MARK, SOI, MARK])
}

pub(crate) fn strip(image: &[u8]) -> Result<Option<Vec<u8>>, Error> {
    let mut out = image.to_vec();
    let mut changed = false;
    let mut start = 0;
    let end = loop {
        let end = clean(&mut out, start, &mut changed)?;
        // A phone's depth map or gain map is a whole JPEG of its own right
        // after the first.
        match out.get(end..) {
            Some(rest) if is(rest) => start = end,
            _ => break end,
        }
    };
    if end < out.len() {
        out.truncate(end);
        changed = true;
    }
    Ok(changed.then_some(out))
}

/// Cleans the JPEG starting at `start` and returns where it ends.
fn clean(image: &mut [u8], start: usize, changed: &mut bool) -> Result<usize, Error> {
    let mut at = start.checked_add(2).ok_or(MALFORMED)?;
    loop {
        if image.get(at) != Some(&MARK) {
            // Cut short: what was before has been cleaned, and there is no
            // more.
            return if at >= image.len() {
                Ok(image.len())
            } else {
                Err(MALFORMED)
            };
        }
        // Any number of fill bytes may come before a marker.
        while image.get(at.checked_add(1).ok_or(MALFORMED)?) == Some(&MARK) {
            at = at.checked_add(1).ok_or(MALFORMED)?;
        }
        let Some(&marker) = image.get(at.checked_add(1).ok_or(MALFORMED)?) else {
            return Ok(image.len());
        };
        let body = at.checked_add(2).ok_or(MALFORMED)?;
        match marker {
            EOI => return Ok(body),
            // Restart markers and TEM stand alone, with no length.
            0xD0..=0xD7 | 0x01 => {
                at = body;
                continue;
            }
            _ => {}
        }
        let length = image
            .get(body..body.checked_add(2).ok_or(MALFORMED)?)
            .and_then(|bytes| <[u8; 2]>::try_from(bytes).ok())
            .map(u16::from_be_bytes)
            .ok_or(MALFORMED)?;
        let end = body
            .checked_add(usize::from(length))
            .filter(|&end| end <= image.len() && length >= 2)
            .ok_or(MALFORMED)?;
        let payload = image
            .get_mut(body.checked_add(2).ok_or(MALFORMED)?..end)
            .ok_or(MALFORMED)?;
        if is_metadata(marker) {
            *changed |= overwrite(marker, payload);
        }
        at = end;
        if marker == SOS {
            at = after_scan(image, at);
        }
    }
}

/// Whether a segment holds metadata rather than something a decoder needs.
const fn is_metadata(marker: u8) -> bool {
    match marker {
        APP0 | APP2 | APP14 => false,
        0xE0..=0xEF | COM => true,
        _ => false,
    }
}

/// Overwrites a metadata segment's contents, and says whether that changed
/// anything.
///
/// Zeros, so the segment is no longer recognised as anything. An Exif segment
/// that set an orientation gets that orientation back, on its own.
fn overwrite(marker: u8, payload: &mut [u8]) -> bool {
    let before = payload.iter().any(|&byte| byte != 0);
    let kept = (marker == APP1)
        .then(|| payload.strip_prefix(exif::HEADER))
        .flatten()
        .and_then(exif::orientation)
        .filter(|_| payload.len() >= exif::HEADER.len() + exif::ONLY_ORIENTATION_LEN);
    let original = kept.map(|_| payload.to_vec());
    payload.fill(0);
    if let Some(orientation) = kept {
        let (header, rest) = payload.split_at_mut(exif::HEADER.len());
        header.copy_from_slice(exif::HEADER);
        if let Some(tiff) = rest.get_mut(..exif::ONLY_ORIENTATION_LEN) {
            tiff.copy_from_slice(&exif::only_orientation(orientation));
        }
        return original.as_deref() != Some(payload);
    }
    before
}

/// Where the next marker is after a scan's coded data.
///
/// In the data a `0xFF` byte is followed by a zero, and restart markers can
/// appear between intervals; anything else is the next segment. The end of
/// the file if none is found.
fn after_scan(image: &[u8], from: usize) -> usize {
    let mut at = from;
    while let Some(&byte) = image.get(at) {
        if byte == MARK {
            match image.get(at.saturating_add(1)).copied() {
                Some(0x00 | 0xD0..=0xD7 | MARK) => {}
                Some(_) => return at,
                None => break,
            }
        }
        at = at.saturating_add(1);
    }
    image.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A segment with a marker, its length and its payload.
    fn segment(marker: u8, payload: &[u8]) -> Vec<u8> {
        let length = u16::try_from(payload.len() + 2).unwrap_or(u16::MAX);
        let mut bytes = vec![MARK, marker];
        bytes.extend(length.to_be_bytes());
        bytes.extend(payload);
        bytes
    }

    fn exif_with(orientation: u16, extra: &[u8]) -> Vec<u8> {
        let mut payload = exif::HEADER.to_vec();
        payload.extend(exif::only_orientation(orientation));
        payload.extend(extra);
        payload
    }

    /// Enough of a JPEG to walk: metadata, a table, a scan, the end.
    fn sample(extra: &[u8]) -> Vec<u8> {
        let mut image = vec![MARK, SOI];
        image.extend(segment(APP0, b"JFIF\0\x01\x01"));
        image.extend(segment(APP1, &exif_with(6, b"GPS 25.03N 121.56E")));
        image.extend(segment(APP1, b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta/>"));
        image.extend(segment(APP2, b"ICC_PROFILE\0"));
        image.extend(segment(COM, b"taken by someone"));
        image.extend(segment(0xDB, &[0; 65]));
        image.extend(segment(SOS, &[1, 1, 0, 0, 63, 0]));
        // Coded data, with a stuffed 0xFF and a restart marker in it.
        image.extend([0x12, MARK, 0x00, 0x34, MARK, 0xD0, 0x56]);
        image.extend([MARK, EOI]);
        image.extend(extra);
        image
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    #[test]
    fn metadata_is_overwritten_and_everything_else_kept() {
        let image = sample(&[]);
        let clean = strip(&image).unwrap_or_default().unwrap_or_default();
        assert_eq!(clean.len(), image.len(), "cleaned in place");
        for gone in [&b"GPS"[..], b"xmpmeta", b"someone"] {
            assert!(!contains(&clean, gone), "{}", String::from_utf8_lossy(gone));
        }
        for kept in [
            &b"JFIF"[..],
            b"ICC_PROFILE",
            &[0x12, MARK, 0x00, 0x34, MARK, 0xD0, 0x56],
        ] {
            assert!(contains(&clean, kept), "{}", String::from_utf8_lossy(kept));
        }
    }

    #[test]
    fn the_orientation_survives_on_its_own() {
        let clean = strip(&sample(&[])).unwrap_or_default().unwrap_or_default();
        let at = clean
            .windows(exif::HEADER.len())
            .position(|window| window == exif::HEADER)
            .unwrap_or_default();
        let tiff = clean.get(at + exif::HEADER.len()..).unwrap_or_default();
        assert_eq!(exif::orientation(tiff), Some(6));
    }

    #[test]
    fn a_second_image_is_cleaned_and_what_follows_it_dropped() {
        let mut second = vec![MARK, SOI];
        second.extend(segment(APP1, &exif_with(1, b"GPS again")));
        second.extend([MARK, EOI]);
        let mut extra = second.clone();
        extra.extend(b"ftypmp42 a motion photo's video");
        let image = sample(&extra);

        let clean = strip(&image).unwrap_or_default().unwrap_or_default();
        assert_eq!(clean.len(), sample(&second).len());
        assert!(!contains(&clean, b"GPS"));
        assert!(!contains(&clean, b"ftyp"));
    }

    fn shade(at: u32) -> u8 {
        u8::try_from(at * 16).unwrap_or(u8::MAX)
    }

    #[test]
    fn a_real_jpeg_decodes_the_same_after() {
        let mut encoded = Vec::new();
        image::RgbImage::from_fn(16, 16, |x, y| image::Rgb([shade(x), shade(y), 90]))
            .write_to(
                &mut std::io::Cursor::new(&mut encoded),
                image::ImageFormat::Jpeg,
            )
            .unwrap_or_default();
        let mut image = vec![MARK, SOI];
        image.extend(segment(APP1, &exif_with(3, b"GPS 25.03N 121.56E")));
        image.extend(encoded.get(2..).unwrap_or_default());

        let clean = strip(&image).unwrap_or_default().unwrap_or_default();
        assert!(!contains(&clean, b"GPS"));
        let before = image::load_from_memory(&image).map(|i| i.to_rgb8());
        let after = image::load_from_memory(&clean).map(|i| i.to_rgb8());
        assert!(after.is_ok(), "{after:?}");
        assert_eq!(before.ok(), after.ok());
    }

    #[test]
    fn a_clean_file_is_left_alone() {
        let clean = strip(&sample(&[])).unwrap_or_default().unwrap_or_default();
        assert_eq!(strip(&clean), Ok(None));
    }

    #[test]
    fn a_broken_length_is_malformed() {
        let mut image = vec![MARK, SOI];
        image.extend([MARK, APP1, 0xFF, 0xF0, b'E', b'x']);
        assert_eq!(strip(&image), Err(MALFORMED));
        assert_eq!(strip(&[MARK, SOI, MARK, APP1, 0, 1]), Err(MALFORMED));
    }

    #[test]
    fn a_file_cut_short_is_cleaned_as_far_as_it_goes() {
        let mut image = sample(&[]);
        image.truncate(image.len() - 4);
        let clean = strip(&image).unwrap_or_default().unwrap_or_default();
        assert!(!contains(&clean, b"GPS"));
        assert_eq!(clean.len(), image.len());
    }
}
