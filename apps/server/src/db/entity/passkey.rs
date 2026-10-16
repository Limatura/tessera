//! The `passkeys` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `passkeys`: one registered authenticator credential.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "passkeys")]
pub struct Model {
    /// ULID in text form.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Whose passkey this is.
    pub user_id: String,
    /// SHA-256 of the credential id, which sign-in looks it up by.
    #[sea_orm(unique)]
    pub credential_hash: String,
    /// The credential id, base64url.
    pub credential_id: String,
    /// The COSE public key, base64url.
    pub public_key: String,
    /// The authenticator's signature counter when last seen.
    pub sign_count: i64,
    /// What the owner calls it.
    pub name: String,
    /// Whether it may sync to the owner's other devices.
    pub backup_eligible: bool,
    /// Whether it has.
    pub backed_up: bool,
    /// When it was registered.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When it last signed in.
    pub last_used_at: Option<TimeDateTimeWithTimeZone>,
}

/// No relations declared; the foreign key is enforced by the database.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
