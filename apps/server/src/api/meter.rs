//! Counting the bytes a share actually sent.
//!
//! # What this measures, and what it does not
//!
//! Bytes that left this process, counted as they pass through the response
//! body, so a transfer that stops half way counts the half that went. Adding
//! the file's size on every request instead would report a full download for
//! every click that was then abandoned.
//!
//! It is **not** bytes that reached anybody:
//!
//! - Behind a reverse proxy, the proxy holds some. Measured against nginx with its default
//!   buffering: a client that took 73319 bytes and disconnected left this process having written
//!   74137. About one percent, always in the same direction.
//! - Behind a caching CDN the gap runs the other way and has no bound: a request served from cache
//!   never reaches this process at all.
//!
//! So the number answers "how much has this instance sent out", the question
//! an operator asks of their own machine. For bytes that crossed the network, a
//! proxy's own log is the authority; nginx has `$body_bytes_sent`.
//!
//! # Why the layer sits outside compression
//!
//! Measured: a 64 KiB run of one repeated character leaves this process as 97
//! bytes once `CompressionLayer` has had it. Counting on the handler's side of
//! that layer reports 65536, six hundred times the truth. So the meter wraps
//! the compressed body, not the one the handler produced.
//!
//! File downloads are `application/octet-stream`, which the compression
//! predicate excludes, so for those the two sides agree. `/raw` serves text and
//! is compressed, and that is where the difference lives.

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use axum::{body::Body, extract::Request, response::Response};
use bytes::Buf as _;
use http_body::{Body as HttpBody, Frame};
use tessera_core::ShareId;

use crate::state::AppState;

/// Marks a response as one share's content, so the meter knows what to credit.
///
/// Put in the response extensions by the handler, not worked out from the path.
/// The handler is the only place that has already resolved the code
/// to a share, and re-resolving it in a middleware would mean a second lookup
/// on every retrieval to learn something the first one knew.
#[derive(Debug, Clone, Copy)]
pub struct Served(pub ShareId);

/// A response body that adds up what goes through it.
struct Metered {
    inner: Body,
    sent: u64,
    share: ShareId,
    state: AppState,
}

impl HttpBody for Metered {
    type Data = bytes::Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let me = &mut *self;
        let polled = Pin::new(&mut me.inner).poll_frame(cx);
        if let Poll::Ready(Some(Ok(frame))) = &polled
            && let Some(data) = frame.data_ref()
        {
            me.sent = me
                .sent
                .saturating_add(u64::try_from(data.remaining()).unwrap_or(u64::MAX));
        }
        polled
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.inner.size_hint()
    }
}

impl Drop for Metered {
    /// The end of the transfer, however it ended.
    ///
    /// A completed body and an abandoned one are both dropped, so the count
    /// lives here: no other moment sees both, since the handler returned
    /// before the first byte moved.
    ///
    /// The write is spawned because `Drop` cannot await. A process being killed
    /// may lose the last few of these, and those are the right thing to lose:
    /// the
    /// alternative is holding the response open to write a statistic.
    fn drop(&mut self) {
        if self.sent == 0 {
            return;
        }
        let (state, share, sent) = (self.state.clone(), self.share, self.sent);
        tokio::spawn(async move {
            if let Err(error) = state.shares.add_bytes_served(share, sent).await {
                tracing::warn!(?error, %share, "could not record what a share sent");
            }
        });
    }
}

/// Wraps every response, and counts the ones a handler marked.
///
/// # Errors
///
/// None of its own; it passes through whatever the inner service returned.
pub async fn count_bytes_served(
    axum::extract::State(state): axum::extract::State<AppState>,
    request: Request,
    next: axum::middleware::Next,
) -> Response {
    let response = next.run(request).await;
    let Some(&Served(share)) = response.extensions().get::<Served>() else {
        return response;
    };
    let (parts, body) = response.into_parts();
    Response::from_parts(
        parts,
        Body::new(Metered {
            inner: body,
            sent: 0,
            share,
            state,
        }),
    )
}
