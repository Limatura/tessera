//! The `device_authorizations` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `device_authorizations`: a program waiting to be approved.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "device_authorizations")]
pub struct Model {
    /// SHA-256 of the device code. The program holds the only copy.
    #[sea_orm(primary_key, auto_increment = false)]
    pub device_code_hash: String,
    /// What the person types on the site, without the dash.
    #[sea_orm(unique)]
    pub user_code: String,
    /// What the program says it is, shown on the approval page.
    pub client_name: String,
    /// The address the request came from, also shown there.
    pub requested_from: String,
    /// Who approved or denied it.
    pub user_id: Option<String>,
    /// What the token will be allowed, space-separated, once approved.
    pub permissions: Option<String>,
    /// When it was approved.
    pub approved_at: Option<TimeDateTimeWithTimeZone>,
    /// When it was denied.
    pub denied_at: Option<TimeDateTimeWithTimeZone>,
    /// When the program asked.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When the codes stop working.
    pub expires_at: TimeDateTimeWithTimeZone,
}

/// No relations declared; the foreign key is enforced by the database.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
