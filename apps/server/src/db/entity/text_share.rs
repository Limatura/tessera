//! The `text_shares` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `text_shares`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "text_shares")]
pub struct Model {
    /// The share this text belongs to. One row per share, at most.
    #[sea_orm(primary_key, auto_increment = false)]
    pub share_id: String,
    /// Syntax-highlighting hint.
    pub language: Option<String>,
    /// The text itself.
    ///
    /// A row existing means the content is here. See the migration for why that
    /// is a stronger statement than a nullable column would have been.
    #[sea_orm(column_type = "Blob")]
    pub content: Vec<u8>,
    /// The scanner's verdict, or `None` for a note never scanned. See the
    /// migration on `file_shares`.
    pub scan_state: Option<String>,
    /// The threat for `infected`, or why it was `skipped`.
    pub scan_detail: Option<String>,
}

/// No relations declared: the foreign key is enforced by the database, and
/// nothing here needs SeaORM to join across it.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
