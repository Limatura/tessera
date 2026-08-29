//! Reading and writing upload sessions.

use std::collections::HashMap;

use sea_orm::{
    ActiveValue::Set,
    ColumnTrait as _, DatabaseConnection, EntityTrait as _, PaginatorTrait as _, QueryFilter,
    QueryOrder as _, QuerySelect as _, QueryTrait as _, TransactionTrait as _,
    sea_query::{Expr, SimpleExpr},
};
use tessera_core::{RoleId, ShareId, UploadId, UserId};
use time::OffsetDateTime;

use crate::{
    db::{
        entity::{
            upload::{self, Entity as UploadEntity},
            upload_part::{self, Entity as UploadPartEntity},
            upload_segment::{self, Entity as UploadSegmentEntity},
        },
        repository::{quota::Scope, total, user::CorruptRow},
    },
    upload::sink::TakenPart,
};

/// What a segment is doing.
///
/// Named, not inferred from which columns are null. "Has a multipart
/// id" is not "has been started": a local filesystem never has one, so reading
/// the state off that column files every local segment as unopened for ever.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentState {
    /// The space is spoken for and nothing has been written.
    ///
    /// No writer yet. This is the reservation, and nothing reserves until the
    /// planner spreads a file over more than one backend. Two large
    /// uploads arriving together must not both be planned onto the same
    /// backend's last free gigabyte, and a row that exists before the first
    /// byte is what stops that.
    Planned,
    /// The destination is committed and bytes may go in.
    Open,
    /// Every byte of this segment is in the store.
    ///
    /// Holding real space in a real bucket while the share does not exist yet,
    /// so the sweep has to be able to see it.
    Complete,
}

impl SegmentState {
    /// The spelling stored in the column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Open => "open",
            Self::Complete => "complete",
        }
    }
}

/// One backend's share of a file still arriving.
///
/// A session has exactly one today. The list exists because cleaning up cannot
/// wait for splitting to arrive: a sweep that reopens the first segment and
/// stops leaves every other one's multipart upload open, and S3 bills for those
/// until they are aborted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// Position in the file, from zero.
    pub seq: i32,
    /// Which configured backend holds it.
    pub backend: String,
    /// The object it is written as.
    pub object_key: String,
    /// The store's own multipart identifier, or `None` on a local filesystem.
    pub multipart_id: Option<String>,
}

/// A file still arriving, in domain terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upload {
    /// Stable identifier.
    pub id: UploadId,
    /// The share to append to, or `None` to create one on completion.
    pub share_id: Option<ShareId>,
    /// Whose upload it is, or `None` for an anonymous one.
    pub user_id: Option<UserId>,
    /// The role whose quota these reserved bytes are charged against.
    pub role_id: Option<RoleId>,
    /// Digest of the secret that lets a caller continue it.
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
    ///
    /// This and the two below are the session's one segment, flattened. The
    /// database holds a list, `upload_segments`, because a file too large for
    /// any single backend has to be written to several. Every session has one
    /// segment today, so the projection loses nothing; once the planner starts
    /// spreading a file, these three become the list itself.
    pub object_key: String,
    /// Which backend the parts are going to.
    ///
    /// Decided when the session opens and never changed: where the bytes are
    /// going, not a pointer to whatever the configuration says now. Moving the
    /// default mid-upload must not strand the parts already written.
    pub backend: String,
    /// The store's own multipart identifier, or `None` on a local filesystem.
    pub multipart_id: Option<String>,
    /// Every backend this file is being written to, in order.
    ///
    /// The three fields above are the first of these, flattened. This is the
    /// list the sweep walks, since it is the operation that must not stop at
    /// the first.
    pub segments: Vec<Segment>,
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

    /// Builds the domain type from the session row and the one segment it has.
    ///
    /// A session with no segment is a corrupt row, not an empty upload: the
    /// segment says where the bytes are going, and a session that forgot it can
    /// neither be continued nor cleaned up.
    fn from_model(
        model: upload::Model,
        rows: Vec<upload_segment::Model>,
    ) -> Result<Self, CorruptRow> {
        let segments: Vec<Segment> = rows
            .into_iter()
            .map(|row| Segment {
                seq: row.seq,
                backend: row.backend,
                object_key: row.object_key,
                multipart_id: row.multipart_id,
            })
            .collect();
        let Some(segment) = segments.first().cloned() else {
            return Err(CorruptRow {
                id: model.id,
                source: tessera_core::DomainError::Empty {
                    field: "upload_segments",
                },
            });
        };
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
            role_id: model
                .role_id
                .as_deref()
                .map(RoleId::parse)
                .transpose()
                .map_err(fail)?,
            token_hash: model.token_hash,
            filename: model.filename,
            content_type: model.content_type,
            declared_bytes: u64::try_from(model.declared_bytes).unwrap_or(0),
            offset_bytes: u64::try_from(model.offset_bytes).unwrap_or(0),
            object_key: segment.object_key,
            backend: segment.backend,
            multipart_id: segment.multipart_id,
            segments,
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
    /// The role to charge the reserved bytes to: the guest role for an
    /// anonymous one, so unauthenticated sessions are still counted somewhere.
    pub role_id: Option<RoleId>,
    /// Digest of the secret handed to the uploader.
    pub token_hash: String,
    /// The name they gave it.
    pub filename: Option<String>,
    /// What they said it is.
    pub content_type: Option<String>,
    /// How much they said they will send.
    pub declared_bytes: u64,
    /// Where the bytes go. Decided once; see the migration.
    pub object_key: String,
    /// Which backend they go to. Decided once, for the same reason.
    pub backend: String,
    /// The store's multipart identifier, when there is one.
    pub multipart_id: Option<String>,
}

