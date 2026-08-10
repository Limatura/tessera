//! The `share_contents` table, as SeaORM sees it.
//!
//! One row per share small enough to keep in the database. Larger shares carry
//! an `object_key` on `shares` and have no row here — which is why this is a
//! separate table rather than a nullable column: a blob nobody asked for should
//! not be pulled into memory by a query that only wanted the expiry date.

use sea_orm::entity::prelude::*;

/// A row of `share_contents`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "share_contents")]
pub struct Model {
    /// The share these bytes belong to.
    #[sea_orm(primary_key, auto_increment = false)]
    pub share_id: String,
    /// The content itself. Bytes rather than text: a "text" share is whatever
    /// the uploader sent, and deciding it must be UTF-8 at the storage layer
    /// would reject files that display perfectly well.
    pub content: Vec<u8>,
}

/// No relations declared; the foreign key is enforced by the database.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
