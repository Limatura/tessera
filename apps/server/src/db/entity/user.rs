//! The `users` table, as SeaORM sees it.
//!
//! This is a *storage* shape, not a domain shape. It holds `String`s where the
//! domain holds [`Username`](tessera_core::Username) and
//! [`EmailAddress`](tessera_core::EmailAddress), because that is what a column
//! is, and because a row read back from a database written by an older version
//! might not satisfy today's rules.
//!
//! Crossing between the two is
//! [`User::from_model`](crate::db::repository::user::User::from_model), and it
//! is fallible by design. See `db::repository::user` for what that buys.

use sea_orm::entity::prelude::*;

/// A row of `users`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    /// ULID in its 26-character text form. Not auto-incrementing: the domain
    /// mints identifiers so they can be known before the row exists.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Already lowercased by the domain before it ever reaches here.
    #[sea_orm(unique)]
    pub username: String,
    /// Domain half already lowercased; local part left exactly as typed.
    #[sea_orm(unique)]
    pub email: String,
    /// PHC-format Argon2 string. Never a plaintext password.
    pub password_hash: String,
    /// Which role the account is in, or `None` for one in no role: signed in
    /// and allowed nothing: a real state, not a fault.
    pub role_id: Option<String>,
    /// How much storage this account may hold, or `None` to ask its role.
    /// `Some(0)` is no ceiling.
    pub quota_bytes: Option<i64>,
    /// Largest single file this account may send, or `None` to ask its role.
    /// `Some(0)` is no limit of its own.
    pub max_file_bytes: Option<i64>,
    /// Set on the generated first-run administrator, so the printed password
    /// cannot become a permanent one.
    pub must_change_password: bool,
    /// When the address was proven reachable, if it ever was.
    /// `active`, `disabled` or `deleted`.
    pub state: String,
    /// When the account was removed, if it was.
    pub deleted_at: Option<TimeDateTimeWithTimeZone>,
    /// Who removed it, or the account itself.
    pub deleted_by: Option<String>,
    /// When the address was proven reachable, if it ever was.
    pub email_verified_at: Option<TimeDateTimeWithTimeZone>,
    /// When the account was created.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When the row last changed.
    pub updated_at: TimeDateTimeWithTimeZone,
}

/// No relations yet; sessions and tokens arrive with the endpoints that use them.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
