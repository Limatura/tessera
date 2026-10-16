//! The `storage_backends` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `storage_backends`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "storage_backends")]
pub struct Model {
    /// What `file_segments.backend` names. Stable, because rows point at it by
    /// value; see the migration.
    #[sea_orm(primary_key, auto_increment = false)]
    pub name: String,
    /// Sealed. See `security::secret`.
    pub url: String,
    /// Sealed JSON object of backend-specific options.
    pub options: String,
    /// Whether new uploads go here.
    pub is_default: bool,
    /// How much it may hold, or `None` for no limit. Typed in; see the
    /// migration for why it cannot be queried.
    pub capacity_bytes: Option<i64>,
    /// Whether it still takes new content. Reads are unaffected.
    pub read_only: bool,
    /// Whether browsers may be redirected to it directly. See the migration.
    pub direct_access: bool,
    /// When it was added.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When it last changed.
    pub updated_at: TimeDateTimeWithTimeZone,
}

/// No relations declared. Nothing has a foreign key to this table, by design.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
