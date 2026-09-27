//! The end-to-end encryption format.
//!
//! A share encrypted with this is opaque to the server: it stores and hands out
//! bytes it has no key for. The key travels in the part of a link after `#`,
//! which a browser never sends, or is derived from a password the server never
//! sees. See `docs/encryption.md` for what that does and does not hide.
//!
//! # Why this crate is shaped the way it is
//!
//! It is compiled twice: natively for the command line and to WebAssembly for
//! the browser. One implementation means the two cannot disagree about the
//! format, and a file one wrote that the other cannot read is the failure that
//! would matter most.
//!
//! So it does no IO of any kind, randomness included. Every key, nonce and salt
//! is an argument, and the caller decides where the bytes came from.
//!
//! # The format
//!
//! A sealed object is a 17-byte header followed by the content in chunks:
//!
//! ```text
//! version (1) ‖ nonce (16) ‖ chunk₀ ‖ chunk₁ ‖ … ‖ chunkₙ₋₁
//! ```
//!
//! Each chunk is up to [`CHUNK_LEN`] bytes of content sealed with
//! ChaCha20-Poly1305, so it is [`TAG_LEN`] bytes longer than what it holds. The
//! nonce in the header is fresh for every object, and the object's key is
//! derived from it, so no two objects share a key. Within an object, chunk `i`
//! is sealed under the nonce
//!
//! ```text
//! i as 11 big-endian bytes ‖ 0x01 if it is the last chunk, else 0x00
//! ```
//!
//! That is the STREAM construction, as `age` uses it. The flag is what makes a
//! cut-short object fail to open instead of opening as a shorter file: the
//! chunk that is now last was sealed as not being last.
//!
//! Every chunk but the last holds exactly [`CHUNK_LEN`] bytes. The last holds
//! at least one, unless the content is empty, in which case there is a single
//! empty chunk. So a sealed length determines the content length, and chunk
//! `i` always sits at the same offset: a `Range` request can fetch it without
//! reading what comes before.

#[cfg(feature = "wasm")]
pub mod wasm;

use argon2::{Algorithm, Argon2, Params, Version};
use base64ct::{Base64UrlUnpadded, Encoding as _};
use chacha20poly1305::{AeadInOut as _, ChaCha20Poly1305, KeyInit as _, aead::array::Array};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// The format version this crate writes, and the only one it reads.
pub const VERSION: u8 = 1;

/// How much content one chunk holds, at most.
///
/// The same as `age`. Small enough that a `Range` request for a few bytes
/// fetches little more than it asked for, and large enough that the tag on each
/// chunk costs 0.02%.
pub const CHUNK_LEN: usize = 64 * 1024;

/// How much longer a chunk is sealed than open.
pub const TAG_LEN: usize = 16;

/// How long the header in front of the chunks is.
pub const HEADER_LEN: usize = 1 + NONCE_LEN;

/// How long the nonce in the header is.
pub const NONCE_LEN: usize = 16;

/// How long a salt for deriving a key from a password is.
pub const SALT_LEN: usize = 16;

/// Argon2id memory cost for a password, in KiB.
///
/// Fixed by the format version, not chosen per share. Stored per share, the
/// parameters would be one more thing to send clients and one more thing that
/// can arrive wrong; raising them means bumping [`VERSION`].
///
/// These take 130 to 160 ms in a desktop browser.
pub const PASSWORD_MEMORY_KIB: u32 = 64 * 1024;

/// Argon2id passes over memory for a password. See [`PASSWORD_MEMORY_KIB`].
pub const PASSWORD_PASSES: u32 = 3;

/// The shortest password a client should derive a key from.
///
/// Not enforced here or by the server, which never sees the password; it is
/// the threshold clients agree to refuse below. Higher than the account
/// minimum because the threat differs: anyone holding the stored share can
/// guess offline, at their own pace, with no throttling.
pub const MIN_PASSWORD_CHARS: usize = 12;

const KEY_LEN: usize = 32;
const SEALED_CHUNK_LEN: usize = CHUNK_LEN + TAG_LEN;
const PAYLOAD_INFO: &[u8] = b"tessera v1 payload";
const ACCESS_INFO: &[u8] = b"tessera v1 access";