/// The restriction that limits open sessions to one scope, or `None` for the
/// whole instance.
fn owner_of(scope: Scope) -> Option<SimpleExpr> {
    match scope {
        Scope::User(id) => Some(upload::Column::UserId.eq(id.to_string())),
        Scope::Role(id) => Some(upload::Column::RoleId.eq(id.to_string())),
        Scope::Instance => None,
    }
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
        let transaction = self.connection.begin().await?;
        UploadEntity::insert(upload::ActiveModel {
            id: Set(id.to_string()),
            share_id: Set(new.share_id.map(|id| id.to_string())),
            user_id: Set(new.user_id.map(|id| id.to_string())),
            role_id: Set(new.role_id.map(|id| id.to_string())),
            token_hash: Set(new.token_hash.clone()),
            filename: Set(new.filename.clone()),
            content_type: Set(new.content_type.clone()),
            declared_bytes: Set(i64::try_from(new.declared_bytes).unwrap_or(i64::MAX)),
            offset_bytes: Set(0),
            created_at: Set(now),
            updated_at: Set(now),
            expires_at: Set(expires_at),
        })
        .exec(&transaction)
        .await?;

        // One statement apart from the session, one transaction with it. A
        // session whose segment did not land could neither be continued nor
        // swept: nothing would record where its bytes were going.
        UploadSegmentEntity::insert(upload_segment::ActiveModel {
            upload_id: Set(id.to_string()),
            seq: Set(0),
            backend: Set(new.backend.clone()),
            object_key: Set(new.object_key.clone()),
            offset_bytes: Set(0),
            length_bytes: Set(i64::try_from(new.declared_bytes).unwrap_or(i64::MAX)),
            multipart_id: Set(new.multipart_id.clone()),
            // `open`, not `planned`: the destination is committed the moment the
            // session exists. `planned` is the reservation the planner will make
            // when it spreads a file, and nothing makes one yet.
            state: Set(SegmentState::Open.as_str().to_owned()),
            created_at: Set(now),
            updated_at: Set(now),
        })
        .exec(&transaction)
        .await?;
        transaction.commit().await?;

        Ok(Upload {
            id,
            share_id: new.share_id,
            user_id: new.user_id,
            role_id: new.role_id,
            token_hash: new.token_hash.clone(),
            filename: new.filename.clone(),
            content_type: new.content_type.clone(),
            declared_bytes: new.declared_bytes,
            offset_bytes: 0,
            object_key: new.object_key.clone(),
            backend: new.backend.clone(),
            multipart_id: new.multipart_id.clone(),
            segments: vec![Segment {
                seq: 0,
                backend: new.backend.clone(),
                object_key: new.object_key.clone(),
                multipart_id: new.multipart_id.clone(),
            }],
            expires_at,
        })
    }

    /// Finds a session, whether or not it has expired.
    ///
    /// Expiry is the caller's decision, kept separate so "gone" and "never
    /// existed" can be told apart in a log while looking alike to a client.
    ///
    /// # Errors
    ///
    /// Returns [`UploadRepositoryError`] if the query fails or the row is not
    /// usable.
    pub async fn find(&self, id: UploadId) -> Result<Option<Upload>, UploadRepositoryError> {
        let Some(model) = UploadEntity::find_by_id(id.to_string())
            .one(&self.connection)
            .await?
        else {
            return Ok(None);
        };
        let segments = UploadSegmentEntity::find()
            .filter(upload_segment::Column::UploadId.eq(model.id.clone()))
            .order_by_asc(upload_segment::Column::Seq)
            .all(&self.connection)
            .await?;
        Ok(Some(Upload::from_model(model, segments)?))
    }

    /// Records bytes that landed, and the parts the store took for them.
    ///
    /// One transaction, because an offset that moved without its parts being
    /// recorded can never complete: `complete_multipart` needs every part
    /// handed back, and no store lists them in a way every backend agrees on.
    ///
    /// # Why the update is conditional
    ///
    /// Two requests can read the same offset, both find it acceptable, and both
    /// write. Checking in the handler and updating here is read-then-act, the
    /// interleaving tus's `409` exists to prevent, so the `WHERE` carries the
    /// offset the caller believed and a row count of zero means another request
    /// moved it first.
    ///
    /// Returns whether the update applied, so the caller can answer `409`
    /// instead of reporting success for bytes that went nowhere.
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
                // Zero until a file is spread over more than one backend. The
                // column exists now so that the day it is not zero is a value
                // change and not a schema change on live rows.
                seq: Set(0),
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
            // Segment first: `complete_multipart` is called once per segment and
            // wants that segment's parts, in its own order.
            .order_by_asc(upload_part::Column::Seq)
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
    /// uploads should be aborted over several passes, not one that holds a
    /// connection for minutes.
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
        let models = UploadEntity::find()
            .filter(upload::Column::ExpiresAt.lte(now))
            .order_by_asc(upload::Column::ExpiresAt)
            .limit(limit)
            .all(&self.connection)
            .await?;
        // One query for the whole batch, not one per session. The sweep
        // runs on a timer over everything that expired, and a query each would
        // make its cost the number of abandoned uploads.
        let ids: Vec<String> = models.iter().map(|model| model.id.clone()).collect();
        let mut segments: HashMap<String, Vec<upload_segment::Model>> = HashMap::new();
        for row in UploadSegmentEntity::find()
            .filter(upload_segment::Column::UploadId.is_in(ids))
            .order_by_asc(upload_segment::Column::Seq)
            .all(&self.connection)
            .await?
        {
            segments.entry(row.upload_id.clone()).or_default().push(row);
        }

        models
            .into_iter()
            .map(|model| {
                let rows = segments.remove(&model.id).unwrap_or_default();
                Upload::from_model(model, rows)
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Room that unfinished uploads have reserved, for one scope.
    ///
    /// The **declared** size, not what has landed. Counting what landed lets
    /// sessions over-commit: with ten gigabytes free, two callers can each say
    /// they will send ten and both be accepted, since neither has sent
    /// anything yet. Reserving the declaration makes the second wait.
    ///
    /// `uploads` carries its own `role_id` for this. Summing every open session
    /// against a member's role would charge them for room a guest is holding,
    /// and the row has no share to read the role from until it completes.
    ///
    /// The cost is that an abandoned session holds its whole declaration until
    /// the sweep takes it. That is the honest reading of "I am going to send
    /// ten gigabytes", and `share.upload_session_ttl_hours` bounds it.
    ///
    /// # Errors
    ///
    /// Returns [`UploadRepositoryError`] if the query fails.
    pub async fn reserved_bytes(&self, scope: Scope) -> Result<u64, UploadRepositoryError> {
        let total: Option<i64> = UploadEntity::find()
            .apply_if(owner_of(scope), QueryFilter::filter)
            .select_only()
            .column_as(
                total::bytes(
                    self.connection.get_database_backend(),
                    Expr::col(upload::Column::DeclaredBytes),
                ),
                "total",
            )
            .into_tuple()
            .one(&self.connection)
            .await?
            .flatten();
        Ok(u64::try_from(total.unwrap_or(0)).unwrap_or(0))
    }

    /// What sessions still in flight have spoken for, by backend.
    ///
    /// The bytes of an upload that has not become a share are in the bucket and
    /// in no figure the placer reads, since `file_segments` only holds content
    /// that made it. Without this a backend fills with half-finished uploads
    /// while the placer goes on offering it, which on a free tier locks up an
    /// account with nothing to explain where the space went.
    ///
    /// The *planned* length, not what has landed, for the same reason
    /// [`reserved_bytes`](Self::reserved_bytes) counts the declaration: a
    /// segment that has written a tenth of itself still wants the rest.
    ///
    /// # Errors
    ///
    /// Returns [`UploadRepositoryError`] if the query fails.
    pub async fn bytes_per_backend(&self) -> Result<HashMap<String, u64>, UploadRepositoryError> {
        let rows: Vec<(String, Option<i64>)> = UploadSegmentEntity::find()
            .select_only()
            .column(upload_segment::Column::Backend)
            .column_as(
                total::bytes(
                    self.connection.get_database_backend(),
                    Expr::col(upload_segment::Column::LengthBytes),
                ),
                "total",
            )
            .group_by(upload_segment::Column::Backend)
            .into_tuple()
            .all(&self.connection)
            .await?;
        Ok(rows
            .into_iter()
            .map(|(backend, bytes)| (backend, u64::try_from(bytes.unwrap_or(0)).unwrap_or(0)))
            .collect())
    }

    /// Every object key an unfinished upload is writing to.
    ///
    /// These have no share yet, so the reconciliation would take them for
    /// orphans and delete the bytes out from under a live upload.
    ///
    /// # Errors
    ///
    /// Returns [`UploadRepositoryError`] if the query fails.
    pub async fn all_object_keys(&self) -> Result<Vec<String>, UploadRepositoryError> {
        Ok(UploadSegmentEntity::find()
            .select_only()
            .column(upload_segment::Column::ObjectKey)
            .into_tuple()
            .all(&self.connection)
            .await?)
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
