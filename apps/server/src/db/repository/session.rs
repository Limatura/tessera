//! Reading and writing browser sessions.

use sea_orm::{
    ColumnTrait as _, DatabaseConnection, EntityTrait as _, PaginatorTrait as _, QueryFilter as _,
    QueryOrder as _, Set,
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
    /// Where the session started, when that could be established.
    ///
    /// `None` covers two cases worth keeping apart from a fabricated value: a
    /// session created before this was recorded, and one whose address there
    /// was no honest answer for.
    pub created_ip: Option<String>,
    /// The `User-Agent` it started with, unparsed and truncated.
    pub created_user_agent: Option<String>,
    /// Why it was ended early, if it was.
    pub ended_reason: Option<SessionEnding>,
}

/// Why a session stopped early.
///
/// Stored as text, and only ever read by a person. It decides nothing, since
/// liveness is [`Session::is_valid_at`] alone, and exists so the account holder
/// can tell "I signed out on the laptop" from "I did not do that".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEnding {
    /// The person signed out of this one.
    SignedOut,
    /// The person ended it from their list of sessions.
    Revoked,
    /// A password change ended every session the account had.
    PasswordChanged,
    /// A password reset did, which usually means the account was in trouble.
    PasswordReset,
    /// The account was suspended.
    ///
    /// Its own reason, not `Revoked`, which reads as "this session was ended".
    /// The person looking at the list wants to know their account was
    /// stopped, not their browser.
    AccountDisabled,
}

impl SessionEnding {
    /// The stored spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SignedOut => "signed_out",
            Self::Revoked => "revoked",
            Self::PasswordChanged => "password_changed",
            Self::PasswordReset => "password_reset",
            Self::AccountDisabled => "account_disabled",
        }
    }

    /// Reads one back, or `None` for a spelling this version does not know.
    ///
    /// Unknown, not an error. A row written by a later version that grew
    /// another reason should still list, without a label. Refusing it would
    /// make a downgrade lose the account's history.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "signed_out" => Some(Self::SignedOut),
            "revoked" => Some(Self::Revoked),
            "password_changed" => Some(Self::PasswordChanged),
            "password_reset" => Some(Self::PasswordReset),
            "account_disabled" => Some(Self::AccountDisabled),
            _ => None,
        }
    }
}

/// What is known about the request that started a session.
///
/// A type, not two more arguments, because they are the same fact from two
/// angles and are only ever passed together.
#[derive(Debug, Clone, Default)]
pub struct SessionOrigin {
    /// The caller's address, as resolved through whatever proxies are trusted.
    pub ip: Option<String>,
    /// The `User-Agent` header, as it arrived.
    pub user_agent: Option<String>,
}

/// How much of a `User-Agent` is kept.
///
/// The caller chooses this string, so something has to bound it or a sign-in
/// writes as many bytes into the database as the request body limit allows.
/// Real ones run to about 200 characters, so nothing legitimate is cut.
const MAX_USER_AGENT: usize = 512;

impl SessionOrigin {
    /// Trims the user agent to something the narrowest column holds.
    ///
    /// Counts characters, not bytes, and builds the result instead of
    /// slicing. Not because a header can arrive as arbitrary UTF-8:
    /// `HeaderValue::to_str` has already refused anything outside visible
    /// ASCII, so here characters and bytes are the same number. It is that a
    /// repository takes a `String` and cannot see where it came from, so use
    /// the operation that cannot panic. Collecting costs an allocation cloning
    /// the string would have cost anyway.
    fn bounded_user_agent(&self) -> Option<String> {
        self.user_agent
            .as_ref()
            .map(|agent| agent.chars().take(MAX_USER_AGENT).collect())
    }
}

impl Session {
    /// Whether the session is still valid at `now`.
    ///
    /// Takes the instant instead of reading the clock, so expiry is testable by
    /// passing a timestamp instead of by sleeping.
    #[must_use]
    pub fn is_valid_at(&self, now: OffsetDateTime) -> bool {
        now < self.expires_at
    }

