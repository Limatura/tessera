//! Holding transfers to `bandwidth.*`.
//!
//! # How a body is slowed down
//!
//! [`Paced`] wraps a body and, before handing on each piece, books the time
//! that piece costs at the configured rate. When the booking lands in the
//! future it waits until then. On the way out that delays the next write to the
//! socket; on the way in it delays the next read, and TCP's own flow control
//! tells the sender to slow down. No bytes are buffered beyond the piece in
//! hand either way.
//!
//! # Two clocks
//!
//! Each direction has one [`Pace`] shared by every transfer, for the
//! server-wide cap, and each body has its own, for the per-transfer cap. A
//! piece waits for whichever of the two is later.
//!
//! A pace is a single "next free moment" (virtual scheduling, as in GCRA), not
//! a bucket refilled on a timer: booking a piece moves the moment on by what it
//! costs, and a transfer that comes back from idle may run ahead of it by at
//! most [`BURST`]. One lock taken briefly per piece, and nothing running in the
//! background.
//!
//! # Why the rate is read per piece
//!
//! So a change from the panel reaches a transfer already under way, which is
//! the transfer an operator is changing it for.

use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use axum::body::Body;
use bytes::{Buf as _, Bytes};
use http_body::{Body as HttpBody, Frame};
use tokio::time::{Instant, Sleep};

use crate::state::AppState;

/// How far a transfer may run ahead of its rate after being idle.
const BURST: Duration = Duration::from_millis(100);

/// The largest piece booked at once. Smaller pieces make the rate smoother;
/// see [`piece_size`] for the lower end.
const MAX_PIECE: usize = 16 * 1024;

/// Which way the bytes go, as seen from the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Into the server: share uploads, the speed test upload.
    Upload,
    /// Out of the server: share downloads, the speed test download.
    Download,
}

/// One clock: the moment the next byte may go.
#[derive(Debug)]
pub struct Pace {
    next: parking_lot::Mutex<Option<Instant>>,
}

impl Default for Pace {
    fn default() -> Self {
        Self {
            next: parking_lot::Mutex::new(None),
        }
    }
}

impl Pace {
    /// Books `bytes` at `rate` bytes per second and says when they may go.
    ///
    /// A rate of zero is unlimited and books nothing.
    pub fn reserve(&self, bytes: usize, rate: u64, now: Instant) -> Instant {
        if rate == 0 || bytes == 0 {
            return now;
        }
        let cost = cost_of(bytes, rate);
        let mut next = self.next.lock();
        let start = next.map_or(now, |at| at.max(now));
        let booked = start + cost;
        *next = Some(booked);
        booked.checked_sub(BURST).map_or(now, |at| at.max(now))
    }
}

/// How long `bytes` take at `rate` bytes per second.
fn cost_of(bytes: usize, rate: u64) -> Duration {
    let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
    let nanos = u128::from(bytes) * 1_000_000_000 / u128::from(rate.max(1));
    Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
}

/// The server-wide clocks, one per direction.
#[derive(Debug, Default)]
pub struct Limits {
    upload: Pace,
    download: Pace,
}

impl Limits {
    const fn pace(&self, direction: Direction) -> &Pace {
        match direction {
            Direction::Upload => &self.upload,
            Direction::Download => &self.download,
        }
    }
}

/// The (server-wide, per-transfer) rates for one direction, now.
fn rates(state: &AppState, direction: Direction) -> (u64, u64) {
    let limits = state.config().bandwidth;
    match direction {
        Direction::Upload => (limits.upload, limits.upload_per_transfer),
        Direction::Download => (limits.download, limits.download_per_transfer),
    }
}

/// How much to book at once, given the slowest rate in force.
///
/// A quarter of a second's worth, between 1 KiB and [`MAX_PIECE`]: at the
/// slowest rate allowed (1 KiB/s) a piece is a second, well inside any stall
/// timeout, and at a fast one the lock is taken once per 16 KiB.
fn piece_size(shared: u64, own: u64) -> usize {
    let slowest = match (shared, own) {
        (0, 0) => return usize::MAX,
        (0, rate) | (rate, 0) => rate,
        (a, b) => a.min(b),
    };
    usize::try_from(slowest / 4)
        .unwrap_or(MAX_PIECE)
        .clamp(1024, MAX_PIECE)
}

