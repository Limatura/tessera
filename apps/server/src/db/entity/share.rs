//! The `shares` table, as SeaORM sees it.
//!
//! Deliberately narrow: this is read on every public request, so the content
//! bytes live in `share_contents` (or the blob store) rather than here. See the
//! migration SQL for why the split is by size rather than by kind.

use sea_orm::entity::prelude::*;

/// A row of `shares`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "shares")]
pub struct Model {
    /// ULID in text form.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// The public retrieval code, as it appears in `/s/{code}`.
    #[sea_orm(unique)]
    pub code: String,
    /// Owning account, or `None` for an anonymous upload.
    pub user_id: Option<String>,
    /// `text` or `file`.
    pub kind: String,

    /// Argon2 hash of the share's password, or `None` if it has none.
    pub password_hash: Option<String>,
    /// How many times it may be handed out, or `None` for unlimited.
    pub use_limit: Option<i32>,
    /// How many times it has been.
    pub use_count: i32,
    /// When it stops being served, or `None` to live until deleted.
    pub expires_at: Option<TimeDateTimeWithTimeZone>,
    /// When an administrator switched it off.
    pub disabled_at: Option<TimeDateTimeWithTimeZone>,

    /// Display name, shown on the share page and in the OG image.
    pub title: Option<String>,
    /// Original file name, for `Content-Disposition`.
    pub filename: Option<String>,
    /// The media type the uploader claimed. Recorded, never echoed.
    pub content_type: Option<String>,
    /// Syntax-highlighting hint for a text share.
    pub language: Option<String>,
    /// Size of the content in bytes.
    pub size_bytes: i64,

    /// `inline` or `object`.
    pub storage: String,
    /// Key in the blob store, when `storage` is `object`.
    pub object_key: Option<String>,

    /// JSON describing how to draw the OG image.
    pub og_config: Option<String>,
    /// SHA-256 of the token that lets an anonymous uploader manage this share.
    pub management_token_hash: Option<String>,
    /// How many times the share page was looked at. Statistics only — the number
    /// that gates access is `use_count`.
    pub view_count: i32,
    /// How many times the content was copied.
    pub copy_count: i32,
    /// When the share was last touched, used to decide what is worth keeping.
    pub last_accessed_at: Option<TimeDateTimeWithTimeZone>,

    /// When the share was created.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When it last changed.
    pub updated_at: TimeDateTimeWithTimeZone,
}

/// No relations declared; the foreign keys are enforced by the database.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