    /// Takes a reference: unlike a token, a session row has nothing to move out
    /// of, since its identifiers are parsed and its timestamps are `Copy`.
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
            created_ip: model.created_ip.clone(),
            created_user_agent: model.created_user_agent.clone(),
            ended_reason: model.ended_reason.as_deref().and_then(SessionEnding::parse),
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
    /// The secret is returned, never stored. Only its digest reaches the
    /// database, so this is the one moment it exists as a usable credential.
    ///
    /// # Errors
    ///
    /// Returns [`SessionRepositoryError`] if the insert fails.
    pub async fn create(
        &self,
        user_id: UserId,
        origin: &SessionOrigin,
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
            created_ip: origin.ip.clone(),
            created_user_agent: origin.bounded_user_agent(),
            ended_reason: None,
        };

        SessionEntity::insert(session::ActiveModel {
            id: Set(session.id.to_string()),
            user_id: Set(session.user_id.to_string()),
            secret_hash: Set(secret.digest()),
            created_at: Set(session.created_at),
            last_seen_at: Set(session.last_seen_at),
            expires_at: Set(session.expires_at),
            created_ip: Set(session.created_ip.clone()),
            created_user_agent: Set(session.created_user_agent.clone()),
            ended_reason: Set(None),
        })
        .exec(&self.connection)
        .await?;

        Ok((session, secret))
    }

    /// Finds the session a secret refers to.
    ///
    /// Looks up by digest, so the secret never appears in a query and the
    /// comparison is the database's index and not ours.
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

    /// Lists an account's sessions, newest first.
    ///
    /// Everything, including the ones that are over. A person reading this is
    /// usually asking whether anyone else has been in, and a session revoked
    /// last Tuesday is exactly what they need to see.
    ///
    /// # Errors
    ///
    /// Returns [`SessionRepositoryError`] if the query fails or a row is not
    /// usable.
    pub async fn find_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<Session>, SessionRepositoryError> {
        let models = SessionEntity::find()
            .filter(session::Column::UserId.eq(user_id.to_string()))
            .order_by_desc(session::Column::CreatedAt)
            .all(&self.connection)
            .await?;
        Ok(models
            .iter()
            .map(Session::from_model)
            .collect::<Result<Vec<_>, _>>()?)
    }

    /// Ends one session, whoever it belongs to.
    ///
    /// # Errors
    ///
    /// Returns [`SessionRepositoryError`] if the update fails.
    pub async fn end(
        &self,
        id: SessionId,
        reason: SessionEnding,
        now: OffsetDateTime,
    ) -> Result<(), SessionRepositoryError> {
        SessionEntity::update_many()
            .col_expr(session::Column::ExpiresAt, now.into())
            .col_expr(session::Column::EndedReason, reason.as_str().into())
            .filter(session::Column::Id.eq(id.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(())
    }
}

/// What ending one session turned out to mean.
///
/// Three outcomes and two answers, by design. Naming another account's session
/// and naming one that is already over both ask for something that is now true,
/// and neither caller is entitled to know which they did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Revocation {
    /// It was live, and now it is not.
    Ended,
    /// It was already over. Nothing was written.
    AlreadyOver,
    /// No session of this account's has that identifier.
    NotYours,
}

impl SessionRepository {
    /// Ends one session, but only if it belongs to this account.
    ///
    /// The owner is in the `WHERE` clause, not checked beforehand. A
    /// check first would answer "not yours" and "does not exist" differently,
    /// turning this into a way to ask who owns an identifier, and it would race
    /// with the row being ended by other means.
    ///
    /// Returns whether a row was ended, so the caller can answer the same way
    /// for a stranger's identifier and one that never existed.
    ///
    /// # Errors
    ///
    /// Returns [`SessionRepositoryError`] if the update fails.
    pub async fn revoke(
        &self,
        id: SessionId,
        user_id: UserId,
        now: OffsetDateTime,
    ) -> Result<Revocation, SessionRepositoryError> {
        let result = SessionEntity::update_many()
            .col_expr(session::Column::ExpiresAt, now.into())
            .col_expr(
                session::Column::EndedReason,
                SessionEnding::Revoked.as_str().into(),
            )
            .filter(session::Column::Id.eq(id.to_string()))
            .filter(session::Column::UserId.eq(user_id.to_string()))
            // Already over, so there is nothing to end, and its ending time
            // must not be moved to now: that timestamp is what the list shows.
            .filter(session::Column::ExpiresAt.gt(now))
            .exec(&self.connection)
            .await?;
        if result.rows_affected == 1 {
            return Ok(Revocation::Ended);
        }

        // Nothing was written. The two reasons are a row of theirs that had
        // already finished and a row that is not theirs, and the caller is told
        // neither. Asking a second time is safe: losing the race means the row
        // is now over, the answer either way.
        let theirs = SessionEntity::find()
            .filter(session::Column::Id.eq(id.to_string()))
            .filter(session::Column::UserId.eq(user_id.to_string()))
            .one(&self.connection)
            .await?;
        Ok(if theirs.is_some() {
            Revocation::AlreadyOver
        } else {
            Revocation::NotYours
        })
    }

