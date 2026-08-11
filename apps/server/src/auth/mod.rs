//! Authentication.
//!
//! Two ways to prove who you are, deliberately kept apart:
//!
//! - **A session cookie**, for browsers. `HttpOnly`, so an XSS cannot read it; opaque, so it means
//!   nothing without the row it points at.
//! - **A bearer token**, for scripts, the CLI, ShareX and the Discord bot.
//!
//! They are separate mechanisms rather than one token type used two ways. Merging
//! them is a common shortcut and a bad one: a browser credential wants to be
//! short-lived, invisible to JavaScript and revoked on logout, while an
//! automation credential wants to be long-lived, pasted into a config file and
//! revoked individually. One type serving both ends up too weak for the first
//! job or too awkward for the second.
//!
//! What both share is that the database stores only a digest. A stolen dump
//! contains nothing that can be presented as a credential.

pub mod bootstrap;
pub mod cookie;
pub mod extract;
pub mod password;
pub mod secret;
pub mod throttle;
