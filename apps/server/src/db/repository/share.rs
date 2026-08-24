//! Reading and writing shares.
//!
//! This module owns the access gate. Everything a public request needs in order
//! to decide "may this be handed out" is decided here, once, so that the three
//! public routes cannot disagree about it.

use std::collections::HashMap;

use sea_orm::{
    ActiveValue::Set,
    ColumnTrait as _, DatabaseConnection, EntityTrait as _, PaginatorTrait as _, QueryFilter as _,
    QueryOrder as _, QuerySelect as _, QueryTrait as _,
    sea_query::{Expr, ExprTrait as _, Func, Iden, Query, SelectStatement},
};
use tessera_core::{DomainError, RetrievalCode, RoleId, ShareId, ShareTicketId, UserId};
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
        repository::{quota::Scope, total, user::CorruptRow},
    },
};

/// `length()`, which every dialect here answers in bytes for a blob.
///
/// Written out rather than taken from `Func`, whose `char_length` renders as
/// `CHAR_LENGTH` — a function Postgres does not have for `bytea`.
struct Length;

impl Iden for Length {
    fn quoted(&self) -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("length")
    }

    fn unquoted(&self) -> &'static str {
        "length"
    }
}

/// The shares a scope owns, as a subquery on `shares.id`, or `None` for the
/// whole instance.
///
/// `Scope::Role` reads `shares.role_id` rather than joining through the
/// account: it is the role recorded when the share was made, so moving somebody
/// to another role does not silently move their old shares' bytes with them.
fn owned_shares(scope: Scope) -> Option<SelectStatement> {
    let (column, owner) = match scope {
        Scope::User(id) => (share::Column::UserId, id.to_string()),
        Scope::Role(id) => (share::Column::RoleId, id.to_string()),
        Scope::Instance => return None,
    };
    Some(
        Query::select()
            .column(share::Column::Id)
            .from(ShareEntity)
            .and_where(Expr::col(column).eq(owner))
            .take(),
    )
}

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

/// What a partial update says about one column: leave it, or write this.
///
/// Distinct from `Option`, which here would have to mean two things at once —
/// the value being written is itself nullable, so `None` cannot also stand for
/// "not mentioned".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change<T> {
    /// The caller did not mention this field.
    Keep,
    /// Write this, including when it is `None`.
    Set(Option<T>),
}

impl<T> Change<T> {
    /// Whether anything would be written.
    #[must_use]
    pub const fn is_set(&self) -> bool {
        matches!(self, Self::Set(_))
    }
}

/// One file being added to a share.
#[derive(Debug, Clone)]
pub struct NewFile {
    /// What it was uploaded as.
    pub filename: Option<String>,
    /// The media type the uploader claimed. Recorded, never echoed.
    pub content_type: Option<String>,
    /// How many bytes.
    pub size_bytes: u64,
    /// Where those bytes are.
    pub storage: StorageLocation,
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
    ///
    /// A share is created with one and grown with
    /// [`append_file`](ShareRepository::append_file) — rather than created with
    /// a list — because the files arrive in separate requests. One request
    /// carrying five means losing five when it drops at the fourth.
    File(NewFile),
    /// Text, which is its own row.
    Text {
        /// Syntax-highlighting hint.
        language: Option<String>,
        /// The text itself.
        body: Vec<u8>,
    },
}

/// One file in a share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePart {
    /// What it was uploaded as.
    pub filename: Option<String>,
    /// The media type, when one was worked out. Recorded, never echoed.
    pub content_type: Option<String>,
    /// How many bytes.
    pub size_bytes: u64,
    /// Where those bytes are.
    pub storage: StorageLocation,
}

/// The text of a share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextPart {
    /// Syntax-highlighting hint.
    pub language: Option<String>,
    /// How many bytes the text is.
    pub size_bytes: u64,
}

/// What a share holds, as the tables that own content describe it.
///
/// # Why an enum now
///
/// It was one flat struct while a share held exactly one thing, and the fields
/// that did not apply were `None`. A share can hold several files, and "several"
/// is not something optional fields express — a reader that wants the second
/// file has nowhere to ask.
///
/// The three cases are the three a reader has to handle differently: stream one
/// object, hand over a row, or explain that there is nothing left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// One or more files, in the order the uploader chose.
    Files(Vec<FilePart>),
    /// Text, which is its own row.
    Text(TextPart),
    /// The content has been removed; only the share survives.
    Gone,
}