    /// Removes every session an account ever had.
    ///
    /// Not the same as ending them. Ending keeps the row, because the session
    /// list is also the login history: where a person signed in from and on
    /// what, the most identifying thing this service stores. Removing the
    /// account and keeping that would leave behind the part that matters most.
    ///
    /// # Errors
    ///
    /// Returns [`SessionRepositoryError`] if the delete fails.
    pub async fn delete_all_for_user(
        &self,
        user_id: UserId,
    ) -> Result<u64, SessionRepositoryError> {
        let result = SessionEntity::delete_many()
            .filter(session::Column::UserId.eq(user_id.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected)
    }

    /// Ends every live session an account has, optionally sparing one.
    ///
    /// `keep` is how "sign out my other devices" leaves the browser doing the
    /// asking signed in. A password change passes `None`, since whoever knew the
    /// old password may be holding a session.
    ///
    /// # Errors
    ///
    /// Returns [`SessionRepositoryError`] if the update fails.
    pub async fn end_all_for_user(
        &self,
        user_id: UserId,
        keep: Option<SessionId>,
        reason: SessionEnding,
        now: OffsetDateTime,
    ) -> Result<u64, SessionRepositoryError> {
        let mut update = SessionEntity::update_many()
            .col_expr(session::Column::ExpiresAt, now.into())
            .col_expr(session::Column::EndedReason, reason.as_str().into())
            .filter(session::Column::UserId.eq(user_id.to_string()))
            .filter(session::Column::ExpiresAt.gt(now));
        if let Some(keep) = keep {
            update = update.filter(session::Column::Id.ne(keep.to_string()));
        }
        let result = update.exec(&self.connection).await?;
        Ok(result.rows_affected)
    }

    /// Removes the row entirely.
    ///
    /// For the one case with no one left to show it to: the account is gone.
    /// Everything else ends a session instead of deleting it.
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

    /// How many rows a cutoff would remove.
    ///
    /// The same filter [`SessionRepository::delete_ended_before`] applies. This
    /// table is also the login history that answers "has anyone else been in
    /// here", so shortening it destroys that answer. Worth a number
    /// beforehand.
    ///
    /// # Errors
    ///
    /// Returns [`SessionRepositoryError`] if the query fails.
    pub async fn count_ended_before(
        &self,
        cutoff: OffsetDateTime,
    ) -> Result<u64, SessionRepositoryError> {
        Ok(SessionEntity::find()
            .filter(session::Column::ExpiresAt.lt(cutoff))
            .count(&self.connection)
            .await?)
    }

    /// Removes sessions that ended longer ago than the history is kept for.
    ///
    /// The same query the expiry sweep used to be, with a different bound: the
    /// rows are the login history now, so they outlive the credential by as
    /// long as an operator wants to look back.
    ///
    /// # Errors
    ///
    /// Returns [`SessionRepositoryError`] if the delete fails.
    pub async fn delete_ended_before(
        &self,
        cutoff: OffsetDateTime,
    ) -> Result<u64, SessionRepositoryError> {
        let result = SessionEntity::delete_many()
            .filter(session::Column::ExpiresAt.lt(cutoff))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected)
    }
}
