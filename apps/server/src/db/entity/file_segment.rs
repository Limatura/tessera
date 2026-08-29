//! The `file_segments` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `file_segments`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "file_segments")]
pub struct Model {
    /// Which file this is part of.
    #[sea_orm(primary_key, auto_increment = false)]
    pub file_id: String,
    /// Position in the file, from zero.
    #[sea_orm(primary_key, auto_increment = false)]
    pub seq: i64,
    /// Which configured backend holds it: a name, not a foreign key. See the
    /// migration.
    pub backend: String,
    /// Where the bytes are within that backend.
    pub object_key: String,
    /// Where this segment starts within the whole file.
    pub offset_bytes: i64,
    /// How long it is.
    pub length_bytes: i64,
    /// When it was recorded.
    pub created_at: TimeDateTimeWithTimeZone,
}

/// No relations declared: the foreign key is enforced by the database, and
/// nothing here needs SeaORM to join across it.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
