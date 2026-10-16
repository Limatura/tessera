//! Password reset links, as rows.

use sea_orm::{
    ColumnTrait as _, DatabaseConnection, EntityTrait as _, QueryFilter as _, Set, sea_query::Expr,
};
use tessera_core::{PasswordResetTokenId, UserId};
use time::OffsetDateTime;

use crate::{
    auth::secret::Secret,
    db::{
        entity::password_reset_token::{ActiveModel, Column, Entity as TokenEntity, Model},
        repository::user::{CorruptRow, UserRepositoryError},
    },
};

/// A reset link, as the server knows it.
#[derive(Debug, Clone)]
pub struct PasswordResetToken {
    /// Row identifier.
    pub id: PasswordResetTokenId,
    /// Whose password this would set.
    pub user_id: UserId,
    /// When it stops working.
    pub expires_at: OffsetDateTime,
    /// When it was spent, if it was.
    pub used_at: Option<OffsetDateTime>,
}

impl PasswordResetToken {
    /// Whether this link may still be followed.
    #[must_use]
    pub fn is_usable_at(&self, now: OffsetDateTime) -> bool {
        self.used_at.is_none() && self.expires_at > now
    }

    fn from_model(model: &Model) -> Result<Self, UserRepositoryError> {
        let fail = |source| CorruptRow {
            id: model.id.clone(),
            source,
        };
        Ok(Self {
            id: PasswordResetTokenId::parse(&model.id).map_err(fail)?,
            user_id: UserId::parse(&model.user_id).map_err(fail)?,
            expires_at: model.expires_at,
            used_at: model.used_at,
        })
    }
}

/// Reset links, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct PasswordResetRepository {
    connection: DatabaseConnection,
}

impl PasswordResetRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// Records a link and returns the secret to put in the email.
    ///
    /// The secret is returned, never stored. Only its digest reaches the
    /// database, so this is the one moment it exists in a form anyone can use.
    /// If the mail fails to send, the link becomes unfollowable, by us as much
    /// as by anyone.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the insert fails.
    pub async fn issue(
        &self,
        user_id: UserId,
        now: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> Result<Secret, UserRepositoryError> {
        let secret = Secret::generate_session();
        let row = ActiveModel {
            id: Set(PasswordResetTokenId::generate().to_string()),
            user_id: Set(user_id.to_string()),
            secret_hash: Set(secret.digest()),
            created_at: Set(now),
            expires_at: Set(expires_at),
            used_at: Set(None),
        };
        TokenEntity::insert(row).exec(&self.connection).await?;
        Ok(secret)
    }

    /// Finds the link a secret refers to, used or not.
    ///
    /// Spent and expired links are returned, not filtered out, so the caller
    /// can tell "this link is finished with" from "no such link",
    /// which are different things to report.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query fails.
    pub async fn find_by_secret(
        &self,
        secret: &Secret,
    ) -> Result<Option<PasswordResetToken>, UserRepositoryError> {
        let found = TokenEntity::find()
            .filter(Column::SecretHash.eq(secret.digest()))
            .one(&self.connection)
            .await?;
        found
            .as_ref()
            .map(PasswordResetToken::from_model)
            .transpose()
    }

    /// Marks a link as spent, and only if it was not already.
    ///
    /// Returns whether this call was the one that spent it. The decision is the
    /// database's, so two clicks arriving together produce one winner. Without
    /// that, a forwarded link could be used twice.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the update fails.
    pub async fn spend(
        &self,
        id: PasswordResetTokenId,
        now: OffsetDateTime,
    ) -> Result<bool, UserRepositoryError> {
        let result = TokenEntity::update_many()
            .col_expr(Column::UsedAt, Expr::value(Some(now)))
            .filter(Column::Id.eq(id.to_string()))
            .filter(Column::UsedAt.is_null())
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// Invalidates every outstanding link for an account.
    ///
    /// Called after a successful reset. Without it, a second link sitting in
    /// the same mailbox stays a working key, and the usual reason there are two
    /// is that the first mail went astray, exactly when the older one
    /// is most likely to be in the wrong hands.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the update fails.
    pub async fn spend_all_for(
        &self,
        user_id: UserId,
        now: OffsetDateTime,
    ) -> Result<u64, UserRepositoryError> {
        let result = TokenEntity::update_many()
            .col_expr(Column::UsedAt, Expr::value(Some(now)))
            .filter(Column::UserId.eq(user_id.to_string()))
            .filter(Column::UsedAt.is_null())
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected)
    }

    /// Removes links that are past their expiry.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the delete fails.
    pub async fn delete_expired(&self, now: OffsetDateTime) -> Result<u64, UserRepositoryError> {
        let result = TokenEntity::delete_many()
            .filter(Column::ExpiresAt.lt(now))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected)
    }
}
