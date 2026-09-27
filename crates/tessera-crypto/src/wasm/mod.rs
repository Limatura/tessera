//! The format, as the browser calls it.
//!
//! A thin layer. Every rule lives in the crate root and is tested there; this
//! only converts types: bytes to slices, lengths to `BigInt`, errors to
//! exceptions. The JavaScript that streams a file through these lives in
//! `packages/crypto`.

use wasm_bindgen::prelude::*;

use crate::Error;

fn thrown(error: Error) -> JsError {
    JsError::new(&error.to_string())
}

fn exactly<const N: usize>(bytes: &[u8], what: &str) -> Result<[u8; N], JsError> {
    bytes
        .try_into()
        .map_err(|_| JsError::new(&format!("{what} must be {N} bytes")))
}

/// The secret a share is sealed under. See [`crate::ShareKey`].
#[wasm_bindgen]
#[derive(Debug)]
pub struct ShareKey(crate::ShareKey);

#[wasm_bindgen]
impl ShareKey {
    /// A key from 32 bytes of `crypto.getRandomValues`.
    ///
    /// # Errors
    ///
    /// When there are not exactly 32.
    #[wasm_bindgen(js_name = fromRandom)]
    pub fn from_random(bytes: &[u8]) -> Result<Self, JsError> {
        Ok(Self(crate::ShareKey::from_random(exactly(bytes, "a key")?)))
    }

    /// The key a password and an encoded salt derive. Takes a noticeable
    /// fraction of a second, as intended.
    ///
    /// # Errors
    ///
    /// When the salt does not decode.
    #[wasm_bindgen(js_name = fromPassword)]
    pub fn from_password(password: &str, salt: &str) -> Result<Self, JsError> {
        let salt = crate::Salt::parse(salt).map_err(thrown)?;
        crate::ShareKey::from_password(password, &salt)
            .map(Self)
            .map_err(thrown)
    }

    /// Reads a key as it appears after `#` in a link.
    ///
    /// # Errors
    ///
    /// When it is not one.
    pub fn parse(encoded: &str) -> Result<Self, JsError> {
        crate::ShareKey::parse(encoded).map(Self).map_err(thrown)
    }

    /// The key as it goes after `#` in a link.
    #[must_use]
    pub fn encode(&self) -> String {
        self.0.encode()
    }

    /// What to present as the share's password, when the key came from one.
    #[wasm_bindgen(js_name = accessToken)]
    #[must_use]
    pub fn access_token(&self) -> String {
        self.0.access_token()
    }
}

/// Encodes 16 bytes of `crypto.getRandomValues` as a salt.
///
/// # Errors
///
/// When there are not exactly 16.
#[wasm_bindgen(js_name = encodeSalt)]
pub fn encode_salt(bytes: &[u8]) -> Result<String, JsError> {
    Ok(crate::Salt::from_random(exactly(bytes, "a salt")?).encode())
}

/// Seals content one chunk at a time. See [`crate::Sealer`].
#[wasm_bindgen]
#[derive(Debug)]
pub struct Sealer {
    inner: crate::Sealer,
    header: Vec<u8>,
}

#[wasm_bindgen]
impl Sealer {
    /// Starts an object with 16 bytes of `crypto.getRandomValues` as its nonce.
    ///
    /// # Errors
    ///
    /// When there are not exactly 16.
    #[wasm_bindgen(constructor)]
    pub fn new(key: &ShareKey, nonce: &[u8]) -> Result<Self, JsError> {
        let (inner, header) = crate::Sealer::new(&key.0, exactly(nonce, "a nonce")?);
        Ok(Self {
            inner,
            header: header.to_vec(),
        })
    }

    /// What goes in front of the first chunk.
    #[must_use]
    pub fn header(&self) -> Vec<u8> {
        self.header.clone()
    }

    /// Seals the next chunk.
    ///
    /// # Errors
    ///
    /// As [`crate::Sealer::seal`].
    pub fn seal(&mut self, chunk: &[u8], last: bool) -> Result<Vec<u8>, JsError> {
        self.inner.seal(chunk, last).map_err(thrown)
    }
}

