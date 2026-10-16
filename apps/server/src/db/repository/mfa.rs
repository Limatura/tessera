//! The second sign-in step, as rows: an account's authenticator, its recovery
//! codes, and the sign-ins waiting on either.
//!
//! One repository for three tables, because they are one feature and each
//! change touches more than one of them: confirming an authenticator issues
//! the codes, and removing it takes them away.

use sea_orm::{
    ActiveModelTrait as _, ColumnTrait as _, Condition, DatabaseConnection, EntityTrait as _,
    PaginatorTrait as _, QueryFilter as _, QuerySelect as _, Set, TransactionTrait as _,
    sea_query::Expr,
};
use tessera_core::UserId;
use time::OffsetDateTime;

use crate::{
    auth::{
        mfa::{TotpSecret, normalise_recovery_code},
        secret::Secret,
    },
    db::entity::{mfa_challenge, recovery_code, totp_authenticator},
    security::secret::{Sealer, SecretError},
};

/// Anything that can go wrong reading or writing these tables.
#[derive(Debug, thiserror::Error)]
pub enum MfaRepositoryError {
    /// The database refused the operation.
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),

    /// The secret could not be sealed or opened.
    #[error(transparent)]
    Secret(#[from] SecretError),

    /// A stored secret opened but is not base32, or a stored id is not one.
    #[error("the stored second-step row for `{0}` is not usable")]
    Corrupt(String),
}

/// An account's authenticator, as the server knows it.
#[derive(Debug, Clone)]
pub struct Authenticator {
    /// The shared secret, opened.
    pub secret: TotpSecret,
    /// Whether a code from it has been accepted. Until then the account is not
    /// asked for one.
    pub confirmed: bool,
}

/// The second-step tables, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct MfaRepository {
    connection: DatabaseConnection,
    sealer: Sealer,
}

impl MfaRepository {
    /// Wraps a connection, and the key the secrets are sealed with.
    #[must_use]
    pub const fn new(connection: DatabaseConnection, sealer: Sealer) -> Self {
        Self { connection, sealer }
    }

    /// The account's authenticator, confirmed or still being set up.
    ///
    /// # Errors
    ///
    /// Returns [`MfaRepositoryError`] if the query fails or the secret will not
    /// open.
    pub async fn authenticator(
        &self,
        user: UserId,
    ) -> Result<Option<Authenticator>, MfaRepositoryError> {
        let Some(row) = totp_authenticator::Entity::find_by_id(user.to_string())
            .one(&self.connection)
            .await?
        else {
            return Ok(None);
        };
        let opened = self.sealer.open(&row.secret)?;
        let secret = TotpSecret::from_base32(&opened)
            .ok_or_else(|| MfaRepositoryError::Corrupt(row.user_id))?;
        Ok(Some(Authenticator {
            secret,
            confirmed: row.confirmed_at.is_some(),
        }))
    }

    /// Whether the account is asked for a second step at sign-in.
    ///
    /// # Errors
    ///
    /// Returns [`MfaRepositoryError`] if the query fails.
    pub async fn is_enabled(&self, user: UserId) -> Result<bool, MfaRepositoryError> {
        let found = totp_authenticator::Entity::find()
            .filter(totp_authenticator::Column::UserId.eq(user.to_string()))
            .filter(totp_authenticator::Column::ConfirmedAt.is_not_null())
            .count(&self.connection)
            .await?;
        Ok(found > 0)
    }

    /// Which of these accounts are asked for a second step, in one query, for
    /// a page of accounts.
    ///
    /// # Errors
    ///
    /// Returns [`MfaRepositoryError`] if the query fails.
    pub async fn with_mfa(
        &self,
        users: &[UserId],
    ) -> Result<std::collections::HashSet<UserId>, MfaRepositoryError> {
        let rows: Vec<String> = totp_authenticator::Entity::find()
            .select_only()
            .column(totp_authenticator::Column::UserId)
            .filter(totp_authenticator::Column::UserId.is_in(users.iter().map(ToString::to_string)))
            .filter(totp_authenticator::Column::ConfirmedAt.is_not_null())
            .into_tuple()
            .all(&self.connection)
            .await?;
        Ok(rows
            .iter()
            .filter_map(|id| UserId::parse(id).ok())
            .collect())
    }

