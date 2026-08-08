//! Tessera domain rules.
//!
//! # What this crate is for
//!
//! Everything that is true about a share regardless of how it is stored or
//! served: what a retrieval code may look like, when a share stops being
//! retrievable, what "expired" means when the clock and the download counter
//! disagree.
//!
//! # What may not go here
//!
//! No HTTP, no SQL, no filesystem, no `async`. The rule is enforced by
//! `Cargo.toml` rather than by review — this crate does not depend on `axum`,
//! `sea-orm`, `object_store` or `tokio`, so code that reaches for them does not
//! compile. CI additionally walks the resolved dependency tree, which catches a
//! banned crate arriving transitively.
//!
//! Two consequences worth stating, because they are the point:
//!
//! - **Tests need nothing.** No runtime, no database, no fixtures. Rules get property-tested by the
//!   hundred in milliseconds.
//! - **Time is an argument, never a call.** Domain functions take `now: OffsetDateTime` instead of
//!   reading the clock, so expiry behaviour is tested by passing a timestamp rather than by
//!   sleeping.
//!
//! # Current state
//!
//! Deliberately almost empty. The domain types (`RetrievalCode`, `ShareId`,
//! `Share`, the retrievability rules) land in the next round; this crate exists
//! now because the boundary is cheap to create before there is anything to move
//! across it, and expensive afterwards. Merging it back into `tessera-server`
//! later is a file move; splitting it out later is not.

/// Version of the domain rules, for diagnostics.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    /// Guards the promise in the module docs. If this test ever needs `#[tokio::test]`,
    /// an `await`, or a fixture, something that belongs in `tessera-server` has
    /// been put here instead.
    #[test]
    fn the_crate_is_testable_without_a_runtime() {
        assert!(!super::VERSION.is_empty());
    }
}
