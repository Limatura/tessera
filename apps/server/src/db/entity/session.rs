//! The `sessions` table, as SeaORM sees it.

use sea_orm::entity::prelude::*;

/// A row of `sessions`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "sessions")]
pub struct Model {
    /// ULID in text form.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Owning account.
    pub user_id: String,
    /// SHA-256 of the session secret. Never the secret itself.
    ///
    /// Kept even after the session ends. It is not a credential, since nothing
    /// can be derived from it, and `expires_at` is what refuses the session.
    #[sea_orm(unique)]
    pub secret_hash: String,
    /// When the session began.
    pub created_at: TimeDateTimeWithTimeZone,
    /// When it was last presented, so idle sessions can be recognised.
    pub last_seen_at: TimeDateTimeWithTimeZone,
    /// When it stops being accepted.
    pub expires_at: TimeDateTimeWithTimeZone,
    /// The address the session was started from, if there was one worth
    /// recording. Written once; see the migration for why it is not updated.
    pub created_ip: Option<String>,
    /// The `User-Agent` the session was started with, as it arrived.
    pub created_user_agent: Option<String>,
    /// Which way of signing in started this session: `password` or `oidc`.
    ///
    /// A provider can disable somebody, and a session that outlives that is
    /// what single sign-on exists to prevent. What can be done about it depends
    /// on what the provider supports, so the session has to know where it came
    /// from.
    pub origin: String,
    /// Why this session was ended early, if it was.
    ///
    /// `None` means it simply ran out. See the migration for why this decides
    /// nothing: liveness is `expires_at` alone.
    pub ended_reason: Option<String>,
}

/// No relations declared: the foreign key is enforced by the database, and
/// nothing here needs SeaORM to join across it.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
