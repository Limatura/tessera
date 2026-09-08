//! The `user_preferences` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `user_preferences`: one person's choices.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "user_preferences")]
pub struct Model {
    /// Whose choices these are. One row per person, so this is the key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: String,
    /// The appearance choices as JSON, `{}` when none have been made.
    pub appearance: String,
    /// When they last changed.
    pub updated_at: TimeDateTimeWithTimeZone,
}

/// The account the row belongs to.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Deleting the account deletes the row.
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id",
        on_delete = "Cascade"
    )]
    User,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
