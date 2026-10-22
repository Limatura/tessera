//! The one Exif field worth keeping: which way up the picture goes.
//!
//! Exif is a TIFF structure. JPEG and WebP put it behind a six-byte `Exif\0\0`
//! header, PNG puts it in an `eXIf` chunk on its own.

/// What a JPEG or WebP puts before the TIFF structure.
pub(crate) const HEADER: &[u8; 6] = b"Exif\0\0";

/// The TIFF tag for orientation.
const ORIENTATION: u16 = 0x0112;

/// The TIFF field type for an unsigned 16-bit value.
const SHORT: u16 = 3;

/// The length of [`only_orientation`]'s structure.
pub(crate) const ONLY_ORIENTATION_LEN: usize = 26;

/// The orientation in a TIFF structure, if it has one other than the default.
///
/// Read from the first image directory, where cameras write it. `None` for
/// anything that does not parse, the default (1, upright), or a value outside
/// the eight the standard defines.
pub(crate) fn orientation(tiff: &[u8]) -> Option<u16> {
    let big = match tiff.get(..2)? {
        b"MM" => true,
        b"II" => false,
        _ => return None,
    };
    let short = |at: usize| -> Option<u16> {
        let bytes: [u8; 2] = tiff.get(at..at.checked_add(2)?)?.try_into().ok()?;
        Some(if big {
            u16::from_be_bytes(bytes)
        } else {
            u16::from_le_bytes(bytes)
        })
    };
    let long = |at: usize| -> Option<u32> {
        let bytes: [u8; 4] = tiff.get(at..at.checked_add(4)?)?.try_into().ok()?;
        Some(if big {
            u32::from_be_bytes(bytes)
        } else {
            u32::from_le_bytes(bytes)
        })
    };
    if short(2)? != 42 {
        return None;
    }
    let directory = usize::try_from(long(4)?).ok()?;
    let entries = short(directory)?;
    for index in 0..usize::from(entries) {
        let entry = directory
            .checked_add(2)?
            .checked_add(index.checked_mul(12)?)?;
        if short(entry)? != ORIENTATION {
            continue;
        }
        if short(entry.checked_add(2)?)? != SHORT {
            return None;
        }
        let value = short(entry.checked_add(8)?)?;
        return (2..=8).contains(&value).then_some(value);
    }
    None
}

/// A TIFF structure holding nothing but `orientation`.
pub(crate) const fn only_orientation(orientation: u16) -> [u8; ONLY_ORIENTATION_LEN] {
    let [high, low] = orientation.to_be_bytes();
    let [tag_high, tag_low] = ORIENTATION.to_be_bytes();
    let [type_high, type_low] = SHORT.to_be_bytes();
    // Big-endian, the first directory right after the header, one entry: the
    // orientation as one short. Then no next directory.
    [
        b'M', b'M', 0, 42, 0, 0, 0, 8, //
        0, 1, //
        tag_high, tag_low, type_high, type_low, 0, 0, 0, 1, high, low, 0, 0, //
        0, 0, 0, 0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_orientation_reads_back_from_its_own_structure() {
        for value in 2..=8 {
            assert_eq!(orientation(&only_orientation(value)), Some(value));
        }
    }

    #[test]
    fn upright_and_nonsense_have_none() {
        assert_eq!(orientation(&only_orientation(1)), None);
        assert_eq!(orientation(&only_orientation(9)), None);
        assert_eq!(orientation(b"MM"), None);
        assert_eq!(orientation(b"XX\0\x2a\0\0\0\x08"), None);
        let mut cut = only_orientation(6).to_vec();
        cut.truncate(15);
        assert_eq!(orientation(&cut), None);
    }

    #[test]
    fn little_endian_is_read_too() {
        let tiff = [
            b'I', b'I', 42, 0, 8, 0, 0, 0, //
            1, 0, //
            0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, //
            0, 0, 0, 0,
        ];
        assert_eq!(orientation(&tiff), Some(6));
    }
}
