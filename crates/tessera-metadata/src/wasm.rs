//! [`strip`](crate::strip), as the browser calls it.
//!
//! A thin layer, like the one in `tessera-crypto`: the rules live in the crate
//! root and are tested there. This only converts types. The JavaScript that
//! decides which files to hand it lives in `packages/metadata`.

use wasm_bindgen::prelude::*;

/// Whether `head`, the start of a file, is a format [`strip`] handles.
#[wasm_bindgen]
#[must_use]
pub fn handles(head: &[u8]) -> bool {
    crate::handles(head)
}

/// The image without its metadata, or `undefined` when there is nothing to
/// change.
///
/// # Errors
///
/// When the file starts like a JPEG, PNG or WebP but is not a well-formed one.
#[wasm_bindgen]
pub fn strip(image: &[u8]) -> Result<Option<Vec<u8>>, JsError> {
    crate::strip(image).map_err(|error| JsError::new(&error.to_string()))
}
