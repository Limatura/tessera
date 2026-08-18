//! Reading and writing shares.
//!
//! This module owns the access gate. Everything a public request needs in order
//! to decide "may this be handed out" is decided here, once, so that the three
//! public routes cannot disagree about it.

use std::collections::HashMap;

use sea_orm::{
    ActiveValue::Set,
    ColumnTrait as _, DatabaseConnection, EntityTrait as _, PaginatorTrait as _, QueryFilter as _,
    QueryOrder as _, QuerySelect as _,
    sea_query::{Expr, ExprTrait as _},
};
use tessera_core::{DomainError, RetrievalCode, ShareId, ShareTicketId, UserId};
use time::OffsetDateTime;

use crate::{
    auth::secret::Secret,
    db::{
        entity::{
            file_share::{self, Entity as FileShareEntity},
            share::{self, Entity as ShareEntity},
            share_ticket::{self, Entity as ShareTicketEntity},
            text_share::{self, Entity as TextShareEntity},
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

/// What a share is being created with.
///
/// # Why the details are an argument and not fields on [`Share`]
///
/// They used to be columns on `shares`, which worked while a share held exactly
/// one thing. It stops working the moment it can hold several — `filename` has
/// no honest value for a share with three files — so they live on the tables
/// that own them, and creating a share says what goes in those tables.
///
/// #42 turns the file case into a list. The enum is the shape that survives
/// that; a set of optional columns is not.
#[derive(Debug, Clone)]
pub enum NewContent {
    /// One file, wherever its bytes ended up.
    File {
        /// What it was uploaded as.
        filename: Option<String>,
        /// The media type the uploader claimed. Recorded, never echoed.
        content_type: Option<String>,
        /// How many bytes.
        size_bytes: u64,
        /// Where those bytes are.
        storage: StorageLocation,
    },
    /// Text, which is its own row.
    Text {
        /// Syntax-highlighting hint.
        language: Option<String>,
        /// The text itself.
        body: Vec<u8>,
    },
}

/// What a share holds, as the tables that own content describe it.
///
/// One shape for both kinds rather than an enum: every reader here wants the
/// same four answers — what it is called, what type it is, how big it is, where
/// the bytes are — and which fields are `None` follows from the kind, which the
/// share already states. An enum would make each of those four a `match` at
/// sixty call sites to say the same thing.
///
/// #42 turns files into many parts. This is the first part, which is what every
/// current reader means by "the file".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Content {
    /// What it was uploaded as. Always `None` for text.
    pub filename: Option<String>,
    /// The media type, when one was worked out. Always `None` for text.
    pub content_type: Option<String>,
    /// What it is written in, when the uploader said. Always `None` for a file.
    pub language: Option<String>,
    /// How many bytes.
    pub size_bytes: u64,
    /// Where those bytes are.
    pub storage: StorageLocation,
}

impl Content {
    fn from_file(model: &file_share::Model) -> Result<Self, ShareRepositoryError> {
        let fail = |source| CorruptRow {
            id: model.share_id.clone(),
            source,
        };
        Ok(Self {
            filename: model.filename.clone(),
            content_type: model.content_type.clone(),
            language: None,
            size_bytes: u64::try_from(model.size_bytes).map_err(|_| {
                fail(DomainError::Malformed {
                    field: "size_bytes",
                    reason: "must not be negative",
                })
            })?,
            storage: StorageLocation::parse(&model.storage, model.object_key.clone())
                .map_err(fail)?,
        })
    }

    /// A share whose content has been removed.
    ///
    /// Every reader already handles [`StorageLocation::Gone`] — it is what a
    /// reaped share looks like — so a row without its content answers the same
    /// way rather than needing a state of its own.
    const fn gone() -> Self {
        Self {
            filename: None,
            content_type: None,
            language: None,
            size_bytes: 0,
            storage: StorageLocation::Gone,
        }
    }

    fn from_text(model: &text_share::Model) -> Self {
        Self {
            filename: None,
            content_type: None,
            language: model.language.clone(),
            // The row is the content, so its length is the size and it is in the
            // database by definition. Nothing to reconcile with a column.
            size_bytes: u64::try_from(model.content.len()).unwrap_or(u64::MAX),
            storage: StorageLocation::Inline,
        }
    }
}

/// Something a client saw happen that the server could not.
///
/// # Why these two and not retrieval
///
/// A retrieval is a limit, so the server has to be the one counting it — a
/// client that reported its own would be a client deciding whether the limit
/// applied, and `curl /s/{code}/raw` would walk past it. These two have no side
/// effect and no server-side moment to observe: nothing reaches the server when
/// somebody reads a note on screen or presses a copy button.
///
/// So the trade is the other way round from `use_count`. These numbers can be
/// inflated by anybody who can call the endpoint, and that is tolerable because
/// nothing depends on them being right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareEvent {
    /// The content was shown to somebody.
    View,
    /// The content was copied to a clipboard.
    Copy,
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
    /// The content has been removed; only the row survives.
    ///
    /// A share that has expired or been used up is kept for a while so that
    /// following its link says "this expired" rather than "no such code". The
    /// *content* is not needed for that — [`crate::api::gate::open`] refuses a
    /// dead share before anything reads it — so the bytes go at death and the
    /// row goes later, which is the difference between holding a week of dead
    /// payloads and holding none.
    Gone,
}

impl StorageLocation {
    const GONE: &'static str = "gone";
    const INLINE: &'static str = "inline";
    const OBJECT: &'static str = "object";

    /// The blob-store key, when there is one.
    ///
    /// Saves every caller a `match` whose only other arm is "no key", and keeps
    /// the variant shape from leaking into code that just wants the key.
    ///
    /// `Gone` answers `None` like `Inline` does. That is deliberate: the two
    /// mean different things to a sweep, which asks
    /// [`is_reapable`](Self::is_reapable) instead, and the same thing to
    /// everybody else — there is no object to fetch.
    #[must_use]
    pub fn object_key(&self) -> Option<&str> {
        match self {
            Self::Gone | Self::Inline => None,
            Self::Object { key } => Some(key),
        }
    }

    /// Whether content still exists that a sweep would need to remove.
    #[must_use]
    pub const fn is_reapable(&self) -> bool {
        !matches!(*self, Self::Gone)
    }

    /// The value stored in the `storage` column.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match *self {
            Self::Gone => Self::GONE,
            Self::Inline => Self::INLINE,
            Self::Object { .. } => Self::OBJECT,
        }
    }

    fn parse(storage: &str, object_key: Option<String>) -> Result<Self, DomainError> {
        match (storage, object_key) {
            // `object_key` is kept rather than cleared, so a row still records
            // which object was removed — the one thing worth having if a
            // deletion ever turns out to have been wrong.
            (Self::GONE, _) => Ok(Self::Gone),
            (Self::INLINE, _) => Ok(Self::Inline),
            (Self::OBJECT, Some(key)) => Ok(Self::Object { key }),
            (Self::OBJECT, None) => Err(DomainError::Malformed {
                field: "object_key",
                reason: "must be present when storage is `object`",
            }),
            _ => Err(DomainError::Malformed {
                field: "storage",
                reason: "must be `inline`, `object` or `gone`",
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

    /// JSON describing how to draw the OG image.
    pub og_config: Option<String>,
    /// SHA-256 of the token that lets an anonymous uploader manage this share.
    pub management_token_hash: Option<String>,
    /// How many times the share page was looked at. Statistics only.
    pub view_count: u32,
    /// How many times the content was copied.
    pub copy_count: u32,
    /// How many bytes this share has sent out.
    ///
    /// Bytes that left this process, not bytes that reached anybody — see the
    /// migration for what a proxy in front does to the difference.
    pub bytes_served: u64,
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
            kind: ShareKind::parse(&model.kind).map_err(fail)?,

            password_hash: model.password_hash,
            use_limit: model.use_limit.map(unsigned),
            use_count: unsigned(model.use_count),
            expires_at: model.expires_at,
            disabled_at: model.disabled_at,

            title: model.title,

            og_config: model.og_config,
            management_token_hash: model.management_token_hash,
            view_count: unsigned(model.view_count),
            copy_count: unsigned(model.copy_count),
            bytes_served: u64::try_from(model.bytes_served).unwrap_or(0),
            last_accessed_at: model.last_accessed_at,

            created_at: model.created_at,
            updated_at: model.updated_at,
        })
    }

    fn to_active_model(&self) -> share::ActiveModel {
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

            og_config: Set(self.og_config.clone()),
            management_token_hash: Set(self.management_token_hash.clone()),
            view_count: Set(signed(self.view_count)),
            copy_count: Set(signed(self.copy_count)),
            bytes_served: Set(i64::try_from(self.bytes_served).unwrap_or(i64::MAX)),
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
    /// # Why a file share is written twice
    ///
    /// A file's name, type, size and location now live in `file_shares`, and
    /// they are still written to `shares` as well. That duplication is the point
    /// of this step rather than an oversight: the new table has to be true from
    /// the first row, and the code that reads it is a separate change. Writing
    /// only the new one would break every reader at once; writing only the old
    /// one would leave a table that the migration filled and nothing maintained,
    /// which is worse than not having it.
    ///
    /// The columns on `shares` go when the last reader does.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the insert fails — including when the
    /// code is already taken, which the unique index rejects.
    pub async fn insert(
        &self,
        share: &Share,
        content: &NewContent,
    ) -> Result<(), ShareRepositoryError> {
        ShareEntity::insert(share.to_active_model())
            .exec(&self.connection)
            .await?;

        match content {
            NewContent::File {
                filename,
                content_type,
                size_bytes,
                storage,
            } => {
                FileShareEntity::insert(file_share::ActiveModel {
                    // Derived from the share rather than generated, so that a
                    // share with one file keeps an identifier somebody can
                    // follow back.
                    id: Set(share.id.to_string()),
                    share_id: Set(share.id.to_string()),
                    position: Set(0),
                    filename: Set(filename.clone()),
                    content_type: Set(content_type.clone()),
                    size_bytes: Set(i64::try_from(*size_bytes).unwrap_or(i64::MAX)),
                    storage: Set(storage.as_str().to_owned()),
                    object_key: Set(storage.object_key().map(str::to_owned)),
                    created_at: Set(share.created_at),
                })
                .exec(&self.connection)
                .await?;
            }
            NewContent::Text { language, body } => {
                TextShareEntity::insert(text_share::ActiveModel {
                    share_id: Set(share.id.to_string()),
                    language: Set(language.clone()),
                    content: Set(body.clone()),
                })
                .exec(&self.connection)
                .await?;
            }
        }
        Ok(())
    }

    /// The text of a share, when it has been moved to its own table.
    ///
    /// `None` for a file, and for a text share whose content is still in the
    /// object store waiting for the sweep to fetch it back. A caller that gets
    /// `None` falls back to the old path, which is what makes the move something
    /// that can happen gradually rather than during an upgrade.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails.
    pub async fn text_of(
        &self,
        share_id: ShareId,
    ) -> Result<Option<text_share::Model>, ShareRepositoryError> {
        Ok(TextShareEntity::find_by_id(share_id.to_string())
            .one(&self.connection)
            .await?)
    }

    /// The files a share holds, in the order they were given.
    ///
    /// One row today, because a share still carries exactly one thing. It is
    /// here so the table has a reader and so the shape multi-file needs already
    /// exists.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails.
    pub async fn files_of(
        &self,
        share_id: ShareId,
    ) -> Result<Vec<file_share::Model>, ShareRepositoryError> {
        Ok(FileShareEntity::find()
            .filter(file_share::Column::ShareId.eq(share_id.to_string()))
            .order_by_asc(file_share::Column::Position)
            .all(&self.connection)
            .await?)
    }

    /// Every object this share has in the store.
    ///
    /// # Why a list
    ///
    /// One today, because a share holds one file. #42 makes it several, and a
    /// sweep that removed the first and stopped would leave the rest paid for
    /// and unreachable — a leak nothing would ever find, because the row that
    /// named them is gone.
    ///
    /// Text has no objects: its content is its row.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails.
    pub async fn object_keys_of(
        &self,
        share_id: ShareId,
    ) -> Result<Vec<String>, ShareRepositoryError> {
        Ok(FileShareEntity::find()
            .filter(file_share::Column::ShareId.eq(share_id.to_string()))
            .order_by_asc(file_share::Column::Position)
            .all(&self.connection)
            .await?
            .into_iter()
            .filter_map(|model| model.object_key)
            .collect())
    }

    /// Reads a share's content details from the table that owns them.
    ///
    /// # Why this exists rather than more columns on [`Share`]
    ///
    /// The columns describing content — filename, type, size, where the bytes
    /// are — moved off `shares` onto `file_shares` and `text_shares`. They are
    /// still written to both places, so this is the switch that decides which
    /// copy the program believes.
    ///
    /// # The fallback, and why it is only here
    ///
    /// A row can still be missing its subtype row: the `text_shares` backfill
    /// joined `share_contents`, so text that an older version streamed out to
    /// the object store was not copied across, and the sweep gets to those a
    /// few at a time. Serving such a share from the old columns is correct —
    /// they still say where the bytes are.
    ///
    /// The point of putting it in one function is that removing it later is one
    /// deletion. Spread across the handlers it would be sixty-odd, and the ones
    /// nobody found would quietly keep the old columns alive.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if a query fails or a row is not usable.
    pub async fn content_of(&self, share: &Share) -> Result<Content, ShareRepositoryError> {
        match share.kind {
            ShareKind::File => {
                let parts = self.files_of(share.id).await?;
                // No row means no content. That is not a share missing its
                // details — it is a share whose content has been removed, and
                // `Gone` is what every reader already knows to do with that.
                parts
                    .first()
                    .map_or_else(|| Ok(Content::gone()), Content::from_file)
            }
            ShareKind::Text => Ok(self
                .text_of(share.id)
                .await?
                .map_or_else(Content::gone, |text| Content::from_text(&text))),
        }
    }

    /// The same, for a page of shares, without a query each.
    ///
    /// The listing endpoint builds one response per share. Asking per share
    /// would make a page of fifty shares fifty-one queries, which is the shape
    /// of slowness that only appears once somebody has enough shares to care.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if a query fails or a row is not usable.
    pub async fn contents_for(
        &self,
        shares: &[Share],
    ) -> Result<HashMap<ShareId, Content>, ShareRepositoryError> {
        let ids: Vec<String> = shares.iter().map(|share| share.id.to_string()).collect();
        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        let files = FileShareEntity::find()
            .filter(file_share::Column::ShareId.is_in(ids.clone()))
            .order_by_asc(file_share::Column::Position)
            .all(&self.connection)
            .await?;
        let texts = TextShareEntity::find()
            .filter(text_share::Column::ShareId.is_in(ids))
            .all(&self.connection)
            .await?;

        let mut found: HashMap<String, Content> = HashMap::with_capacity(shares.len());
        for text in &texts {
            found.insert(text.share_id.clone(), Content::from_text(text));
        }
        for file in &files {
            // First position wins; the rest are #42's business. `or_insert`
            // rather than `insert` because the query is ordered by position and
            // a later part must not overwrite the first.
            if let std::collections::hash_map::Entry::Vacant(slot) =
                found.entry(file.share_id.clone())
            {
                slot.insert(Content::from_file(file)?);
            }
        }

        let mut resolved = HashMap::with_capacity(shares.len());
        for share in shares {
            let content = found
                .remove(&share.id.to_string())
                .unwrap_or_else(Content::gone);
            resolved.insert(share.id, content);
        }
        Ok(resolved)
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

    /// Ends every share one account uploaded.
    ///
    /// # Why this expires them rather than deleting them
    ///
    /// The bytes live in an object store, and removing five hundred objects
    /// inside one request is a request that times out half way through with no
    /// record of where it got to. Expiring them is one `UPDATE`; the reaper
    /// then removes the content a batch at a time, and the retention sweep
    /// removes the rows later — machinery that already exists and is already
    /// tested.
    ///
    /// # What it costs
    ///
    /// A visitor following one of these links is told it expired rather than
    /// that somebody removed it. That is the right amount to tell them: both
    /// mean the link is finished, and the difference is the operator's business.
    ///
    /// `disabled_at` would have been the more literal marker and is deliberately
    /// not used — the reaper leaves disabled shares alone, because disabling is
    /// something an operator may undo and re-enabling a share whose content has
    /// been destroyed gives them a live link to nothing.
    ///
    /// Returns how many were ended.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the update fails.
    pub async fn expire_all_for_user(
        &self,
        user_id: UserId,
        now: OffsetDateTime,
    ) -> Result<u64, ShareRepositoryError> {
        let result = ShareEntity::update_many()
            .col_expr(share::Column::ExpiresAt, now.into())
            .col_expr(share::Column::UpdatedAt, now.into())
            .filter(share::Column::UserId.eq(user_id.to_string()))
            // Ones that already finished are left alone, so the count means
            // "this many were still live" rather than "this many rows exist".
            .filter(
                share::Column::ExpiresAt
                    .is_null()
                    .or(share::Column::ExpiresAt.gt(now)),
            )
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected)
    }

    /// Every share on the instance, newest first, a page at a time.
    ///
    /// # Why this is paged and `list_for_user` is not
    ///
    /// One person's shares are bounded by what one person uploaded. Every share
    /// on the instance is not, and an administrator opening the page on a busy
    /// instance would otherwise pull the whole table across the wire to render
    /// the first twenty rows.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails or a row is not
    /// usable.
    pub async fn list_all(
        &self,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<Share>, ShareRepositoryError> {
        let models = ShareEntity::find()
            .order_by_desc(share::Column::CreatedAt)
            .order_by_desc(share::Column::Id)
            .limit(limit)
            .offset(offset)
            .all(&self.connection)
            .await?;
        models
            .into_iter()
            .map(|model| Share::from_model(model).map_err(Into::into))
            .collect()
    }

    /// How many shares exist, for the listing to page through.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails.
    pub async fn count_all(&self) -> Result<u64, ShareRepositoryError> {
        Ok(ShareEntity::find().count(&self.connection).await?)
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

    /// How much this instance has sent out, over every share it still has.
    ///
    /// Shares that have been swept take their tally with them, so this is "what
    /// the rows still here have sent", not "what this instance has ever sent".
    /// Keeping the latter would mean a counter that survives its subject, and
    /// nothing here has one of those yet.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails.
    pub async fn total_bytes_served(&self) -> Result<u64, ShareRepositoryError> {
        let total: Option<i64> = ShareEntity::find()
            .select_only()
            .column_as(share::Column::BytesServed.sum(), "total")
            .into_tuple()
            .one(&self.connection)
            .await?
            .flatten();
        Ok(u64::try_from(total.unwrap_or(0)).unwrap_or(0))
    }

    /// Adds to what a share has sent out.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the update fails.
    pub async fn add_bytes_served(
        &self,
        id: ShareId,
        bytes: u64,
    ) -> Result<(), ShareRepositoryError> {
        let bytes = i64::try_from(bytes).unwrap_or(i64::MAX);
        ShareEntity::update_many()
            .col_expr(
                share::Column::BytesServed,
                Expr::col(share::Column::BytesServed).add(bytes),
            )
            .filter(share::Column::Id.eq(id.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(())
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
    pub async fn record_event(
        &self,
        id: ShareId,
        kind: ShareEvent,
    ) -> Result<(), ShareRepositoryError> {
        let column = match kind {
            ShareEvent::View => share::Column::ViewCount,
            ShareEvent::Copy => share::Column::CopyCount,
        };
        ShareEntity::update_many()
            .col_expr(column, Expr::col(column).add(1))
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

    /// Lists shares that are finished with and old enough to remove.
    ///
    /// Two ways to be finished: past an expiry, or having been retrieved as many
    /// times as it was allowed to be. Cleaning only the first would leave the
    /// second accumulating forever, which is the same bug with a longer fuse —
    /// a burn-after-reading share is dead the moment it is read and has no
    /// expiry to notice.
    ///
    /// `cutoff` is the retention boundary rather than "now": a dead share is
    /// kept for a while so its link can still explain itself, and only then
    /// removed.
    ///
    /// Returned rather than deleted here because the payload lives in the blob
    /// store, which no database action reaches. The caller removes the object
    /// first and the row second.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails or a row is not
    /// usable.
    /// Shares whose content can go now, because nothing can reach it any more.
    ///
    /// Dead *at* `now`, not before some cutoff: the row is what has to linger so
    /// a visitor is told "this expired" instead of "no such code", and the bytes
    /// are not needed for that. [`crate::api::gate::open`] refuses a dead share
    /// before anything reads its content, so from the moment it dies the object
    /// is unreachable — keeping it is pure cost.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails or a row is corrupt.
    pub async fn find_reapable(
        &self,
        now: OffsetDateTime,
        limit: u64,
    ) -> Result<Vec<Share>, ShareRepositoryError> {
        let expired = share::Column::ExpiresAt
            .is_not_null()
            .and(share::Column::ExpiresAt.lt(now));
        let exhausted = share::Column::UseLimit
            .is_not_null()
            .and(Expr::col(share::Column::UseCount).gte(Expr::col(share::Column::UseLimit)));

        // Disabled shares are deliberately not here. Expiry and exhaustion are
        // one-way — no amount of waiting brings them back — but disabling is
        // something an operator did and may undo, and re-enabling a share whose
        // content has been destroyed would give them a live link to nothing.
        // The row's own retention still removes it eventually, content and all.
        let models = ShareEntity::find()
            .filter(expired.or(exhausted))
            // Storage now lives on `file_shares`. Text has no object, so a
            // text share is reapable on the strength of its own row alone —
            // which is why this is a `NOT EXISTS` over gone files rather than a
            // join that would drop every text share.
            .filter(
                share::Column::Kind.eq(ShareKind::Text.as_str()).or(
                    share::Column::Id.in_subquery(
                        sea_orm::sea_query::Query::select()
                            .column(file_share::Column::ShareId)
                            .from(FileShareEntity)
                            .and_where(
                                file_share::Column::Storage.ne(StorageLocation::GONE),
                            )
                            .to_owned(),
                    ),
                ),
            )
            .order_by_asc(share::Column::Id)
            .limit(limit)
            .all(&self.connection)
            .await?;
        models
            .into_iter()
            .map(|model| Share::from_model(model).map_err(Into::into))
            .collect()
    }

    /// Removes a share's content and records that it is gone.
    ///
    /// Both halves, because "content" means two different places. Content above
    /// `inline_threshold_bytes` is an object, which the caller has already
    /// deleted by the time it gets here; below it, the bytes are a row in
    /// `share_contents`, and that row is this function's to remove.
    ///
    /// Marking without deleting the inline row would be the dangerous half of a
    /// lie: the quota would report the space as free while the database still
    /// held it, and the gap would widen for as long as the retention window.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if either statement fails.
    pub async fn mark_content_removed(&self, id: ShareId) -> Result<(), ShareRepositoryError> {
        // `file_shares` says where the bytes are, and readers now believe it
        // over the column above. Marking only the column would leave the table
        // pointing at an object that has just been deleted, so a reader that
        // slipped past the gate would ask the store for it and get a `500`
        // where the honest answer is "this is gone".
        FileShareEntity::update_many()
            .col_expr(
                file_share::Column::Storage,
                Expr::value(StorageLocation::GONE.to_owned()),
            )
            .col_expr(
                file_share::Column::ObjectKey,
                Expr::value(Option::<String>::None),
            )
            .filter(file_share::Column::ShareId.eq(id.to_string()))
            .exec(&self.connection)
            .await?;
        // Text's content *is* its row, so removing the row is removing the
        // content — there is no separate marker to keep in step.
        TextShareEntity::delete_by_id(id.to_string())
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    /// Bytes this instance is currently holding.
    ///
    /// Counts only shares whose content still exists — a row kept around to
    /// answer "this expired" costs a few hundred bytes of database and no
    /// storage, and charging a quota for it would make the number a lie.
    ///
    /// Summed rather than tracked in a counter: a counter has to be kept in step
    /// with every insert, delete and failed upload, and the first time it drifts
    /// an instance either refuses uploads it has room for or accepts ones it
    /// does not.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails.
    pub async fn total_stored_bytes(&self) -> Result<u64, ShareRepositoryError> {
        // Two tables now, because content lives on the table that owns it. A
        // file's size is a column; text's size is the length of its row, and
        // `length()` returns bytes for a binary column on all three dialects —
        // checked against each of them rather than assumed.
        let files: Option<i64> = FileShareEntity::find()
            .filter(file_share::Column::Storage.ne(StorageLocation::GONE))
            .select_only()
            .column_as(file_share::Column::SizeBytes.sum(), "total")
            .into_tuple()
            .one(&self.connection)
            .await?
            .flatten();
        let texts: Option<i64> = TextShareEntity::find()
            .select_only()
            .column_as(
                Expr::expr(sea_orm::sea_query::Func::char_length(Expr::col(
                    text_share::Column::Content,
                )))
                .sum(),
                "total",
            )
            .into_tuple()
            .one(&self.connection)
            .await?
            .flatten();
        let total = files.unwrap_or(0).saturating_add(texts.unwrap_or(0));
        Ok(u64::try_from(total).unwrap_or(0))
    }

    /// How many shares exist, and how many of those a visitor could still open.
    ///
    /// Two numbers rather than one because they answer different questions. The
    /// total is what the database is holding; the live count is what the
    /// instance is actually serving, and the gap between them is what the sweep
    /// has not got to yet.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if either query fails.
    pub async fn counts_at(&self, now: OffsetDateTime) -> Result<(u64, u64), ShareRepositoryError> {
        let total = ShareEntity::find().count(&self.connection).await?;
        // Live means the gate would let somebody in, and each clause mirrors one
        // of `Share::availability_at` — not disabled, not past its expiry, not
        // exhausted. Both `expires_at` and `use_limit` are nullable, and in each
        // of them null is the *unlimited* case, so neither can be compared
        // numerically on its own.
        let live = ShareEntity::find()
            .filter(share::Column::DisabledAt.is_null())
            .filter(
                share::Column::ExpiresAt
                    .is_null()
                    .or(share::Column::ExpiresAt.gt(now)),
            )
            .filter(
                share::Column::UseLimit
                    .is_null()
                    .or(Expr::col(share::Column::UseCount).lt(Expr::col(share::Column::UseLimit))),
            )
            .count(&self.connection)
            .await?;
        Ok((total, live))
    }

    /// Shares whose rows can go, because they have been dead long enough that
    /// nobody is still following the link that made them.
    ///
    /// `cutoff` is the retention boundary rather than "now": a dead share is
    /// kept for a while so its link can still explain itself, and only then
    /// removed. Its *content* is not kept that long — see
    /// [`find_reapable`](Self::find_reapable), which runs first and takes the
    /// bytes at death.
    ///
    /// Returned rather than deleted here because the payload lives in the blob
    /// store, which no database action reaches. The caller removes the object
    /// first and the row second.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails or a row is not
    /// usable.
    pub async fn find_dead(
        &self,
        cutoff: OffsetDateTime,
        limit: u64,
    ) -> Result<Vec<Share>, ShareRepositoryError> {
        let expired = share::Column::ExpiresAt
            .is_not_null()
            .and(share::Column::ExpiresAt.lt(cutoff));
        let exhausted = share::Column::UseLimit
            .is_not_null()
            .and(Expr::col(share::Column::UseCount).gte(Expr::col(share::Column::UseLimit)))
            .and(share::Column::LastAccessedAt.is_not_null())
            .and(share::Column::LastAccessedAt.lt(cutoff));

        let models = ShareEntity::find()
            .filter(expired.or(exhausted))
            .order_by_asc(share::Column::Id)
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
