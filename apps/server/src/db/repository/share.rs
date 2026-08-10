//! Reading and writing shares.
//!
//! This module owns the access gate. Everything a public request needs in order
//! to decide "may this be handed out" is decided here, once, so that the three
//! public routes cannot disagree about it.

use sea_orm::{
    ActiveValue::Set,
    ColumnTrait as _, DatabaseConnection, EntityTrait as _, QueryFilter as _, QueryOrder as _,
    QuerySelect as _,
    sea_query::{Expr, ExprTrait as _},
};
use tessera_core::{DomainError, RetrievalCode, ShareId, ShareTicketId, UserId};
use time::OffsetDateTime;

use crate::{
    auth::secret::Secret,
    db::{
        entity::{
            share::{self, Entity as ShareEntity},
            share_content::{self, Entity as ShareContentEntity},
            share_ticket::{self, Entity as ShareTicketEntity},
        },
        repository::user::CorruptRow,
    },
};

/// What a share carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareKind {
    /// A note or a snippet of code.
    Text,
    /// An uploaded file.
    File,
}

impl ShareKind {
    /// The value stored in the `kind` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::File => "file",
        }
    }

    fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "text" => Ok(Self::Text),
            "file" => Ok(Self::File),
            _ => Err(DomainError::Malformed {
                field: "kind",
                reason: "must be `text` or `file`",
            }),
        }
    }
}

/// Where a share's bytes live.
///
/// An enum rather than a `storage` string plus a nullable `object_key`, so that
/// "object storage with no key" is a state the rest of the program cannot be
/// handed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageLocation {
    /// Small enough to keep in the database.
    Inline,
    /// In the blob store, under this key.
    Object {
        /// Key as it was written. Stored rather than recomputed, so the layout
        /// can change later without moving existing objects.
        key: String,
    },
}

impl StorageLocation {
    const INLINE: &'static str = "inline";
    const OBJECT: &'static str = "object";

    /// The blob-store key, when there is one.
    ///
    /// Saves every caller a `match` whose only other arm is "no key", and keeps
    /// the two-variant shape from leaking into code that just wants the key.
    #[must_use]
    pub fn object_key(&self) -> Option<&str> {
        match self {
            Self::Inline => None,
            Self::Object { key } => Some(key),
        }
    }

    /// The value stored in the `storage` column.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match *self {
            Self::Inline => Self::INLINE,
            Self::Object { .. } => Self::OBJECT,
        }
    }

    fn parse(storage: &str, object_key: Option<String>) -> Result<Self, DomainError> {
        match (storage, object_key) {
            (Self::INLINE, _) => Ok(Self::Inline),
            (Self::OBJECT, Some(key)) => Ok(Self::Object { key }),
            (Self::OBJECT, None) => Err(DomainError::Malformed {
                field: "object_key",
                reason: "must be present when storage is `object`",
            }),
            _ => Err(DomainError::Malformed {
                field: "storage",
                reason: "must be `inline` or `object`",
            }),
        }
    }
}

/// A share, in domain terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Share {
    /// Stable identifier.
    pub id: ShareId,
    /// How a person refers to it.
    pub code: RetrievalCode,
    /// Owner, or `None` for an anonymous upload.
    pub user_id: Option<UserId>,
    /// What it carries.
    pub kind: ShareKind,

    /// Argon2 hash of the share's password, if it has one.
    pub password_hash: Option<String>,
    /// How many times it may be handed out, or `None` for unlimited.
    pub use_limit: Option<u32>,
    /// How many times it has been.
    pub use_count: u32,
    /// When it stops being served.
    pub expires_at: Option<OffsetDateTime>,
    /// When an administrator switched it off.
    pub disabled_at: Option<OffsetDateTime>,

    /// Display name.
    pub title: Option<String>,
    /// Original file name.
    pub filename: Option<String>,
    /// The media type the uploader claimed. Recorded, never echoed.
    pub content_type: Option<String>,
    /// Syntax-highlighting hint.
    pub language: Option<String>,
    /// Size of the content in bytes.
    pub size_bytes: u64,
    /// Where the bytes are.
    pub storage: StorageLocation,

    /// JSON describing how to draw the OG image.
    pub og_config: Option<String>,
    /// SHA-256 of the token that lets an anonymous uploader manage this share.
    pub management_token_hash: Option<String>,
    /// How many times the share page was looked at. Statistics only.
    pub view_count: u32,
    /// When the share was last touched.
    pub last_accessed_at: Option<OffsetDateTime>,

    /// When it was created.
    pub created_at: OffsetDateTime,
    /// When it last changed.
    pub updated_at: OffsetDateTime,
}

