//! Reading and writing API tokens.

use sea_orm::{
    ActiveValue::Set, ColumnTrait as _, DatabaseConnection, EntityTrait as _, QueryFilter as _,
    QueryOrder as _,
};
use tessera_core::{ApiTokenId, UserId};
use time::OffsetDateTime;

use crate::{
    auth::secret::Secret,
    db::{
        entity::api_token::{self, Entity as ApiTokenEntity},
        repository::user::CorruptRow,
    },
};

/// An API token, in domain terms. Never carries the secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiToken {
    /// Stable identifier.
    pub id: ApiTokenId,
    /// Whose token this is.
    pub user_id: UserId,
    /// What the owner called it.
    pub name: String,
    /// Leading characters of the token, so a listing can identify it.
    pub display_prefix: String,
    /// When it was issued.
    pub created_at: OffsetDateTime,
    /// When it was last accepted.
    pub last_used_at: Option<OffsetDateTime>,
    /// When it stops being accepted, or `None` to live until revoked.
    pub expires_at: Option<OffsetDateTime>,
    /// When it was revoked.
    pub revoked_at: Option<OffsetDateTime>,
}

impl ApiToken {
    /// Whether the token may be accepted at `now`.
    ///
    /// Revocation and expiry are separate states on purpose: a revoked token
    /// stays in the table so that an audit can still show it existed, and this
    /// is the single place that decides both stop it working.
    #[must_use]
    pub fn is_usable_at(&self, now: OffsetDateTime) -> bool {
        self.revoked_at.is_none() && self.expires_at.is_none_or(|expiry| now < expiry)
    }

    fn from_model(model: api_token::Model) -> Result<Self, CorruptRow> {
        let fail = |source| CorruptRow {
            id: model.id.clone(),
            source,
        };
        Ok(Self {
            id: ApiTokenId::parse(&model.id).map_err(fail)?,
            user_id: UserId::parse(&model.user_id).map_err(fail)?,
            name: model.name,
            display_prefix: model.display_prefix,
            created_at: model.created_at,
            last_used_at: model.last_used_at,
            expires_at: model.expires_at,
            revoked_at: model.revoked_at,
        })
    }
}

/// Anything that can go wrong reading or writing a token.
#[derive(Debug, thiserror::Error)]
pub enum ApiTokenRepositoryError {
    /// The database refused the operation.
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),

    /// A stored row does not satisfy today's rules.
    #[error(transparent)]
    Corrupt(#[from] CorruptRow),
}

/// API tokens, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct ApiTokenRepository {
    connection: DatabaseConnection,
}

impl ApiTokenRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// Issues a token and returns the secret, which is the only time it exists.
    ///
    /// # Errors
    ///
    /// Returns [`ApiTokenRepositoryError`] if the insert fails.
    pub async fn create(
        &self,
        user_id: UserId,
        name: String,
        now: OffsetDateTime,
        expires_at: Option<OffsetDateTime>,
    ) -> Result<(ApiToken, Secret), ApiTokenRepositoryError> {
        let secret = Secret::generate_api_token();
        let token = ApiToken {
            id: ApiTokenId::generate(),
            user_id,
            name,
            display_prefix: secret.display_prefix(),
            created_at: now,
            last_used_at: None,
            expires_at,
            revoked_at: None,
        };

        ApiTokenEntity::insert(api_token::ActiveModel {
            id: Set(token.id.to_string()),
            user_id: Set(token.user_id.to_string()),
            name: Set(token.name.clone()),
            secret_hash: Set(secret.digest()),
            display_prefix: Set(token.display_prefix.clone()),
            created_at: Set(token.created_at),
            last_used_at: Set(None),
            expires_at: Set(token.expires_at),
            revoked_at: Set(None),
        })
        .exec(&self.connection)
        .await?;

        Ok((token, secret))
    }

    /// Finds the token a secret refers to, whether or not it is still usable.
    ///
    /// Usability is [`ApiToken::is_usable_at`]'s decision, kept separate so that
    /// "revoked" and "never existed" are distinguishable in a log while looking
    /// identical to the caller.
    ///
    /// # Errors
    ///
    /// Returns [`ApiTokenRepositoryError`] if the query fails or the row is not
    /// usable.
    pub async fn find_by_secret(
        &self,
        secret: &Secret,
    ) -> Result<Option<ApiToken>, ApiTokenRepositoryError> {
        let model = ApiTokenEntity::find()
            .filter(api_token::Column::SecretHash.eq(secret.digest()))
            .one(&self.connection)
            .await?;
        Ok(model.map(ApiToken::from_model).transpose()?)
    }

    /// Lists an account's tokens, newest first.
    ///
    /// # Errors
    ///
    /// Returns [`ApiTokenRepositoryError`] if the query fails or a row is not
    /// usable.
    pub async fn list_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<ApiToken>, ApiTokenRepositoryError> {
        let models = ApiTokenEntity::find()
            .filter(api_token::Column::UserId.eq(user_id.to_string()))
            .order_by_desc(api_token::Column::CreatedAt)
            .all(&self.connection)
            .await?;
        models
            .into_iter()
            .map(|model| ApiToken::from_model(model).map_err(Into::into))
            .collect()
    }

    /// Records that a token was accepted.
    ///
    /// # Errors
    ///
    /// Returns [`ApiTokenRepositoryError`] if the update fails.
    pub async fn touch(
        &self,
        id: ApiTokenId,
        now: OffsetDateTime,
    ) -> Result<(), ApiTokenRepositoryError> {
        ApiTokenEntity::update_many()
            .col_expr(api_token::Column::LastUsedAt, Some(now).into())
            .filter(api_token::Column::Id.eq(id.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    /// Revokes a token belonging to `user_id`.
    ///
    /// Scoped to the owner in the query rather than checked afterwards: a
    /// separate ownership check is a race and an easy thing to forget, where a
    /// `WHERE` clause simply cannot revoke somebody else's token.
    ///
    /// Returns whether a row changed, so a caller can tell "revoked" from "no
    /// such token of yours" without a second query.
    ///
    /// # Errors
    ///
    /// Returns [`ApiTokenRepositoryError`] if the update fails.
    pub async fn revoke(
        &self,
        id: ApiTokenId,
        user_id: UserId,
        now: OffsetDateTime,
    ) -> Result<bool, ApiTokenRepositoryError> {
        let result = ApiTokenEntity::update_many()
            .col_expr(api_token::Column::RevokedAt, Some(now).into())
            .filter(api_token::Column::Id.eq(id.to_string()))
            .filter(api_token::Column::UserId.eq(user_id.to_string()))
            .filter(api_token::Column::RevokedAt.is_null())
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected > 0)
    }
}