    /// Starts setting up an authenticator, replacing one that was started and
    /// never confirmed.
    ///
    /// Returns `false`, writing nothing, when the account already has a
    /// confirmed one: replacing that would turn the second step off for as long
    /// as the new one goes unconfirmed, without the owner having said so.
    ///
    /// # Errors
    ///
    /// Returns [`MfaRepositoryError`] if a write fails or there is no key to
    /// seal with.
    pub async fn begin(
        &self,
        user: UserId,
        secret: &TotpSecret,
        now: OffsetDateTime,
    ) -> Result<bool, MfaRepositoryError> {
        let sealed = self.sealer.seal(&secret.to_base32())?;
        let transaction = self.connection.begin().await?;
        let existing = totp_authenticator::Entity::find_by_id(user.to_string())
            .one(&transaction)
            .await?;
        if existing
            .as_ref()
            .is_some_and(|row| row.confirmed_at.is_some())
        {
            return Ok(false);
        }
        if existing.is_some() {
            totp_authenticator::Entity::delete_by_id(user.to_string())
                .exec(&transaction)
                .await?;
        }
        totp_authenticator::ActiveModel {
            user_id: Set(user.to_string()),
            secret: Set(sealed),
            confirmed_at: Set(None),
            last_used_step: Set(None),
            created_at: Set(now),
        }
        .insert(&transaction)
        .await?;
        transaction.commit().await?;
        Ok(true)
    }

    /// Confirms the authenticator being set up, and issues its recovery codes.
    ///
    /// `step` is the one the confirming code came from, recorded so it cannot
    /// sign in as well. Returns `false`, writing nothing, when there was nothing
    /// unconfirmed to confirm.
    ///
    /// # Errors
    ///
    /// Returns [`MfaRepositoryError`] if a write fails.
    pub async fn confirm(
        &self,
        user: UserId,
        step: i64,
        recovery_codes: &[String],
        now: OffsetDateTime,
    ) -> Result<bool, MfaRepositoryError> {
        let transaction = self.connection.begin().await?;
        let result = totp_authenticator::Entity::update_many()
            .col_expr(
                totp_authenticator::Column::ConfirmedAt,
                Expr::value(Some(now)),
            )
            .col_expr(
                totp_authenticator::Column::LastUsedStep,
                Expr::value(Some(step)),
            )
            .filter(totp_authenticator::Column::UserId.eq(user.to_string()))
            .filter(totp_authenticator::Column::ConfirmedAt.is_null())
            .exec(&transaction)
            .await?;
        if result.rows_affected != 1 {
            return Ok(false);
        }
        Self::write_recovery_codes(&transaction, user, recovery_codes, now).await?;
        transaction.commit().await?;
        Ok(true)
    }

