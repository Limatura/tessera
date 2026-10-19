//! `tess`: send and fetch shares from a terminal.
//!
//! A client of the same HTTP API the web page uses, and of the same encryption
//! format: `tessera-crypto`, natively here and as WebAssembly there. A file one
//! of them sealed, the other opens.
//!
//! The binary in `main.rs` only reads arguments and prompts; everything that
//! talks to a server is here, so the tests can drive it against a real one.

pub mod client;
pub mod config;
pub mod get;
pub mod link;
pub mod login;
pub mod names;
pub mod output;
pub mod resume;
pub mod send;
pub mod shares;
pub mod style;
pub mod ui;

/// Everything that can stop a command.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Something about the command as typed.
    #[error("{0}")]
    Usage(String),
    /// The share has a password and none was given. The binary asks for one
    /// and tries again.
    #[error("this share needs a password")]
    PasswordRequired,
    /// The server refused, and said why.
    #[error("{message}")]
    Server {
        /// The HTTP status.
        status: u16,
        /// The server's machine-readable reason, when it gave one.
        code: Option<String>,
        /// What the server said, or the status's own name when it said nothing.
        message: String,
    },
    /// The server answered with something this client does not understand.
    #[error("unexpected answer from the server: {0}")]
    Protocol(String),
    /// The request did not get an answer.
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    /// Reading or writing a local file.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The config file does not read.
    #[error("{path}: {message}")]
    Config {
        /// Where it is.
        path: std::path::PathBuf,
        /// What is wrong with it.
        message: String,
    },
    /// Sealing or opening.
    #[error(transparent)]
    Crypto(#[from] tessera_crypto::Error),
}

/// What every command returns.
pub type Result<T, E = Error> = std::result::Result<T, E>;
