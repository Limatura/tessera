//! The `file_shares` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `file_shares`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "file_shares")]
pub struct Model {
    /// ULID in text form.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Which share this file belongs to.
    pub share_id: String,
    /// Where it sits in the share, from zero.
    pub position: i64,
    /// The name the uploader gave it.
    pub filename: Option<String>,
    /// Recorded, never echoed; see the migration.
    pub content_type: Option<String>,
    /// `exe`, `elf` or `macho` when the first bytes say it is a program; see
    /// the migration.
    pub detected_type: Option<String>,
    /// The name and media type, sealed, for a file in an encrypted share.
    pub sealed_name: Option<String>,
    /// Size of the content.
    pub size_bytes: i64,
    /// The scanner's verdict, or `None` for content never scanned. See the
    /// migration.
    pub scan_state: Option<String>,
    /// The threat for `infected`, or why it was `skipped`.
    pub scan_detail: Option<String>,
    /// `inline`, `object` or `gone`, carried over from `shares` as it was.
    pub storage: String,
    /// The object key, when the bytes are in the store.
    pub object_key: Option<String>,
    /// When the file was recorded.
    pub created_at: TimeDateTimeWithTimeZone,
}

/// No relations declared: the foreign key is enforced by the database, and
/// nothing here needs SeaORM to join across it.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
