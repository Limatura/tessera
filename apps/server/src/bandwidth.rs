//! Holding transfers to `bandwidth.*`.
//!
//! # How a body is slowed down
//!
//! A transfer asks for a piece, and gets it only when the clock says that piece
//! is due; otherwise it sleeps until then and asks again. On the way out,
//! `Paced` wraps the response body, so the wait delays the next write to the
//! socket. On the way in, [`Throttle`] is awaited by the handler between
//! reads, so the wait delays the next read and TCP's own flow control tells the
//! sender to slow down. No bytes are buffered beyond the piece in hand either
//! way.
//!
//! Uploads use [`Throttle`] rather than a wrapped body so the wait happens
//! outside `server.upload_stall_secs`. That timer asks whether the client is
//! still sending; time spent waiting for our own turn is not the client's
//! silence, and under a low shared cap with many uploads it can be long.
//!
//! # Two clocks
//!
//! Each direction has one [`Pace`] shared by every transfer, for the
//! server-wide cap, and each body has its own, for the per-transfer cap. A
//! piece waits for whichever of the two is later.
//!
//! A pace is a single "next free moment" (GCRA), not a bucket refilled on a
//! timer. A piece is due once that moment is no more than `BURST` away;
//! booking it moves the moment on by what it costs. A piece that is not due
//! books nothing, so the moment is never far in the future: a transfer that
//! goes away leaves at most one piece behind, and a limit that is raised or
//! cleared is felt by every waiting transfer as soon as it asks again. One
//! lock taken briefly per piece, and nothing running in the background.
//!
//! # Why the rate is read per piece
//!
//! So a change from the panel reaches a transfer already under way, which is
//! the transfer an operator is changing it for. A waiting transfer sleeps no
//! longer than the clock is ahead, which is at most a piece plus `BURST`.

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
    /// Books `bytes` at `rate` bytes per second if they are due.
    ///
    /// A rate of zero is unlimited and books nothing.
    ///
    /// # Errors
    ///
    /// Returns when to ask again if they are not due yet.
    pub fn try_book(&self, bytes: usize, rate: u64, now: Instant) -> Result<(), Instant> {
        if rate == 0 || bytes == 0 {
            return Ok(());
        }
        let mut next = self.next.lock();
        let start = next.map_or(now, |at| at.max(now));
        if let Some(wait) = start.checked_duration_since(now)
            && wait > BURST
        {
            return Err(start - BURST);
        }
        *next = Some(start + cost_of(bytes, rate));
        Ok(())
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

/// One transfer's place in the queue: its own clock, and the shared one.
struct Gate {
    state: AppState,
    direction: Direction,
    /// The per-transfer clock.
    own: Pace,
    /// The per-transfer clock is booked for the piece being waited on, so it is
    /// not booked a second time when the shared one makes the piece wait.
    own_booked: bool,
}

impl Gate {
    fn new(state: &AppState, direction: Direction) -> Self {
        Self {
            state: state.clone(),
            direction,
            own: Pace::default(),
            own_booked: false,
        }
    }

    /// How much to ask for at once, from the rates in force now.
    fn piece(&self, available: usize) -> usize {
        let (shared, own) = rates(&self.state, self.direction);
        piece_size(shared, own).min(available)
    }

    /// `None` when `len` bytes may go now; otherwise when to ask again.
    ///
    /// The rates are read on every ask, so a change applies to a transfer that
    /// is already waiting.
    fn ask(&mut self, len: usize, now: Instant) -> Option<Instant> {
        let (shared, own) = rates(&self.state, self.direction);
        if !self.own_booked {
            if let Err(at) = self.own.try_book(len, own, now) {
                return Some(at);
            }
            self.own_booked = true;
        }
        if let Err(at) = self
            .state
            .bandwidth
            .pace(self.direction)
            .try_book(len, shared, now)
        {
            return Some(at);
        }
        self.own_booked = false;
        None
    }
}

/// Holds an upload to its limits, between reads.
///
/// Await [`Throttle::pass`] with the size of each piece read, outside any
/// stall timer, before reading the next.
pub struct Throttle(Gate);

impl std::fmt::Debug for Throttle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Throttle").finish_non_exhaustive()
    }
}

