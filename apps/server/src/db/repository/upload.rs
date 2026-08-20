//! Reading and writing upload sessions.

use sea_orm::{
    ActiveValue::Set, ColumnTrait as _, DatabaseConnection, EntityTrait as _, PaginatorTrait as _,
    QueryFilter as _, QueryOrder as _, QuerySelect as _, TransactionTrait as _,
};
use tessera_core::{ShareId, UploadId, UserId};
use time::OffsetDateTime;

use crate::{
    db::{
        entity::{
            upload::{self, Entity as UploadEntity},
            upload_part::{self, Entity as UploadPartEntity},
        },
        repository::user::CorruptRow,
    },
    upload::sink::TakenPart,
};

/// A file still arriving, in domain terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upload {
    /// Stable identifier.
    pub id: UploadId,
    /// The share to append to, or `None` to create one on completion.
    pub share_id: Option<ShareId>,
    /// Whose upload it is, or `None` for an anonymous one.
    pub user_id: Option<UserId>,
    /// Digest of the secret that lets somebody continue it.
    pub token_hash: String,
    /// The name the uploader gave it.
    pub filename: Option<String>,
    /// What the uploader said it was.
    pub content_type: Option<String>,
    /// What the client said it would send.
    pub declared_bytes: u64,
    /// How much has landed. The answer to a `HEAD`.
    pub offset_bytes: u64,
    /// Decided at creation and never recomputed.
    pub object_key: String,
    /// The store's own multipart identifier, or `None` on a local filesystem.
    pub multipart_id: Option<String>,
    /// When the sweep may remove it.
    pub expires_at: OffsetDateTime,
}

impl Upload {
    /// Whether every declared byte has arrived.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.offset_bytes >= self.declared_bytes
    }

    /// How much is still expected.
    #[must_use]
    pub const fn remaining(&self) -> u64 {
        self.declared_bytes.saturating_sub(self.offset_bytes)
    }

    fn from_model(model: upload::Model) -> Result<Self, CorruptRow> {
        let fail = |source| CorruptRow {
            id: model.id.clone(),
            source,
        };
        Ok(Self {
            id: UploadId::parse(&model.id).map_err(fail)?,
            share_id: model
                .share_id
                .as_deref()
                .map(ShareId::parse)
                .transpose()
                .map_err(fail)?,
            user_id: model
                .user_id
                .as_deref()
                .map(UserId::parse)
                .transpose()
                .map_err(fail)?,
            token_hash: model.token_hash,
            filename: model.filename,
            content_type: model.content_type,
            declared_bytes: u64::try_from(model.declared_bytes).unwrap_or(0),
            offset_bytes: u64::try_from(model.offset_bytes).unwrap_or(0),
            object_key: model.object_key,
            multipart_id: model.multipart_id,
            expires_at: model.expires_at,
        })
    }
}

/// What one needs to open an upload session.
#[derive(Debug, Clone)]
pub struct NewUpload {
    /// The share to append to, or `None` to create one on completion.
    pub share_id: Option<ShareId>,
    /// Whose upload it is.
    pub user_id: Option<UserId>,
    /// Digest of the secret handed to the uploader.
    pub token_hash: String,
    /// The name they gave it.
    pub filename: Option<String>,
    /// What they said it is.
    pub content_type: Option<String>,
    /// How much they said they will send.
    pub declared_bytes: u64,
    /// Where the bytes go. Decided once — see the migration.
    pub object_key: String,
    /// The store's multipart identifier, when there is one.
    pub multipart_id: Option<String>,
}

/// Anything that can go wrong reading or writing an upload.
#[derive(Debug, thiserror::Error)]
pub enum UploadRepositoryError {
    /// The database refused the operation.
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),

    /// A stored row does not satisfy today's rules.
    #[error(transparent)]
    Corrupt(#[from] CorruptRow),
}

/// Upload sessions, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct UploadRepository {
    connection: DatabaseConnection,
}

