//! Writing one body across the backends a plan named.
//!
//! [`split::plan`](crate::storage::split::plan) says where the bytes go;
//! this puts them there. It is the easiest part of splitting to get wrong: the
//! state where "the first backend is finished and the second has not been opened
//! yet" has to be true of something rather than implied — so it is a unit that
//! can be
//! tested on its own, against a store that runs out part-way.
//!
//! # What it guarantees
//!
//! - Each segment receives exactly the bytes the plan gave it. A segment that received one byte too
//!   few is a file that reassembles short; one too many overwrites the next segment's first byte.
//! - A segment is completed before the next is opened. Two multipart uploads open at once is two
//!   things to abort on a failure and two ways to lose track of one.
//! - Parts within a segment are whole `part_size` blocks apart from the last, which is what S3
//!   requires and what the plan's alignment exists to make possible.
//!
//! # Who cleans up after a failure
//!
//! Both answers are right, for different callers, which is why [`Writer::abandon`]
//! exists rather than a `Drop`.
//!
//! A **resumable** session has rows: the segments are in `upload_segments`, and
//! the sweep walks them. Aborting here would be a second cleaner racing the
//! first, and throwing the identifiers away would leave parts nothing can ever
//! abort.
//!
//! A **direct** `POST /shares` has no rows at all — there is nothing to record
//! against until the share exists. If this does not clean up, nothing does, and
//! S3 bills for the parts for ever. So that caller calls `abandon`.

use bytes::Bytes;
use object_store::{MultipartUpload, PutPayload};

use crate::{error::Result, state::AppState, storage::split::Segment};

/// One segment, as the writer needs it: where the plan put it, and what it is
/// called in the store.
#[derive(Debug, Clone)]
pub struct Planned {
    /// What the planner decided.
    pub segment: Segment,
    /// The object it will be written as.
    pub object_key: String,
}

/// A segment that has been written and closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// Which backend took it.
    pub backend: String,
    /// The object it was written as.
    pub object_key: String,
    /// Where it starts within the file.
    pub offset: u64,
    /// How many bytes it holds.
    pub length: u64,
}

/// Routes a body into the segments a plan named, in order.
pub struct Writer<'a> {
    state: &'a AppState,
    plan: Vec<Planned>,
    part_size: usize,
    /// Which segment is being filled.
    at: usize,
    /// The upload for that segment, once it has been opened.
    open: Option<Box<dyn MultipartUpload>>,
    /// Bytes held back because they do not yet make a whole part.
    buffered: Vec<u8>,
    /// How much has gone into the current segment.
    into_current: u64,
    /// Segments already closed.
    done: Vec<Written>,
}

impl std::fmt::Debug for Writer<'_> {
    /// Where it is up to, not what it holds: the open upload is a trait object
    /// with nothing printable, and the buffer is bytes nobody wants in a log.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Writer")
            .field("segments", &self.plan.len())
            .field("at", &self.at)
            .field("into_current", &self.into_current)
            .field("open", &self.open.is_some())
            .finish_non_exhaustive()
    }
}

impl<'a> Writer<'a> {
    /// Starts a writer over `plan`.
    #[must_use]
    pub fn new(state: &'a AppState, plan: Vec<Planned>, part_size: usize) -> Self {
        Self {
            state,
            plan,
            part_size,
            at: 0,
            open: None,
            buffered: Vec::new(),
            into_current: 0,
            done: Vec::new(),
        }
    }