impl Content {
    /// How many bytes the whole share holds.
    #[must_use]
    pub fn size_bytes(&self) -> u64 {
        match self {
            Self::Files(files) => files.iter().map(|file| file.size_bytes).sum(),
            Self::Text(text) => text.size_bytes,
            Self::Gone => 0,
        }
    }

    /// The one file, when there is exactly one.
    ///
    /// The case worth naming: a share with one file is downloaded as that file,
    /// resumable and under its own name. Two or more become an archive, which
    /// is neither.
    #[must_use]
    pub fn single_file(&self) -> Option<&FilePart> {
        match self {
            Self::Files(files) if files.len() == 1 => files.first(),
            _ => None,
        }
    }

    /// Every file, or nothing for text.
    #[must_use]
    pub fn files(&self) -> &[FilePart] {
        match self {
            Self::Files(files) => files,
            Self::Text(_) | Self::Gone => &[],
        }
    }

    fn from_files(models: &[file_share::Model]) -> Result<Self, ShareRepositoryError> {
        if models.is_empty() {
            return Ok(Self::Gone);
        }
        let parts = models
            .iter()
            .map(|model| {
                let fail = |source| CorruptRow {
                    id: model.share_id.clone(),
                    source,
                };
                Ok(FilePart {
                    filename: model.filename.clone(),
                    content_type: model.content_type.clone(),
                    size_bytes: u64::try_from(model.size_bytes).map_err(|_| {
                        fail(DomainError::Malformed {
                            field: "size_bytes",
                            reason: "must not be negative",
                        })
                    })?,
                    storage: StorageLocation::parse(&model.storage, model.object_key.clone())
                        .map_err(fail)?,
                })
            })
            .collect::<Result<Vec<_>, ShareRepositoryError>>()?;
        Ok(Self::Files(parts))
    }

