//! Reading and writing API tokens.

use std::collections::HashMap;

use sea_orm::{
    ActiveValue::Set, ColumnTrait as _, DatabaseConnection, EntityTrait as _, QueryFilter as _,
    QueryOrder as _, TransactionTrait as _,
};
use tessera_core::{ApiTokenId, Permission, PermissionSet, UserId};
use time::OffsetDateTime;

use crate::{
    auth::secret::Secret,
    db::{
        entity::{
            api_token::{self, Entity as ApiTokenEntity},
            api_token_permission::{self, Entity as TokenPermissionEntity},
        },
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
    /// What it may do, which is at most what its owner may do.
    ///
    /// Stored on the token rather than derived from its owner: the point of a
    /// token is to hand a script something narrower than an account. The
    /// intersection with the owner's own permissions happens at the request, so
    /// demoting somebody narrows every token they hold at the same moment.
    pub permissions: PermissionSet,
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

    fn from_model(model: api_token::Model, permissions: PermissionSet) -> Result<Self, CorruptRow> {
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
            permissions,
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
        permissions: &PermissionSet,
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
            permissions: permissions.clone(),
            expires_at,
            revoked_at: None,
        };

        let transaction = self.connection.begin().await?;
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
        .exec(&transaction)
        .await?;
        for permission in permissions.iter() {
            TokenPermissionEntity::insert(api_token_permission::ActiveModel {
                token_id: Set(token.id.to_string()),
                permission: Set(permission.as_str().to_owned()),
            })
            .exec(&transaction)
            .await?;
        }
        transaction.commit().await?;

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
        let Some(model) = ApiTokenEntity::find()
            .filter(api_token::Column::SecretHash.eq(secret.digest()))
            .one(&self.connection)
            .await?
        else {
            return Ok(None);
        };
        let permissions = self.permissions_of(&model.id).await?;
        Ok(Some(ApiToken::from_model(model, permissions)?))
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
            // Tiebreak on the identifier, which is a ULID and therefore already
            // in creation order. Two tokens issued in the same microsecond would
            // otherwise come back in an order the database chose.
            .order_by_desc(api_token::Column::Id)
            .all(&self.connection)
            .await?;
        let mut held = self.permissions_by_token(user_id).await?;
        models
            .into_iter()
            .map(|model| {
                let permissions = held.remove(&model.id).unwrap_or_default();
                ApiToken::from_model(model, permissions).map_err(Into::into)
            })
            .collect()
    }

    /// What one token may do.
    async fn permissions_of(
        &self,
        token_id: &str,
    ) -> Result<PermissionSet, ApiTokenRepositoryError> {
        Ok(TokenPermissionEntity::find()
            .filter(api_token_permission::Column::TokenId.eq(token_id))
            .all(&self.connection)
            .await?
            .iter()
            .filter_map(|row| row.permission.parse::<Permission>().ok())
            .collect())
    }

    /// The same for every token one account holds, in one query.
    async fn permissions_by_token(
        &self,
        user_id: UserId,
    ) -> Result<HashMap<String, PermissionSet>, ApiTokenRepositoryError> {
        let ids: Vec<String> = ApiTokenEntity::find()
            .filter(api_token::Column::UserId.eq(user_id.to_string()))
            .all(&self.connection)
            .await?
            .into_iter()
            .map(|model| model.id)
            .collect();
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut grouped: HashMap<String, Vec<Permission>> = HashMap::new();
        for row in TokenPermissionEntity::find()
            .filter(api_token_permission::Column::TokenId.is_in(ids))
            .all(&self.connection)
            .await?
        {
            if let Ok(permission) = row.permission.parse::<Permission>() {
                grouped.entry(row.token_id).or_default().push(permission);
            }
        }
        Ok(grouped
            .into_iter()
            .map(|(id, permissions)| (id, permissions.into_iter().collect()))
            .collect())
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
