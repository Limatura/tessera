//! Tessera server: anonymous file courier backend.
//!
//! Everything except process startup lives in this library so that the binaries
//! (`tessera-server`, `tessera-openapi`), the integration tests and the benchmarks
//! all drive exactly the same code paths.
//!
//! Module layout follows capability, not MVC layer:
//!
//! - [`config`]: configuration loading and start-up validation
//! - [`telemetry`]: `tracing` subscriber setup
//! - [`error`]: the crate error type and its HTTP representation
//! - [`db`]: connection pool and migration runner (SQLite, optionally Postgres)
//! - [`storage`]: blob storage abstraction (local filesystem, optionally S3)
//! - [`api`]: route assembly and the OpenAPI document
//! - [`assets`]: the embedded SPA and its fallback routing
//! - [`manifest`]: the web app manifest and the share target it declares
//! - [`state`]: the shared application state handed to every handler
//! - [`topic`]: changes announced inside the process, for event streams
//!
//! Lint policy is declared in `Cargo.toml` under `[lints]` so that it applies to
//! every target in the package, including the build script and the benches.

pub mod api;
pub mod assets;
pub mod auth;
pub mod cli;
pub mod config;
pub mod db;
pub mod error;
pub mod mail;
pub mod maintenance;
pub mod manifest;
#[cfg(feature = "og-images")]
pub mod og;
pub mod scan;
pub mod security;
pub mod settings;
pub mod state;
pub mod storage;
pub mod telemetry;
pub mod topic;
pub mod upload;

/// The domain rules, re-exported so that callers depend on one crate.
///
/// Aliased to `domain`, not re-exported under its own name. Inside this crate
/// `core` collides with the standard library's, and the alias says what the
/// module *is* instead of which package it came from.
pub use tessera_core as domain;

pub use crate::{
    config::Config,
    error::{Error, Result},
    state::AppState,
};

/// Version string reported by `/health` and embedded in the OpenAPI document.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Commit this binary was built from, or `unknown` outside a git checkout.
///
/// Between releases the version number is identical across dozens of builds, so
/// this is what makes a bug report actionable. See `build.rs` for how it is
/// resolved.
pub const BUILD_REV: &str = env!("TESSERA_BUILD_REV");

/// Crate name, used as the default `tracing` target filter prefix.
pub const NAME: &str = env!("CARGO_PKG_NAME");
