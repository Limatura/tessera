//! Queries, and the translation between stored rows and domain types.
//!
//! Every module here owns one table: it knows the columns, it knows how to turn
//! them into the types `tessera-core` defines, and it is the only place that
//! does. Code above this layer never sees a `Model`, never sees a `String` that
//! should have been a [`Username`](tessera_core::Username), and never has to
//! wonder whether a value has been validated.

pub mod access_event;
pub mod api_token;
pub mod invite;
pub mod password_reset;
pub mod pending_registration;
pub mod quota;
pub mod role;
pub mod session;
pub mod setting;
pub mod share;
pub(crate) mod total;
pub mod upload;
pub mod user;