    /// Takes another chunk of the body.
    ///
    /// A chunk that crosses a segment boundary is split at it: the first part
    /// finishes the segment it belongs to, the segment is completed, and the
    /// rest opens the next. That is the case the whole type exists for, and it
    /// is why this cannot be "write the buffer when it is full".
    ///
    /// # Errors
    ///
    /// Returns [`Error`](crate::error::Error) if a store refuses, or if the body
    /// is longer than the plan allowed for.
    pub async fn write(&mut self, chunk: &[u8]) -> Result<()> {
        let mut rest = chunk;
        while !rest.is_empty() {
            let Some(planned) = self.plan.get(self.at).cloned() else {
                return Err(crate::error::Error::bad_request(
                    "the body is longer than it said it would be",
                ));
            };

            let room = planned.segment.length.saturating_sub(self.into_current);
            let take = usize::try_from(room).unwrap_or(usize::MAX).min(rest.len());
            let (mine, tail) = rest.split_at(take);
            rest = tail;

            self.open_if_needed(&planned).await?;
            self.buffered.extend_from_slice(mine);
            self.into_current = self
                .into_current
                .saturating_add(u64::try_from(take).unwrap_or(u64::MAX));
            self.flush_whole_parts().await?;

            if self.into_current == planned.segment.length {
                self.close_current(&planned).await?;
            }
        }
        Ok(())
    }

    /// Closes whatever is still open and hands back what was written.
    ///
    /// The last segment is the one allowed to be short — the plan's own rule —
    /// so this completes it whatever it holds.
    ///
    /// # Errors
    ///
    /// Returns [`Error`](crate::error::Error) if the store refuses the
    /// completion.
    pub async fn finish(mut self) -> Result<Vec<Written>> {
        if let Some(planned) = self.plan.get(self.at).cloned()
            && self.open.is_some()
        {
            self.close_current(&planned).await?;
        }
        Ok(self.done)
    }

    /// Gives back everything this writer put in a store.
    ///
    /// The open segment is aborted and the finished ones are deleted. For a
    /// caller with no rows to record the segments against — a direct
    /// `POST /shares` — this is the only thing that will ever free them.
    ///
    /// Failures are logged rather than returned: this runs on the way out of an
    /// error, and one store refusing must not hide the error that got here.
    pub async fn abandon(mut self) {
        if let Some(mut upload) = self.open.take()
            && let Err(error) = upload.abort().await
        {
            tracing::warn!(?error, "could not abort a segment's upload");
        }
        for piece in &self.done {
            let Some(store) = self.state.backend(&piece.backend) else {
                continue;
            };
            if let Err(error) = object_store::ObjectStoreExt::delete(
                store.as_ref(),
                &object_store::path::Path::from(piece.object_key.as_str()),
            )
            .await
            {
                tracing::warn!(?error, key = piece.object_key, "could not remove a segment");
            }
        }
    }

    async fn open_if_needed(&mut self, planned: &Planned) -> Result<()> {
        if self.open.is_some() {
            return Ok(());
        }
        let store = self
            .state
            .backend(&planned.segment.backend)
            .ok_or(crate::error::Error::Unavailable)?;
        self.open = Some(
            object_store::ObjectStoreExt::put_multipart(
                store.as_ref(),
                &object_store::path::Path::from(planned.object_key.as_str()),
            )
            .await?,
        );
        Ok(())
    }

    async fn flush_whole_parts(&mut self) -> Result<()> {
        let Some(upload) = self.open.as_mut() else {
            return Ok(());
        };
        while self.buffered.len() >= self.part_size {
            let rest = self.buffered.split_off(self.part_size);
            let part = std::mem::replace(&mut self.buffered, rest);
            upload.put_part(PutPayload::from(Bytes::from(part))).await?;
        }
        Ok(())
    }

    async fn close_current(&mut self, planned: &Planned) -> Result<()> {
        let Some(mut upload) = self.open.take() else {
            return Ok(());
        };
        if !self.buffered.is_empty() {
            upload
                .put_part(PutPayload::from(Bytes::from(std::mem::take(
                    &mut self.buffered,
                ))))
                .await?;
        }
        upload.complete().await?;
        self.done.push(Written {
            backend: planned.segment.backend.clone(),
            object_key: planned.object_key.clone(),
            offset: planned.segment.offset,
            length: self.into_current,
        });
        self.at += 1;
        self.into_current = 0;
        Ok(())
    }
}