    fn from_text(model: &text_share::Model) -> Self {
        Self::Text(TextPart {
            language: model.language.clone(),
            // The row is the content, so its length is the size.
            size_bytes: u64::try_from(model.content.len()).unwrap_or(u64::MAX),
        })
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
    /// Owner, or `None` for an anonymous upload — **or** for one whose account
    /// has since been deleted. [`role_id`](Self::role_id) tells those apart.
    pub user_id: Option<UserId>,
    /// The role the uploader was in when they uploaded it.
    ///
    /// A snapshot: moving somebody to another role later does not change which
    /// quota their old shares count against. Anonymous uploads carry `guest`.
    /// `None` on a share that predates the column or whose role was removed,
    /// and both mean the instance-wide ceiling decides.
    pub role_id: Option<RoleId>,
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
            role_id: model
                .role_id
                .as_deref()
                .map(RoleId::parse)
                .transpose()
                .map_err(fail.clone())?,
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
            role_id: Set(self.role_id.map(|id| id.to_string())),
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
            NewContent::File(file) => {
                FileShareEntity::insert(file_share::ActiveModel {
                    // Derived from the share rather than generated, so that a
                    // share with one file keeps an identifier somebody can
                    // follow back.
                    id: Set(share.id.to_string()),
                    share_id: Set(share.id.to_string()),
                    position: Set(0),
                    filename: Set(file.filename.clone()),
                    content_type: Set(file.content_type.clone()),
                    size_bytes: Set(i64::try_from(file.size_bytes).unwrap_or(i64::MAX)),
                    storage: Set(file.storage.as_str().to_owned()),
                    object_key: Set(file.storage.object_key().map(str::to_owned)),
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

    /// Adds one more file to a share.
    ///
    /// # Why the position is worked out here
    ///
    /// It is the count of what is already there, and reading it in the handler
    /// would leave a window for two uploads to pick the same one. Deciding it
    /// in the same statement that writes the row is what stops two files
    /// claiming to be third.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if a query fails.
    pub async fn append_file(
        &self,
        share_id: ShareId,
        file: &NewFile,
        now: OffsetDateTime,
    ) -> Result<u32, ShareRepositoryError> {
        let position = FileShareEntity::find()
            .filter(file_share::Column::ShareId.eq(share_id.to_string()))
            .count(&self.connection)
            .await?;
        FileShareEntity::insert(file_share::ActiveModel {
            // Its own identifier, unlike the first file's — that one borrows the
            // share's so a single-file share stays followable.
            id: Set(ShareId::generate().to_string()),
            share_id: Set(share_id.to_string()),
            position: Set(i64::try_from(position).unwrap_or(i64::MAX)),
            filename: Set(file.filename.clone()),
            content_type: Set(file.content_type.clone()),
            size_bytes: Set(i64::try_from(file.size_bytes).unwrap_or(i64::MAX)),
            storage: Set(file.storage.as_str().to_owned()),
            object_key: Set(file.storage.object_key().map(str::to_owned)),
            created_at: Set(now),
        })
        .exec(&self.connection)
        .await?;
        Ok(u32::try_from(position).unwrap_or(u32::MAX))
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

    /// Reads what a share holds, from the table that owns it.
    ///
    /// # Why this exists rather than more columns on [`Share`]
    ///
    /// The columns describing content — filename, type, size, where the bytes
    /// are — only make sense for one kind of share, and half of them were
    /// always `NULL`. They live on `file_shares` and `text_shares` now, and
    /// `kind` is what says which to read.
    ///
    /// A share whose subtype row is gone reads as [`Content::Gone`] rather than
    /// failing: that is what a share whose content was swept looks like, and
    /// the listing still has to be able to show it.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if a query fails or a row is not usable.
    pub async fn content_of(&self, share: &Share) -> Result<Content, ShareRepositoryError> {
        match share.kind {
            ShareKind::File => Content::from_files(&self.files_of(share.id).await?),
            ShareKind::Text => Ok(self
                .text_of(share.id)
                .await?
                .map_or(Content::Gone, |text| Content::from_text(&text))),
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

        let mut texts_by_share: HashMap<String, Content> = HashMap::with_capacity(texts.len());
        for text in &texts {
            texts_by_share.insert(text.share_id.clone(), Content::from_text(text));
        }
        // Grouped rather than taken one at a time: the query is ordered by
        // position, so pushing in order keeps the uploader's order.
        let mut files_by_share: HashMap<String, Vec<file_share::Model>> = HashMap::new();
        for file in files {
            files_by_share
                .entry(file.share_id.clone())
                .or_default()
                .push(file);
        }

        let mut resolved = HashMap::with_capacity(shares.len());
        for share in shares {
            let key = share.id.to_string();
            let content = match share.kind {
                ShareKind::File => {
                    Content::from_files(files_by_share.get(&key).map_or(&[][..], Vec::as_slice))?
                }
                ShareKind::Text => texts_by_share.remove(&key).unwrap_or(Content::Gone),
            };
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

    /// How many shares an account has, for its listing to page through.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails.
    pub async fn count_for_user(&self, user_id: UserId) -> Result<u64, ShareRepositoryError> {
        Ok(ShareEntity::find()
            .filter(share::Column::UserId.eq(user_id.to_string()))
            .count(&self.connection)
            .await?)
    }

    /// Lists an account's shares, newest first, a page at a time.
    ///
    /// Paged for the same reason [`ShareRepository::list_all`] is: an account
    /// driven by a script is not bounded by what a person would upload by
    /// hand, and an unpaged listing only becomes a problem once somebody has
    /// enough shares to have stopped watching.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails or a row is not
    /// usable.
    pub async fn list_for_user(
        &self,
        user_id: UserId,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<Share>, ShareRepositoryError> {
        let models = ShareEntity::find()
            .filter(share::Column::UserId.eq(user_id.to_string()))
            .order_by_desc(share::Column::CreatedAt)
            // ULIDs are time-ordered, so this breaks a tie the way a reader
            // expects. Without it two shares created in the same microsecond
            // come back in whatever order the database felt like, and a listing
            // that reshuffles between requests looks broken.
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

    /// Rewrites the settings a share can be changed after the fact.
    ///
    /// One statement, and only the columns the caller named — a read-modify-write
    /// here would let two people editing the same share each undo the other's
    /// field without either seeing a conflict.
    ///
    /// Returns whether a row matched, so a caller can tell "changed nothing"
    /// from "no such share".
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the update fails.
    pub async fn update_settings(
        &self,
        id: ShareId,
        expires_at: Change<OffsetDateTime>,
        use_limit: Change<u32>,
        password_hash: Change<String>,
        now: OffsetDateTime,
    ) -> Result<bool, ShareRepositoryError> {
        let mut update = ShareEntity::update_many()
            .col_expr(share::Column::UpdatedAt, now.into())
            .filter(share::Column::Id.eq(id.to_string()));
        if let Change::Set(value) = expires_at {
            update = update.col_expr(share::Column::ExpiresAt, value.into());
        }
        if let Change::Set(value) = use_limit {
            update = update.col_expr(
                share::Column::UseLimit,
                value
                    .map(|limit| i32::try_from(limit).unwrap_or(i32::MAX))
                    .into(),
            );
        }
        if let Change::Set(value) = password_hash {
            update = update.col_expr(share::Column::PasswordHash, value.into());
        }
        Ok(update.exec(&self.connection).await?.rows_affected > 0)
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
            .column_as(
                total::bytes(
                    self.connection.get_database_backend(),
                    Expr::col(share::Column::BytesServed),
                ),
                "total",
            )
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
                        Query::select()
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
    /// Both halves, because "content" means two different places. A file is an
    /// object, which the caller has already deleted by the time it gets here;
    /// text is a row in `text_shares`, and that row is this function's to
    /// remove.
    ///
    /// Marking without deleting the row would be the dangerous half of a lie:
    /// the quota would report the space as free while the database still held
    /// it, and the gap would widen for as long as the retention window.
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
        self.stored_bytes(Scope::Instance).await
    }

    /// The same, for one account, one role, or the whole instance.
    ///
    /// Content lives on `file_shares` and `text_shares`, which carry no owner —
    /// the owner is on `shares`. Rather than declare a relation for one query,
    /// the restriction goes in as a subquery on the share id, which is the
    /// primary key on one side and an indexed foreign key on the other.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if a query fails.
    pub async fn stored_bytes(&self, scope: Scope) -> Result<u64, ShareRepositoryError> {
        // Two tables now, because content lives on the table that owns it. A
        // file's size is a column; text's size is the length of its row.
        //
        // `length()`, not `CHAR_LENGTH()`. The column is a blob on every
        // dialect — BLOB, bytea, LONGBLOB — and `length()` is the one spelling
        // all three answer in bytes for one. Postgres has no `char_length` for
        // `bytea` at all: it does not return the wrong number, it refuses to
        // plan the query, so on Postgres this whole total failed the moment an
        // operator set `storage.max_total_bytes`. Found by running the backend
        // suite against a real Postgres rather than reasoning about it.
        let files: Option<i64> = FileShareEntity::find()
            .filter(file_share::Column::Storage.ne(StorageLocation::GONE))
            .apply_if(owned_shares(scope), |query, owned| {
                query.filter(file_share::Column::ShareId.in_subquery(owned))
            })
            .select_only()
            .column_as(
                total::bytes(
                    self.connection.get_database_backend(),
                    Expr::col(file_share::Column::SizeBytes),
                ),
                "total",
            )
            .into_tuple()
            .one(&self.connection)
            .await?
            .flatten();
        let texts: Option<i64> = TextShareEntity::find()
            .apply_if(owned_shares(scope), |query, owned| {
                query.filter(text_share::Column::ShareId.in_subquery(owned))
            })
            .select_only()
            .column_as(
                total::bytes(
                    self.connection.get_database_backend(),
                    Func::cust(Length)
                        .arg(Expr::col(text_share::Column::Content))
                        .into(),
                ),
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
    /// How many dead shares a cutoff would take, and how much they hold.
    ///
    /// The same condition [`ShareRepository::find_dead`] sweeps on, counted
    /// instead of listed and with no page limit — this answers "what would
    /// shortening the retention destroy", and a number capped at one page
    /// would understate it exactly when the answer matters.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails.
    pub async fn count_dead_before(
        &self,
        cutoff: OffsetDateTime,
    ) -> Result<(u64, u64), ShareRepositoryError> {
        let count = ShareEntity::find()
            .filter(Self::dead_before(cutoff))
            .count(&self.connection)
            .await?;
        // Files only. Text lives in its own table and is measured by row
        // length, which is a second query for a number that rounds to nothing
        // beside the files — and this figure exists to answer "how much disk
        // would this free", where a paste does not move the answer.
        let bytes: Option<i64> = FileShareEntity::find()
            .filter(file_share::Column::Storage.ne(StorageLocation::GONE))
            .filter(
                file_share::Column::ShareId.in_subquery(
                    sea_orm::QuerySelect::query(
                        &mut ShareEntity::find()
                            .filter(Self::dead_before(cutoff))
                            .select_only()
                            .column(share::Column::Id),
                    )
                    .take(),
                ),
            )
            .select_only()
            .column_as(
                total::bytes(
                    self.connection.get_database_backend(),
                    Expr::col(file_share::Column::SizeBytes),
                ),
                "total",
            )
            .into_tuple()
            .one(&self.connection)
            .await?
            .flatten();
        Ok((count, u64::try_from(bytes.unwrap_or(0)).unwrap_or(0)))
    }

    /// A share is dead when it has run out of time, or out of uses and has not
    /// been touched since. Written once because the sweep and the count that
    /// warns about the sweep must agree — two spellings of this would mean the
    /// warning describes a different set from the one that gets removed.
    fn dead_before(cutoff: OffsetDateTime) -> sea_orm::Condition {
        use sea_orm::Condition;
        let expired = share::Column::ExpiresAt
            .is_not_null()
            .and(share::Column::ExpiresAt.lt(cutoff));
        let exhausted = share::Column::UseLimit
            .is_not_null()
            .and(Expr::col(share::Column::UseCount).gte(Expr::col(share::Column::UseLimit)))
            .and(share::Column::LastAccessedAt.is_not_null())
            .and(share::Column::LastAccessedAt.lt(cutoff));
        Condition::any().add(expired).add(exhausted)
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
        let models = ShareEntity::find()
            .filter(Self::dead_before(cutoff))
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
    /// Pushes a ticket's expiry out, without letting it outlive its ceiling.
    ///
    /// # Why the arithmetic is here and not in SQL
    ///
    /// `created_at + ttl` is a date addition, and the three dialects store the
    /// column differently — TEXT on SQLite, `TIMESTAMPTZ` on Postgres,
    /// `DATETIME(6)` on MySQL. One expression that means the same thing on all
    /// three is not worth writing for a value the caller already has in hand.
    ///
    /// Two range requests extending at once is not a race worth closing: both
    /// compute nearly the same instant, and the `WHERE` keeps whichever is
    /// later. That is unlike the use count, where two writers must not both
    /// succeed.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the update fails.
    pub async fn touch(
        &self,
        ticket: &ShareTicket,
        now: OffsetDateTime,
        idle: time::Duration,
        ttl: time::Duration,
    ) -> Result<(), ShareRepositoryError> {
        let ceiling = ticket.created_at.saturating_add(ttl);
        let wanted = Ord::min(now.saturating_add(idle), ceiling);
        ShareTicketEntity::update_many()
            .col_expr(share_ticket::Column::ExpiresAt, wanted.into())
            // Never backwards. Without this, a request that arrives late with a
            // stale clock could shorten a ticket somebody is still using.
            .filter(share_ticket::Column::ExpiresAt.lt(wanted))
            .filter(share_ticket::Column::Id.eq(ticket.id.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(())
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

    /// Every authorisation outstanding for one share.
    ///
    /// Only the tests ask this — the serving path looks a ticket up by its
    /// secret, never by share. It is here rather than in a test helper because
    /// a test that reached into the table directly would stop noticing when the
    /// table changed shape.
    ///
    /// # Errors
    ///
    /// Returns [`ShareRepositoryError`] if the query fails or a row is not
    /// usable.
    pub async fn for_share(
        &self,
        share_id: ShareId,
    ) -> Result<Vec<ShareTicket>, ShareRepositoryError> {
        ShareTicketEntity::find()
            .filter(share_ticket::Column::ShareId.eq(share_id.to_string()))
            .order_by_desc(share_ticket::Column::CreatedAt)
            .all(&self.connection)
            .await?
            .iter()
            .map(|model| ShareTicket::from_model(model).map_err(Into::into))
            .collect()
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