/// Why something could not be sealed or opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A key did not decode, or decoded to the wrong length.
    #[error("that is not a share key")]
    Key,
    /// A salt did not decode, or decoded to the wrong length.
    #[error("that is not a salt")]
    Salt,
    /// The header names a version this build does not read.
    #[error("sealed with a version this build cannot open")]
    Version,
    /// A length that no sealed object can have.
    #[error("that length does not describe a sealed object")]
    Length,
    /// A chunk of the wrong size for its position, or one past the end.
    #[error("a chunk is the wrong size for where it is")]
    Chunk,
    /// The content was altered, or the key is not the one it was sealed with.
    ///
    /// One variant for both: telling them apart is what a forger wants to
    /// learn.
    #[error("the content was altered, or the key is wrong")]
    Unauthentic,
    /// The password could not be hashed. Only a password longer than four
    /// gigabytes gets here.
    #[error("the password could not be used")]
    Password,
}

/// The secret a share is sealed under.
///
/// Either random, and carried in the link after `#`, or derived from a
/// password. Every object in the share is sealed under a key derived from this
/// one and the object's own nonce.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct ShareKey([u8; KEY_LEN]);

impl ShareKey {
    /// A key from 32 bytes the caller drew from a secure random source.
    #[must_use]
    pub const fn from_random(bytes: [u8; KEY_LEN]) -> Self {
        Self(bytes)
    }

    /// The key a password and salt derive.
    ///
    /// The password is taken as the bytes of its UTF-8 encoding, unnormalised,
    /// the same as an account password is.
    ///
    /// # Errors
    ///
    /// [`Error::Password`] if the password is too long to hash.
    pub fn from_password(password: &str, salt: &Salt) -> Result<Self, Error> {
        let params = Params::new(PASSWORD_MEMORY_KIB, PASSWORD_PASSES, 1, Some(KEY_LEN))
            .map_err(|_| Error::Password)?;
        let mut key = [0; KEY_LEN];
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(password.as_bytes(), &salt.0, &mut key)
            .map_err(|_| Error::Password)?;
        Ok(Self(key))
    }

    /// Reads a key as it appears in a link.
    ///
    /// # Errors
    ///
    /// [`Error::Key`] if it is not 32 bytes of unpadded URL-safe base64.
    pub fn parse(encoded: &str) -> Result<Self, Error> {
        let mut key = [0; KEY_LEN];
        match Base64UrlUnpadded::decode(encoded, &mut key) {
            Ok(decoded) if decoded.len() == KEY_LEN => Ok(Self(key)),
            _ => Err(Error::Key),
        }
    }

    /// The key as it appears in a link: 43 characters of unpadded URL-safe
    /// base64, safe after `#` without escaping.
    #[must_use]
    pub fn encode(&self) -> String {
        Base64UrlUnpadded::encode_string(&self.0)
    }

    /// What a client presents to the server as the share's password, for a
    /// share whose key came from one.
    ///
    /// Derived from the key, not the password, so the server can reject wrong
    /// guesses and count retrievals as for any protected share, while holding
    /// nothing that opens the content. Recovering the key from this means
    /// inverting HKDF.
    #[must_use]
    pub fn access_token(&self) -> String {
        let mut token = [0; KEY_LEN];
        derive(None, &self.0, ACCESS_INFO, &mut token);
        let encoded = Base64UrlUnpadded::encode_string(&token);
        token.zeroize();
        encoded
    }

    fn cipher(&self, nonce: &[u8; NONCE_LEN]) -> ChaCha20Poly1305 {
        let mut key = [0; KEY_LEN];
        derive(Some(nonce), &self.0, PAYLOAD_INFO, &mut key);
        let cipher = ChaCha20Poly1305::new(&Array::from(key));
        key.zeroize();
        cipher
    }
}

/// Prints nothing of the key, so it cannot end up in a log.
impl std::fmt::Debug for ShareKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ShareKey(<redacted>)")
    }
}

