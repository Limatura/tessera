//! A store that stops taking bytes at a point of the test's choosing.
//!
//! Every store the suite runs against (Garage, `RustFS`, a local directory) has
//! more room than any test will use, so "the backend filled up while the upload
//! was in flight" has never once been exercised. That is the path where a
//! partial object is left behind, a session records bytes that are not there,
//! and the sweep has to know what to do about it. The alternative is finding
//! out in production, and this needs to be covered *before* a file is spread
//! over several backends.
//!
//! The point is that it stops **mid-upload**. A store that refuses from the
//! first byte proves only that a refusal is handled; this one takes what fits
//! and then fails, as a real bucket hitting its quota does.

use std::{
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use object_store::{
    Error, GetOptions, GetResult, ListResult, MultipartId, MultipartUpload, ObjectMeta,
    ObjectStore, PutMultipartOptions, PutOptions, PutPayload, PutResult, Result, UploadPart,
    memory::InMemory,
    multipart::{MultipartStore, PartId},
    path::Path,
};

/// Wraps a store and refuses writes once `capacity` bytes have gone in.
#[derive(Debug)]
pub(crate) struct Brimming {
    inner: Arc<InMemory>,
    capacity: u64,
    written: Arc<AtomicU64>,
}

impl Brimming {
    /// A store with room for `capacity` bytes and not one more.
    pub(crate) fn with_room_for(capacity: u64) -> Self {
        Self {
            inner: Arc::new(InMemory::new()),
            capacity,
            written: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Takes `bytes` out of what is left, or says there is not enough.
    ///
    /// Counted before the write, not after, because a store that accepts
    /// the write and *then* reports itself full is not what a full bucket does.
    fn take(&self, bytes: u64) -> Result<()> {
        let mut current = self.written.load(Ordering::SeqCst);
        loop {
            if current.saturating_add(bytes) > self.capacity {
                return Err(Error::Generic {
                    store: "Brimming",
                    source: format!(
                        "no room: {} of {} bytes used, {bytes} more asked for",
                        current, self.capacity
                    )
                    .into(),
                });
            }
            match self.written.compare_exchange(
                current,
                current + bytes,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return Ok(()),
                Err(actual) => current = actual,
            }
        }
    }
}

impl std::fmt::Display for Brimming {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Brimming({} bytes)", self.capacity)
    }
}

#[async_trait::async_trait]
impl ObjectStore for Brimming {
    async fn put_opts(
        &self,
        location: &Path,
        payload: PutPayload,
        options: PutOptions,
    ) -> Result<PutResult> {
        self.take(u64::try_from(payload.content_length()).unwrap_or(u64::MAX))?;
        self.inner.put_opts(location, payload, options).await
    }

    async fn put_multipart_opts(
        &self,
        location: &Path,
        options: PutMultipartOptions,
    ) -> Result<Box<dyn MultipartUpload>> {
        let upload = self.inner.put_multipart_opts(location, options).await?;
        Ok(Box::new(BrimmingUpload {
            inner: upload,
            capacity: self.capacity,
            written: Arc::clone(&self.written),
        }))
    }

    async fn get_opts(&self, location: &Path, options: GetOptions) -> Result<GetResult> {
        self.inner.get_opts(location, options).await
    }

    /// Deletion goes through the stream form: `ObjectStore` has no `delete` of
    /// its own, and the single-object one is an extension built on this.
    fn delete_stream(
        &self,
        locations: futures_util::stream::BoxStream<'static, Result<Path>>,
    ) -> futures_util::stream::BoxStream<'static, Result<Path>> {
        self.inner.delete_stream(locations)
    }

    fn list(
        &self,
        prefix: Option<&Path>,
    ) -> futures_util::stream::BoxStream<'static, Result<ObjectMeta>> {
        self.inner.list(prefix)
    }

    async fn list_with_delimiter(&self, prefix: Option<&Path>) -> Result<ListResult> {
        self.inner.list_with_delimiter(prefix).await
    }

    async fn copy_opts(
        &self,
        from: &Path,
        to: &Path,
        options: object_store::CopyOptions,
    ) -> Result<()> {
        self.inner.copy_opts(from, to, options).await
    }
}

/// One multipart upload, counting against the same budget.
///
/// This is the half that matters: a large file is written a part at a time, so
/// the store runs out somewhere in the middle, not at the door.
#[derive(Debug)]
struct BrimmingUpload {
    inner: Box<dyn MultipartUpload>,
    capacity: u64,
    written: Arc<AtomicU64>,
}

#[async_trait::async_trait]
impl MultipartUpload for BrimmingUpload {
    fn put_part(&mut self, payload: PutPayload) -> UploadPart {
        let bytes = u64::try_from(payload.content_length()).unwrap_or(u64::MAX);
        let used = self.written.fetch_add(bytes, Ordering::SeqCst) + bytes;
        if used > self.capacity {
            let capacity = self.capacity;
            return Box::pin(async move {
                Err(Error::Generic {
                    store: "Brimming",
                    source: format!("no room: {used} bytes written, {capacity} allowed").into(),
                })
            });
        }
        self.inner.put_part(payload)
    }

    async fn complete(&mut self) -> Result<PutResult> {
        self.inner.complete().await
    }

    async fn abort(&mut self) -> Result<()> {
        self.inner.abort().await
    }
}

/// The part-level view, so the fixture can hand this to the resumable path too.
#[async_trait::async_trait]
impl MultipartStore for Brimming {
    async fn create_multipart(&self, path: &Path) -> Result<MultipartId> {
        self.inner.create_multipart(path).await
    }

    async fn put_part(
        &self,
        path: &Path,
        id: &MultipartId,
        part_idx: usize,
        data: PutPayload,
    ) -> Result<PartId> {
        self.take(u64::try_from(data.content_length()).unwrap_or(u64::MAX))?;
        self.inner.put_part(path, id, part_idx, data).await
    }

    async fn complete_multipart(
        &self,
        path: &Path,
        id: &MultipartId,
        parts: Vec<PartId>,
    ) -> Result<PutResult> {
        self.inner.complete_multipart(path, id, parts).await
    }

    async fn abort_multipart(&self, path: &Path, id: &MultipartId) -> Result<()> {
        self.inner.abort_multipart(path, id).await
    }
}

#[expect(dead_code, reason = "used by some test binaries and not others")]
impl Brimming {
    /// How many bytes have been taken, for a test that wants to say where the
    /// store gave out.
    pub(crate) fn written(&self) -> u64 {
        self.written.load(Ordering::SeqCst)
    }

    /// Every range this store has, so a test can see what survived a failure.
    pub(crate) async fn get_ranges(
        &self,
        path: &Path,
        ranges: &[Range<u64>],
    ) -> Result<Vec<bytes::Bytes>> {
        self.inner.get_ranges(path, ranges).await
    }
}
