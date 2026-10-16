//! Keeping secrets this instance has to be able to read back.
//!
//! One module, because there is one key. [`secret`] seals the few settings that
//! must live in the database and still be usable: a backend URL, a relay
//! password, an OIDC client secret. Everything else is either hashed
//! (passwords, tokens) or kept out of the database.

pub mod secret;