fn derive(salt: Option<&[u8]>, key: &[u8], info: &[u8], out: &mut [u8; KEY_LEN]) {
    // `expand` fails only when asked for more than 255 hash lengths of output,
    // and this asks for one.
    let _ = Hkdf::<Sha256>::new(salt, key).expand(info, out);
}

/// The salt a password is stretched with. Not secret: it is stored with the
/// share and handed to anyone who asks, since deriving the key needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Salt([u8; SALT_LEN]);

impl Salt {
    /// A salt from 16 bytes the caller drew from a secure random source.
    #[must_use]
    pub const fn from_random(bytes: [u8; SALT_LEN]) -> Self {
        Self(bytes)
    }

    /// Reads a salt as the server stores it.
    ///
    /// # Errors
    ///
    /// [`Error::Salt`] if it is not 16 bytes of unpadded URL-safe base64.
    pub fn parse(encoded: &str) -> Result<Self, Error> {
        let mut salt = [0; SALT_LEN];
        match Base64UrlUnpadded::decode(encoded, &mut salt) {
            Ok(decoded) if decoded.len() == SALT_LEN => Ok(Self(salt)),
            _ => Err(Error::Salt),
        }
    }

    /// The salt as 22 characters of unpadded URL-safe base64.
    #[must_use]
    pub fn encode(&self) -> String {
        Base64UrlUnpadded::encode_string(&self.0)
    }
}

/// How long `plain_len` bytes of content are once sealed, or `None` past what a
/// `u64` can say.
#[must_use]
pub const fn sealed_len(plain_len: u64) -> Option<u64> {
    let chunks = chunk_count(plain_len);
    match chunks.checked_mul(to_u64(TAG_LEN)) {
        Some(tags) => match plain_len.checked_add(tags) {
            Some(sum) => sum.checked_add(to_u64(HEADER_LEN)),
            None => None,
        },
        None => None,
    }
}

/// How long the content in a sealed object of `sealed_len` bytes is.
///
/// # Errors
///
/// [`Error::Length`] if no content seals to that length.
pub const fn plain_len(sealed_len: u64) -> Result<u64, Error> {
    let Some(body) = sealed_len.checked_sub(to_u64(HEADER_LEN)) else {
        return Err(Error::Length);
    };
    let sealed_chunk = to_u64(SEALED_CHUNK_LEN);
    let chunks = body.div_ceil(sealed_chunk);
    // Every length has a last chunk, and it has to hold its tag.
    let chunks = if chunks == 0 { 1 } else { chunks };
    let Some(plain) = body.checked_sub(chunks * to_u64(TAG_LEN)) else {
        return Err(Error::Length);
    };
    // A last chunk holding nothing is only valid as the only chunk: otherwise
    // the content ended on a boundary and there is no empty chunk after it.
    let last = plain - (chunks - 1) * to_u64(CHUNK_LEN);
    if last == 0 && chunks > 1 {
        return Err(Error::Length);
    }
    Ok(plain)
}

const fn chunk_count(plain_len: u64) -> u64 {
    match plain_len.div_ceil(to_u64(CHUNK_LEN)) {
        0 => 1,
        chunks => chunks,
    }
}

const fn to_u64(value: usize) -> u64 {
    // Every constant this is used on is far below `u32::MAX`, and no target
    // this builds for has a `usize` wider than 64 bits.
    #[expect(
        clippy::as_conversions,
        reason = "const context; `u64::try_from` is not const"
    )]
    let widened = value as u64;
    widened
}

fn chunk_nonce(index: u64, last: bool) -> Array<u8, chacha20poly1305::consts::U12> {
    let mut nonce = [0; 12];
    // Eight bytes of index at the low end of an eleven-byte counter, then the
    // flag.
    for (slot, byte) in nonce.iter_mut().skip(3).zip(index.to_be_bytes()) {
        *slot = byte;
    }
    if let Some(flag) = nonce.last_mut() {
        *flag = u8::from(last);
    }
    Array(nonce)
}

