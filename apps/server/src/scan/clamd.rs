//! ClamAV's daemon, spoken to directly.
//!
//! `INSTREAM` over TCP: the command, then the file as length-prefixed chunks,
//! then a zero length, and one line back. No library; the protocol is those
//! four steps and a library would be a dependency for them.
//!
//! # The size limit
//!
//! clamd refuses a stream longer than its `StreamMaxLength` by answering
//! `INSTREAM size limit exceeded` and closing the connection, which it may do
//! while this side is still writing. So a failed write is not the end: the
//! answer is read anyway, and it is what decides.

use bytes::Bytes;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpStream,
};
use tokio_stream::{Stream, StreamExt as _};

use super::{Unreachable, Verdict};

/// The port clamd listens on unless told otherwise.
pub const DEFAULT_PORT: u16 = 3310;

/// How much of the file goes in one chunk. Anything under clamd's limit works;
/// this keeps the buffer small.
const CHUNK: usize = 64 * 1024;

/// The longest answer read. A real one is a threat name and a few words.
const MAX_ANSWER: usize = 1024;

/// Scans `body` with the clamd at `address` (`host:port`).
///
/// # Errors
///
/// [`Unreachable`] when clamd cannot be reached, the file cannot be read, or
/// the answer is not one of the three clamd gives.
pub async fn scan<S, E>(address: &str, mut body: S) -> Result<Verdict, Unreachable>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
    E: std::fmt::Display,
{
    let mut socket =
        tokio::time::timeout(super::worker::CONNECT_TIMEOUT, TcpStream::connect(address))
            .await
            .map_err(|_| Unreachable::Timeout)??;
    socket.write_all(b"zINSTREAM\0").await?;

    let mut refused_mid_stream = false;
    'send: while let Some(piece) = body.next().await {
        let piece = piece.map_err(|error| Unreachable::Store(error.to_string()))?;
        for chunk in piece.chunks(CHUNK) {
            let length = u32::try_from(chunk.len()).unwrap_or(u32::MAX).to_be_bytes();
            if socket.write_all(&length).await.is_err() || socket.write_all(chunk).await.is_err() {
                refused_mid_stream = true;
                break 'send;
            }
        }
    }
    if !refused_mid_stream {
        // The end of the stream. A clamd that has already answered and closed
        // refuses this too, and its answer is still waiting to be read.
        socket.write_all(&0_u32.to_be_bytes()).await.ok();
    }

    let mut answer = Vec::new();
    let mut buffer = [0; 256];
    while answer.len() < MAX_ANSWER {
        let read = socket.read(&mut buffer).await?;
        let Some(received) = buffer.get(..read).filter(|received| !received.is_empty()) else {
            break;
        };
        answer.extend_from_slice(received);
        if answer.contains(&0) {
            break;
        }
    }
    parse(&String::from_utf8_lossy(&answer))
}

/// Reads clamd's one line.
fn parse(answer: &str) -> Result<Verdict, Unreachable> {
    let answer = answer.trim_end_matches(['\0', '\n']).trim();
    if answer.contains("size limit exceeded") {
        return Ok(Verdict::TooLarge);
    }
    // `stream: OK` and `stream: Eicar-Signature FOUND`, with or without the
    // `stream:` prefix depending on the version.
    let verdict = answer.strip_prefix("stream:").unwrap_or(answer).trim();
    if verdict == "OK" {
        return Ok(Verdict::Clean);
    }
    if let Some(threat) = verdict.strip_suffix(" FOUND") {
        return Ok(Verdict::Infected(threat.trim().to_owned()));
    }
    Err(Unreachable::Answer(answer.to_owned()))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::clean("stream: OK\0", Verdict::Clean)]
    #[case::infected(
        "stream: Win.Test.EICAR_HDB-1 FOUND\0",
        Verdict::Infected("Win.Test.EICAR_HDB-1".to_owned())
    )]
    #[case::no_prefix("OK\n", Verdict::Clean)]
    #[case::too_large("INSTREAM size limit exceeded. ERROR\0", Verdict::TooLarge)]
    fn reads_what_clamd_says(#[case] answer: &str, #[case] expected: Verdict) {
        assert_eq!(parse(answer).unwrap(), expected);
    }

    /// An error is not a verdict. Calling it clean would pass a file nobody
    /// looked at.
    #[rstest]
    #[case::error("stream: Can't allocate memory ERROR\0")]
    #[case::nothing("")]
    fn anything_else_is_no_answer(#[case] answer: &str) {
        assert!(parse(answer).is_err());
    }
}
