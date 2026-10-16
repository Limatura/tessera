//! The `totp_authenticators` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `totp_authenticators`: one account's authenticator app.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "totp_authenticators")]
pub struct Model {
    /// Owning account. One authenticator per account, so this is the key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: String,
    /// The shared secret, sealed with `security.secret_key`.
    pub secret: String,
    /// When a code from the app was first accepted. `None` while it is still
    /// being set up, and then the account is not asked for a code.
    pub confirmed_at: Option<TimeDateTimeWithTimeZone>,
    /// The time step of the last code accepted, so it cannot be used again.
    pub last_used_step: Option<i64>,
    /// When setting it up began.
    pub created_at: TimeDateTimeWithTimeZone,
}

/// No relations declared; the foreign key is enforced by the database.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