/// Seals content one chunk at a time, in order.
///
/// For content too large to hold at once. A file is read [`CHUNK_LEN`] bytes
/// at a time and each piece handed to [`seal`](Self::seal) as it is read.
pub struct Sealer {
    cipher: ChaCha20Poly1305,
    next: u64,
    finished: bool,
}

impl Sealer {
    /// Starts sealing an object, and returns the header to write in front of
    /// it.
    ///
    /// `nonce` must come from a secure random source and never be used twice
    /// with the same key.
    #[must_use]
    pub fn new(key: &ShareKey, nonce: [u8; NONCE_LEN]) -> (Self, [u8; HEADER_LEN]) {
        let sealer = Self {
            cipher: key.cipher(&nonce),
            next: 0,
            finished: false,
        };
        let mut header = [0; HEADER_LEN];
        if let Some((version, rest)) = header.split_first_mut() {
            *version = VERSION;
            rest.copy_from_slice(&nonce);
        }
        (sealer, header)
    }

    /// Seals the next chunk.
    ///
    /// Every chunk but the last must hold exactly [`CHUNK_LEN`] bytes, and the
    /// last at least one unless it is also the first.
    ///
    /// # Errors
    ///
    /// [`Error::Chunk`] for a chunk of the wrong size, or any chunk after the
    /// last.
    pub fn seal(&mut self, chunk: &[u8], last: bool) -> Result<Vec<u8>, Error> {
        let fits = if last {
            chunk.len() <= CHUNK_LEN && (!chunk.is_empty() || self.next == 0)
        } else {
            chunk.len() == CHUNK_LEN
        };
        if self.finished || !fits {
            return Err(Error::Chunk);
        }
        let mut sealed = Vec::with_capacity(chunk.len() + TAG_LEN);
        sealed.extend_from_slice(chunk);
        self.cipher
            .encrypt_in_place(&chunk_nonce(self.next, last), &[], &mut sealed)
            .map_err(|_| Error::Chunk)?;
        self.next += 1;
        self.finished = last;
        Ok(sealed)
    }
}

/// Printed without its state, which is a key.
impl std::fmt::Debug for Sealer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sealer")
            .field("next", &self.next)
            .field("finished", &self.finished)
            .finish_non_exhaustive()
    }
}

/// Opens the chunks of one sealed object, in any order.
///
/// Opening chunk `i` alone works because the object's full length is known: it
/// says which chunk is last, and so which flag each chunk was sealed with.
pub struct Opener {
    cipher: ChaCha20Poly1305,
    chunks: u64,
    last_len: u64,
}

impl Opener {
    /// Prepares to open an object of `sealed_len` bytes that starts with
    /// `header`.
    ///
    /// # Errors
    ///
    /// [`Error::Version`] for a header this build does not read, [`Error::Length`]
    /// for a header of the wrong size or a length no object can have.
    pub fn new(key: &ShareKey, header: &[u8], sealed_len: u64) -> Result<Self, Error> {
        let Some((&version, nonce)) = header.split_first() else {
            return Err(Error::Length);
        };
        if version != VERSION {
            return Err(Error::Version);
        }
        let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| Error::Length)?;
        let plain = plain_len(sealed_len)?;
        let chunks = chunk_count(plain);
        Ok(Self {
            cipher: key.cipher(&nonce),
            chunks,
            last_len: plain - (chunks - 1) * to_u64(CHUNK_LEN),
        })
    }

    /// How many chunks the object has.
    #[must_use]
    pub const fn chunks(&self) -> u64 {
        self.chunks
    }

    /// Where chunk `index` sits in the sealed object, as a start offset and a
    /// length, for a `Range` request.
    ///
    /// # Errors
    ///
    /// [`Error::Chunk`] past the last chunk.
    pub const fn locate(&self, index: u64) -> Result<(u64, u64), Error> {
        if index >= self.chunks {
            return Err(Error::Chunk);
        }
        let start = to_u64(HEADER_LEN) + index * to_u64(SEALED_CHUNK_LEN);
        let len = if index + 1 == self.chunks {
            self.last_len + to_u64(TAG_LEN)
        } else {
            to_u64(SEALED_CHUNK_LEN)
        };
        Ok((start, len))
    }

    /// Opens chunk `index`.
    ///
    /// # Errors
    ///
    /// [`Error::Chunk`] when the chunk is the wrong size for its position, and
    /// [`Error::Unauthentic`] when it does not open.
    pub fn open(&self, index: u64, sealed: &[u8]) -> Result<Vec<u8>, Error> {
        let (_, expected) = self.locate(index)?;
        if u64::try_from(sealed.len()).ok() != Some(expected) {
            return Err(Error::Chunk);
        }
        let mut open = sealed.to_vec();
        let last = index + 1 == self.chunks;
        if self
            .cipher
            .decrypt_in_place(&chunk_nonce(index, last), &[], &mut open)
            .is_err()
        {
            open.zeroize();
            return Err(Error::Unauthentic);
        }
        Ok(open)
    }
}

