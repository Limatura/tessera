//! The `upload_segments` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `upload_segments`: one backend's share of a file still arriving.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "upload_segments")]
pub struct Model {
    /// The session this piece belongs to.
    #[sea_orm(primary_key, auto_increment = false)]
    pub upload_id: String,
    /// Position in the file, from zero.
    #[sea_orm(primary_key, auto_increment = false)]
    pub seq: i32,
    /// Which configured backend holds it.
    pub backend: String,
    /// The object this piece is written as.
    pub object_key: String,
    /// Where this piece starts within the whole file.
    ///
    /// How much has landed is deliberately not here — `uploads.offset_bytes`
    /// says it for the whole file, and a second copy is a second thing to keep
    /// true.
    pub offset_bytes: i64,
    /// How long this piece will be.
    pub length_bytes: i64,
    /// The store's own multipart identifier. `None` on a local filesystem, and
    /// until the segment is opened.
    pub multipart_id: Option<String>,
    /// `planned`, `open` or `complete` — see
    /// [`State`](super::super::repository::upload::SegmentState).
    pub state: String,
    /// When the segment was planned.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When its state last changed.
    pub updated_at: TimeDateTimeWithTimeZone,
}

/// No relations declared: the foreign key is enforced by the database, and
/// nothing here needs SeaORM to join across it.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
