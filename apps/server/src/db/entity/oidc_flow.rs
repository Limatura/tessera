//! The `oidc_flows` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `oidc_flows`: a sign-in that has left for the provider and has not
/// come back yet.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "oidc_flows")]
pub struct Model {
    /// The CSRF token. Also the primary key, so a state cannot be written
    /// twice and, once deleted, cannot be replayed.
    #[sea_orm(primary_key, auto_increment = false)]
    pub state: String,
    /// Sent to the provider and expected back inside the token.
    pub nonce: String,
    /// The PKCE secret, which only ever goes to the token endpoint.
    pub verifier: String,
    /// `login` or `link`.
    pub purpose: String,
    /// Whose account a `link` attaches to.
    pub user_id: Option<String>,
    /// An invitation to spend once the sign-in succeeds.
    pub invite_code: Option<String>,
    /// When the browser left.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When this stops being answerable.
    pub expires_at: TimeDateTimeWithTimeZone,
}

/// No relations declared: the foreign key is enforced by the database, and
/// nothing here needs SeaORM to join across it.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
