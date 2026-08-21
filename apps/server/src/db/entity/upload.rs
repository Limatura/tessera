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
    /// Digest of the secret that lets somebody continue it.
    pub token_hash: String,
    /// The name the uploader gave it.
    pub filename: Option<String>,
    /// Recorded, never echoed — see the migration on `file_shares`.
    pub content_type: Option<String>,
    /// What the client said it would send.
    pub declared_bytes: i64,
    /// How much has landed. The answer to a `HEAD`.
    pub offset_bytes: i64,
    /// Decided at creation and never recomputed — see the migration.
    pub object_key: String,
    /// The store's own multipart identifier, or `None` on a local filesystem.
    pub multipart_id: Option<String>,
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