/// Printed without its state, which is a key.
impl std::fmt::Debug for Opener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Opener")
            .field("chunks", &self.chunks)
            .finish_non_exhaustive()
    }
}

/// Seals something small enough to hold at once: a name, a title, a note.
#[must_use]
pub fn seal(key: &ShareKey, nonce: [u8; NONCE_LEN], content: &[u8]) -> Vec<u8> {
    let (mut sealer, header) = Sealer::new(key, nonce);
    let mut object = header.to_vec();
    let mut pieces = content.chunks(CHUNK_LEN).peekable();
    if pieces.peek().is_none() {
        // Sealing an empty first-and-last chunk cannot fail.
        object.extend(sealer.seal(&[], true).unwrap_or_default());
    }
    while let Some(piece) = pieces.next() {
        // Every piece is `CHUNK_LEN` but the last, which is what `seal` wants.
        object.extend(
            sealer
                .seal(piece, pieces.peek().is_none())
                .unwrap_or_default(),
        );
    }
    object
}

/// Opens something [`seal`] sealed.
///
/// # Errors
///
/// Whatever [`Opener::new`] or [`Opener::open`] return.
pub fn open(key: &ShareKey, sealed: &[u8]) -> Result<Vec<u8>, Error> {
    let length = u64::try_from(sealed.len()).map_err(|_| Error::Length)?;
    let (header, body) = sealed.split_at_checked(HEADER_LEN).ok_or(Error::Length)?;
    let opener = Opener::new(key, header, length)?;
    let mut content = Vec::new();
    for (index, chunk) in (0..).zip(body.chunks(SEALED_CHUNK_LEN)) {
        content.extend(opener.open(index, chunk)?);
    }
    Ok(content)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rstest::rstest;

    use super::*;

    const KEY: [u8; 32] = [7; 32];
    const NONCE: [u8; NONCE_LEN] = [9; NONCE_LEN];

    fn key() -> ShareKey {
        ShareKey::from_random(KEY)
    }

    fn content(len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| u8::try_from(i % 251).unwrap_or(0))
            .collect()
    }

    #[rstest]
    #[case::empty(0)]
    #[case::one_byte(1)]
    #[case::just_under_a_chunk(CHUNK_LEN - 1)]
    #[case::exactly_a_chunk(CHUNK_LEN)]
    #[case::just_over_a_chunk(CHUNK_LEN + 1)]
    #[case::three_chunks_exactly(3 * CHUNK_LEN)]
    #[case::three_chunks_and_a_bit(3 * CHUNK_LEN + 5)]
    fn content_comes_back_as_it_went_in(#[case] len: usize) {
        let plain = content(len);
        let sealed = seal(&key(), NONCE, &plain);
        assert_eq!(open(&key(), &sealed).unwrap(), plain);
        assert_eq!(
            sealed_len(u64::try_from(len).unwrap()),
            Some(u64::try_from(sealed.len()).unwrap())
        );
        assert_eq!(
            plain_len(u64::try_from(sealed.len()).unwrap()),
            Ok(u64::try_from(len).unwrap())
        );
    }

    /// A single changed bit anywhere, header included, and nothing opens.
    #[test]
    fn a_changed_byte_does_not_open() {
        let sealed = seal(&key(), NONCE, &content(2 * CHUNK_LEN + 3));
        for position in [1, HEADER_LEN, HEADER_LEN + CHUNK_LEN, sealed.len() - 1] {
            let mut altered = sealed.clone();
            if let Some(byte) = altered.get_mut(position) {
                *byte ^= 1;
            }
            assert_eq!(
                open(&key(), &altered),
                Err(Error::Unauthentic),
                "at {position}"
            );
        }
    }

    /// The reason the last chunk carries a flag. Dropping whole chunks from the
    /// end leaves an object of a valid length whose final chunk says it is not
    /// final.
    #[test]
    fn an_object_cut_short_on_a_chunk_boundary_does_not_open() {
        let sealed = seal(&key(), NONCE, &content(3 * CHUNK_LEN));
        let cut = sealed.get(..HEADER_LEN + 2 * SEALED_CHUNK_LEN).unwrap();
        assert_eq!(open(&key(), cut), Err(Error::Unauthentic));
    }

    #[test]
    fn chunks_swapped_do_not_open() {
        let sealed = seal(&key(), NONCE, &content(2 * CHUNK_LEN + 1));
        let (header, body) = sealed.split_at(HEADER_LEN);
        let (first, rest) = body.split_at(SEALED_CHUNK_LEN);
        let (second, last) = rest.split_at(SEALED_CHUNK_LEN);
        let swapped = [header, second, first, last].concat();
        assert_eq!(open(&key(), &swapped), Err(Error::Unauthentic));
    }

    #[test]
    fn the_wrong_key_does_not_open() {
        let sealed = seal(&key(), NONCE, b"meant for someone else");
        let other = ShareKey::from_random([8; 32]);
        assert_eq!(open(&other, &sealed), Err(Error::Unauthentic));
    }

    /// What a `Range` request relies on: any chunk, fetched alone, opens to the
    /// same bytes a front-to-back read would give.
    #[test]
    fn any_chunk_opens_alone() {
        let plain = content(3 * CHUNK_LEN + 17);
        let sealed = seal(&key(), NONCE, &plain);
        let opener = Opener::new(
            &key(),
            sealed.get(..HEADER_LEN).unwrap(),
            u64::try_from(sealed.len()).unwrap(),
        )
        .unwrap();
        assert_eq!(opener.chunks(), 4);
        for index in [3, 0, 2, 1] {
            let (start, len) = opener.locate(index).unwrap();
            let start = usize::try_from(start).unwrap();
            let chunk = sealed
                .get(start..start + usize::try_from(len).unwrap())
                .unwrap();
            let offset = usize::try_from(index).unwrap() * CHUNK_LEN;
            let expected = plain
                .get(offset..(offset + CHUNK_LEN).min(plain.len()))
                .unwrap();
            assert_eq!(opener.open(index, chunk).unwrap(), expected);
        }
        assert_eq!(opener.locate(4), Err(Error::Chunk));
    }

    #[test]
    fn a_sealer_refuses_chunks_that_break_the_layout() {
        let (mut sealer, _) = Sealer::new(&key(), NONCE);
        assert_eq!(sealer.seal(&content(10), false), Err(Error::Chunk));
        sealer.seal(&content(CHUNK_LEN), false).unwrap();
        // Empty is only allowed as the first and last chunk at once.
        assert_eq!(sealer.seal(&[], true), Err(Error::Chunk));
        sealer.seal(&content(1), true).unwrap();
        assert_eq!(sealer.seal(&content(1), true), Err(Error::Chunk));
    }

    #[test]
    fn a_header_from_another_version_is_refused() {
        let mut sealed = seal(&key(), NONCE, b"from the future");
        if let Some(version) = sealed.first_mut() {
            *version = 2;
        }
        assert_eq!(open(&key(), &sealed), Err(Error::Version));
    }

    #[rstest]
    #[case::shorter_than_a_header(3)]
    #[case::header_and_no_tag(u64::try_from(HEADER_LEN + TAG_LEN).unwrap() - 1)]
    #[case::an_empty_chunk_after_a_full_one(
        u64::try_from(HEADER_LEN + SEALED_CHUNK_LEN + TAG_LEN).unwrap()
    )]
    fn lengths_no_object_can_have_are_refused(#[case] length: u64) {
        assert_eq!(plain_len(length), Err(Error::Length));
    }

    #[test]
    fn a_key_survives_a_link() {
        let encoded = key().encode();
        assert_eq!(encoded.len(), 43);
        assert!(
            encoded
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
        let sealed = seal(&key(), NONCE, b"round trip");
        assert_eq!(
            open(&ShareKey::parse(&encoded).unwrap(), &sealed).unwrap(),
            b"round trip"
        );
    }

    #[rstest]
    #[case::too_short("AAAA")]
    #[case::padded("BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=")]
    #[case::not_base64("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!")]
    fn a_malformed_key_is_refused(#[case] encoded: &str) {
        assert_eq!(ShareKey::parse(encoded).unwrap_err(), Error::Key);
    }

    /// The server is handed the token; it must not be able to open anything
    /// with it.
    #[test]
    fn the_access_token_is_not_the_key() {
        let token = key().access_token();
        assert_ne!(token, key().encode());
        let sealed = seal(&key(), NONCE, b"not for the server");
        assert_eq!(
            open(&ShareKey::parse(&token).unwrap(), &sealed),
            Err(Error::Unauthentic)
        );
    }

    #[test]
    fn debug_output_never_contains_the_key() {
        let rendered = format!("{:?}", key());
        assert!(!rendered.contains(&key().encode()));
        assert!(!rendered.contains('7'), "leaked: {rendered}");
    }

    /// Pinned output. The browser and the command line can read each other's
    /// files only while this holds, so a change here means a new format
    /// version, not an updated test.
    ///
    /// The same values come out of Python's `cryptography` (HKDF and
    /// ChaCha20-Poly1305) and the Argon2 reference implementation via
    /// `argon2-cffi`, given these inputs.
    #[test]
    fn the_format_has_not_moved() {
        let key = ShareKey::from_random(std::array::from_fn(|i| u8::try_from(i).unwrap()));
        let nonce = std::array::from_fn(|i| u8::try_from(0xa0 + i).unwrap());
        let sealed = seal(&key, nonce, b"tessera");
        assert_eq!(
            hex(&sealed),
            "01a0a1a2a3a4a5a6a7a8a9aaabacadaeaf\
             ff159ab0bc53fd3de9a5f988a30c7dc89876256fac4ab9"
        );
        assert_eq!(
            key.access_token(),
            "DB3xOr4CmNWw6gamlTsJanup3mgZq1ZD17GowkvifhE"
        );
    }

    /// Slow in a debug build, so there is one of it: the parameters and the
    /// derivation are pinned together.
    #[test]
    fn a_password_derives_the_same_key_every_time() {
        let salt = Salt::from_random([3; SALT_LEN]);
        let key = ShareKey::from_password("correct horse battery staple", &salt).unwrap();
        assert_eq!(key.encode(), "c-t0FiYWQY1kPwjcCFZTnqYUAMsmj4XOjfAdgld5W40");
        assert_eq!(salt.encode().len(), 22);
        assert_eq!(Salt::parse(&salt.encode()), Ok(salt));
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().fold(String::new(), |mut out, byte| {
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{byte:02x}"));
            out
        })
    }

    proptest! {
        /// Lengths map both ways for anything a file could be.
        #[test]
        fn lengths_round_trip(plain in 0_u64..(1 << 50)) {
            let sealed = sealed_len(plain).unwrap();
            prop_assert_eq!(plain_len(sealed), Ok(plain));
        }
    }

    proptest! {
        // Each case seals up to two chunks through a debug build, so a handful
        // of lengths, not the default 256.
        #![proptest_config(ProptestConfig::with_cases(24))]

        #[test]
        fn arbitrary_content_round_trips(plain in proptest::collection::vec(any::<u8>(), 0..(2 * CHUNK_LEN + 10))) {
            let sealed = seal(&key(), NONCE, &plain);
            prop_assert_eq!(open(&key(), &sealed).unwrap(), plain);
        }
    }
}
