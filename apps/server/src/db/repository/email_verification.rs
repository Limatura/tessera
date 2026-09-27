//! Links that prove an account's address, as rows.
//!
//! Same shape as [`password_reset`](super::password_reset), plus the address
//! the link was sent to: the only address a link can prove.

use sea_orm::{
    ColumnTrait as _, DatabaseConnection, EntityTrait as _, QueryFilter as _, Set, sea_query::Expr,
};
use tessera_core::{EmailAddress, EmailVerificationId, UserId};
use time::OffsetDateTime;

use crate::{
    auth::secret::Secret,
    db::{
        entity::email_verification::{ActiveModel, Column, Entity as LinkEntity, Model},
        repository::user::{CorruptRow, UserRepositoryError},
    },
};

/// A verification link, as the server knows it.
#[derive(Debug, Clone)]
pub struct EmailVerification {
    /// Row identifier.
    pub id: EmailVerificationId,
    /// Whose address this would prove.
    pub user_id: UserId,
    /// The address it was sent to.
    pub email: EmailAddress,
    /// When it stops working.
    pub expires_at: OffsetDateTime,
    /// When it was used, if it was.
    pub used_at: Option<OffsetDateTime>,
}

impl EmailVerification {
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
            id: EmailVerificationId::parse(&model.id).map_err(fail)?,
            user_id: UserId::parse(&model.user_id).map_err(fail)?,
            email: EmailAddress::parse(&model.email).map_err(fail)?,
            expires_at: model.expires_at,
            used_at: model.used_at,
        })
    }
}

/// Verification links, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct EmailVerificationRepository {
    connection: DatabaseConnection,
}

impl EmailVerificationRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// Records a link for `email` and returns the secret to put in the mail.
    ///
    /// Returned, never stored: only its digest is saved.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the insert fails.
    pub async fn issue(
        &self,
        user_id: UserId,
        email: &EmailAddress,
        now: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> Result<Secret, UserRepositoryError> {
        let secret = Secret::generate_session();
        let row = ActiveModel {
            id: Set(EmailVerificationId::generate().to_string()),
            user_id: Set(user_id.to_string()),
            email: Set(email.as_str().to_owned()),
            secret_hash: Set(secret.digest()),
            created_at: Set(now),
            expires_at: Set(expires_at),
            used_at: Set(None),
        };
        LinkEntity::insert(row).exec(&self.connection).await?;
        Ok(secret)
    }

    /// Finds the link a secret refers to, used or not.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query fails.
    pub async fn find_by_secret(
        &self,
        secret: &Secret,
    ) -> Result<Option<EmailVerification>, UserRepositoryError> {
        let found = LinkEntity::find()
            .filter(Column::SecretHash.eq(secret.digest()))
            .one(&self.connection)
            .await?;
        found
            .as_ref()
            .map(EmailVerification::from_model)
            .transpose()
    }

    /// Marks a link as used, only if it was not already.
    ///
    /// Returns whether this call used it. The database decides, so two clicks
    /// produce one winner.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the update fails.
    pub async fn spend(
        &self,
        id: EmailVerificationId,
        now: OffsetDateTime,
    ) -> Result<bool, UserRepositoryError> {
        let result = LinkEntity::update_many()
            .col_expr(Column::UsedAt, Expr::value(Some(now)))
            .filter(Column::Id.eq(id.to_string()))
            .filter(Column::UsedAt.is_null())
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// Removes expired links.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the delete fails.
    pub async fn delete_expired(&self, now: OffsetDateTime) -> Result<u64, UserRepositoryError> {
        let result = LinkEntity::delete_many()
            .filter(Column::ExpiresAt.lt(now))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected)
    }
}
