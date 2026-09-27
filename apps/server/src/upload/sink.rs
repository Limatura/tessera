//! Where the bytes of an unfinished upload go.
//!
//! # Why there are two of these
//!
//! `object_store` has two ways to build one object from several requests, and
//! only one of them works for us on every backend.
//!
//! [`MultipartStore`] hands out an identifier that outlives the call, so the
//! parts of one object can be written by separate HTTP requests, all a
//! resumable upload needs. `aws`, `azure` and `gcp` implement it.
//! **`LocalFileSystem` does not**, and there is nothing to work around: a local
//! store has no multipart concept to expose.
//!
//! So local disk gets its own path, the simpler of the two. A file opened for
//! appending *is* a resumable upload and its length *is* the offset: no parts
//! to remember, no minimum size, nothing to abort but an `unlink`.
//!
//! # The minimum part size, and the remainder
//!
//! S3 refuses every part but the last below 5 MiB. A client that sends 2 MiB
//! and pauses leaves us holding bytes that cannot become a part yet.
//!
//! tusd parks those in a sibling `.part` object and prepends them on resume.
//! This does not: only whole parts are written, and the offset reported back is
//! the end of the last whole part. The client re-sends the tail, which the
//! protocol already expects, since the offset a server reports is what it
//! accepted. At most one part is re-sent, and only after an interruption.
//!
//! (`REMAINDER_SUFFIX` below is `.part` too, and that is confusing enough to
//! say out loud: it names the file the *local-disk* sink appends to, a
//! different mechanism with the same conventional extension.)
//!
//! Garage enforces neither the floor nor equal sizes, measured against
//! `dxflrs/garage:v2.3.0`, so the CI job cannot catch a mistake here. The
//! arithmetic is pinned by the unit tests below instead of by the run against a
//! real store.

use std::{path::PathBuf, sync::Arc};

use bytes::Bytes;
use object_store::{
    MultipartId, ObjectStore,
    multipart::{MultipartStore, PartId},
    path::Path as ObjectPath,
};

use crate::error::{Error, Result};

/// Where a part number starts. S3 counts from one.
const FIRST_PART: i32 = 1;

/// Suffix of the object holding bytes too few to be a part.
pub(crate) const REMAINDER_SUFFIX: &str = ".part";

/// One part the store has taken, as it needs handing back at completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TakenPart {
    /// Its position, from one.
    pub number: i32,
    /// The store's identifier for it, an `ETag` on S3.
    pub content_id: String,
}

/// How an upload's bytes reach the store.
#[derive(Clone)]
pub enum Sink {
    /// A store that can hold a multipart upload open across requests.
    Multipart {
        /// The store, which must also implement [`MultipartStore`].
        store: Arc<dyn MultipartStore>,
        /// Its identifier for this upload.
        id: MultipartId,
    },
    /// A local filesystem, which appends to a partial file instead.
    Appending {
        /// Where the partial file lives.
        path: PathBuf,
    },
}

/// What a chunk of bytes did to the upload.
#[derive(Debug, Default)]
pub struct Written {
    /// Parts the store took, in order, to be recorded.
    pub parts: Vec<TakenPart>,
    /// Bytes accepted: all of them, or the call failed.
    pub accepted: u64,
}

// `dyn MultipartStore` is not `Debug`, and a store in a log line would be its
// configuration, not anything about this upload.
impl core::fmt::Debug for Sink {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Multipart { id, .. } => {
                formatter.debug_struct("Multipart").field("id", id).finish()
            }
            Self::Appending { path } => formatter
                .debug_struct("Appending")
                .field("path", path)
                .finish(),
        }
    }
}

impl Sink {
    /// Opens a sink for `key`, choosing by what the store can do.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Storage`] if the store refuses to begin.
    pub async fn open(
        store: &Arc<dyn ObjectStore>,
        multipart: Option<&Arc<dyn MultipartStore>>,
        local_root: Option<&PathBuf>,
        key: &str,
    ) -> Result<Self> {
        let _ = store;
        if let Some(multipart) = multipart {
            let id = multipart
                .create_multipart(&ObjectPath::from(key))
                .await
                .map_err(Error::Storage)?;
            return Ok(Self::Multipart {
                store: Arc::clone(multipart),
                id,
            });
        }
        let root = local_root.ok_or_else(|| {
            Error::bad_request("this instance's store cannot hold a resumable upload")
        })?;
        let path = root.join(format!("{key}{REMAINDER_SUFFIX}"));
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::File::create(&path).await?;
        Ok(Self::Appending { path })
    }
}