impl Throttle {
    /// A fresh per-transfer clock, with the shared one for `direction`.
    #[must_use]
    pub fn new(state: &AppState, direction: Direction) -> Self {
        Self(Gate::new(state, direction))
    }

    /// Returns once `bytes` more may go.
    pub async fn pass(&mut self, mut bytes: usize) {
        while bytes > 0 {
            let piece = self.0.piece(bytes);
            while let Some(at) = self.0.ask(piece, Instant::now()) {
                tokio::time::sleep_until(at).await;
            }
            bytes -= piece;
        }
    }
}

/// Slows `body` to the limits for `direction`, for a response.
///
/// Always wraps, even with no limit set, so a limit set while the transfer is
/// running still reaches it. Unlimited costs a config read per frame.
pub fn limit(state: &AppState, direction: Direction, body: Body) -> Body {
    Body::new(Paced {
        inner: body,
        gate: Gate::new(state, direction),
        rest: Bytes::new(),
        sleep: None,
    })
}

/// A body handed on no faster than its limits.
struct Paced {
    inner: Body,
    gate: Gate,
    /// What is left of the last frame, not yet handed on.
    rest: Bytes,
    /// The wait before the next piece is asked for again.
    sleep: Option<Pin<Box<Sleep>>>,
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
            if let Some(sleep) = &mut me.sleep {
                if sleep.as_mut().poll(cx).is_pending() {
                    return Poll::Pending;
                }
                me.sleep = None;
            }
            if !me.rest.is_empty() {
                let piece = me.gate.piece(me.rest.len());
                match me.gate.ask(piece, Instant::now()) {
                    None => {
                        let piece = me.rest.split_to(piece);
                        return Poll::Ready(Some(Ok(Frame::data(piece))));
                    }
                    Some(at) => {
                        me.sleep = Some(Box::pin(tokio::time::sleep_until(at)));
                        continue;
                    }
                }
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
        self.rest.is_empty() && self.inner.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        let pending = u64::try_from(self.rest.len()).unwrap_or(u64::MAX);
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
        assert_eq!(pace.try_book(1 << 20, 0, now), Ok(()));
        assert_eq!(pace.try_book(1 << 20, 0, now), Ok(()));
    }

    #[test]
    fn a_rate_spaces_pieces_out_after_the_burst() {
        let pace = Pace::default();
        let now = Instant::now();
        // 1000 B/s: each 100 bytes cost 100 ms. The first two are due at once,
        // the second because it starts within the burst of now.
        assert_eq!(pace.try_book(100, 1000, now), Ok(()));
        assert_eq!(pace.try_book(100, 1000, now), Ok(()));
        // The third would start at 200 ms: due when 100 ms have passed.
        let due = now + Duration::from_millis(100);
        assert_eq!(pace.try_book(100, 1000, now), Err(due));
        assert_eq!(pace.try_book(100, 1000, due), Ok(()));
    }

    #[test]
    fn a_piece_that_is_not_due_books_nothing() {
        let pace = Pace::default();
        let now = Instant::now();
        pace.try_book(1000, 1000, now).unwrap();
        // A hundred waiting transfers leave the clock where it was, so a rate
        // raised now is felt by all of them when they ask again.
        for _ in 0..100 {
            assert!(pace.try_book(1000, 1000, now).is_err());
        }
        let later = now + Duration::from_millis(900);
        assert_eq!(pace.try_book(1000, 1000, later), Ok(()));
        // 1 second booked, then one more: the clock is 2 s from the start.
        assert_eq!(
            pace.try_book(1000, 1000, later),
            Err(now + Duration::from_millis(1900))
        );
    }

    #[test]
    fn idle_time_is_not_saved_up() {
        let pace = Pace::default();
        let start = Instant::now();
        pace.try_book(1000, 1000, start).unwrap();
        // Ten seconds idle, then two seconds' worth: only the burst is free.
        let later = start + Duration::from_secs(10);
        pace.try_book(1000, 1000, later).unwrap();
        pace.try_book(1000, 1000, later).unwrap_err();
        assert_eq!(
            pace.try_book(1000, 1000, later),
            Err(later + Duration::from_millis(900))
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
