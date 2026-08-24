//! The `shares` table, as SeaORM sees it.
//!
//! Deliberately narrow: this is read on every public request, so what a share
//! holds lives in `text_shares` and `file_shares` rather than here. `kind` says
//! which of the two to read.

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
    /// Which role uploaded it, as it stood at the time.
    ///
    /// `None` on a share that predates the column, or one whose role has since
    /// been removed. Both mean "the instance-wide ceiling decides".
    pub role_id: Option<String>,
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

    /// JSON describing how to draw the OG image.
    pub og_config: Option<String>,
    /// SHA-256 of the token that lets an anonymous uploader manage this share.
    pub management_token_hash: Option<String>,
    /// How many times the share page was looked at. Statistics only — the number
    /// that gates access is `use_count`.
    pub view_count: i32,
    /// How many times the content was copied.
    pub copy_count: i32,
    /// How many bytes this share has sent out. See the migration.
    pub bytes_served: i64,
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