/// How many parts an upload of `declared` bytes is cut into.
///
/// A client writing parts directly to the store must agree with this server
/// on it. Every part but the last is `part_size`; the last is the remainder.
/// Zero for an empty upload, which has no parts.
#[must_use]
pub const fn part_count(declared: u64, part_size: u64) -> u64 {
    if part_size == 0 {
        return 0;
    }
    declared.div_ceil(part_size)
}

/// Splits `pending` into whole parts and what is left over.
///
/// Returned, not written, so the arithmetic can be tested without a
/// store: it is the half that Garage will not catch being wrong.
#[must_use]
pub fn split_into_parts(pending: &[u8], part_size: usize, is_final: bool) -> (Vec<Bytes>, Vec<u8>) {
    // `chunks_exact`, not arithmetic on indices. It cannot slice past
    // the end, and its `remainder` is exactly the tail that is too short.
    let mut parts: Vec<Bytes> = pending
        .chunks_exact(part_size)
        .map(Bytes::copy_from_slice)
        .collect();
    let remainder = pending.chunks_exact(part_size).remainder().to_vec();
    if is_final && !remainder.is_empty() {
        // The last part is the one allowed to be short, so on the final write
        // the remainder becomes a part instead of being parked.
        parts.push(Bytes::from(remainder));
        return (parts, Vec::new());
    }
    (parts, remainder)
}

/// The part number that follows `taken` parts.
#[must_use]
pub fn next_part_number(taken: usize) -> i32 {
    // Saturating, not wrapping. An upload with more parts than an `i32`
    // holds was refused long before this, and a wrapped part number would be
    // accepted by the database and rejected by the store.
    FIRST_PART.saturating_add(i32::try_from(taken).unwrap_or(i32::MAX))
}

impl Sink {
    /// Writes whole parts, returning what the store took.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Storage`] if a part is refused, or [`Error::Io`] if the
    /// local file cannot be extended.
    pub async fn write(
        &self,
        key: &str,
        already_taken: usize,
        at: u64,
        parts: Vec<Bytes>,
    ) -> Result<Written> {
        let mut written = Written::default();
        match self {
            Self::Multipart { store, id } => {
                for (offset, bytes) in parts.into_iter().enumerate() {
                    let number = next_part_number(already_taken + offset);
                    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
                    let PartId { content_id } = store
                        .put_part(
                            &ObjectPath::from(key),
                            id,
                            usize::try_from(number - FIRST_PART).unwrap_or(0),
                            bytes.into(),
                        )
                        .await
                        .map_err(Error::Storage)?;
                    written.parts.push(TakenPart { number, content_id });
                    written.accepted = written.accepted.saturating_add(length);
                }
            }
            Self::Appending { path } => {
                use tokio::io::{AsyncSeekExt as _, AsyncWriteExt as _};
                let file = tokio::fs::OpenOptions::new().write(true).open(path).await?;
                // Cut back to what is recorded before adding to it. Two
                // requests can both append before either records its offset,
                // and the loser has already put its bytes in the file, so
                // without this the next legitimate write lands after them.
                // Truncating makes the write idempotent about where it starts.
                file.set_len(at).await?;
                let mut file = file;
                file.seek(std::io::SeekFrom::End(0)).await?;
                for bytes in parts {
                    written.accepted = written
                        .accepted
                        .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
                    file.write_all(&bytes).await?;
                }
                file.flush().await?;
            }
        }
        Ok(written)
    }
}

impl Sink {
    /// Whether bytes have to accumulate before any of them can be written.
    ///
    /// Only a multipart store does. The block size exists because S3 refuses
    /// every part but the last below 5 MiB. A file on disk takes a byte at a
    /// time, so buffering for it is pure loss: a local upload sending one
    /// megabyte at a time would never reach a block boundary, never advance its
    /// offset, and re-send the same megabyte for ever.
    #[must_use]
    pub const fn buffers(&self) -> bool {
        matches!(self, Self::Multipart { .. })
    }

