//! The `uploads` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `uploads`: a file still arriving.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "uploads")]
pub struct Model {
    /// ULID in text form.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// The share to append to, or `None` to create one on completion.
    pub share_id: Option<String>,
    /// Whose upload it is, or `None` for an anonymous one holding the token.
    pub user_id: Option<String>,
    /// The role the uploader was in when the session opened. Mirrors
    /// `shares.role_id`: the reserved bytes are charged to the role at the
    /// time, not to whatever the account is in now.
    pub role_id: Option<String>,
    /// Digest of the secret that lets a caller continue it.
    pub token_hash: String,
    /// The name the uploader gave it.
    pub filename: Option<String>,
    /// Recorded, never echoed; see the migration on `file_shares`.
    pub content_type: Option<String>,
    /// What the share this creates will be sealed as: `link`, `password`, or
    /// `None` for a share the server can read.
    pub encryption: Option<String>,
    /// The salt a password-derived key is stretched with.
    pub key_salt: Option<String>,
    /// The name and media type, sealed, for a file in an encrypted share.
    pub sealed_name: Option<String>,
    /// What the client said it would send.
    pub declared_bytes: i64,
    /// How much has landed. The answer to a `HEAD`.
    pub offset_bytes: i64,
    // Where the bytes are going is `upload_segments`, one row per backend the
    // file is spread over. It used to be three columns here, which could say
    // "this session is writing to that bucket" and could not say "the first
    // backend is finished and the second has not been opened yet".
    /// When the session was opened.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When it last received a byte.
    pub updated_at: TimeDateTimeWithTimeZone,
    /// When the sweep may remove it.
    pub expires_at: TimeDateTimeWithTimeZone,
}

/// No relations declared: the foreign key is enforced by the database, and
/// nothing here needs SeaORM to join across it.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