/// Opens the chunks of one object, in any order. See [`crate::Opener`].
#[wasm_bindgen]
#[derive(Debug)]
pub struct Opener(crate::Opener);

#[wasm_bindgen]
impl Opener {
    /// Prepares to open an object of `sealed_len` bytes starting with `header`.
    ///
    /// # Errors
    ///
    /// As [`crate::Opener::new`].
    #[wasm_bindgen(constructor)]
    pub fn new(key: &ShareKey, header: &[u8], sealed_len: u64) -> Result<Self, JsError> {
        crate::Opener::new(&key.0, header, sealed_len)
            .map(Self)
            .map_err(thrown)
    }

    /// How many chunks the object has.
    #[must_use]
    #[expect(
        clippy::missing_const_for_fn,
        reason = "`#[wasm_bindgen]` cannot export a `const fn`"
    )]
    pub fn chunks(&self) -> u64 {
        self.0.chunks()
    }

    /// Where chunk `index` starts in the sealed object.
    ///
    /// # Errors
    ///
    /// Past the last chunk.
    pub fn start(&self, index: u64) -> Result<u64, JsError> {
        self.0.locate(index).map(|(start, _)| start).map_err(thrown)
    }

    /// How many sealed bytes chunk `index` is.
    ///
    /// # Errors
    ///
    /// Past the last chunk.
    pub fn length(&self, index: u64) -> Result<u64, JsError> {
        self.0.locate(index).map(|(_, len)| len).map_err(thrown)
    }

    /// Opens chunk `index`.
    ///
    /// # Errors
    ///
    /// As [`crate::Opener::open`].
    pub fn open(&self, index: u64, chunk: &[u8]) -> Result<Vec<u8>, JsError> {
        self.0.open(index, chunk).map_err(thrown)
    }
}

/// Seals something small at once. See [`crate::seal`].
///
/// # Errors
///
/// When the nonce is not 16 bytes.
#[wasm_bindgen]
pub fn seal(key: &ShareKey, nonce: &[u8], content: &[u8]) -> Result<Vec<u8>, JsError> {
    Ok(crate::seal(&key.0, exactly(nonce, "a nonce")?, content))
}

/// Opens what [`seal`] sealed.
///
/// # Errors
///
/// When it does not open.
#[wasm_bindgen]
pub fn open(key: &ShareKey, sealed: &[u8]) -> Result<Vec<u8>, JsError> {
    crate::open(&key.0, sealed).map_err(thrown)
}

/// How long `plain_len` bytes are once sealed.
///
/// # Errors
///
/// Past what a `u64` can say.
#[wasm_bindgen(js_name = sealedLength)]
pub fn sealed_length(plain_len: u64) -> Result<u64, JsError> {
    crate::sealed_len(plain_len).ok_or_else(|| thrown(Error::Length))
}

/// How long the content in `sealed_len` sealed bytes is.
///
/// # Errors
///
/// For a length no object can have.
#[wasm_bindgen(js_name = plainLength)]
pub fn plain_length(sealed_len: u64) -> Result<u64, JsError> {
    crate::plain_len(sealed_len).map_err(thrown)
}

/// The shortest password a key may come from. See
/// [`crate::MIN_PASSWORD_CHARS`].
#[wasm_bindgen(js_name = minimumPasswordLength)]
#[must_use]
#[expect(
    clippy::missing_const_for_fn,
    reason = "`#[wasm_bindgen]` cannot export a `const fn`"
)]
pub fn minimum_password_length() -> usize {
    crate::MIN_PASSWORD_CHARS
}

/// How much content one chunk holds, for a caller cutting a file up.
#[wasm_bindgen(js_name = chunkLength)]
#[must_use]
#[expect(
    clippy::missing_const_for_fn,
    reason = "`#[wasm_bindgen]` cannot export a `const fn`"
)]
pub fn chunk_length() -> usize {
    crate::CHUNK_LEN
}