    /// Seals the object, so it becomes readable at `key`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Storage`] if the store refuses to complete, or
    /// [`Error::Io`] if the finished file cannot be moved into place.
    pub async fn complete(&self, key: &str, parts: Vec<TakenPart>) -> Result<()> {
        match self {
            Self::Multipart { store, id } => {
                let parts = parts
                    .into_iter()
                    .map(|part| PartId {
                        content_id: part.content_id,
                    })
                    .collect();
                store
                    .complete_multipart(&ObjectPath::from(key), id, parts)
                    .await
                    .map_err(Error::Storage)?;
            }
            Self::Appending { path } => {
                // The partial file becomes the object by losing its suffix. A
                // rename within one directory is atomic, so a reader either
                // finds the finished object or finds nothing, never half.
                let finished = path.with_extension("");
                tokio::fs::rename(path, &finished).await?;
            }
        }
        Ok(())
    }

    /// Throws away everything an abandoned upload left behind.
    ///
    /// Reports whether it worked, because the caller must not forget an upload
    /// it failed to free. S3 does not clean up abandoned multipart uploads on
    /// its own (AWS recommends a bucket lifecycle rule), so the parts stay
    /// billed until they are aborted, and dropping our row would throw away the
    /// identifier and with it any way to try again.
    ///
    /// Errors come back as `false`, not propagated. This runs from the
    /// sweep, and one uncooperative store must not stop the rest of the pass.
    pub async fn discard(&self, key: &str) -> bool {
        match self {
            Self::Multipart { store, id } => {
                if let Err(error) = store.abort_multipart(&ObjectPath::from(key), id).await {
                    tracing::warn!(?error, key, "could not abort an abandoned upload");
                    return false;
                }
            }
            Self::Appending { path } => {
                if let Err(error) = tokio::fs::remove_file(path).await
                    && error.kind() != std::io::ErrorKind::NotFound
                {
                    tracing::warn!(?error, ?path, "could not remove a partial upload");
                    return false;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PART: usize = 5 * 1024 * 1024;

    /// The floor S3 enforces and Garage does not: every part but the last is
    /// exactly `part_size`, whatever sizes the network delivered.
    #[test]
    fn every_part_but_the_last_is_the_full_size() {
        let pending = vec![0_u8; PART * 2 + 17];
        let (parts, remainder) = split_into_parts(&pending, PART, false);

        assert_eq!(parts.len(), 2);
        assert!(parts.iter().all(|part| part.len() == PART));
        assert_eq!(remainder.len(), 17, "the short tail waits for more");
    }

    /// A chunk too small to be a part is kept, not sent and not dropped.
    ///
    /// Sending it would make a part below the floor; dropping it would lose the
    /// bytes. Parking it is the only remaining answer, and it is what tusd does.
    #[test]
    fn a_chunk_below_the_floor_becomes_no_part_at_all() {
        let (parts, remainder) = split_into_parts(&[7; 1024], PART, false);
        assert!(parts.is_empty());
        assert_eq!(remainder.len(), 1024);
    }

    /// The last part is the one allowed to be short, so the remainder goes out
    /// instead of being parked for ever.
    #[test]
    fn the_final_write_sends_the_remainder_as_a_part() {
        let (parts, remainder) = split_into_parts(&[7; 1024], PART, true);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].len(), 1024);
        assert!(remainder.is_empty());
    }

    /// An exact multiple leaves nothing behind, on either kind of write.
    #[test]
    fn an_exact_multiple_leaves_no_remainder() {
        for is_final in [false, true] {
            let (parts, remainder) = split_into_parts(&vec![0; PART * 3], PART, is_final);
            assert_eq!(parts.len(), 3, "final={is_final}");
            assert!(remainder.is_empty(), "final={is_final}");
        }
    }

    /// Nothing in, nothing out: a `PATCH` with an empty body is legal and must
    /// not produce a zero-length part, which S3 refuses.
    #[test]
    fn nothing_produces_no_part() {
        for is_final in [false, true] {
            let (parts, remainder) = split_into_parts(&[], PART, is_final);
            assert!(parts.is_empty(), "final={is_final}");
            assert!(remainder.is_empty(), "final={is_final}");
        }
    }

    /// S3 counts parts from one. Starting at zero makes `complete` reject the
    /// whole upload after every byte has already been paid for.
    #[test]
    fn part_numbers_start_at_one() {
        assert_eq!(next_part_number(0), 1);
        assert_eq!(next_part_number(3), 4);
    }
}

/// The rules the stores impose on a multipart upload, as our own invariant.
///
/// # Why these are tested here and not against a store
///
/// `config.rs` already records that S3 refuses every part but the last below
/// 5 MiB, that Cloudflare R2 additionally requires every part but the last to
/// be the *same* size, and that **Garage enforces neither, so the CI job cannot
/// catch a mistake here**. Waiting for a store to reject us means the first
/// report comes from an operator on an untested backend, halfway through a
/// large upload.
///
/// They are ours to satisfy, so they are asserted where they are produced.
#[cfg(test)]
mod part_rules {
    use proptest::prelude::*;

    use super::{part_count, split_into_parts};

    /// S3's floor. A part below this is refused unless it is the last one.
    const S3_MINIMUM_PART: usize = 5 * 1024 * 1024;

    proptest! {
        /// Every part but the last is exactly `part_size`.
        ///
        /// That satisfies both rules at once: equal to each other for R2, and
        /// at or above the floor whenever `part_size` is, for S3.
        #[test]
        fn non_final_parts_are_all_one_size(
            len in 0_usize..70_000,
            part_size in 1_usize..9_000,
            is_final in any::<bool>(),
        ) {
            let pending = vec![7_u8; len];
            let (parts, remainder) = split_into_parts(&pending, part_size, is_final);

            let last_may_be_short = usize::from(is_final);
            for (index, part) in parts.iter().enumerate() {
                if index + last_may_be_short < parts.len() {
                    prop_assert_eq!(
                        part.len(),
                        part_size,
                        "part {} of {} is not the agreed size",
                        index,
                        parts.len()
                    );
                }
            }
            prop_assert!(remainder.len() < part_size);
        }

        /// Nothing is dropped and nothing is duplicated: the parts and whatever
        /// is parked add up to what came in.
        #[test]
        fn everything_that_came_in_comes_out(
            len in 0_usize..70_000,
            part_size in 1_usize..9_000,
            is_final in any::<bool>(),
        ) {
            let pending = vec![7_u8; len];
            let (parts, remainder) = split_into_parts(&pending, part_size, is_final);
            let total: usize = parts.iter().map(bytes::Bytes::len).sum::<usize>() + remainder.len();
            prop_assert_eq!(total, len);
        }

        /// A finished upload parks nothing: whatever is left has to become the
        /// last part, or those bytes are never written.
        #[test]
        fn a_final_write_leaves_nothing_parked(
            len in 0_usize..70_000,
            part_size in 1_usize..9_000,
        ) {
            let (_, remainder) = split_into_parts(&vec![7_u8; len], part_size, true);
            prop_assert!(remainder.is_empty());
        }

        /// With a part size at or above S3's floor, every part this produces is
        /// one a store will take, except the last, which may be short and only
        /// exists on the final write.
        #[test]
        fn a_conforming_part_size_produces_conforming_parts(
            len in 0_usize..(24 * 1024 * 1024),
            extra in 0_usize..(3 * 1024 * 1024),
        ) {
            let part_size = S3_MINIMUM_PART + extra;
            let (parts, _) = split_into_parts(&vec![7_u8; len], part_size, true);
            for (index, part) in parts.iter().enumerate() {
                if index + 1 < parts.len() {
                    prop_assert!(
                        part.len() >= S3_MINIMUM_PART,
                        "S3 refuses a non-final part of {} bytes",
                        part.len()
                    );
                }
            }
        }
    }
    /// The arithmetic a client writing parts directly must match. One byte past
    /// a boundary is a whole extra part; exactly on one is not.
    #[test]
    fn a_direct_upload_is_cut_into_the_parts_a_client_expects() {
        assert_eq!(part_count(0, 8), 0, "an empty upload has nothing to write");
        assert_eq!(part_count(1, 8), 1);
        assert_eq!(part_count(8, 8), 1, "exactly one part");
        assert_eq!(part_count(9, 8), 2, "one byte over is a second part");
        assert_eq!(part_count(16, 8), 2);
        assert_eq!(part_count(17, 8), 3);
        assert_eq!(
            part_count(u64::MAX, 8),
            u64::MAX.div_ceil(8),
            "no overflow at the edge"
        );
        assert_eq!(
            part_count(10, 0),
            0,
            "a zero part size is no parts, not a panic"
        );
    }
}
