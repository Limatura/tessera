//! Keeping secrets this instance has to be able to read back.
//!
//! One module, because there is one key. [`secret`] seals the handful of
//! settings that must live in the database and must still be usable: a
//! backend's URL, a relay's password, an identity provider's client secret.
//! Everything else is either hashed (passwords, tokens) or kept out of the
//! database entirely.

pub mod secret;
