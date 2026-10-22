//! Removes the metadata a photo carries before it is shared.
//!
//! A phone writes where a photo was taken into the file, along with the camera,
//! the time and sometimes the owner's name. Whoever downloads the share gets all
//! of it. This removes it from JPEG, PNG and WebP files and leaves the picture
//! alone: no pixel is decoded or re-encoded.
//!
//! # What stays
//!
//! - The orientation. Without it a photo taken with the phone on its side shows up sideways, so it
//!   is kept, on its own.
//! - Colour profiles, and everything else a decoder needs to draw the picture.
//!
//! # How
//!
//! A JPEG is changed in place: each metadata segment keeps its length and has
//! its contents overwritten. A JPEG can carry more images after the first (the
//! depth map or HDR gain map of some phones), found through offsets that
//! removing bytes would break. Whatever follows the last of those images, such
//! as the video of a motion photo, is cut off: it carries metadata of its own.
//!
//! PNG and WebP files are rebuilt without their metadata chunks, since nothing
//! in them points at a byte offset.
//!
//! HEIC is not handled yet.

mod exif;
mod jpeg;
mod png;
mod webp;

/// Why a file that looked like an image could not be cleaned.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// It starts like one of the formats but its structure does not hold
    /// together, so its metadata could not be found reliably.
    #[error("the file looks like a {0} but is not a well-formed one")]
    Malformed(&'static str),
}

/// Whether `head`, the start of a file, is a format [`strip`] handles.
///
/// Twelve bytes are enough to tell, so a caller can decide before reading the
/// whole file.
#[must_use]
pub fn handles(head: &[u8]) -> bool {
    jpeg::is(head) || png::is(head) || webp::is(head)
}

/// The image without its metadata.
///
/// `Ok(None)` when there is nothing to change: the file is not a JPEG, PNG or
/// WebP image, or it carries no metadata.
///
/// # Errors
///
/// [`Error::Malformed`] when the file starts like one of the formats but its
/// structure is broken. Its metadata may still be in it.
pub fn strip(image: &[u8]) -> Result<Option<Vec<u8>>, Error> {
    if jpeg::is(image) {
        jpeg::strip(image)
    } else if png::is(image) {
        png::strip(image)
    } else if webp::is(image) {
        webp::strip(image)
    } else {
        Ok(None)
    }
}