    /// Records that a code from `step` was used, if no code from that step or a
    /// later one has been.
    ///
    /// Returns whether this call recorded it. The database decides, so the same
    /// code sent twice at once signs in once.
    ///
    /// # Errors
    ///
    /// Returns [`MfaRepositoryError`] if the update fails.
    pub async fn use_step(&self, user: UserId, step: i64) -> Result<bool, MfaRepositoryError> {
        let result = totp_authenticator::Entity::update_many()
            .col_expr(
                totp_authenticator::Column::LastUsedStep,
                Expr::value(Some(step)),
            )
            .filter(totp_authenticator::Column::UserId.eq(user.to_string()))
            .filter(totp_authenticator::Column::ConfirmedAt.is_not_null())
            .filter(
                Condition::any()
                    .add(totp_authenticator::Column::LastUsedStep.is_null())
                    .add(totp_authenticator::Column::LastUsedStep.lt(step)),
            )
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// Replaces the account's recovery codes with a new set.
    ///
    /// # Errors
    ///
    /// Returns [`MfaRepositoryError`] if a write fails.
    pub async fn replace_recovery_codes(
        &self,
        user: UserId,
        codes: &[String],
        now: OffsetDateTime,
    ) -> Result<(), MfaRepositoryError> {
        let transaction = self.connection.begin().await?;
        Self::write_recovery_codes(&transaction, user, codes, now).await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn write_recovery_codes(
        connection: &impl sea_orm::ConnectionTrait,
        user: UserId,
        codes: &[String],
        now: OffsetDateTime,
    ) -> Result<(), MfaRepositoryError> {
        recovery_code::Entity::delete_many()
            .filter(recovery_code::Column::UserId.eq(user.to_string()))
            .exec(connection)
            .await?;
        let rows = codes.iter().map(|code| recovery_code::ActiveModel {
            id: Set(ulid::Ulid::generate().to_string()),
            user_id: Set(user.to_string()),
            code_hash: Set(recovery_digest(code)),
            created_at: Set(now),
            used_at: Set(None),
        });
        recovery_code::Entity::insert_many(rows)
            .exec_without_returning(connection)
            .await?;
        Ok(())
    }

    /// Uses up one recovery code, if it is the account's and unused.
    ///
    /// Returns whether this call used it. The database decides, so a code sent
    /// twice at once works once.
    ///
    /// # Errors
    ///
    /// Returns [`MfaRepositoryError`] if the update fails.
    pub async fn spend_recovery_code(
        &self,
        user: UserId,
        code: &str,
        now: OffsetDateTime,
    ) -> Result<bool, MfaRepositoryError> {
        let result = recovery_code::Entity::update_many()
            .col_expr(recovery_code::Column::UsedAt, Expr::value(Some(now)))
            .filter(recovery_code::Column::UserId.eq(user.to_string()))
            .filter(recovery_code::Column::CodeHash.eq(recovery_digest(code)))
            .filter(recovery_code::Column::UsedAt.is_null())
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// How many unused recovery codes the account has.
    ///
    /// # Errors
    ///
    /// Returns [`MfaRepositoryError`] if the query fails.
    pub async fn recovery_codes_left(&self, user: UserId) -> Result<u64, MfaRepositoryError> {
        Ok(recovery_code::Entity::find()
            .filter(recovery_code::Column::UserId.eq(user.to_string()))
            .filter(recovery_code::Column::UsedAt.is_null())
            .count(&self.connection)
            .await?)
    }

    /// Removes the account's authenticator, its recovery codes, and any sign-in
    /// waiting on them.
    ///
    /// Returns whether there was an authenticator, confirmed or not.
    ///
    /// # Errors
    ///
    /// Returns [`MfaRepositoryError`] if a delete fails.
    pub async fn remove(&self, user: UserId) -> Result<bool, MfaRepositoryError> {
        let transaction = self.connection.begin().await?;
        let removed = totp_authenticator::Entity::delete_by_id(user.to_string())
            .exec(&transaction)
            .await?;
        recovery_code::Entity::delete_many()
            .filter(recovery_code::Column::UserId.eq(user.to_string()))
            .exec(&transaction)
            .await?;
        mfa_challenge::Entity::delete_many()
            .filter(mfa_challenge::Column::UserId.eq(user.to_string()))
            .exec(&transaction)
            .await?;
        transaction.commit().await?;
        Ok(removed.rows_affected == 1)
    }

    /// Records a sign-in that has passed its password, and returns the ticket
    /// that finishes it.
    ///
    /// Returned, never stored: only its digest is saved.
    ///
    /// # Errors
    ///
    /// Returns [`MfaRepositoryError`] if the insert fails.
    pub async fn open_challenge(
        &self,
        user: UserId,
        now: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> Result<Secret, MfaRepositoryError> {
        let ticket = Secret::generate_session();
        mfa_challenge::ActiveModel {
            secret_hash: Set(ticket.digest()),
            user_id: Set(user.to_string()),
            created_at: Set(now),
            expires_at: Set(expires_at),
        }
        .insert(&self.connection)
        .await?;
        Ok(ticket)
    }

    /// Whose sign-in a ticket continues, while it is still good.
    ///
    /// # Errors
    ///
    /// Returns [`MfaRepositoryError`] if the query fails or the row is corrupt.
    pub async fn challenge(
        &self,
        ticket: &Secret,
        now: OffsetDateTime,
    ) -> Result<Option<UserId>, MfaRepositoryError> {
        let found = mfa_challenge::Entity::find_by_id(ticket.digest())
            .filter(mfa_challenge::Column::ExpiresAt.gt(now))
            .one(&self.connection)
            .await?;
        found
            .map(|row| {
                UserId::parse(&row.user_id).map_err(|_| MfaRepositoryError::Corrupt(row.user_id))
            })
            .transpose()
    }

    /// Ends a waiting sign-in, because it finished.
    ///
    /// Returns whether this call ended it, so two requests finishing the same
    /// ticket at once start one session.
    ///
    /// # Errors
    ///
    /// Returns [`MfaRepositoryError`] if the delete fails.
    pub async fn close_challenge(&self, ticket: &Secret) -> Result<bool, MfaRepositoryError> {
        let result = mfa_challenge::Entity::delete_by_id(ticket.digest())
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// Removes sign-ins that were never finished.
    ///
    /// # Errors
    ///
    /// Returns [`MfaRepositoryError`] if the delete fails.
    pub async fn delete_expired_challenges(
        &self,
        now: OffsetDateTime,
    ) -> Result<u64, MfaRepositoryError> {
        let result = mfa_challenge::Entity::delete_many()
            .filter(mfa_challenge::Column::ExpiresAt.lte(now))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected)
    }
}

/// What a recovery code is stored as.
fn recovery_digest(code: &str) -> String {
    Secret::from_client(normalise_recovery_code(code)).digest()
}
