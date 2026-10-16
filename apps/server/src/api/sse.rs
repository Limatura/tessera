//! Server-sent events: the framing every event stream shares.
//!
//! - A comment line every [`KEEP_ALIVE`], so a proxy or browser that drops an idle connection keeps
//!   this one, and a client can tell a quiet stream from a dead one.
//! - `X-Accel-Buffering: no`, so nginx passes each event on as it comes instead of holding it until
//!   a buffer fills.
//! - The stream ends when the process starts draining. Graceful shutdown waits for every open
//!   response, and an event stream would otherwise hold it until its client went away. The client
//!   reconnects to whichever instance is up.
//!
//! Compression is already off: `text/event-stream` is excluded by
//! `tower_http`'s default predicate, which [`super::compression_predicate`]
//! builds on, and a compressor would buffer events.
//!
//! Browsers' `EventSource` cannot send headers, so a client authenticating
//! with one (a management token, say) reads the stream with `fetch` instead.

use std::{convert::Infallible, time::Duration};

use axum::response::{
    IntoResponse as _, Response,
    sse::{Event, KeepAlive, Sse},
};
use tokio_stream::{Stream, StreamExt as _};

use crate::state::AppState;

/// How often an idle stream sends a comment line.
pub const KEEP_ALIVE: Duration = Duration::from_secs(15);

/// `events` as a `text/event-stream` response, ending early if the process
/// starts draining.
pub fn respond<S>(state: &AppState, events: S) -> Response
where
    S: Stream<Item = Result<Event, Infallible>> + Send + 'static,
{
    let draining = state.draining();
    let events = async_stream::stream! {
        let mut events = std::pin::pin!(events);
        loop {
            tokio::select! {
                () = draining.cancelled() => break,
                event = events.next() => match event {
                    Some(event) => yield event,
                    None => break,
                },
            }
        }
    };
    (
        [("x-accel-buffering", "no")],
        Sse::new(events).keep_alive(KeepAlive::new().interval(KEEP_ALIVE)),
    )
        .into_response()
}
