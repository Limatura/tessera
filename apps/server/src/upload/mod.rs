//! Uploads that survive a dropped connection.
//!
//! A file arrives across several requests instead of one, so a transfer that
//! dies at ninety percent resumes from ninety percent. The protocol is tus's
//! offset model — see `api::uploads` — and where the bytes go while the upload
//! is unfinished is [`sink`].

pub mod sink;

use crate::{db::repository::upload::Segment, state::AppState};

/// Rebuilds the handle to an upload an earlier request left behind.
///
/// Nothing is created: the multipart upload already exists at the store and the
/// partial file already exists on disk, both from the `POST` that opened the
/// session. `None` when neither is reachable, which means the store changed
/// under a session that was already open.
///
/// Shared by the `PATCH` route and the sweep, because "which sink does this row
/// mean" has one answer and two callers — and the sweep getting it wrong would
/// abort nothing while reporting that it had.
#[must_use]
pub fn reopen(state: &AppState, segment: &Segment) -> Option<sink::Sink> {
    // The backend the session recorded, not whichever one new uploads go to.
    // Reopening against the wrong store aborts nothing while reporting that it
    // did, and writes the next part where the earlier ones are not.
    if let (Some(handle), Some(id)) = (
        state.backend_multipart(&segment.backend),
        &segment.multipart_id,
    ) {
        return Some(sink::Sink::Multipart {
            store: std::sync::Arc::clone(&handle.0),
            id: id.clone(),
        });
    }
    // The partial file only exists for the backend built from `storage.url` —
    // that is the only one that can be a local directory.
    if segment.backend != crate::storage::DEFAULT_BACKEND {
        return None;
    }
    crate::storage::local_path(&state.config().storage.url).map(|root| sink::Sink::Appending {
        path: std::path::PathBuf::from(root).join(format!("{}.part", segment.object_key)),
    })
}
