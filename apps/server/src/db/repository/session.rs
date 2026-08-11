//! Reading and writing browser sessions.

use sea_orm::{
    ActiveValue::Set, ColumnTrait as _, DatabaseConnection, EntityTrait as _, QueryFilter as _,
};
use tessera_core::{SessionId, UserId};
use time::OffsetDateTime;

use crate::{
    auth::secret::Secret,
    db::{
        entity::session::{self, Entity as SessionEntity},
        repository::user::CorruptRow,
    },
};

/// A browser session, in domain terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// Stable identifier.
    pub id: SessionId,
    /// Whose session this is.
    pub user_id: UserId,
    /// When the session began.
    pub created_at: OffsetDateTime,
    /// When it was last presented.
    pub last_seen_at: OffsetDateTime,
    /// When it stops being accepted.
    pub expires_at: OffsetDateTime,
}

impl Session {
    /// Whether the session is still valid at `now`.
    ///
    /// Takes the instant rather than reading the clock, so expiry is testable by
    /// passing a timestamp instead of by sleeping.
    #[must_use]
    pub fn is_valid_at(&self, now: OffsetDateTime) -> bool {
        now < self.expires_at
    }

    /// Takes a reference: unlike a token, a session row has nothing to move out
    /// of — its identifiers are parsed and its timestamps are `Copy`.
    fn from_model(model: &session::Model) -> Result<Self, CorruptRow> {
        let fail = |source| CorruptRow {
            id: model.id.clone(),
            source,
        };
        Ok(Self {
            id: SessionId::parse(&model.id).map_err(fail)?,
            user_id: UserId::parse(&model.user_id).map_err(fail)?,
            created_at: model.created_at,
            last_seen_at: model.last_seen_at,
            expires_at: model.expires_at,
        })
    }
}

/// Anything that can go wrong reading or writing a session.
#[derive(Debug, thiserror::Error)]
pub enum SessionRepositoryError {
    /// The database refused the operation.
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),

    /// A stored row does not satisfy today's rules.
    #[error(transparent)]
    Corrupt(#[from] CorruptRow),
}

/// Sessions, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct SessionRepository {
    connection: DatabaseConnection,
}

impl SessionRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// Starts a session and returns the secret to hand to the browser.
    ///
    /// The secret is returned rather than stored: only its digest reaches the
    /// database, so this is the one moment it exists in a form that could be
    /// presented as a credential.
    ///
    /// # Errors
    ///
    /// Returns [`SessionRepositoryError`] if the insert fails.
    pub async fn create(
        &self,
        user_id: UserId,
        now: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> Result<(Session, Secret), SessionRepositoryError> {
        let secret = Secret::generate_session();
        let session = Session {
            id: SessionId::generate(),
            user_id,
            created_at: now,
            last_seen_at: now,
            expires_at,
        };

        SessionEntity::insert(session::ActiveModel {
            id: Set(session.id.to_string()),
            user_id: Set(session.user_id.to_string()),
            secret_hash: Set(secret.digest()),
            created_at: Set(session.created_at),
            last_seen_at: Set(session.last_seen_at),
            expires_at: Set(session.expires_at),
        })
        .exec(&self.connection)
        .await?;

        Ok((session, secret))
    }

    /// Finds the session a secret refers to.
    ///
    /// Looks up by digest, so the secret itself never appears in a query — and
    /// the comparison is the database's index rather than ours.
    ///
    /// # Errors
    ///
    /// Returns [`SessionRepositoryError`] if the query fails or the row is not
    /// usable.
    pub async fn find_by_secret(
        &self,
        secret: &Secret,
    ) -> Result<Option<Session>, SessionRepositoryError> {
        let model = SessionEntity::find()
            .filter(session::Column::SecretHash.eq(secret.digest()))
            .one(&self.connection)
            .await?;
        Ok(model.as_ref().map(Session::from_model).transpose()?)
    }

    /// Records that a session was used.
    ///
    /// # Errors
    ///
    /// Returns [`SessionRepositoryError`] if the update fails.
    pub async fn touch(
        &self,
        id: SessionId,
        now: OffsetDateTime,
    ) -> Result<(), SessionRepositoryError> {
        SessionEntity::update_many()
            .col_expr(session::Column::LastSeenAt, now.into())
            .filter(session::Column::Id.eq(id.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    /// Ends one session.
    ///
    /// # Errors
    ///
    /// Returns [`SessionRepositoryError`] if the delete fails.
    pub async fn delete(&self, id: SessionId) -> Result<(), SessionRepositoryError> {
        SessionEntity::delete_by_id(id.to_string())
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    /// Ends every session belonging to an account.
    ///
    /// Called after a password change: whoever knew the old password may be
    /// holding a session, and changing the password is how a person says they
    /// want that to stop.
    ///
    /// # Errors
    ///
    /// Returns [`SessionRepositoryError`] if the delete fails.
    pub async fn delete_for_user(&self, user_id: UserId) -> Result<u64, SessionRepositoryError> {
        let result = SessionEntity::delete_many()
            .filter(session::Column::UserId.eq(user_id.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected)
    }

    /// Removes sessions that have expired.
    ///
    /// # Errors
    ///
    /// Returns [`SessionRepositoryError`] if the delete fails.
    pub async fn delete_expired(&self, now: OffsetDateTime) -> Result<u64, SessionRepositoryError> {
        let result = SessionEntity::delete_many()
            .filter(session::Column::ExpiresAt.lt(now))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected)
    }
}