impl Share {
    /// Whether the share has run out of permitted retrievals.
    #[must_use]
    pub fn is_exhausted(&self) -> bool {
        self.use_limit.is_some_and(|limit| self.use_count >= limit)
    }

    /// Whether the share has passed its expiry.
    #[must_use]
    pub fn is_expired_at(&self, now: OffsetDateTime) -> bool {
        self.expires_at.is_some_and(|expiry| now >= expiry)
    }

    /// Whether a password must be presented before this can be handed out.
    #[must_use]
    pub const fn needs_password(&self) -> bool {
        self.password_hash.is_some()
    }

    /// Why this share cannot be handed out, if it cannot.
    ///
    /// Password checking is **not** here: verifying an Argon2 hash is expensive
    /// and asynchronous, and this is the cheap synchronous half. The caller runs
    /// it first, and only pays for the hash if everything else passed.
    ///
    /// # Why the reasons are distinguishable
    ///
    /// "Expired" and "no such code" could be collapsed into one answer to avoid
    /// confirming that a code was ever real. They are kept apart because the
    /// share page has to render a password prompt for a live share, which
    /// already reveals existence for any valid code — so the collapse would buy
    /// nothing and cost the reader an explanation of why their link stopped
    /// working.
    #[must_use]
    pub fn availability_at(&self, now: OffsetDateTime) -> Availability {
        if self.disabled_at.is_some() {
            Availability::Disabled
        } else if self.is_expired_at(now) {
            Availability::Expired
        } else if self.is_exhausted() {
            Availability::Exhausted
        } else {
            Availability::Available
        }
    }

    fn from_model(model: share::Model) -> Result<Self, CorruptRow> {
        let id = model.id.clone();
        let fail = move |source| CorruptRow { id, source };

        Ok(Self {
            id: ShareId::parse(&model.id).map_err(fail.clone())?,
            code: RetrievalCode::parse(&model.code).map_err(fail.clone())?,
            user_id: model
                .user_id
                .as_deref()
                .map(UserId::parse)
                .transpose()
                .map_err(fail.clone())?,
            kind: ShareKind::parse(&model.kind).map_err(fail.clone())?,

            password_hash: model.password_hash,
            use_limit: model.use_limit.map(unsigned),
            use_count: unsigned(model.use_count),
            expires_at: model.expires_at,
            disabled_at: model.disabled_at,

            title: model.title,
            filename: model.filename,
            content_type: model.content_type,
            language: model.language,
            size_bytes: model.size_bytes.try_into().unwrap_or(0),
            storage: StorageLocation::parse(&model.storage, model.object_key).map_err(fail)?,

            og_config: model.og_config,
            management_token_hash: model.management_token_hash,
            view_count: unsigned(model.view_count),
            last_accessed_at: model.last_accessed_at,

            created_at: model.created_at,
            updated_at: model.updated_at,
        })
    }

    fn to_active_model(&self) -> share::ActiveModel {
        let object_key = match &self.storage {
            StorageLocation::Inline => None,
            StorageLocation::Object { key } => Some(key.clone()),
        };

        share::ActiveModel {
            id: Set(self.id.to_string()),
            code: Set(self.code.as_str().to_owned()),
            user_id: Set(self.user_id.map(|id| id.to_string())),
            kind: Set(self.kind.as_str().to_owned()),

            password_hash: Set(self.password_hash.clone()),
            use_limit: Set(self.use_limit.map(signed)),
            use_count: Set(signed(self.use_count)),
            expires_at: Set(self.expires_at),
            disabled_at: Set(self.disabled_at),

            title: Set(self.title.clone()),
            filename: Set(self.filename.clone()),
            content_type: Set(self.content_type.clone()),
            language: Set(self.language.clone()),
            size_bytes: Set(self.size_bytes.try_into().unwrap_or(i64::MAX)),
            storage: Set(self.storage.as_str().to_owned()),
            object_key: Set(object_key),

            og_config: Set(self.og_config.clone()),
            management_token_hash: Set(self.management_token_hash.clone()),
            view_count: Set(signed(self.view_count)),
            last_accessed_at: Set(self.last_accessed_at),

            created_at: Set(self.created_at),
            updated_at: Set(self.updated_at),
        }
    }
}

/// A negative count is not representable in the domain, and the column is only
/// ever written from one. Clamping is the honest reading of a row that has one
/// anyway: it means "none", not "minus three".
fn unsigned(value: i32) -> u32 {
    u32::try_from(value).unwrap_or(0)
}

