//! SeaORM entities: the shape of each table.
//!
//! Entities live here rather than in `tessera-core` because they carry SeaORM's
//! derives, and letting that crate depend on an ORM would dissolve the boundary
//! the workspace split exists to enforce.
//!
//! The cost is a conversion at the edge — see `db::repository`. It is a real
//! cost, and it is the one being paid deliberately: the domain rules stay
//! testable without a database, and swapping the persistence layer stays a
//! change to one module rather than to every type in the program.

pub mod access_event;
pub mod api_token;
pub mod api_token_permission;
pub mod file_segment;
pub mod file_share;
pub mod invite;
pub mod password_reset_token;
pub mod pending_registration;
pub mod role;
pub mod role_permission;
pub mod session;
pub mod setting;
pub mod share;
pub mod share_ticket;
pub mod storage_backend;
pub mod text_share;
pub mod upload;
pub mod upload_part;
pub mod upload_segment;
pub mod user;
