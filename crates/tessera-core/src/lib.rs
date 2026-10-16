//! Tessera domain rules.
//!
//! # What this crate is for
//!
//! Everything that is true about the domain regardless of how it is stored or
//! served: what a username may look like, what makes a password acceptable, when
//! a share stops being retrievable.
//!
//! The pattern throughout is the **parsed newtype**: a value is checked once, at
//! the edge, and every function downstream takes a type that could not have been
//! constructed from something invalid. [`Username`] cannot hold `alice@bob`;
//! [`EmailAddress`] cannot hold a value with two `@`s. Validation stops being
//! something each call site must remember and becomes something the compiler
//! already knows.
//!
//! # What may not go here
//!
//! No HTTP, no SQL, no filesystem, no `async`. The rule is enforced by
//! `Cargo.toml` and not by review: this crate does not depend on `axum`,
//! `sea-orm`, `object_store` or `tokio`, so code reaching for them does not
//! compile. CI additionally walks the resolved dependency tree, which catches a
//! banned crate arriving transitively.
//!
//! Two consequences worth stating, because they are the point:
//!
//! - **Tests need nothing.** No runtime, no database, no fixtures. The rules are property-tested by
//!   the hundred in milliseconds.
//! - **Time is an argument, never a call.** Rules that depend on the clock take `now` as a
//!   parameter instead of reading it, so expiry behaviour is tested by passing a timestamp rather
//!   than by sleeping.
//!
//! Persistence traits live in `tessera-server`, not here. A
//! repository is inherently `async`, and importing `async` into this crate to
//! declare one would cost the guarantee above while buying little: the
//! swappability such a port usually provides is already supplied by SeaORM.

mod account_state;
mod appearance;
mod code;
mod email;
mod error;
mod id;
mod password;
mod permission;
mod username;

pub use crate::{
    account_state::{AccountState, tombstone},
    appearance::{Accent, Appearance, Mode, Theme},
    code::RetrievalCode,
    email::EmailAddress,
    error::DomainError,
    id::{
        AccessEventId, ApiTokenId, EmailVerificationId, InviteId, PasswordResetTokenId,
        PendingRegistrationId, RoleId, SessionId, ShareId, ShareTicketId, UploadId, UserId,
    },
    password::{Password, PasswordPolicy},
    permission::{Permission, PermissionSet},
    username::Username,
};

/// Version of the domain rules, for diagnostics.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