fn signed(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// Whether a share may be handed out, and why not when it may not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// Nothing stands in the way but the password, if there is one.
    Available,
    /// An administrator switched it off.
    Disabled,
    /// It is past its expiry.
    Expired,
    /// It has been handed out as many times as it was allowed to be.
    Exhausted,
}

/// Anything that can go wrong reading or writing a share.
#[derive(Debug, thiserror::Error)]
pub enum ShareRepositoryError {
    /// The database refused the operation.
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),

    /// A stored row does not satisfy today's rules.
    #[error(transparent)]
    Corrupt(#[from] CorruptRow),
}

/// Shares, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct ShareRepository {
    connection: DatabaseConnection,
}

impl ShareRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// Stores a new share, and its content when that content is inline.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the insert fails — including when the
    /// code is already taken, which the unique index rejects.
    pub async fn insert(
        &self,
        share: &Share,
        inline_content: Option<Vec<u8>>,
    ) -> Result<(), ShareRepositoryError> {
        ShareEntity::insert(share.to_active_model())
            .exec(&self.connection)
            .await?;

        if let Some(content) = inline_content {
            ShareContentEntity::insert(share_content::ActiveModel {
                share_id: Set(share.id.to_string()),
                content: Set(content),
            })
            .exec(&self.connection)
            .await?;
        }
        Ok(())
    }

    /// Finds a share by the code a person typed.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails or the row is not
    /// usable.
    pub async fn find_by_code(
        &self,
        code: &RetrievalCode,
    ) -> Result<Option<Share>, ShareRepositoryError> {
        let model = ShareEntity::find()
            .filter(share::Column::Code.eq(code.as_str()))
            .one(&self.connection)
            .await?;
        Ok(model.map(Share::from_model).transpose()?)
    }

    /// Finds a share by its identifier.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails or the row is not
    /// usable.
    pub async fn find_by_id(&self, id: ShareId) -> Result<Option<Share>, ShareRepositoryError> {
        let model = ShareEntity::find_by_id(id.to_string())
            .one(&self.connection)
            .await?;
        Ok(model.map(Share::from_model).transpose()?)
    }

    /// Reads inline content.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails.
    pub async fn inline_content(
        &self,
        id: ShareId,
    ) -> Result<Option<Vec<u8>>, ShareRepositoryError> {
        let model = ShareContentEntity::find_by_id(id.to_string())
            .one(&self.connection)
            .await?;
        Ok(model.map(|row| row.content))
    }

    /// Lists an account's shares, newest first.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails or a row is not
    /// usable.
    pub async fn list_for_user(&self, user_id: UserId) -> Result<Vec<Share>, ShareRepositoryError> {
        let models = ShareEntity::find()
            .filter(share::Column::UserId.eq(user_id.to_string()))
            .order_by_desc(share::Column::CreatedAt)
            // ULIDs are time-ordered, so this breaks a tie the way a reader
            // expects. Without it two shares created in the same microsecond
            // come back in whatever order the database felt like, and a listing
            // that reshuffles between requests looks broken.
            .order_by_desc(share::Column::Id)
            .all(&self.connection)
            .await?;
        models
            .into_iter()
            .map(|model| Share::from_model(model).map_err(Into::into))
            .collect()
    }

    /// Spends one of a share's permitted retrievals.
    ///
    /// Returns whether one was spent. `false` means the limit was already
    /// reached, and the caller must not hand anything out.
    ///
    /// # Why this is one statement and not read-then-write
    ///
    /// Two requests arriving together on a share with one use left must not both
    /// succeed. Reading the count and writing it back leaves exactly that race,
    /// and no amount of care in the handler closes it. Putting the limit in the
    /// `WHERE` clause makes the database do the comparison and the increment in
    /// the same statement, which is atomic on all three backends without a
    /// transaction.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the update fails.
    pub async fn consume(
        &self,
        id: ShareId,
        now: OffsetDateTime,
    ) -> Result<bool, ShareRepositoryError> {
        let result = ShareEntity::update_many()
            .col_expr(
                share::Column::UseCount,
                Expr::col(share::Column::UseCount).add(1),
            )
            .col_expr(share::Column::LastAccessedAt, Some(now).into())
            .filter(share::Column::Id.eq(id.to_string()))
            .filter(
                share::Column::UseLimit
                    .is_null()
                    .or(Expr::col(share::Column::UseCount).lt(Expr::col(share::Column::UseLimit))),
            )
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected > 0)
    }

    /// Records that the share page was looked at.
    ///
    /// Deliberately separate from [`consume`](Self::consume): a look is not a
    /// retrieval, and a crawler unfurling a link must never spend somebody's
    /// one-time share.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the update fails.
    pub async fn record_view(&self, id: ShareId) -> Result<(), ShareRepositoryError> {
        ShareEntity::update_many()
            .col_expr(
                share::Column::ViewCount,
                Expr::col(share::Column::ViewCount).add(1),
            )
            .filter(share::Column::Id.eq(id.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    /// Deletes a share. Its inline content goes with it, by foreign key.
    ///
    /// Any object in the blob store does **not** — no database action reaches
    /// it, so the caller is responsible for that.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the delete fails.
    pub async fn delete(&self, id: ShareId) -> Result<(), ShareRepositoryError> {
        ShareEntity::delete_by_id(id.to_string())
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    /// Lists shares that have passed their expiry, so their objects can be
    /// removed before the rows are.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails or a row is not
    /// usable.
    pub async fn find_expired(
        &self,
        now: OffsetDateTime,
        limit: u64,
    ) -> Result<Vec<Share>, ShareRepositoryError> {
        let models = ShareEntity::find()
            .filter(share::Column::ExpiresAt.lt(now))
            .order_by_asc(share::Column::ExpiresAt)
            .limit(limit)
            .all(&self.connection)
            .await?;
        models
            .into_iter()
            .map(|model| Share::from_model(model).map_err(Into::into))
            .collect()
    }
}

/// One authorisation to retrieve a share.
///
/// Issued when a retrieval is spent, then presented for every request the
/// transfer needs. See the migration SQL for why it is a row rather than a
/// signed value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareTicket {
    /// Stable identifier.
    pub id: ShareTicketId,
    /// The share this authorises, and only this share.
    pub share_id: ShareId,
    /// When the retrieval was authorised.
    pub created_at: OffsetDateTime,
    /// When the authorisation stops being honoured.
    pub expires_at: OffsetDateTime,
}

impl ShareTicket {
    /// Whether the ticket is still good at `now`.
    #[must_use]
    pub fn is_valid_at(&self, now: OffsetDateTime) -> bool {
        now < self.expires_at
    }

    fn from_model(model: &share_ticket::Model) -> Result<Self, CorruptRow> {
        let fail = |source| CorruptRow {
            id: model.id.clone(),
            source,
        };
        Ok(Self {
            id: ShareTicketId::parse(&model.id).map_err(fail)?,
            share_id: ShareId::parse(&model.share_id).map_err(fail)?,
            created_at: model.created_at,
            expires_at: model.expires_at,
        })
    }
}

/// Download authorisations.
#[derive(Debug, Clone)]
pub struct ShareTicketRepository {
    connection: DatabaseConnection,
}

impl ShareTicketRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// Records an authorisation and returns the secret to hand to the caller.
    ///
    /// The secret is returned rather than stored: only its digest reaches the
    /// database, so this is the one moment it exists in a presentable form.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the insert fails.
    pub async fn issue(
        &self,
        share_id: ShareId,
        now: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> Result<(ShareTicket, Secret), ShareRepositoryError> {
        let secret = Secret::generate_session();
        let ticket = ShareTicket {
            id: ShareTicketId::generate(),
            share_id,
            created_at: now,
            expires_at,
        };

        ShareTicketEntity::insert(share_ticket::ActiveModel {
            id: Set(ticket.id.to_string()),
            share_id: Set(ticket.share_id.to_string()),
            secret_hash: Set(secret.digest()),
            created_at: Set(ticket.created_at),
            expires_at: Set(ticket.expires_at),
        })
        .exec(&self.connection)
        .await?;

        Ok((ticket, secret))
    }

    /// Finds the authorisation a secret refers to, whether or not it is still
    /// valid.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails or the row is not
    /// usable.
    pub async fn find_by_secret(
        &self,
        secret: &Secret,
    ) -> Result<Option<ShareTicket>, ShareRepositoryError> {
        let model = ShareTicketEntity::find()
            .filter(share_ticket::Column::SecretHash.eq(secret.digest()))
            .one(&self.connection)
            .await?;
        Ok(model.as_ref().map(ShareTicket::from_model).transpose()?)
    }

    /// Removes authorisations that have run out.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the delete fails.
    pub async fn delete_expired(&self, now: OffsetDateTime) -> Result<u64, ShareRepositoryError> {
        let result = ShareTicketEntity::delete_many()
            .filter(share_ticket::Column::ExpiresAt.lt(now))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected)
    }
}
