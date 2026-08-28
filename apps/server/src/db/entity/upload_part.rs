//! The `upload_parts` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `upload_parts`: one part the store has already taken.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "upload_parts")]
pub struct Model {
    /// Which upload this part belongs to.
    #[sea_orm(primary_key, auto_increment = false)]
    pub upload_id: String,
    /// Which segment of the file it belongs to.
    ///
    /// Part numbers restart at one for every multipart upload, and a file spread
    /// over two backends is two of them — so the number alone does not identify
    /// a part.
    #[sea_orm(primary_key, auto_increment = false)]
    pub seq: i32,
    /// Its position, from one — what S3 calls the part number.
    #[sea_orm(primary_key, auto_increment = false)]
    pub part_number: i32,
    /// The store's identifier for the part, its `ETag` on S3.
    pub content_id: String,
}

/// No relations declared: the foreign key is enforced by the database, and
/// nothing here needs SeaORM to join across it.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
