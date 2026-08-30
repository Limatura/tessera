//! Signing in through an identity provider.
//!
//! The shape of the whole thing is in `docs/oidc.md`. What is here is the
//! machinery: what the provider says about itself, and (as the rest lands) the
//! authorisation-code exchange and what may be believed about the result.
//!
//! # What this decides and what it does not
//!
//! The provider decides **who may sign in**. tessera decides **what they may
//! do**. An account admitted here lands in `auth.default_role`, exactly as one
//! that registered with a password, and is moved by an administrator.

pub mod admit;
pub mod claims;
pub mod discovery;
pub mod flow;
pub mod token;