impl UploadRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// Opens a session.
    ///
    /// # Errors
    ///
    /// Returns [`UploadRepositoryError`] if the insert fails.
    pub async fn create(
        &self,
        new: &NewUpload,
        now: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> Result<Upload, UploadRepositoryError> {
        let id = UploadId::generate();
        UploadEntity::insert(upload::ActiveModel {
            id: Set(id.to_string()),
            share_id: Set(new.share_id.map(|id| id.to_string())),
            user_id: Set(new.user_id.map(|id| id.to_string())),
            token_hash: Set(new.token_hash.clone()),
            filename: Set(new.filename.clone()),
            content_type: Set(new.content_type.clone()),
            declared_bytes: Set(i64::try_from(new.declared_bytes).unwrap_or(i64::MAX)),
            offset_bytes: Set(0),
            object_key: Set(new.object_key.clone()),
            multipart_id: Set(new.multipart_id.clone()),
            created_at: Set(now),
            updated_at: Set(now),
            expires_at: Set(expires_at),
        })
        .exec(&self.connection)
        .await?;

        Ok(Upload {
            id,
            share_id: new.share_id,
            user_id: new.user_id,
            token_hash: new.token_hash.clone(),
            filename: new.filename.clone(),
            content_type: new.content_type.clone(),
            declared_bytes: new.declared_bytes,
            offset_bytes: 0,
            object_key: new.object_key.clone(),
            multipart_id: new.multipart_id.clone(),
            expires_at,
        })
    }

    /// Finds a session, whether or not it has expired.
    ///
    /// Expiry is the caller's decision, kept separate so that "gone" and "never
    /// existed" can be told apart in a log while looking identical to a client.
    ///
    /// # Errors
    ///
    /// Returns [`UploadRepositoryError`] if the query fails or the row is not
    /// usable.
    pub async fn find(&self, id: UploadId) -> Result<Option<Upload>, UploadRepositoryError> {
        UploadEntity::find_by_id(id.to_string())
            .one(&self.connection)
            .await?
            .map(Upload::from_model)
            .transpose()
            .map_err(Into::into)
    }

    /// Records bytes that landed, and the parts the store took for them.
    ///
    /// One transaction, because an offset that moved without its parts being
    /// recorded is an upload that can never complete — `complete_multipart`
    /// needs every part handed back, and the store will not list them for us in
    /// a way every backend agrees on.
    ///
    /// # Why the update is conditional
    ///
    /// Two requests can read the same offset, both find it acceptable, and both
    /// write. Checking in the handler and updating here would be read-then-act,
    /// which is exactly the interleaving tus's `409` exists to prevent — so the
    /// `WHERE` carries the offset the caller believed, and a row count of zero
    /// means somebody else moved it first.
    ///
    /// Returns whether the update applied, so the caller can answer `409`
    /// rather than reporting success for bytes that went nowhere.
    ///
    /// # Errors
    ///
    /// Returns [`UploadRepositoryError`] if any statement fails.
    pub async fn advance(
        &self,
        id: UploadId,
        expected: u64,
        offset_bytes: u64,
        parts: &[TakenPart],
        now: OffsetDateTime,
    ) -> Result<bool, UploadRepositoryError> {
        let transaction = self.connection.begin().await?;
        for part in parts {
            UploadPartEntity::insert(upload_part::ActiveModel {
                upload_id: Set(id.to_string()),
                part_number: Set(part.number),
                content_id: Set(part.content_id.clone()),
            })
            .exec(&transaction)
            .await?;
        }
        let moved = UploadEntity::update_many()
            .col_expr(
                upload::Column::OffsetBytes,
                i64::try_from(offset_bytes).unwrap_or(i64::MAX).into(),
            )
            .col_expr(upload::Column::UpdatedAt, now.into())
            .filter(upload::Column::Id.eq(id.to_string()))
            .filter(upload::Column::OffsetBytes.eq(i64::try_from(expected).unwrap_or(i64::MAX)))
            .exec(&transaction)
            .await?;
        if moved.rows_affected == 0 {
            transaction.rollback().await?;
            return Ok(false);
        }
        transaction.commit().await?;
        Ok(true)
    }

    /// The parts the store has taken, in the order it wants them back.
    ///
    /// # Errors
    ///
    /// Returns [`UploadRepositoryError`] if the query fails.
    pub async fn parts_of(&self, id: UploadId) -> Result<Vec<TakenPart>, UploadRepositoryError> {
        Ok(UploadPartEntity::find()
            .filter(upload_part::Column::UploadId.eq(id.to_string()))
            .order_by_asc(upload_part::Column::PartNumber)
            .all(&self.connection)
            .await?
            .into_iter()
            .map(|row| TakenPart {
                number: row.part_number,
                content_id: row.content_id,
            })
            .collect())
    }

    /// Removes a session and its parts.
    ///
    /// The parts go with it through `ON DELETE CASCADE`.
    ///
    /// # Errors
    ///
    /// Returns [`UploadRepositoryError`] if the delete fails.
    pub async fn delete(&self, id: UploadId) -> Result<(), UploadRepositoryError> {
        UploadEntity::delete_by_id(id.to_string())
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    /// Sessions the sweep may remove, oldest first.
    ///
    /// Bounded, because the sweep runs on a timer and a thousand abandoned
    /// uploads should take a thousand aborts spread over several passes rather
    /// than one pass that holds a connection for minutes.
    ///
    /// # Errors
    ///
    /// Returns [`UploadRepositoryError`] if the query fails or a row is not
    /// usable.
    pub async fn expired(
        &self,
        now: OffsetDateTime,
        limit: u64,
    ) -> Result<Vec<Upload>, UploadRepositoryError> {
        UploadEntity::find()
            .filter(upload::Column::ExpiresAt.lte(now))
            .order_by_asc(upload::Column::ExpiresAt)
            .limit(limit)
            .all(&self.connection)
            .await?
            .into_iter()
            .map(Upload::from_model)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Bytes that unfinished uploads have already put in the store.
    ///
    /// Counted against the quota: an upload that has landed eight megabytes is
    /// holding eight megabytes, whether or not anybody ever finishes it. Without
    /// this, opening sessions and abandoning them is free storage — which is
    /// the hole `vastsa/FileCodeBox#362` left open.
    ///
    /// # Errors
    ///
    /// Returns [`UploadRepositoryError`] if the query fails.
    pub async fn in_flight_bytes(&self) -> Result<u64, UploadRepositoryError> {
        let total: Option<i64> = UploadEntity::find()
            .select_only()
            .column_as(upload::Column::OffsetBytes.sum(), "total")
            .into_tuple()
            .one(&self.connection)
            .await?
            .flatten();
        Ok(u64::try_from(total.unwrap_or(0)).unwrap_or(0))
    }

    /// How many sessions are open, for the instance statistics.
    ///
    /// # Errors
    ///
    /// Returns [`UploadRepositoryError`] if the query fails.
    pub async fn count(&self) -> Result<u64, UploadRepositoryError> {
        Ok(UploadEntity::find().count(&self.connection).await?)
    }
}
