//! Invite codes, as rows.

use sea_orm::{
    ColumnTrait as _, DatabaseConnection, EntityTrait as _, PaginatorTrait as _, QueryFilter as _,
    QueryOrder as _, Set,
    sea_query::{Expr, ExprTrait as _},
};
use tessera_core::InviteId;
use time::OffsetDateTime;

use crate::{
    auth::secret::Secret,
    db::{
        entity::invite::{ActiveModel, Column, Entity as InviteEntity, Model},
        repository::user::{CorruptRow, UserRepositoryError},
    },
};

/// One invite code.
#[derive(Debug, Clone)]
pub struct Invite {
    /// Row identifier.
    pub id: InviteId,
    /// What the operator called it.
    pub note: Option<String>,
    /// How many accounts it may create.
    pub max_uses: i64,
    /// How many it has created.
    pub used: i64,
    /// When it was made.
    pub created_at: OffsetDateTime,
    /// When it stops working, if it does.
    pub expires_at: Option<OffsetDateTime>,
}

impl Invite {
    /// Whether this code could still admit an account at `now`.
    ///
    /// Both halves matter and they fail differently: a code with no places left
    /// is finished, while a code past its date may still hold unusable places.
    /// Neither is told apart in a response; see the handler.
    #[must_use]
    pub fn is_usable_at(&self, now: OffsetDateTime) -> bool {
        self.used < self.max_uses && self.expires_at.is_none_or(|expiry| expiry > now)
    }

    fn from_model(model: &Model) -> Result<Self, UserRepositoryError> {
        let fail = |source| CorruptRow {
            id: model.id.clone(),
            source,
        };
        Ok(Self {
            id: InviteId::parse(&model.id).map_err(fail)?,
            note: model.note.clone(),
            max_uses: model.max_uses,
            used: model.used,
            created_at: model.created_at,
            expires_at: model.expires_at,
        })
    }
}

/// Invite codes, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct InviteRepository {
    connection: DatabaseConnection,
}

impl InviteRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// Mints a code and returns it, once.
    ///
    /// The secret is returned, never stored. Only its digest reaches the
    /// database, so this is the one moment the code exists in a form that could
    /// be typed into a registration form.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the insert fails.
    pub async fn issue(
        &self,
        note: Option<String>,
        max_uses: i64,
        now: OffsetDateTime,
        expires_at: Option<OffsetDateTime>,
    ) -> Result<(Invite, Secret), UserRepositoryError> {
        let secret = Secret::generate_session();
        let invite = Invite {
            id: InviteId::generate(),
            note,
            max_uses,
            used: 0,
            created_at: now,
            expires_at,
        };

        InviteEntity::insert(ActiveModel {
            id: Set(invite.id.to_string()),
            code_hash: Set(secret.digest()),
            note: Set(invite.note.clone()),
            max_uses: Set(invite.max_uses),
            used: Set(0),
            created_at: Set(invite.created_at),
            expires_at: Set(invite.expires_at),
        })
        .exec(&self.connection)
        .await?;

        Ok((invite, secret))
    }

    /// Finds the invite a code refers to, usable or not.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query fails.
    pub async fn find_by_code(&self, code: &Secret) -> Result<Option<Invite>, UserRepositoryError> {
        let found = InviteEntity::find()
            .filter(Column::CodeHash.eq(code.digest()))
            .one(&self.connection)
            .await?;
        found.as_ref().map(Invite::from_model).transpose()
    }

    /// Takes one place on an invite, if there is one left.
    ///
    /// # Why the condition is in the `UPDATE`
    ///
    /// Reading the count and then writing it back would let two confirmations
    /// arriving together both see the last place and both take it. The database
    /// is what decides, and it decides once: `used < max_uses` is part of the
    /// statement, so exactly one of them changes a row.
    ///
    /// Returns whether a place was taken.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the update fails.
    pub async fn spend(
        &self,
        id: InviteId,
        now: OffsetDateTime,
    ) -> Result<bool, UserRepositoryError> {
        let has_room = Expr::col(Column::Used).lt(Expr::col(Column::MaxUses));
        let in_date = Column::ExpiresAt.is_null().or(Column::ExpiresAt.gt(now));
        let result = InviteEntity::update_many()
            .col_expr(Column::Used, Expr::col(Column::Used).add(1_i64))
            .filter(Column::Id.eq(id.to_string()))
            .filter(has_room)
            .filter(in_date)
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// Lists every invite, newest first.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query fails or a row is not usable.
    pub async fn list(&self) -> Result<Vec<Invite>, UserRepositoryError> {
        let models = InviteEntity::find()
            .order_by_desc(Column::CreatedAt)
            .all(&self.connection)
            .await?;
        models.iter().map(Invite::from_model).collect()
    }

    /// How many invites could still admit an account.
    ///
    /// Counted in the database, not by reading them all and filtering. The same
    /// answer, without carrying every row across the wire to throw most
    /// of them away.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query fails.
    pub async fn count_usable_at(&self, now: OffsetDateTime) -> Result<u64, UserRepositoryError> {
        Ok(InviteEntity::find()
            .filter(Expr::col(Column::Used).lt(Expr::col(Column::MaxUses)))
            .filter(Column::ExpiresAt.is_null().or(Column::ExpiresAt.gt(now)))
            .count(&self.connection)
            .await?)
    }

    /// Removes one invite.
    ///
    /// The places it already gave out stay given: an account created through a
    /// code does not depend on the code still existing. Revoking one stops it
    /// admitting anyone else, what an operator who posted a code in the wrong
    /// chat window wants.
    ///
    /// Returns whether an invite was removed.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the delete fails.
    pub async fn delete(&self, id: InviteId) -> Result<bool, UserRepositoryError> {
        let result = InviteEntity::delete_by_id(id.to_string())
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// Removes invites that expired, whether or not they were used.
    ///
    /// A code with places left on it is not swept: the operator may still be
    /// handing it out, and "it stopped working and I do not know why" is a worse
    /// outcome than a row that sits there.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the delete fails.
    pub async fn delete_expired(&self, now: OffsetDateTime) -> Result<u64, UserRepositoryError> {
        let result = InviteEntity::delete_many()
            .filter(Column::ExpiresAt.lt(now))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected)
    }
}
