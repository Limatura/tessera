//! Writes the OpenAPI contract to disk (or stdout).
//!
//! `openapi.json` at the repository root is generated, not authored. CI runs this
//! binary and fails if the result differs from what is committed, which is what
//! keeps the frontend and the Discord bot clients honest.
//!
//! Usage:
//!
//! ```text
//! cargo run --bin tessera-openapi              # writes ../../openapi.json
//! cargo run --bin tessera-openapi -- path.json # writes path.json
//! cargo run --bin tessera-openapi -- -         # writes stdout
//! ```
//!
//! The crate denies `print_stdout` / `print_stderr` so that the server logs
//! through `tracing` and nothing else. This binary is the exception that proves
//! it: it is a build-time CLI, and its standard streams *are* its interface.
#![expect(
    clippy::print_stderr,
    reason = "build-time CLI: stderr is where it reports what it wrote"
)]

use std::{io::Write as _, path::PathBuf};

use anyhow::Context as _;

/// Repository-root contract, relative to this crate's manifest directory.
const DEFAULT_OUTPUT: &str = "../../openapi.json";

fn main() -> anyhow::Result<()> {
    let target = std::env::args().nth(1);

    let document = serde_json::to_string_pretty(&tessera_server::api::openapi_json())
        .context("could not serialise the OpenAPI document")?;
    // A trailing newline keeps the file POSIX-clean and stops editors and
    // formatters from producing a one-byte diff.
    let document = format!("{document}\n");

    match target.as_deref() {
        Some("-") => {
            let mut stdout = std::io::stdout().lock();
            stdout
                .write_all(document.as_bytes())
                .context("could not write to stdout")?;
            stdout.flush().context("could not flush stdout")?;
        }
        other => {
            let path = other.map_or_else(
                || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_OUTPUT),
                PathBuf::from,
            );
            std::fs::write(&path, &document)
                .with_context(|| format!("could not write {}", path.display()))?;
            eprintln!("wrote {} ({} bytes)", path.display(), document.len());
        }
    }

    Ok(())
}