/// Slows `body` to the limits for `direction`.
///
/// Always wraps, even with no limit set, so a limit set while the transfer is
/// running still reaches it. Unlimited costs a config read per frame.
pub fn limit(state: &AppState, direction: Direction, body: Body) -> Body {
    Body::new(Paced {
        inner: body,
        state: state.clone(),
        direction,
        own: Pace::default(),
        rest: Bytes::new(),
        held: None,
    })
}

/// A body handed on no faster than its limits.
struct Paced {
    inner: Body,
    state: AppState,
    direction: Direction,
    /// The per-transfer clock.
    own: Pace,
    /// What is left of the last frame, not yet booked.
    rest: Bytes,
    /// A booked piece, and the wait before it may go.
    held: Option<(Bytes, Pin<Box<Sleep>>)>,
}

impl Paced {
    /// Takes the next piece of `rest` and books it. Returns it if it may go now.
    fn book(&mut self) -> Option<Bytes> {
        let (shared, own) = rates(&self.state, self.direction);
        let size = piece_size(shared, own).min(self.rest.len());
        let piece = self.rest.split_to(size);
        if shared == 0 && own == 0 {
            return Some(piece);
        }
        let now = Instant::now();
        let at = self
            .state
            .bandwidth
            .pace(self.direction)
            .reserve(piece.len(), shared, now)
            .max(self.own.reserve(piece.len(), own, now));
        if at <= now {
            return Some(piece);
        }
        self.held = Some((piece, Box::pin(tokio::time::sleep_until(at))));
        None
    }
}

impl HttpBody for Paced {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let me = &mut *self;
        loop {
            if let Some((_, sleep)) = &mut me.held {
                if sleep.as_mut().poll(cx).is_pending() {
                    return Poll::Pending;
                }
                if let Some((piece, _)) = me.held.take() {
                    return Poll::Ready(Some(Ok(Frame::data(piece))));
                }
            }
            if !me.rest.is_empty() {
                if let Some(piece) = me.book() {
                    return Poll::Ready(Some(Ok(Frame::data(piece))));
                }
                continue;
            }
            match Pin::new(&mut me.inner).poll_frame(cx) {
                Poll::Ready(Some(Ok(frame))) => match frame.into_data() {
                    Ok(mut data) => {
                        me.rest = data.copy_to_bytes(data.remaining());
                    }
                    // Trailers carry no content and are not paced.
                    Err(frame) => return Poll::Ready(Some(Ok(frame))),
                },
                other => return other,
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        self.held.is_none() && self.rest.is_empty() && self.inner.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        let pending = self.rest.len() + self.held.as_ref().map_or(0, |(piece, _)| piece.len());
        let pending = u64::try_from(pending).unwrap_or(u64::MAX);
        let inner = self.inner.size_hint();
        let mut hint = http_body::SizeHint::new();
        hint.set_lower(inner.lower().saturating_add(pending));
        if let Some(upper) = inner.upper() {
            hint.set_upper(upper.saturating_add(pending));
        }
        hint
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlimited_books_nothing() {
        let pace = Pace::default();
        let now = Instant::now();
        assert_eq!(pace.reserve(1 << 20, 0, now), now);
        assert_eq!(pace.reserve(1 << 20, 0, now), now);
    }

    #[test]
    fn a_rate_spaces_pieces_out_after_the_burst() {
        let pace = Pace::default();
        let now = Instant::now();
        // 1000 B/s: 100 bytes cost 100 ms, which the burst covers.
        assert_eq!(pace.reserve(100, 1000, now), now);
        // The next 100 are booked at 200 ms, minus the burst.
        assert_eq!(
            pace.reserve(100, 1000, now),
            now + Duration::from_millis(100)
        );
        assert_eq!(
            pace.reserve(1000, 1000, now),
            now + Duration::from_millis(1100)
        );
    }

    #[test]
    fn idle_time_is_not_saved_up() {
        let pace = Pace::default();
        let start = Instant::now();
        pace.reserve(1000, 1000, start);
        // Ten seconds idle, then a second's worth: only the burst is free.
        let later = start + Duration::from_secs(10);
        assert_eq!(
            pace.reserve(1000, 1000, later),
            later + Duration::from_millis(900)
        );
    }

    #[test]
    fn pieces_stay_between_one_and_sixteen_kib() {
        assert_eq!(piece_size(0, 0), usize::MAX);
        assert_eq!(piece_size(1024, 0), 1024);
        assert_eq!(piece_size(0, 40_000), 10_000);
        assert_eq!(piece_size(10_000_000, 40_000), 10_000);
        assert_eq!(piece_size(10_000_000, 0), MAX_PIECE);
    }
}
