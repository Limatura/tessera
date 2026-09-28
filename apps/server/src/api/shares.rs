//! Creating, listing and deleting shares.
//!
//! # How content arrives
//!
//! The body *is* the content, and the metadata comes from query parameters. One
//! rule, and it works for everything that has to post here:
//!
//! ```text
//! curl --data-binary @project.zip 'http://host/api/v1/shares?filename=project.zip'
//! fetch(url, { method: 'POST', body: file })   // the browser streams a File
//! ```
//!
//! `multipart/form-data` is not accepted. A `fetch` with a `File` body streams
//! natively, so the only caller that would need multipart is a plain HTML
//! `<form>`. Supporting one costs a parser, a dependency and a buffering path.

// utoipa's `params(..)` emits a fully-qualified path, and the generated code
// lands outside the function it decorates, so the suppression must be
// module-wide. Dropping `params(..)` would drop the query parameters from the
// contract.
#![expect(
    unused_qualifications,
    reason = "emitted by utoipa's params(..), not written here"
)]

use std::time::Duration;

use axum::{
    Json,
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use http_body_util::BodyExt as _;
use object_store::{ObjectStoreExt as _, PutPayload, path::Path as ObjectPath};
use serde::{Deserialize, Serialize};
use tessera_core::{Password, Permission, RetrievalCode, RoleId, ShareId, UserId};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};

use crate::{
    api::{
        Patch, SHARES_TAG,
        extract::{ApiJson, ApiPath, ApiQuery},
        gate, patched, sealed,
    },
    auth::{
        extract::{ClientAddress, CurrentUser, MaybeUser},
        password,
        proxy::Origin,
        secret::Secret,
        throttle::Decision,
    },
    db::repository::{
        quota::Scope,
        share::{
            Change, Content, Encryption, NewContent, NewFile, Share, ShareEvent, ShareKind,
            StorageLocation,
        },
        user::User,
    },
    error::{Error, ErrorCode, Result, Tier},
    state::AppState,
};

/// Attempts to find an unused retrieval code before giving up.
///
/// Collisions are vanishingly rare at the default length, so repeated failures
/// mean the code space is crowded and the operator should hear about it.
const CODE_ATTEMPTS: u8 = 8;

/// Longest `filename` and `title` a share may carry, in characters.
///
/// Tied to the MySQL column width. Without a check here the backends disagree:
/// SQLite and Postgres store the value, MySQL refuses the row, so the same
/// request gets `201` or `500` depending on the operator's database.
///
/// 191 because that is what utf8mb4 allows in an indexable `VARCHAR`.
const MAX_TEXT_FIELD: usize = 191;

/// Longest `language` hint, matching its narrower column.
const MAX_LANGUAGE: usize = 64;

/// The request body of `POST /shares`: the content itself, as bytes.
///
/// A named type only so the contract can say `format: binary`. Declaring the
/// body as `Vec<u8>` makes utoipa emit an *array of integers*, which a docs UI
/// renders as a JSON array editor and a generated client types as `number[]`.
///
/// An empty body is refused with a `400`, and that cannot be expressed here.
/// `min_length` is field-level, and the `value_type` above overrides the
/// field's schema; drop it and the body stops being binary. So the handler
/// enforces it, and a generator reading this will think an empty body is
/// valid.
#[derive(Debug, ToSchema)]
#[schema(value_type = String, format = Binary)]
pub struct ShareBody(
    #[expect(dead_code, reason = "a schema marker; the body is read as a stream")] Vec<u8>,
);

/// Metadata for `POST /shares`, supplied as query parameters.
// `parameter_in = Query` is load-bearing. Without it `IntoParams` defaults to
// `Path`: the document puts these in a path with no placeholders for them and
// marks all five required, though the handler treats them as optional. No
// client built from that can construct a valid request.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct CreateShareParams {
    /// `link` to make a share that sends whoever opens it to the web address in
    /// the body, an `http` or `https` URL. Omit for a file or a note, which the
    /// other parameters tell apart.
    #[param(example = "link")]
    pub kind: Option<String>,
    /// Original file name. Its presence is what makes this a file share rather
    /// than a note.
    #[param(example = "project.zip")]
    pub filename: Option<String>,
    /// Display name, shown on the share page and in the link preview.
    #[param(example = "Q3 report")]
    pub title: Option<String>,
    /// Syntax-highlighting hint for a text share.
    #[param(example = "typescript")]
    pub language: Option<String>,
    /// How long the share should last. Omit to use the instance's default.
    #[param(example = 24)]
    pub expires_in_hours: Option<u64>,
    /// How many times it may be retrieved. Omit for unlimited; `1` is
    /// burn-after-reading.
    #[param(example = 3)]
    pub max_uses: Option<u32>,
    /// `link` or `password` to make an end-to-end encrypted share, whose body
    /// arrives already sealed. See `docs/encryption.md`. Refused unless
    /// `/capabilities` says `encryption`.
    #[param(example = "link")]
    pub encryption: Option<String>,
    /// For `encryption=password`: the salt the password was stretched with, as
    /// 16 bytes of unpadded URL-safe base64. Handed to anyone who asks.
    pub key_salt: Option<String>,
    /// For an encrypted file: its name and media type, sealed. Replaces
    /// `filename` and, like it, makes this a file share.
    pub sealed_name: Option<String>,
    /// For an encrypted share: its title and a note's language, sealed.
    /// Replaces `title` and `language`.
    pub sealed_details: Option<String>,
}

/// A share as the API describes it. Never carries the content.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ShareResponse {
    /// Stable identifier.
    #[schema(example = "01K1ZQK9V8QF5S3T4M2N6P7R8S")]
    pub id: String,
    /// The retrieval code, as a person reads it.
    #[schema(example = "0ZPQ4M7K")]
    pub code: String,
    /// Where the share lives, relative to this instance's origin.
    ///
    /// Relative by design. The server does not know what host it is reached
    /// through, and guessing wrong behind a reverse proxy produces broken
    /// links. The caller already knows the origin it called.
    #[schema(example = "/s/0ZPQ4M7K")]
    pub path: String,
    /// `text`, `file` or `link`.
    #[schema(example = "file")]
    pub kind: String,
    /// Display name.
    pub title: Option<String>,
    /// Every file this share holds. Empty for text and for a locked share.
    pub files: Vec<SharedFile>,
    /// Syntax-highlighting hint.
    pub language: Option<String>,
    /// Size of the content in bytes.
    #[schema(example = 1_048_576_u64)]
    pub size_bytes: u64,
    /// Whether a password must be presented to retrieve it.
    pub has_password: bool,
    /// `link` or `password` for an end-to-end encrypted share, `null` for one
    /// the server can read.
    #[schema(example = "link")]
    pub encryption: Option<String>,
    /// The salt a password-derived key was stretched with, for `password`.
    pub key_salt: Option<String>,
    /// The title and a note's language, sealed, on an encrypted share.
    pub sealed_details: Option<String>,
    /// Whether it is waiting for the malware scanner. Until it is done,
    /// downloads answer `423` with `Retry-After`.
    pub scanning: bool,
    /// How many retrievals remain, or `null` for unlimited.
    pub uses_remaining: Option<u32>,
    /// How many times it has been retrieved.
    ///
    /// Counted by the server on the transfer path, so this is the number to
    /// trust. The two below are what a client reported.
    pub use_count: u32,
    /// How many times a client reported showing it.
    pub view_count: u32,
    /// How many times a client reported it being copied.
    pub copy_count: u32,
    /// When it was last retrieved, RFC 3339.
    ///
    /// Answers the question people ask of their own share: has it been picked
    /// up yet?
    pub last_accessed_at: Option<String>,
    /// When it stops being served, RFC 3339, or `null` to live until deleted.
    pub expires_at: Option<String>,
    /// When it was created, RFC 3339.
    pub created_at: String,

    /// Token that lets an anonymous uploader delete this share.
    ///
    /// **Shown once, at creation, and only for an anonymous upload.** Only its
    /// digest is stored. Without it, a mistaken anonymous upload could never be
    /// taken back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub management_token: Option<String>,
}

/// One file in a share, as the API describes it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SharedFile {
    /// Its position in the share, from zero.
    ///
    /// Sent explicitly, so a client that sorts or filters the list can still
    /// restore the uploader's order.
    pub position: u32,
    /// What it was uploaded as.
    #[schema(example = "holiday.jpg")]
    pub filename: Option<String>,
    /// Its name and media type, sealed, in an encrypted share.
    pub sealed_name: Option<String>,
    /// Its size in bytes.
    ///
    /// For an encrypted share, the sealed size, which is what a download sends.
    pub size_bytes: u64,
}

impl SharedFile {
    fn list(content: &Content) -> Vec<Self> {
        content
            .files()
            .iter()
            .enumerate()
            .map(|(position, file)| Self {
                position: u32::try_from(position).unwrap_or(u32::MAX),
                filename: file.filename.clone(),
                sealed_name: file.sealed_name.clone(),
                size_bytes: file.size_bytes,
            })
            .collect()
    }
}

impl ShareResponse {
    pub(crate) fn from_share(
        share: &Share,
        content: &Content,
        management_token: Option<String>,
    ) -> Self {
        Self {
            id: share.id.to_string(),
            code: share.code.as_str().to_owned(),
            path: format!("/s/{}", share.code),
            kind: share.kind.as_str().to_owned(),
            title: share.title.clone(),
            files: SharedFile::list(content),
            language: match &content {
                Content::Text(text) => text.language.clone(),
                Content::Files(_) | Content::Gone => None,
            },
            size_bytes: content.size_bytes(),
            has_password: share.needs_password(),
            encryption: share.encryption.as_ref().map(|e| e.as_str().to_owned()),
            key_salt: share
                .encryption
                .as_ref()
                .and_then(Encryption::salt)
                .map(str::to_owned),
            sealed_details: share.sealed_details.clone(),
            scanning: content.awaiting_scan(),
            uses_remaining: share
                .use_limit
                .map(|limit| limit.saturating_sub(share.use_count)),
            use_count: share.use_count,
            view_count: share.view_count,
            copy_count: share.copy_count,
            last_accessed_at: share.last_accessed_at.map(format_timestamp),
            expires_at: share.expires_at.map(format_timestamp),
            created_at: format_timestamp(share.created_at),
            management_token,
        }
    }
}

fn format_timestamp(value: OffsetDateTime) -> String {
    value
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| value.unix_timestamp().to_string())
}

/// Creates a share from the request body.
///
/// # Errors
///
/// `401` when the instance requires an account to upload, `400` when a parameter
/// is unusable, `413` when the body exceeds the configured limit, `409` when no
/// unused code could be found.
#[utoipa::path(
    post,
    path = "/shares",
    operation_id = "create_share",
    tag = SHARES_TAG,
    params(CreateShareParams),
    request_body(
        content = ShareBody,
        description = "The content itself. Send the file as the raw body: no wrapper, no encoding.",
        content_type = "application/octet-stream",
    ),
    responses(
        // The code is random, so nothing can derive one. These links are how a
        // contract-driven test chains create into the three operations below,
        // and how a client learns which field to carry.
        (status = 201, description = "Created", body = ShareResponse,
            links(
                ("describe" = (operation_id = "describe_share",
                    parameters(("code" = "$response.body#/code")))),
                ("unlock" = (operation_id = "unlock_share",
                    parameters(("code" = "$response.body#/code")))),
                ("delete" = (operation_id = "delete_share",
                    parameters(("code" = "$response.body#/code")))),
            )),
        (status = 400, description = "A parameter is unusable", body = crate::error::ErrorBody),
        (status = 401, description = "This instance requires an account to upload", body = crate::error::ErrorBody),
        (status = 403, description = "The credential does not carry `shares:create`", body = crate::error::ErrorBody),
        (status = 413, description = "Body exceeds the configured limit", body = crate::error::ErrorBody),
        (status = 507, description = "Out of room; the body says whether that is the account's quota, its role's, or the instance's", body = crate::error::ErrorBody),
    ),
    security((), ("session" = []), ("bearer" = [])),
)]
pub async fn create(
    State(state): State<AppState>,
    MaybeUser(current): MaybeUser,
    ApiQuery(mut params): ApiQuery<CreateShareParams>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response> {
    if current.is_none() && !state.config().features.anonymous_upload {
        return Err(Error::Unauthorized);
    }
    // An anonymous caller has no permissions to check, so the setting above
    // decides. For a signed-in one, this keeps a token minted for a screenshot
    // tool from doing anything else.
    if let Some(user) = current.as_ref() {
        user.require(Permission::SharesCreate)?;
    }

    if params.max_uses == Some(0) {
        return Err(Error::out_of_range("max_uses", Some(1), None).with_message(
            "`max_uses` must be at least 1; omit it for a share that never runs out",
        ));
    }
    let Described {
        kind,
        filename,
        title,
        language,
        encryption,
        sealed_name,
        sealed_details,
    } = described(&state, &mut params)?;

    let now = OffsetDateTime::now_utc();
    let expires_at = resolve_expiry(&state, params.expires_in_hours, now)?;
    let password_hash = match share_password(&headers)? {
        Some(password) => Some(password::hash(password, state.config().auth.hashing).await?),
        None => None,
    };

    let id = ShareId::generate();

    // Held until the row exists. Between the last byte and the insert, the
    // bytes are in the store and in no table, so dropping the guard there lets
    // another upload claim room that is already spent. That was the
    // `[201, 201]` this fixed.
    let (
        Ingested {
            storage,
            size_bytes,
            inline,
            segments,
        },
        _arriving,
    ) = ingest(
        &state,
        Ingest {
            id,
            kind,
            owner: current.as_ref().map(|current| &current.user),
            now,
        },
        body,
    )
    .await?;

    // Nothing to hand out, and the retrieval code it would occupy is not free.
    if size_bytes == 0 {
        return Err(Error::new(ErrorCode::BodyEmpty));
    }

    // Anonymous uploads get a token so their author can take one back. A
    // signed-in uploader already has their account.
    let management_secret = current.is_none().then(Secret::generate_session);

    let share = Share {
        id,
        // Replaced on each attempt below. The id and object key do not change,
        // so a retry costs one insert, not a re-upload.
        code: RetrievalCode::parse("0000").unwrap_or_else(|_| unreachable!("four zeroes parse")),
        user_id: current.as_ref().map(|user| user.user.id),
        // Already in hand for a signed-in caller, and the guest role for an
        // anonymous one. Neither touches the database.
        role_id: charged_role_of(current.as_ref().map(|current| &current.user)),
        kind,
        encryption: encryption.clone(),
        password_hash,
        use_limit: params.max_uses,
        use_count: 0,
        expires_at,
        disabled_at: None,
        title,
        sealed_details,
        og_config: None,
        management_token_hash: management_secret.as_ref().map(Secret::digest),
        view_count: 0,
        copy_count: 0,
        bytes_served: 0,
        bytes_redirected: 0,
        last_accessed_at: None,
        created_at: now,
        updated_at: now,
    };

    let scan = crate::scan::initial(&state, encryption.is_some());
    let content = match kind {
        ShareKind::File => NewContent::File(NewFile {
            segments,
            filename,
            // What the uploader claimed. Recorded but never echoed; serving an
            // attacker-chosen media type back turns a `text/html` upload into
            // stored XSS on our own origin.
            content_type: recorded_type(encryption.is_some(), &headers),
            sealed_name,
            scan,
            size_bytes,
            storage,
        }),
        ShareKind::Text | ShareKind::Link => written(kind, language, inline, scan)?,
    };
    let share = insert_with_fresh_code(&state, share, &content).await?;
    let content = state.shares.content_of(&share).await?;
    let response = ShareResponse::from_share(
        &share,
        &content,
        management_secret.map(|secret| secret.expose().to_owned()),
    );
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

/// The row a note or a link is kept in.
///
/// `body` is always present by the time a share exists: text is its own row,
/// and `ingest` refuses anything it could not hold.
fn written(
    kind: ShareKind,
    language: Option<String>,
    body: Option<Vec<u8>>,
    scan: Option<crate::db::repository::scan::Scan>,
) -> Result<NewContent> {
    let body = body.unwrap_or_default();
    Ok(if kind == ShareKind::Link {
        // An address, not content: there is nothing in it for a scanner to
        // find, and what it points at can change after any verdict.
        NewContent::Text {
            language: None,
            body: link_target(&body)?.into_bytes(),
            scan: None,
        }
    } else {
        NewContent::Text {
            language,
            body,
            scan,
        }
    })
}

/// The longest web address a link share takes, in bytes. Past two thousand
/// or so, browsers and servers along the way start refusing it anyway.
const MAX_LINK_BYTES: usize = 2048;

/// The address a link share sends people to, checked: `http` or `https`, with
/// a host.
///
/// Anything else is refused rather than stored. A `javascript:` or `data:`
/// address in a `Location` header is at best a broken redirect, and this
/// server's name on it makes it worth trying.
fn link_target(body: &[u8]) -> Result<String> {
    let refused = || {
        Error::invalid("url", "http_url")
            .with_message("the body must be one `http` or `https` address")
    };
    if body.len() > MAX_LINK_BYTES {
        return Err(Error::new(ErrorCode::ValueLength)
            .with_detail("field", "url")
            .with_detail("max", MAX_LINK_BYTES)
            .with_message(format!(
                "the address must be at most {MAX_LINK_BYTES} bytes"
            )));
    }
    let text = std::str::from_utf8(body).map_err(|_| refused())?.trim();
    let url = url::Url::parse(text).map_err(|_| refused())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) {
        return Err(refused());
    }
    Ok(url.into())
}

/// What a new share says about itself, each part checked.
struct Described {
    /// `link` if asked for; otherwise a name, sealed or not, is what makes a
    /// share a file share.
    kind: ShareKind,
    filename: Option<String>,
    title: Option<String>,
    language: Option<String>,
    encryption: Option<Encryption>,
    sealed_name: Option<String>,
    sealed_details: Option<String>,
}

/// Takes the descriptive parameters out of a creation request and checks them
/// together: an encrypted share may only describe itself sealed.
fn described(state: &AppState, params: &mut CreateShareParams) -> Result<Described> {
    let link = match params.kind.take().as_deref() {
        None => false,
        Some("link") => true,
        Some(_) => {
            return Err(Error::invalid("kind", "one_of")
                .with_message("`kind` must be `link`, or omitted for a file or a note"));
        }
    };
    if link {
        // The server has to read the address to send anyone to it, and there
        // is nothing to name or highlight.
        let unwanted = [
            ("filename", params.filename.is_some()),
            ("sealed_name", params.sealed_name.is_some()),
            ("language", params.language.is_some()),
            ("encryption", params.encryption.is_some()),
            ("key_salt", params.key_salt.is_some()),
            ("sealed_details", params.sealed_details.is_some()),
        ];
        if let Some((field, _)) = unwanted.iter().find(|(_, given)| *given) {
            return Err(Error::invalid(field, "not_for_link")
                .with_message(format!("`{field}` does not apply to a link share")));
        }
    }
    let filename = checked_label(params.filename.take(), "filename", MAX_TEXT_FIELD)?;
    let sealed_name = sealed::checked(params.sealed_name.take(), "sealed_name")?;
    let described = Described {
        kind: if link {
            ShareKind::Link
        } else if filename.is_some() || sealed_name.is_some() {
            ShareKind::File
        } else {
            ShareKind::Text
        },
        filename,
        title: checked_label(params.title.take(), "title", MAX_TEXT_FIELD)?,
        language: checked_label(params.language.take(), "language", MAX_LANGUAGE)?,
        encryption: sealed::requested(state, params.encryption.as_deref(), params.key_salt.take())?,
        sealed_name,
        sealed_details: sealed::checked(params.sealed_details.take(), "sealed_details")?,
    };
    sealed::coherent(
        described.encryption.is_some(),
        &[
            ("filename", described.filename.is_some()),
            ("title", described.title.is_some()),
            ("language", described.language.is_some()),
        ],
        &[
            ("sealed_name", described.sealed_name.is_some()),
            ("sealed_details", described.sealed_details.is_some()),
        ],
    )?;
    Ok(described)
}

/// Which slice of a share listing to return.
///
/// Shared with the administrative listing, which pages the same way over a
/// different response body.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
// Same reason as `CreateShareParams`: answering `200` to a query the handler
// does not understand teaches the caller it worked.
#[serde(deny_unknown_fields)]
pub struct SharePageParams {
    /// How many to return. Capped, and defaulted when omitted.
    #[serde(default)]
    pub limit: Option<u64>,
    /// How many to skip.
    #[serde(default)]
    pub offset: Option<u64>,
}

/// The most shares one request will return.
const MAX_SHARE_PAGE: u64 = 200;
/// How many it returns when the caller does not say.
const DEFAULT_SHARE_PAGE: u64 = 50;

/// The largest paging figure this API accepts.
///
/// `sea-query`'s encoders convert both the limit and the offset to `i64` and
/// `unwrap()` the conversion, so anything above `i64::MAX` panics inside the
/// driver. That is a `500` no handler can catch, reachable by any caller who
/// can read a listing. The document also says `int64`, so a value the schema
/// does not admit should be refused, not quietly reinterpreted.
///
/// Found by the contract fuzzer, the only reader that would send one.
const MAX_PAGE_FIGURE: u64 = i64::MAX.unsigned_abs();

/// Resolves a page request into a limit and an offset.
///
/// The bounds live in one function, not at each call site. A route that forgot
/// one gets the unbounded query this replaced, or the panic above.
///
/// # Why one bound refuses and the other clamps
///
/// Asking for more rows than the page cap is reasonable, and the server may
/// answer with fewer, as every paged API does. A figure the document does not
/// admit at all was never a real request.
///
/// # Errors
///
/// `400` when either figure is larger than the largest this API accepts.
pub fn page_bounds(params: &SharePageParams) -> Result<(u64, u64)> {
    for (name, value) in [("limit", params.limit), ("offset", params.offset)] {
        if value.is_some_and(|value| value > MAX_PAGE_FIGURE) {
            return Err(Error::out_of_range(name, None, Some(MAX_PAGE_FIGURE))
                .with_message(format!("`{name}` must be at most {MAX_PAGE_FIGURE}")));
        }
    }
    Ok((
        params
            .limit
            .unwrap_or(DEFAULT_SHARE_PAGE)
            .min(MAX_SHARE_PAGE),
        params.offset.unwrap_or(0),
    ))
}

/// What a client needs to draw pagination, for any paged list.
///
/// # Why only list endpoints have an envelope
///
/// A paged response carries the rows and facts about the collection. Without a
/// place for the latter, `total` ends up beside the rows under a per-endpoint
/// name, and clients read each list differently. Other endpoints return a
/// single object with nothing like that to carry, so wrapping them would only
/// add nesting.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
pub struct PageMeta {
    /// How many exist in total, not how many are on this page.
    #[schema(example = 137)]
    pub total: u64,
    /// The page size used, which is not always the one requested.
    #[schema(example = 50)]
    pub limit: u64,
    /// How many were skipped.
    #[schema(example = 0)]
    pub offset: u64,
}

/// A page of the caller's shares.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SharePage {
    /// The shares on this page.
    pub data: Vec<ShareResponse>,
    /// The collection they came from.
    pub meta: PageMeta,
}

/// Lists the caller's shares, newest first.
///
/// # Why this is paged
///
/// One person's shares are bounded by what one person uploaded, which holds for
/// the web page and not for a script. An account with fifty thousand shares made
/// every visit read fifty thousand rows and build fifty thousand responses.
///
/// # Errors
///
/// `401` when nothing was signed in.
#[utoipa::path(
    get,
    path = "/shares",
    operation_id = "list_shares",
    tag = SHARES_TAG,
    params(SharePageParams),
    responses(
        (status = 200, description = "A page of the caller's shares", body = SharePage),
        (status = 400, description = "`limit` or `offset` is not a number", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 403, description = "The credential does not carry `shares:read`", body = crate::error::ErrorBody),
    ),
    security(("session" = []), ("bearer" = [])),
)]
pub async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiQuery(params): ApiQuery<SharePageParams>,
) -> Result<Json<SharePage>> {
    current.require(Permission::SharesRead)?;

    let (limit, offset) = page_bounds(&params)?;
    let shares = state
        .shares
        .list_for_user(current.user.id, limit, offset)
        .await?;
    // One query for the page, not one per share. See `contents_for`.
    let contents = state.shares.contents_for(&shares).await?;
    Ok(Json(SharePage {
        data: shares
            .iter()
            .filter_map(|share| {
                contents
                    .get(&share.id)
                    .map(|content| ShareResponse::from_share(share, content, None))
            })
            .collect(),
        meta: PageMeta {
            total: state.shares.count_for_user(current.user.id).await?,
            limit,
            offset,
        },
    }))
}

/// Deletes a share, and whatever it put in the blob store.
///
/// Accepts either the owning account or, for an anonymous share, the management
/// token from creation, sent as `X-Share-Management-Token`.
///
/// # Errors
///
/// `404` when no share of the caller's has that code. The same answer as one
/// belonging to another account, because telling them apart would confirm
/// which codes exist.
#[utoipa::path(
    delete,
    path = "/shares/{code}",
    operation_id = "delete_share",
    tag = SHARES_TAG,
    params(("code" = String, Path, description = "The retrieval code. Normalised before lookup: O reads as 0, I and L read as 1, case is folded, spaces and hyphens are dropped, so `0ZPQ4M7K`, `ozpq-4m7k` and `OZPQ4M7K` are the same share.")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 404, description = "No share of yours has that code", body = crate::error::ErrorBody),
    ),
    security((), ("session" = []), ("bearer" = [])),
)]
pub async fn delete(
    State(state): State<AppState>,
    MaybeUser(current): MaybeUser,
    ApiPath(code): ApiPath<String>,
    headers: HeaderMap,
) -> Result<StatusCode> {
    let code = RetrievalCode::parse(&code).map_err(|_| Error::NotFound)?;
    let share = state
        .shares
        .find_by_code(&code)
        .await?
        .ok_or(Error::NotFound)?;

    // Owning it is not enough; the credential must carry the permission too. A
    // token given only `shares:create` can upload on its owner's behalf but
    // cannot delete what the owner has.
    let owns = current.as_ref().is_some_and(|user| {
        share.user_id == Some(user.user.id) && user.permissions.allows(Permission::SharesDelete)
    });
    let holds_token = share.management_token_hash.as_ref().is_some_and(|stored| {
        management_token(&headers).is_some_and(|presented| presented.digest() == *stored)
    });
    if !owns && !holds_token {
        return Err(Error::NotFound);
    }

    // Objects first, and every one of them, since a share can hold several. A
    // row without its object is a broken share; an object without its row is an
    // orphan the sweep can still find by listing.
    remove_objects(&state, share.id).await?;
    state.shares.delete(share.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Deletes every object a share has, each from the backend that holds it.
///
/// Not from the backend new uploads go to: asking a store for a key it never
/// had returns "not found", and the bytes would stay orphaned where they are.
///
/// # Errors
///
/// [`Error::Unavailable`] when a backend holding one did not open, or the
/// store's own error when a delete fails.
pub(crate) async fn remove_objects(state: &AppState, share: ShareId) -> Result<()> {
    for (backend, key) in state.shares.objects_of(share).await? {
        let store = state.backend(&backend).ok_or(Error::Unavailable)?;
        store.delete(&ObjectPath::from(key.as_str())).await?;
    }
    Ok(())
}

/// Body of `PATCH /shares/{code}`.
///
/// Every field is optional and omitting one leaves that setting alone. Sending
/// `null` clears it: absent and null are different instructions here.
///
/// The password travels in `X-Share-Password`, as everywhere else in this API,
/// because a header stays out of the query string, the access log and the
/// `Referer`. Only the instruction to remove it lives here, and that is not a
/// secret.
#[derive(Debug, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PatchShareRequest {
    /// How long from now the share should last. `null` means it stops expiring.
    #[serde(default, deserialize_with = "patched")]
    #[schema(value_type = Option<u64>, example = 24)]
    pub expires_in_hours: Patch<u64>,

    /// How many more times it may be retrieved. `null` means unlimited.
    ///
    /// Counted against the retrievals it has already had, so lowering this
    /// below `use_count` retires the share instead of reviving it.
    #[serde(default, deserialize_with = "patched")]
    #[schema(value_type = Option<u32>, example = 1)]
    pub max_uses: Patch<u32>,

    /// Removes the share's password.
    ///
    /// Setting one is `X-Share-Password`; this is the other direction. Sending
    /// both is refused, not guessed at.
    #[serde(default)]
    pub remove_password: bool,
}

/// Changes a share's settings after it exists.
///
/// # Why this exists
///
/// This replaces deciding everything at creation, which does not survive
/// contact with how people share a file. The link goes out, and then the
/// meeting moves, or the recipient asks for another day, or the wrong person
/// was in the thread and the password has to change now.
///
/// # Who may
///
/// The same two answers as `DELETE`: the owning account, or the holder of the
/// management token an anonymous upload was given. The permission checked is
/// `shares:delete`. There is no `shares:update` today, and gating a change
/// behind the right to delete outright is the conservative reading.
///
/// # Errors
///
/// `400` for a value the instance will not accept, `404` when no share of the
/// caller's has that code. The same answer as one belonging to another account,
/// because telling them apart would confirm which codes exist.
#[utoipa::path(
    patch,
    path = "/shares/{code}",
    operation_id = "update_share",
    tag = SHARES_TAG,
    params(
        ("code" = String, Path, description = "The retrieval code. Normalised before lookup: O reads as 0, I and L read as 1, case is folded, spaces and hyphens are dropped, so `0ZPQ4M7K`, `ozpq-4m7k` and `OZPQ4M7K` are the same share."),
        ("X-Share-Management-Token" = Option<String>, Header, description = "The token an anonymous upload was given"),
        ("X-Share-Password" = Option<String>, Header, description = "A new password for the share"),
    ),
    request_body = PatchShareRequest,
    responses(
        (status = 200, description = "The share as it now stands", body = ShareResponse),
        (status = 400, description = "A setting the instance will not accept", body = crate::error::ErrorBody),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
        (status = 404, description = "No share of yours has that code", body = crate::error::ErrorBody),
    ),
    security((), ("session" = []), ("bearer" = [])),
)]
pub async fn patch(
    State(state): State<AppState>,
    MaybeUser(current): MaybeUser,
    ApiPath(code): ApiPath<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<PatchShareRequest>,
) -> Result<Json<ShareResponse>> {
    let code = RetrievalCode::parse(&code).map_err(|_| Error::NotFound)?;
    let share = state
        .shares
        .find_by_code(&code)
        .await?
        .ok_or(Error::NotFound)?;

    // The same check `delete` makes: owning the share is not enough if the
    // credential in hand does not carry the permission.
    let owns = current.as_ref().is_some_and(|user| {
        share.user_id == Some(user.user.id) && user.permissions.allows(Permission::SharesDelete)
    });
    let holds_token = share.management_token_hash.as_ref().is_some_and(|stored| {
        management_token(&headers).is_some_and(|presented| presented.digest() == *stored)
    });
    if !owns && !holds_token {
        return Err(Error::NotFound);
    }

    let supplied = share_password(&headers)?;
    if supplied.is_some() && body.remove_password {
        return Err(Error::new(ErrorCode::ConflictingFields)
            .with_detail("fields", serde_json::json!(["X-Share-Password", "remove_password"]))
            .with_message(
            "send `X-Share-Password` to set a password or `remove_password` to clear it, not both",
        ));
    }
    // The key came from the password, and the server holds a token derived from
    // that key. A different token locks out everyone who knows the real
    // password, and none hands the sealed bytes to anyone. Setting one is how a
    // client that uploaded before choosing a password records it.
    if matches!(share.encryption, Some(Encryption::Password { .. }))
        && (body.remove_password || (supplied.is_some() && share.needs_password()))
    {
        return Err(Error::new(ErrorCode::SharePasswordIsKey).with_message(
            "this share's key comes from its password, so the password cannot be changed or \
             removed",
        ));
    }

    let now = OffsetDateTime::now_utc();
    let expires_at = match body.expires_in_hours {
        None => Change::Keep,
        // Clearing the expiry would push the share past the instance ceiling
        // through a route never meant to grant that. An instance with no
        // ceiling has nothing to bypass.
        Some(None) => {
            if state.config().share.max_expiry().is_some() {
                return Err(Error::out_of_range(
                    "expires_in_hours",
                    Some(1),
                    Some(state.config().share.max_expiry_hours),
                )
                .with_message(format!(
                    "this instance keeps shares for at most {} hours, so `expires_in_hours` \
                     cannot be cleared",
                    state.config().share.max_expiry_hours
                )));
            }
            Change::Set(None)
        }
        Some(Some(hours)) => Change::Set(resolve_expiry(&state, Some(hours), now)?),
    };

    let use_limit = match body.max_uses {
        None => Change::Keep,
        Some(None) => Change::Set(None),
        Some(Some(0)) => {
            return Err(Error::out_of_range("max_uses", Some(1), None).with_message(
                "`max_uses` must be at least 1; send null for a share that never runs out",
            ));
        }
        Some(Some(limit)) => Change::Set(Some(limit)),
    };

    let password_hash = if body.remove_password {
        Change::Set(None)
    } else {
        match supplied {
            None => Change::Keep,
            Some(password) => Change::Set(Some(
                password::hash(password, state.config().auth.hashing).await?,
            )),
        }
    };

    state
        .shares
        .update_settings(share.id, expires_at, use_limit, password_hash, now)
        .await?;

    // Read back instead of patching the copy in hand, so the response reports
    // what the database holds.
    let share = state
        .shares
        .find_by_id(share.id)
        .await?
        .ok_or(Error::NotFound)?;
    let content = state.shares.content_of(&share).await?;
    Ok(Json(ShareResponse::from_share(&share, &content, None)))
}

/// Which role a share should be charged to.
///
/// The uploader's role, or `guest` for an anonymous upload. Anonymous uploads
/// carry a role instead of NULL, because `user_id IS NULL` already means two
/// things: no one was signed in, or the account has since been deleted. It
/// cannot answer "was this anonymous".
///
/// Read at upload time and stored, not looked up when the quota is checked.
/// Moving an account to another role must not rewrite which quota its old
/// shares counted against.
///
/// # Errors
///
/// Returns [`Error`] if the account cannot be read.
async fn charged_role(state: &AppState, user_id: Option<UserId>) -> Result<Option<RoleId>> {
    let Some(user_id) = user_id else {
        return Ok(guest_role());
    };
    Ok(state
        .users
        .find_by_id(user_id)
        .await?
        .and_then(|user| user.role_id))
}

/// The same answer for a caller that already holds the account.
///
/// The extractor already read the row to authenticate the request, so asking
/// the database again would be a second query for a value in hand.
pub(crate) fn charged_role_of(user: Option<&User>) -> Option<RoleId> {
    user.map_or_else(guest_role, |user| user.role_id)
}

/// The role anonymous work is charged to.
///
/// `parse` on a constant this crate owns. A failure would mean the seeded id is
/// malformed, which no deployment can cause.
fn guest_role() -> Option<RoleId> {
    RoleId::parse(RoleId::GUEST).ok()
}

/// Where a share's bytes ended up, and how many there were.
struct Ingested {
    storage: StorageLocation,
    size_bytes: u64,
    inline: Option<Vec<u8>>,
    /// Every piece, when the file was spread. Empty for the one-object case,
    /// meaning every upload made with the switch off.
    segments: Vec<crate::db::repository::share::NewSegment>,
}

/// Reads the body, keeping it in the database if it is small and streaming it to
/// the blob store if it is not.
///
/// # Why it is not `bytes().await`
///
/// Reading the body in one go costs its size in memory: 100 MiB per upload in
/// flight at the default limit. On a small machine that is the difference
/// between working and being killed by the OOM reaper. So the body is consumed
/// a frame at a time, buffered while it might still be small enough to inline,
/// and pushed to the store in fixed blocks once it is not.
///
/// # Why the size limit is enforced here
///
/// `DefaultBodyLimit` does not limit the connection. It is a value that
/// *extractors* consult, so `Bytes`, `String` and `Json` respect it while a
/// handler reading frames itself does not. Without this check, the layer in
/// `api::build` does nothing at all for this route.
///
/// # Errors
///
/// Returns [`Error`] if the body cannot be read or the store will not take it.
async fn ingest(
    state: &AppState,
    request: Ingest<'_>,
    body: Body,
) -> Result<(Ingested, crate::storage::in_flight::Arrival)> {
    // Read before the body is consumed. The size hint covers an in-memory body
    // as well as `Content-Length`; a chunked body reports nothing, and that is
    // the honest answer.
    let expected_bytes = <Body as http_body::Body>::size_hint(&body).exact();
    let mut upload: Option<crate::upload::segments::Writer<'_>> = None;
    // Opened here so it outlives the read loop. The caller holds it until the
    // row is written; the keys go in once the scope and backend are known.
    let mut arriving = state.in_flight.begin(Vec::new());
    let result = ingest_into(
        state,
        request,
        expected_bytes,
        body,
        &mut upload,
        &mut arriving,
    )
    .await;

    if let Some(writer) = upload.take() {
        // Only reachable on failure; the success path takes the writer out of
        // the slot after finishing it. A direct `POST /shares` records no
        // segments, because there is nothing to record against until the share
        // exists. If this does not free them, nothing will, and the store bills
        // for the parts for ever.
        writer.abandon().await;
    }
    result.map(|ingested| (ingested, arriving))
}

/// What is left before the first ceiling refuses, counting uploads still
/// arriving.
///
/// An open upload session has reserved the room it declared, so it spends quota
/// before a byte of it arrives. Without that, two callers can each be promised
/// the same free space and the instance goes over its own limit.
///
/// Read once per upload, not held in a counter. An upload is not a hot path,
/// and a counter would have to survive every failed ingest, delete and sweep.
/// Once it drifted, the instance would refuse uploads it had room for, or
/// accept ones it did not, and nothing would say which.
///
/// # This is a guard, not an accountant
///
/// Uploads in flight each read this before any of them has a row to be counted,
/// so together they can land past the line. The overshoot is bounded by the
/// number of concurrent uploads times `server.max_body_bytes`, and the next
/// upload is refused.
///
/// Closing that window means reserving space up front and releasing it on every
/// failure path, or deleting the object after the fact. Both add a way to lose
/// bytes that should have been kept. Set the figure with the slack you would
/// give a disk.
///
/// # Errors
///
/// Returns [`Error`] if a total cannot be read.
pub(crate) async fn remaining_room(state: &AppState, user: Option<&User>) -> Result<Room> {
    tightest(state, ceilings(state, user).await?).await
}

/// The same, ignoring `storage.max_total_bytes`.
///
/// For telling a caller how much room *they* have. How full the disk is goes
/// only to whoever may read instance-wide figures; answering a personal quota
/// with it would tell every signed-in account how full the machine is.
///
/// # Errors
///
/// Returns [`Error`] if a total cannot be read.
pub(crate) async fn personal_room(state: &AppState, user: Option<&User>) -> Result<Room> {
    let personal = ceilings(state, user)
        .await?
        .into_iter()
        .filter(|ceiling| ceiling.tier != Tier::Instance)
        .collect();
    tightest(state, personal).await
}

/// Whichever of these ceilings has the least room left.
///
/// Later ceilings win a tie so the answer names the nearest one: "your quota is
/// full" is actionable where "the instance is full" is not, and when both are
/// equally full both are true.
async fn tightest(state: &AppState, ceilings: Vec<Ceiling>) -> Result<Room> {
    let mut tightest = Room {
        bytes: u64::MAX,
        tier: Tier::Instance,
        scope: Scope::Instance,
    };
    for ceiling in ceilings {
        let used = state
            .shares
            .stored_bytes(ceiling.scope)
            .await?
            .saturating_add(state.uploads.reserved_bytes(ceiling.scope).await?);
        let left = ceiling.bytes.saturating_sub(used);
        if left <= tightest.bytes {
            tightest = Room {
                bytes: left,
                tier: ceiling.tier,
                scope: ceiling.scope,
            };
        }
    }
    Ok(tightest)
}

/// How much room is left, and which ceiling is the one running out.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Room {
    /// Bytes before that ceiling refuses. [`u64::MAX`] when nothing limits it.
    pub bytes: u64,
    /// Which one it is, so a refusal can say who can do something about it.
    pub tier: Tier,
    /// What the ceiling is counted over.
    ///
    /// Carried so the in-flight ledger can be asked the same question the
    /// database was, "how much is spoken for under this scope", for the bytes
    /// no table has seen yet.
    pub scope: Scope,
}

/// One planned file, and everything the loop learned by planning it.
struct Opened<'a> {
    /// The writer that routes the body into the plan's segments.
    writer: crate::upload::segments::Writer<'a>,
    /// What the first backend has room for.
    limit: u64,
    /// What to count these bytes under while they arrive.
    ledger_key: String,
}

/// Plans where the file goes, and starts counting the bytes against the first
/// backend it named.
///
/// The two go together: from the moment a backend is chosen, everything read so
/// far is spoken for there as much as what follows.
///
/// The ledger and the running limit follow the *first* segment, the one the
/// bytes are going into now. The planner has already checked every other
/// segment, and re-checking per chunk would be three queries a chunk for a
/// figure that cannot have moved in this upload's favour.
///
/// # Errors
///
/// As [`plan_object`].
async fn open_object<'a>(
    state: &'a AppState,
    id: ShareId,
    now: OffsetDateTime,
    expected_bytes: Option<u64>,
    arriving: &mut crate::storage::in_flight::Arrival,
) -> Result<Opened<'a>> {
    let plan = plan_object(state, id, now, expected_bytes).await?;
    let first = plan
        .first()
        .ok_or(Error::Unavailable)?
        .segment
        .backend
        .clone();
    // One segment: the running check below is all that stands between a body
    // and a backend that cannot hold it, so the limit is that backend's room.
    //
    // Several: the planner has already found room for every piece. Comparing
    // the file against the *first* backend's share would refuse the uploads
    // splitting exists to accept, so the limit is the plan itself, which the
    // writer also enforces by running off the end.
    let limit = if plan.len() > 1 {
        plan.iter().map(|p| p.segment.length).sum()
    } else {
        backend_free(state, &first).await?
    };
    let ledger_key = format!("backend:{first}");
    arriving.also_count_under(ledger_key.clone());
    let part_size = state.config().server.part_size_bytes;
    Ok(Opened {
        writer: crate::upload::segments::Writer::new(state, plan, part_size),
        limit,
        ledger_key,
    })
}

/// Reads the ceiling again if it is time to, then stops the body if these bytes
/// are past it.
///
/// The caller carries `room` and `next_read` across the loop, so the re-read is
/// periodic instead of per chunk.
///
/// # Errors
///
/// [`Error::InsufficientStorage`] naming the tier that refused, or whatever the
/// query fails with.
async fn enforce_ceiling(
    state: &AppState,
    owner: Option<&User>,
    so_far: u64,
    part_size: usize,
    room: &mut Room,
    next_read: &mut u64,
) -> Result<()> {
    if so_far >= *next_read {
        *room = remaining_room(state, owner).await?;
        *next_read = so_far.saturating_add(re_read_interval(room.bytes, part_size));
    }
    refuse_if_past(state, &scope_key(room.scope), so_far, *room)
}

/// Stops the body when these bytes, plus everything else arriving under `key`,
/// are past the ceiling.
///
/// # Errors
///
/// [`Error::InsufficientStorage`], naming the tier that refused.
fn refuse_if_past(state: &AppState, key: &str, mine: u64, room: Room) -> Result<()> {
    if would_pass(state, key, mine, room.bytes) {
        return Err(Error::InsufficientStorage(room.tier));
    }
    Ok(())
}

/// Whether these bytes, plus everything else arriving under `key`, are past
/// `ceiling`.
///
/// The upload's own bytes are excluded from the ledger figure and added back
/// here, so an upload is never refused for competing with itself.
fn would_pass(state: &AppState, key: &str, mine: u64, ceiling: u64) -> bool {
    mine.saturating_add(state.in_flight.elsewhere(key, mine)) > ceiling
}

/// The ledger key for a quota scope.
///
/// Spelled out, not derived from `Debug`. It is compared across requests, and a
/// formatting change would silently split one scope into two.
fn scope_key(scope: Scope) -> String {
    match scope {
        Scope::User(id) => format!("user:{id}"),
        Scope::Role(id) => format!("role:{id}"),
        Scope::Instance => "instance".to_owned(),
    }
}

/// One ceiling that applies to this caller.
#[derive(Debug, Clone, Copy)]
struct Ceiling {
    scope: Scope,
    tier: Tier,
    bytes: u64,
}

/// Which backend a file of `size` should be written to.
///
/// # Errors
///
/// [`Error::InsufficientStorage`] when nothing has room for the whole file. Not
/// a `413`, because the file is not too big in itself. The pool is too full,
/// and a smaller file, or the same one later, may fit.
pub(crate) async fn place(state: &AppState, size: u64) -> Result<String> {
    place_where(state, Some(size), |_| true).await
}

/// The same, restricted to backends that can hold a resumable upload open.
///
/// That is any backend with a multipart view, plus a local directory, where the
/// appending sink writes a partial file alongside. Others (a WebDAV server, a
/// store built without multipart) cannot, and placing a resumable upload there
/// would fail with `400 this instance's store cannot hold a resumable upload`
/// when another backend could have taken it.
///
/// # Errors
///
/// [`Error::InsufficientStorage`] when nothing suitable has room.
pub(crate) async fn place_resumable(state: &AppState, size: u64) -> Result<String> {
    let pool = state.backends.load_full();
    place_where(state, Some(size), |name| {
        state.backend_multipart(name).is_some() || pool.local_root(name).is_some()
    })
    .await
}

/// The same, restricted to backends that can be written a piece at a time.
///
/// A file past `share.inline_threshold_bytes` is streamed out with
/// `put_multipart` while it is still arriving, so a backend without it cannot
/// take one however much room it has. Filtering here, instead of failing at the
/// write, is the difference between "too large for anywhere we can put it" and
/// a `500` raised halfway through the body. See
/// [`Stores::put_multipart`](crate::storage::Stores::put_multipart).
///
/// # Errors
///
/// [`Error::PayloadTooLarge`] when the only backends with room cannot take a
/// streamed write, [`Error::InsufficientStorage`] when none had room anyway.
async fn place_streamed(state: &AppState, size: Option<u64>) -> Result<String> {
    // Loaded once, outside the predicate. Asking the `ArcSwap` per candidate
    // would let a reload land mid-decision and judge two candidates against two
    // different sets of backends.
    let pool = state.backends.load();
    place_where(state, size, |name| {
        pool.open
            .get(name)
            .is_some_and(|stores| stores.put_multipart)
    })
    .await
}

/// What each backend is holding, counting what has not become a share yet.
///
/// Two sources: `file_segments` for content that made it, and `upload_segments`
/// for sessions still in flight. Leaving the second out is how a bucket fills
/// with half-finished uploads while the placer goes on offering it.
///
/// # Errors
///
/// Returns [`Error`] if either query fails.
async fn backend_usage(state: &AppState) -> Result<crate::storage::Usage> {
    let mut used = state.shares.bytes_per_backend().await?;
    // Sorted before merging: the order two maps are walked in is not stable,
    // and a figure depending on it differs between two runs of the same query.
    let mut in_flight: Vec<(String, u64)> = state
        .uploads
        .bytes_per_backend()
        .await?
        .into_iter()
        .collect();
    in_flight.sort_unstable();
    for (backend, bytes) in in_flight {
        let entry = used.entry(backend).or_insert(0);
        *entry = entry.saturating_add(bytes);
    }
    Ok(used)
}

/// The three sizes the read loop needs, read once.
///
/// Read together, because each `state.config()` is a load from the swap. Three
/// in a row can straddle a change made mid-upload, leaving the loop enforcing
/// one configuration's limit with another's part size.
struct Sizes {
    /// `share.max_text_bytes`.
    text_limit: usize,
    /// `share.inline_threshold_bytes`.
    threshold: usize,
    /// `server.part_size_bytes`.
    part_size: usize,
}

impl From<&crate::config::Config> for Sizes {
    fn from(config: &crate::config::Config) -> Self {
        Self {
            text_limit: config.share.max_text_bytes,
            threshold: config.share.inline_threshold_bytes,
            part_size: config.server.part_size_bytes,
        }
    }
}

/// How many bytes to take before asking the database for the ceiling again.
///
/// # Why the ceiling is read more than once
///
/// `in_flight` covers two uploads that overlap in time. It does not cover one
/// that *finishes* between another's snapshot and its writes: the first's row
/// appears as its ledger entry goes, and the second is still comparing against
/// a figure taken before either existed. Measured as `[201, 201]` about one
/// full-suite run in three, and still caught by
/// `two_uploads_at_once_cannot_both_spend_the_last_of_the_room` after the
/// ledger landed.
///
/// # Why not every chunk
///
/// That is correct and costs a query per chunk, thousands on a large upload.
/// A quarter of what is left, capped at a part: an instance with room to spare
/// pays almost nothing, and a nearly full one checks often enough that the
/// overshoot is a fraction of what remained. Never zero, or the loop is back to
/// querying every chunk.
fn re_read_interval(room: u64, part_size: usize) -> u64 {
    let cap = u64::try_from(part_size).unwrap_or(u64::MAX);
    let quarter = room / 4;
    let every = if quarter < cap { quarter } else { cap };
    if every == 0 { 1 } else { every }
}

/// The placer to use for a size that may or may not be known.
///
/// `None` is not "zero bytes": everything fits zero, so asking
/// [`choose`](crate::storage::placement::choose) that question hands back the
/// fullest backend. See
/// [`choose_unsized`](crate::storage::placement::choose_unsized).
fn pick(
    pool: &[crate::storage::placement::Candidate],
    size: Option<u64>,
) -> Option<&crate::storage::placement::Candidate> {
    match size {
        Some(size) => crate::storage::placement::choose(pool, size),
        None => crate::storage::placement::choose_unsized(pool),
    }
}

async fn place_where(
    state: &AppState,
    size: Option<u64>,
    suitable: impl Fn(&str) -> bool,
) -> Result<String> {
    let pool = state.backends.load();
    let used = backend_usage(state).await?;
    let all = crate::storage::candidates(&pool.configured, &pool.open, &used);
    let candidates: Vec<crate::storage::placement::Candidate> = all
        .iter()
        .filter(|candidate| suitable(&candidate.name))
        .cloned()
        .collect();

    if let Some(chosen) = pick(&candidates, size) {
        return Ok(chosen.name.clone());
    }

    // Two failures wear the same shape here. "Every backend is full" is fixed
    // by adding room; "the backends with room cannot take a file written this
    // way" is not about room, and adding more changes nothing.
    Err(if pick(&all, size).is_some() {
        Error::PayloadTooLarge
    } else {
        Error::InsufficientStorage(short_of(&all, size))
    })
}

/// Which way the backends fall short of a file that none of them will take.
///
/// Full is not the only reason. Two backends with 27 MB and 22 MB left hold
/// 49 MB between them and still cannot take 38 MB whole, and calling that
/// "the instance is full" sends the reader looking for space that exists.
fn short_of(pool: &[crate::storage::placement::Candidate], size: Option<u64>) -> Tier {
    let Some(size) = size else {
        return Tier::Instance;
    };
    let free: u64 = pool
        .iter()
        .filter(|candidate| !candidate.read_only)
        .map(|candidate| candidate.free_bytes().unwrap_or(u64::MAX))
        .fold(0, u64::saturating_add);
    if free >= size {
        Tier::Backend
    } else {
        Tier::Instance
    }
}

/// Places the file and opens a multipart upload on the backend chosen.
///
/// Placed before the first byte goes anywhere. Choosing after the upload
/// started would record a name the bytes are not at.
///
/// # Errors
///
/// Returns [`Error`] if nothing has room, or the chosen backend will not open.
async fn plan_object(
    state: &AppState,
    id: ShareId,
    now: OffsetDateTime,
    expected_bytes: Option<u64>,
) -> Result<Vec<crate::upload::segments::Planned>> {
    let base = object_key(id, now);
    let config = state.config();

    // Splitting needs a size to cut against, and a chunked body declares none.
    // Finding out halfway that the pool is short would leave pieces on several
    // backends with no way to finish, so without a length the file is placed
    // whole, as it is with the switch off.
    if let (true, Some(size)) = (config.storage.split_across_backends, expected_bytes) {
        let pool = state.backends.load();
        let used = backend_usage(state).await?;
        let candidates = crate::storage::candidates(&pool.configured, &pool.open, &used);
        let rules = crate::storage::split::Rules {
            part_size: u64::try_from(config.server.part_size_bytes).unwrap_or(u64::MAX),
            split: true,
        };
        let plan = crate::storage::split::plan(&candidates, size, rules)
            .ok_or_else(|| Error::InsufficientStorage(short_of(&candidates, Some(size))))?;
        return Ok(plan
            .into_iter()
            .enumerate()
            .map(|(seq, segment)| crate::upload::segments::Planned {
                // One object per piece, named by position and sharing a prefix
                // so an operator can see in a bucket that they belong together.
                object_key: if seq == 0 {
                    base.clone()
                } else {
                    format!("{base}.{seq}")
                },
                segment,
            })
            .collect());
    }

    let chosen = place_streamed(state, expected_bytes).await?;
    Ok(vec![crate::upload::segments::Planned {
        segment: crate::storage::split::Segment {
            backend: chosen,
            offset: 0,
            length: expected_bytes.unwrap_or(u64::MAX),
        },
        object_key: base,
    }])
}

/// How much room one named backend has, or [`u64::MAX`] when nothing limits it.
///
/// # Errors
///
/// Returns [`Error`] if the usage cannot be read.
async fn backend_free(state: &AppState, name: &str) -> Result<u64> {
    let pool = state.backends.load();
    let used = backend_usage(state).await?;
    Ok(
        crate::storage::candidates(&pool.configured, &pool.open, &used)
            .into_iter()
            .find(|candidate| candidate.name == name)
            .and_then(|candidate| candidate.free_bytes())
            .unwrap_or(u64::MAX),
    )
}

/// The largest single file any backend can currently take, or `None` when one
/// of them is unlimited.
///
/// Not the sum of what is free: nothing is split across backends, so a pool of
/// five holding 100 MB each cannot take 200 MB, and reporting 500 MB would
/// explain nothing when the upload is refused.
///
/// # Errors
///
/// Returns [`Error`] if the usage cannot be read.
pub(crate) async fn largest_placeable(state: &AppState) -> Result<Option<u64>> {
    let pool = state.backends.load();
    let used = backend_usage(state).await?;
    Ok(crate::storage::placement::largest_acceptable(
        &crate::storage::candidates(&pool.configured, &pool.open, &used),
    ))
}

/// The largest single file this caller may send.
///
/// Nearest wins, same as the storage quota: the account's own figure, then its
/// role's, then `server.max_body_bytes`. `0` at a layer means no limit *there*
/// and hands the question to the instance, which always applies.
///
/// Not a [`Tier`], because a refusal here is a `413` and knowing which layer
/// said so changes nothing: the file is too big and stays too big. A `507` is
/// different, where whose quota is full tells the caller whether deleting their
/// own shares would help.
///
/// # Errors
///
/// Returns [`Error`] if the role cannot be read.
pub(crate) async fn max_file_bytes(state: &AppState, user: Option<&User>) -> Result<u64> {
    let instance = u64::try_from(state.config().server.max_body_bytes).unwrap_or(u64::MAX);

    if let Some(user) = user
        && let Some(bytes) = user.max_file_bytes
    {
        return Ok(narrower(bytes, instance));
    }
    let Some(role) = charged_role_of(user) else {
        return Ok(instance);
    };
    let role = state
        .roles
        .find_by_id(role)
        .await?
        .and_then(|role| role.max_file_bytes);
    Ok(role.map_or(instance, |bytes| narrower(bytes, instance)))
}

/// A layer's figure against the instance's, where `0` means "no limit here".
fn narrower(layer: u64, instance: u64) -> u64 {
    if layer == 0 {
        return instance;
    }
    layer.min(instance)
}

/// Every ceiling this caller has to fit under.
///
/// Two at most, and both apply; neither overrides the other.
///
/// * `storage.max_total_bytes`, the disk. Nothing may pass it. A role granted "no ceiling" has no
///   ceiling *of its own*, and that is not permission to fill the machine: whoever set the role
///   does not own the disk.
/// * The caller's own figure, from their account if it has one and from their role otherwise.
///   Nearest wins, so raising one person needs no role invented for them.
///
/// A tier saying `0` has no ceiling *there* and drops out. A tier saying nothing
/// at all asks the next one, and that is why the columns are nullable.
///
/// # Errors
///
/// Returns [`Error`] if the role cannot be read.
async fn ceilings(state: &AppState, user: Option<&User>) -> Result<Vec<Ceiling>> {
    let mut ceilings = Vec::with_capacity(2);

    let instance = state.config().storage.max_total_bytes;
    if instance > 0 {
        ceilings.push(Ceiling {
            scope: Scope::Instance,
            tier: Tier::Instance,
            bytes: instance,
        });
    }

    if let Some(user) = user
        && let Some(bytes) = user.quota_bytes
    {
        if bytes > 0 {
            ceilings.push(Ceiling {
                scope: Scope::User(user.id),
                tier: Tier::Account,
                bytes,
            });
        }
        return Ok(ceilings);
    }

    // Anonymous work is charged to the guest role so an instance that allows it
    // can still bound it. Otherwise the only figure reaching a caller with no
    // account is the instance's, and "let strangers upload, but not more than
    // twenty gigabytes of it" cannot be said.
    let role = charged_role_of(user);
    if let Some(role) = role
        && let Some(bytes) = state
            .roles
            .find_by_id(role)
            .await?
            .and_then(|role| role.quota_bytes)
        && bytes > 0
    {
        ceilings.push(Ceiling {
            scope: Scope::Role(role),
            tier: Tier::Role,
            bytes,
        });
    }
    Ok(ceilings)
}

/// A filename from a query parameter, length- and control-character-checked.
///
/// # Errors
///
/// Returns `value_length` or `value_invalid` if it is too long or holds a control
/// character.
pub(crate) fn checked_filename(value: Option<String>) -> Result<Option<String>> {
    checked_label(value, "filename", MAX_TEXT_FIELD)
}

/// What one ingest is: where the bytes go, what they are, and whose ceiling
/// has to allow them.
struct Ingest<'a> {
    /// Names the object, not the share. A share holds several files and each
    /// needs its own key; deriving it from the share meant the second upload
    /// silently overwrote the first, and the download sent one file's bytes
    /// three times. Two rows, one object.
    id: ShareId,
    /// Decides whether the bytes become an object or a row.
    kind: ShareKind,
    /// Whose ceilings apply, and not always the caller's. A management token
    /// lets an account-less uploader append to a member's share.
    owner: Option<&'a User>,
    /// One reading of the clock, shared by the whole request.
    now: OffsetDateTime,
}

async fn ingest_into<'a>(
    state: &'a AppState,
    request: Ingest<'_>,
    // What the body says it will be, when it says. Chooses the backend before
    // the first byte is written; without it the placer can only be asked for
    // "somewhere with any room", which picks one the file does not fit in after
    // parts have already been written there.
    expected_bytes: Option<u64>,
    body: Body,
    upload: &mut Option<crate::upload::segments::Writer<'a>>,
    arriving: &mut crate::storage::in_flight::Arrival,
) -> Result<Ingested> {
    let Ingest {
        id,
        kind,
        owner,
        now,
    } = request;
    // Where the bytes end up follows what they are, not how many there are. A
    // file is an object whatever its size, so an operator finds it in one
    // place; text is a row, so reading a paste is one query.
    //
    // The threshold still decides how much of a file is buffered before the
    // multipart upload starts. That is a memory question, not a storage one.
    let stream_to_object = kind == ShareKind::File;
    let Sizes {
        text_limit,
        threshold,
        part_size,
    } = Sizes::from(state.config().as_ref());
    // The caller's own limit: `server.max_body_bytes`, or something narrower.
    // Read here, not in the handler, so the direct and resumable paths share
    // one definition of "too big for you".
    let limit = max_file_bytes(state, owner).await?;
    // What is left of the instance's allowance, checked as the body arrives
    // and not only up front. The caller need not send a `Content-Length`, and a
    // chunked upload past the line should stop at the line.
    let mut room = remaining_room(state, owner).await?;
    // Re-read as the body arrives. See `re_read_interval`.
    let mut re_read_at = re_read_interval(room.bytes, part_size);
    // What the database said, plus what is arriving now and is not in it yet.
    // Otherwise two uploads that both read `room` before either wrote anything
    // are each told the remainder is theirs. Measured: 80 KiB stored against a
    // 50 KiB ceiling, from two requests.
    arriving.also_count_under(scope_key(room.scope));
    let mut body = body;
    let mut buffered: Vec<u8> = Vec::new();
    let mut size_bytes: u64 = 0;
    // Until a backend is chosen there is nothing to exceed.
    let mut backend_limit = u64::MAX;
    let mut backend_key: Option<String> = None;

    let stall = state.config().server.upload_stall();

    while let Some(frame) = next_frame(&mut body, stall).await? {
        let Ok(chunk) = frame.into_data() else {
            // Trailers carry no content; a body that is only trailers is empty.
            continue;
        };
        size_bytes = size_bytes.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
        if size_bytes > limit {
            return Err(Error::new(ErrorCode::PayloadTooLarge).with_detail("limit_bytes", limit));
        }
        // Checked as it arrives: a paste ten times the limit should stop at the
        // limit, not be read in full and then refused.
        if !stream_to_object && size_bytes > u64::try_from(text_limit).unwrap_or(u64::MAX) {
            return Err(Error::new(ErrorCode::TextTooLarge).with_detail("max_bytes", text_limit));
        }
        arriving.reached(size_bytes);
        enforce_ceiling(
            state,
            owner,
            size_bytes,
            part_size,
            &mut room,
            &mut re_read_at,
        )
        .await?;
        buffered.extend_from_slice(&chunk);

        if !stream_to_object {
            // Text is never streamed out: it is going into a row, and a row is
            // written whole.
            continue;
        }
        if upload.is_none() && buffered.len() <= threshold {
            continue;
        }
        // Past the threshold: start the upload the first time, then keep the
        // buffer drained in whole blocks.
        if upload.is_none() {
            let started = open_object(state, id, now, expected_bytes, arriving).await?;
            backend_limit = started.limit;
            backend_key = Some(started.ledger_key);
            *upload = Some(started.writer);
        }
        // The chosen backend's own ceiling, checked as the bytes arrive, for
        // the same reason the quota is. Past the line here means the provider
        // refuses mid-upload, worse than a 507.
        if let Some(backend_key) = backend_key.as_deref()
            && would_pass(state, backend_key, size_bytes, backend_limit)
        {
            return Err(Error::InsufficientStorage(Tier::Instance));
        }
        if let Some(writer) = upload.as_mut() {
            writer.write(&std::mem::take(&mut buffered)).await?;
        }
    }

    finish(
        state,
        Finish {
            id,
            now,
            stream_to_object,
            size_bytes,
        },
        buffered,
        upload,
    )
    .await
}

/// What `finish` needs that is not the buffer or the upload handle.
struct Finish {
    id: ShareId,
    now: OffsetDateTime,
    /// Whether the content is a file. That decides between an object and a
    /// row.
    stream_to_object: bool,
    /// How much arrived.
    size_bytes: u64,
}

/// Turns what the read loop ended with into the record of where the bytes went.
///
/// Three endings, because the loop stops in three places: past the threshold
/// with a multipart upload open, under it with a file in hand, and under it
/// with text.
///
/// # Errors
///
/// Returns [`Error`] if the store will not take what is left.
async fn finish(
    state: &AppState,
    request: Finish,
    buffered: Vec<u8>,
    upload: &mut Option<crate::upload::segments::Writer<'_>>,
) -> Result<Ingested> {
    let Finish {
        id,
        now,
        stream_to_object,
        size_bytes,
    } = request;
    let mut buffered = buffered;
    match upload.take() {
        Some(mut writer) => {
            if !buffered.is_empty() {
                writer.write(&std::mem::take(&mut buffered)).await?;
            }
            let pieces = writer.finish().await?;
            let first = pieces.first().ok_or_else(|| {
                Error::new(ErrorCode::BodyIncomplete)
                    .with_message("upload state was lost part-way through the body")
            })?;
            Ok(Ingested {
                // The legacy column keeps the first piece, all one column can
                // hold. Every read uses `segments`.
                storage: StorageLocation::Object {
                    backend: first.backend.clone(),
                    key: first.object_key.clone(),
                },
                size_bytes,
                inline: None,
                // Only recorded when there is more than one. A single-object
                // file is what it was before splitting existed, and a one-row
                // list would change every existing upload's shape for nothing.
                segments: if pieces.len() > 1 {
                    pieces
                        .iter()
                        .map(|piece| crate::db::repository::share::NewSegment {
                            backend: piece.backend.clone(),
                            object_key: piece.object_key.clone(),
                            offset_bytes: piece.offset,
                            length_bytes: piece.length,
                        })
                        .collect()
                } else {
                    Vec::new()
                },
            })
        }
        // A file small enough that the multipart upload never started. It still
        // becomes an object, with one `put`: a multipart upload of nine bytes
        // is three round trips to say one thing.
        None if stream_to_object => {
            let object_key = object_key(id, now);
            // The file is in hand, so the size is known before a byte is
            // written and the placer can be asked. The streaming path above is
            // already writing by the time it knows how much there is.
            let backend = place(state, size_bytes).await?;
            let store = state.backend(&backend).ok_or(Error::Unavailable)?;
            put_whole(state, &store, &backend, &object_key, buffered).await?;
            Ok(Ingested {
                storage: StorageLocation::Object {
                    backend,
                    key: object_key,
                },
                size_bytes,
                inline: None,
                segments: Vec::new(),
            })
        }
        None => Ok(Ingested {
            storage: StorageLocation::Inline,
            size_bytes,
            inline: Some(buffered),
            segments: Vec::new(),
        }),
    }
}

/// The next piece of a body's data, or `None` at its end.
///
/// Trailers carry no content, so they are skipped, not returned. The
/// wait is bounded as `next_frame` bounds it.
///
/// # Errors
///
/// Returns [`Error::Timeout`] when nothing arrives for `stall`, or
/// `body_incomplete` when the body ends part-way.
pub(crate) async fn next_chunk(body: &mut Body, stall: Duration) -> Result<Option<Bytes>> {
    while let Some(frame) = next_frame(body, stall).await? {
        if let Ok(data) = frame.into_data() {
            return Ok(Some(data));
        }
    }
    Ok(None)
}

/// The next piece of the body, or `None` at its end.
///
/// The wait is bounded by how long the connection may go silent, not by how
/// long the upload may take: a large file over a slow line is answering all the
/// way through, and a client that stopped sending is not answering at all. Only
/// here can those be told apart, since a request-wide budget expires the same
/// way for both.
///
/// See `api::timeout` for why the wall-clock budget is not applied here.
async fn next_frame(body: &mut Body, stall: Duration) -> Result<Option<http_body::Frame<Bytes>>> {
    match tokio::time::timeout(stall, body.frame()).await {
        Ok(Some(frame)) => frame.map(Some).map_err(|error| {
            Error::new(ErrorCode::BodyIncomplete).with_message(format!("body ended early: {error}"))
        }),
        Ok(None) => Ok(None),
        Err(_) => Err(Error::Timeout),
    }
}

/// Writes a whole object, holding back the write that creates its location.
///
/// WebDAV answers a write into a directory that does not exist with `409`, and
/// `object_store` retries after MKCOL exactly once, not enough when
/// several uploads are creating the same month at once. Everything after the
/// first takes no lock, so this costs one serialised write per backend per
/// month. See [`FirstWrites`](crate::storage::first_write::FirstWrites).
///
/// The streamed path needs no equivalent: it opens a multipart upload, and the
/// only store here with directories cannot do that at all, so `place_streamed`
/// keeps it off that path.
///
/// # Errors
///
/// Whatever the store answers.
async fn put_whole(
    state: &AppState,
    store: &std::sync::Arc<dyn object_store::ObjectStore>,
    backend: &str,
    object_key: &str,
    bytes: Vec<u8>,
) -> Result<()> {
    state
        .first_writes
        .once(
            backend,
            crate::storage::first_write::location_of(object_key),
            store.put(&ObjectPath::from(object_key), PutPayload::from(bytes)),
        )
        .await?;
    Ok(())
}

/// `shares/{yyyy}/{mm}/{id}`.
///
/// Sharded by date, not flat. A personal instance running for years
/// would otherwise put every object in one directory, which a local filesystem
/// handles badly. Sharding on the identifier's leading characters would not
/// help, since a ULID starts with a timestamp. Dates are also what an operator
/// wants to see when deciding what to back up.
pub(crate) fn object_key(id: ShareId, created_at: OffsetDateTime) -> String {
    format!(
        "shares/{:04}/{:02}/{id}",
        created_at.year(),
        u8::from(created_at.month())
    )
}

/// Makes a share whose one file arrived through a resumable upload.
///
/// Separate from [`create`] because the bytes are already in the store: this
/// only writes the rows and picks a code. An upload session has no query to
/// carry a password, expiry or use limit, so those take their defaults and the
/// uploader adjusts the share afterwards.
///
/// # Errors
///
/// Returns [`Error`] if no unused code can be found or the insert fails.
pub(crate) async fn create_from_file(
    state: &AppState,
    user_id: Option<UserId>,
    file: NewFile,
    encryption: Option<Encryption>,
    now: OffsetDateTime,
) -> Result<ShareResponse> {
    let management_secret = user_id.is_none().then(Secret::generate_session);
    let role_id = charged_role(state, user_id).await?;
    let share = Share {
        id: ShareId::generate(),
        code: RetrievalCode::parse("0000").unwrap_or_else(|_| unreachable!("four zeroes parse")),
        user_id,
        role_id,
        kind: ShareKind::File,
        encryption,
        password_hash: None,
        use_limit: None,
        use_count: 0,
        expires_at: resolve_expiry(state, None, now)?,
        disabled_at: None,
        title: None,
        sealed_details: None,
        og_config: None,
        management_token_hash: management_secret.as_ref().map(Secret::digest),
        view_count: 0,
        copy_count: 0,
        bytes_served: 0,
        bytes_redirected: 0,
        last_accessed_at: None,
        created_at: now,
        updated_at: now,
    };
    let content = NewContent::File(file);
    let share = insert_with_fresh_code(state, share, &content).await?;
    let stored = state.shares.content_of(&share).await?;
    Ok(ShareResponse::from_share(
        &share,
        &stored,
        management_secret.map(|secret| secret.expose().to_owned()),
    ))
}

/// The share as its owner sees it, for the response to a finished upload.
///
/// No management token: this share already existed, so its token was handed out
/// when it was made and is not ours to show again.
///
/// # Errors
///
/// Returns [`Error::NotFound`] if the share went away, or [`Error`] if a query
/// fails.
pub(crate) async fn describe_for_owner(
    state: &AppState,
    share_id: ShareId,
) -> Result<ShareResponse> {
    let share = state
        .shares
        .find_by_id(share_id)
        .await?
        .ok_or(Error::NotFound)?;
    let content = state.shares.content_of(&share).await?;
    Ok(ShareResponse::from_share(&share, &content, None))
}

/// Inserts the share, trying another code each time one is already taken.
async fn insert_with_fresh_code(
    state: &AppState,
    mut share: Share,
    content: &NewContent,
) -> Result<Share> {
    for _ in 0..CODE_ATTEMPTS {
        share.code = generate_code(state.config().share.code_length)?;
        match state.shares.insert(&share, content).await {
            Ok(()) => {
                // What was just stored may be waiting on the scanner.
                state.scan_wake.notify_one();
                return Ok(share);
            }
            Err(error) => {
                // Look, do not assume every failure is a collision:
                // assuming would retry through an outage and then report it as
                // a conflict.
                if state.shares.find_by_code(&share.code).await?.is_none() {
                    return Err(error.into());
                }
            }
        }
    }
    Err(Error::new(ErrorCode::CodeSpaceExhausted).with_message(
        "could not find an unused retrieval code; consider raising share.code_length".to_owned(),
    ))
}

fn generate_code(length: usize) -> Result<RetrievalCode> {
    let mut entropy = vec![0_u8; length];
    crate::auth::secret::fill_random(&mut entropy);
    Ok(RetrievalCode::from_entropy(&entropy)?)
}

/// Checks a caller-supplied label: length, and that it cannot break a header.
///
/// Control characters are refused, not stripped. `filename` is echoed in
/// `Content-Disposition` when the share is downloaded, and a CR or LF would end
/// the header and let the caller append one of their own. Stripping would also
/// work, but silently renaming an upload is worse than refusing it.
fn checked_label(value: Option<String>, field: &'static str, max: usize) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > max {
        return Err(Error::new(ErrorCode::ValueLength)
            .with_detail("field", field)
            .with_detail("max", max)
            .with_message(format!("`{field}` must be at most {max} characters")));
    }
    if value.chars().any(char::is_control) {
        return Err(Error::invalid(field, "no_control_characters")
            .with_message(format!("`{field}` must not contain control characters")));
    }
    Ok(Some(value))
}

/// Reads the optional share password from `X-Share-Password`.
///
/// A header, not a query parameter. Query strings end up in access logs,
/// in shell history and in the `Referer` sent to whatever the page links to
/// next.
fn share_password(headers: &HeaderMap) -> Result<Option<Password>> {
    let Some(raw) = headers
        .get("x-share-password")
        .and_then(|value| value.to_str().ok())
    else {
        return Ok(None);
    };
    Ok(Some(Password::parse(raw, gate::CHOOSING)?))
}

/// The media type to record for an upload: the claimed one, except on an
/// encrypted share, where the real type is sealed with the name and the header
/// describes ciphertext.
fn recorded_type(encrypted: bool, headers: &HeaderMap) -> Option<String> {
    if encrypted {
        return None;
    }
    claimed_content_type(headers)
}

/// The media type the uploader claimed, if it said anything specific.
fn claimed_content_type(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.chars().count() <= MAX_TEXT_FIELD)
        .filter(|value| !value.chars().any(char::is_control))
        .map(str::to_owned)
}

fn management_token(headers: &HeaderMap) -> Option<Secret> {
    headers
        .get("x-share-management-token")
        .and_then(|value| value.to_str().ok())
        .map(Secret::from_client)
}

/// Turns a requested lifetime into an instant, refusing one beyond the ceiling.
fn resolve_expiry(
    state: &AppState,
    requested_hours: Option<u64>,
    now: OffsetDateTime,
) -> Result<Option<OffsetDateTime>> {
    let ceiling = state.config().share.max_expiry();
    let Some(hours) = requested_hours else {
        // Most shares arrive without a lifetime, so this figure decides how
        // much an instance holds. See `share.default_expiry_hours`.
        return Ok(state
            .config()
            .share
            .default_expiry()
            .map(|limit| now.saturating_add(duration_of(limit))));
    };

    if hours == 0 {
        return Err(Error::out_of_range(
            "expires_in_hours",
            Some(1),
            state
                .config()
                .share
                .max_expiry()
                .map(|_| state.config().share.max_expiry_hours),
        )
        .with_message("`expires_in_hours` must be at least 1; omit it for the instance default"));
    }
    let requested = std::time::Duration::from_secs(hours.saturating_mul(3600));
    if ceiling.is_some_and(|limit| requested > limit) {
        return Err(Error::out_of_range(
            "expires_in_hours",
            Some(1),
            Some(state.config().share.max_expiry_hours),
        )
        .with_message(format!(
            "`expires_in_hours` must be at most {}",
            state.config().share.max_expiry_hours
        )));
    }
    Ok(Some(now.saturating_add(duration_of(requested))))
}

/// `std::time::Duration` is unsigned and `time::Duration` is not, so the
/// conversion only fails for a span beyond any calendar. Clamping keeps an
/// absurd configuration from breaking uploads.
fn duration_of(value: std::time::Duration) -> time::Duration {
    time::Duration::try_from(value).unwrap_or(time::Duration::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `sea-query` unwraps the `u64` -> `i64` conversion, so an offset above
    /// `i64::MAX` panicked inside the driver and came back as a `500`, from a
    /// query parameter on a route any signed-in caller can reach. Found by the
    /// contract fuzzer.
    #[test]
    fn a_figure_past_what_the_driver_can_carry_is_refused() {
        for params in [
            SharePageParams {
                limit: None,
                offset: Some(u64::MAX),
            },
            SharePageParams {
                limit: Some(u64::MAX),
                offset: None,
            },
        ] {
            assert!(
                page_bounds(&params).is_err(),
                "the driver converts this to i64 and unwraps it: {params:?}"
            );
        }

        let (limit, offset) = page_bounds(&SharePageParams {
            limit: Some(MAX_PAGE_FIGURE),
            offset: Some(MAX_PAGE_FIGURE),
        })
        .expect("the largest figures the driver can carry are still a page");
        assert!(i64::try_from(offset).is_ok());
        assert_eq!(limit, MAX_SHARE_PAGE, "still capped");
    }

    /// The cap is why the listing is bounded.
    #[test]
    fn a_limit_past_the_cap_comes_back_as_the_cap() {
        let (limit, offset) = page_bounds(&SharePageParams {
            limit: Some(100_000),
            offset: None,
        })
        .expect("a large limit is answered with fewer rows, not refused");
        assert_eq!(limit, MAX_SHARE_PAGE);
        assert_eq!(offset, 0);
    }

    #[test]
    fn object_keys_are_sharded_by_month() {
        let id = ShareId::generate();
        let at = OffsetDateTime::from_unix_timestamp(1_767_225_600).expect("representable");
        assert_eq!(object_key(id, at), format!("shares/2026/01/{id}"));
    }

    #[test]
    fn generated_codes_have_the_configured_length() {
        for length in [4, 8, 12] {
            let code = generate_code(length).expect("valid length");
            assert_eq!(code.as_str().chars().count(), length);
        }
    }
}

/// A share as a public visitor sees it.
///
/// Everything but `code`, `kind` and `has_password` is withheld until the share
/// is unlocked. A file name and a size describe the content, and a
/// password-protected share should not describe itself before the password is
/// presented. Its link preview is withheld for the same reason.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PublicShareResponse {
    /// The retrieval code.
    #[schema(example = "0ZPQ4M7K")]
    pub code: String,
    /// `text` or `file`.
    #[schema(example = "file")]
    pub kind: String,
    /// Whether a password must be presented before anything else is revealed.
    pub has_password: bool,
    /// `link` or `password` for an end-to-end encrypted share, `null` otherwise.
    ///
    /// Sent before unlocking, with the salt below, since a client whose key
    /// comes from the password must derive it before presenting the password.
    #[schema(example = "password")]
    pub encryption: Option<String>,
    /// The salt a password-derived key is stretched with, for `password`.
    pub key_salt: Option<String>,
    /// Whether the fields below are filled in.
    pub unlocked: bool,

    /// Display name.
    pub title: Option<String>,
    /// The title and a note's language, sealed, on an encrypted share.
    pub sealed_details: Option<String>,
    /// Every file this share holds. Empty for text and for a locked share.
    pub files: Vec<SharedFile>,
    /// Syntax-highlighting hint.
    pub language: Option<String>,
    /// Size of the content in bytes.
    pub size_bytes: Option<u64>,
    /// Whether it is waiting for the malware scanner and so cannot be
    /// downloaded yet. `false` while locked, since a locked share reveals
    /// nothing about its content.
    pub scanning: bool,
    /// How many retrievals remain, or `null` for unlimited.
    pub uses_remaining: Option<u32>,
    /// When it stops being served, RFC 3339.
    pub expires_at: Option<String>,
    /// When it was created, RFC 3339.
    pub created_at: Option<String>,
}

impl PublicShareResponse {
    fn locked(share: &Share) -> Self {
        Self {
            code: share.code.as_str().to_owned(),
            kind: share.kind.as_str().to_owned(),
            has_password: true,
            encryption: share.encryption.as_ref().map(|e| e.as_str().to_owned()),
            key_salt: share
                .encryption
                .as_ref()
                .and_then(Encryption::salt)
                .map(str::to_owned),
            unlocked: false,
            title: None,
            sealed_details: None,
            files: Vec::new(),
            language: None,
            size_bytes: None,
            scanning: false,
            uses_remaining: None,
            expires_at: None,
            created_at: None,
        }
    }

    fn unlocked(share: &Share, content: &Content) -> Self {
        Self {
            has_password: share.needs_password(),
            unlocked: true,
            title: share.title.clone(),
            sealed_details: share.sealed_details.clone(),
            files: SharedFile::list(content),
            language: match content {
                Content::Text(text) => text.language.clone(),
                Content::Files(_) | Content::Gone => None,
            },
            size_bytes: Some(content.size_bytes()),
            scanning: content.awaiting_scan(),
            uses_remaining: share
                .use_limit
                .map(|limit| limit.saturating_sub(share.use_count)),
            expires_at: share.expires_at.map(format_timestamp),
            created_at: Some(format_timestamp(share.created_at)),
            ..Self::locked(share)
        }
    }
}

/// What `POST /shares/{code}/unlock` hands back.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TicketResponse {
    /// Present it as `X-Share-Ticket`, or as `?t=` on a download link.
    #[schema(example = "9f2c...")]
    pub ticket: String,
    /// When the authorisation stops being honoured, RFC 3339.
    pub expires_at: String,
    /// How many retrievals remain after this one.
    pub uses_remaining: Option<u32>,
}

/// Describes a share without spending one of its retrievals.
///
/// This is what the share page reads to decide what to render. It never
/// consumes, so a link preview, a prefetch or a mail-scanner following the URL
/// cannot destroy a one-time share.
///
/// # Errors
///
/// `404` when nothing has that code, `410` when it has expired or run out,
/// `401` when it is password-protected and the password given was wrong.
#[utoipa::path(
    get,
    path = "/shares/{code}",
    operation_id = "describe_share",
    tag = SHARES_TAG,
    params(
        ("code" = String, Path, description = "The retrieval code. Normalised before lookup: O reads as 0, I and L read as 1, case is folded, spaces and hyphens are dropped, so `0ZPQ4M7K`, `ozpq-4m7k` and `OZPQ4M7K` are the same share."),
        ("X-Share-Password" = Option<String>, Header, description = "The share's password, if it has one"),
    ),
    responses(
        (status = 200, description = "What this share is", body = PublicShareResponse),
        (status = 401, description = "Wrong password", body = crate::error::ErrorBody),
        (status = 404, description = "No such code", body = crate::error::ErrorBody),
        (status = 410, description = "Expired or exhausted", body = crate::error::ErrorBody),
        (status = 429, description = "Too many lookups or password attempts", body = crate::error::ErrorBody),
    ),
)]
pub async fn describe(
    State(state): State<AppState>,
    ApiPath(code): ApiPath<String>,
    ClientAddress(Origin { source, .. }): ClientAddress,
    headers: HeaderMap,
) -> Result<Json<PublicShareResponse>> {
    let share = gate::open(&state, &code, &source).await?;

    if !share.needs_password() {
        let content = state.shares.content_of(&share).await?;
        return Ok(Json(PublicShareResponse::unlocked(&share, &content)));
    }
    // A locked share still answers, so the page can render a password prompt
    // instead of a dead end. It answers only "this exists and wants one".
    if headers.get(gate::PASSWORD_HEADER).is_none() {
        return Ok(Json(PublicShareResponse::locked(&share)));
    }
    gate::unlock(&state, &share, &headers, &source).await?;
    // Read only once the password is right, so a wrong guess costs a gate check
    // and nothing more.
    let content = state.shares.content_of(&share).await?;
    Ok(Json(PublicShareResponse::unlocked(&share, &content)))
}

/// Spends one retrieval and returns a ticket for the transfer.
///
/// # Why this is a separate step
///
/// One authorisation costs one retrieval; the ticket it returns is then good
/// for every request the transfer needs, each `Range`, resumption and retry.
/// Counting requests instead would let a resumed download spend all three uses
/// of a share that allows three.
///
/// # Errors
///
/// `401` when the password is wrong, `404` when nothing has that code, `410`
/// when it has expired or run out.
#[utoipa::path(
    post,
    path = "/shares/{code}/unlock",
    operation_id = "unlock_share",
    tag = SHARES_TAG,
    params(
        ("code" = String, Path, description = "The retrieval code. Normalised before lookup: O reads as 0, I and L read as 1, case is folded, spaces and hyphens are dropped, so `0ZPQ4M7K`, `ozpq-4m7k` and `OZPQ4M7K` are the same share."),
        ("X-Share-Password" = Option<String>, Header, description = "The share's password, if it has one"),
    ),
    responses(
        (status = 200, description = "Authorised", body = TicketResponse),
        (status = 401, description = "Wrong password", body = crate::error::ErrorBody),
        (status = 404, description = "No such code", body = crate::error::ErrorBody),
        (status = 410, description = "Expired or exhausted", body = crate::error::ErrorBody),
        (status = 423, description = "Still being checked for malware; `Retry-After` says when to ask again", body = crate::error::ErrorBody),
        (status = 429, description = "Too many lookups or password attempts", body = crate::error::ErrorBody),
    ),
)]
pub async fn unlock(
    State(state): State<AppState>,
    ApiPath(code): ApiPath<String>,
    ClientAddress(Origin { source, .. }): ClientAddress,
    headers: HeaderMap,
) -> Result<Json<TicketResponse>> {
    let share = gate::open(&state, &code, &source).await?;
    // The password first, so a wrong guess never costs the share a retrieval.
    gate::unlock(&state, &share, &headers, &source).await?;
    // And the scan: a ticket for content that cannot be served yet would use up
    // a one-time share for nothing.
    crate::scan::release(&state, &state.shares.content_of(&share).await?)?;

    let secret = gate::spend(&state, &share).await?;
    let expires_at = OffsetDateTime::now_utc().saturating_add(
        time::Duration::try_from(state.config().share.ticket_ttl())
            .unwrap_or_else(|_| time::Duration::days(1)),
    );

    Ok(Json(TicketResponse {
        ticket: secret.expose().to_owned(),
        expires_at: format_timestamp(expires_at),
        uses_remaining: share
            .use_limit
            .map(|limit| limit.saturating_sub(share.use_count.saturating_add(1))),
    }))
}

/// Body of `POST /shares/{code}/events`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ShareEventRequest {
    /// What happened: `view` or `copy`.
    ///
    /// Retrieval is not one of these. See [`record_event`].
    #[schema(example = "view")]
    pub kind: String,
}

/// Records something a client saw that the server could not.
///
/// # Why retrieval is not accepted here
///
/// `use_count` is a limit. A client that could report its own retrievals would
/// be a client deciding whether the limit applied, and one `curl` at
/// `/s/{code}/raw` would walk past it. The server counts that itself, on the
/// transfer path, where it cannot be skipped.
///
/// These two are the opposite case. Nothing reaches the server when a note is
/// read on screen or a copy button is pressed, so only a client can see
/// them. Anybody who can call this can inflate them, and that is tolerable
/// because nothing depends on them.
///
/// # Why it does not say whether the share exists
///
/// `204` whatever happens, including for a code that names nothing. Answering
/// `404` would turn an unauthenticated endpoint into a way to ask which codes
/// are real, the same oracle the retrieval throttle exists to close.
///
/// # Errors
///
/// `400` when `kind` is not one of the two, `429` when the source is asking too
/// often.
#[utoipa::path(
    post,
    path = "/shares/{code}/events",
    operation_id = "record_share_event",
    tag = SHARES_TAG,
    params(("code" = String, Path, description = "The share's retrieval code")),
    request_body = ShareEventRequest,
    responses(
        (status = 204, description = "Recorded, or the code names nothing"),
        (status = 400, description = "`kind` is not `view` or `copy`", body = crate::error::ErrorBody),
        (status = 429, description = "Too many events from this source", body = crate::error::ErrorBody),
    ),
    security(()),
)]
pub async fn record_event(
    State(state): State<AppState>,
    ClientAddress(Origin { source, .. }): ClientAddress,
    ApiPath(code): ApiPath<String>,
    ApiJson(body): ApiJson<ShareEventRequest>,
) -> Result<StatusCode> {
    let kind = match body.kind.as_str() {
        "view" => ShareEvent::View,
        "copy" => ShareEvent::Copy,
        // Named, not listed as invalid. A client sending `download` has
        // misread what this endpoint is for, and saying so helps more than
        // repeating the two words it accepts.
        "download" | "retrieval" => {
            return Err(Error::invalid("kind", "server_counted").with_message(
                "retrievals are counted by the server on the transfer path, not reported here",
            ));
        }
        _ => {
            return Err(
                Error::invalid("kind", "one_of").with_message("`kind` must be `view` or `copy`")
            );
        }
    };

    // Keyed on the source, like the retrieval lookup: what varies is the code,
    // so counting per code would count nothing.
    if let Decision::Deny { retry_after } = state.throttles.lookup.check(&source) {
        return Err(Error::TooManyRequests {
            retry_after_secs: retry_after.as_secs(),
        });
    }

    // Parsed after the throttle so a flood of unparsable codes costs the same
    // as a flood of real ones.
    let Ok(code) = RetrievalCode::parse(&code) else {
        return Ok(StatusCode::NO_CONTENT);
    };
    if let Some(share) = state.shares.find_by_code(&code).await? {
        state.shares.record_event(share.id, kind).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Adds another file to a share.
///
/// # Why files arrive one at a time
///
/// A request carrying five files loses all five when it drops at the fourth,
/// which is what uploading a folder of photos over a home connection runs
/// into. A file that fails here is retried alone.
///
/// # Why the share exists before its second file
///
/// There is no draft state. A share is created by its first file and grown by
/// the rest, so nothing has to be finalised, and an interrupted upload leaves a
/// share with what did arrive instead of nothing.
///
/// The cost is that a link opened mid-upload shows a partial share. The
/// uploader hands out the link afterwards.
///
/// # Errors
///
/// `400` for a name that is unusable or described the wrong way for the share,
/// `403` when the caller cannot show it is theirs, `404` when no share has that
/// code, `409` when the share holds text, `413` when the body is too large,
/// `507` when something is out of room. The body says whose limit it was.
#[utoipa::path(
    post,
    path = "/shares/{code}/files",
    operation_id = "add_share_file",
    tag = SHARES_TAG,
    params(("code" = String, Path, description = "The share to add to"), AddFileParams),
    request_body(content = Vec<u8>, description = "The file", content_type = "application/octet-stream"),
    responses(
        (status = 201, description = "The share as it now stands", body = ShareResponse),
        (status = 400, description = "A name that is unusable, or described the wrong way for the share: in the clear on an encrypted one, sealed on a plain one", body = crate::error::ErrorBody),
        (status = 403, description = "Not yours", body = crate::error::ErrorBody),
        (status = 404, description = "No such share", body = crate::error::ErrorBody),
        (status = 409, description = "That share holds text", body = crate::error::ErrorBody),
        (status = 413, description = "Body exceeds the configured limit", body = crate::error::ErrorBody),
        (status = 507, description = "Out of room; the body says whether that is the account's quota, its role's, or the instance's", body = crate::error::ErrorBody),
    ),
    security((), ("session" = []), ("bearer" = [])),
)]
pub async fn add_file(
    State(state): State<AppState>,
    MaybeUser(current): MaybeUser,
    ApiPath(code): ApiPath<String>,
    ApiQuery(params): ApiQuery<AddFileParams>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response> {
    let code = RetrievalCode::parse(&code).map_err(|_| Error::NotFound)?;
    let share = state
        .shares
        .find_by_code(&code)
        .await?
        .ok_or(Error::NotFound)?;

    // Adding to another account's share would let a stranger put content behind
    // its link. The same two proofs the delete route accepts: the management
    // token the uploader was handed, or being the account that owns it.
    let owns = current.as_ref().is_some_and(|user| {
        share.user_id == Some(user.user.id) && user.permissions.allows(Permission::SharesCreate)
    });
    let holds_token = share.management_token_hash.as_ref().is_some_and(|stored| {
        management_token(&headers).is_some_and(|presented| presented.digest() == *stored)
    });
    if !owns && !holds_token {
        // The same answer as a share that is not there, for the same reason
        // `delete` gives it: otherwise this says which codes exist.
        return Err(Error::NotFound);
    }

    if share.kind != ShareKind::File {
        return Err(Error::new(ErrorCode::ShareIsText));
    }

    let filename = checked_label(params.filename, "filename", MAX_TEXT_FIELD)?;
    let sealed_name = sealed::checked(params.sealed_name, "sealed_name")?;
    sealed::coherent(
        share.encryption.is_some(),
        &[("filename", filename.is_some())],
        &[("sealed_name", sealed_name.is_some())],
    )?;
    let now = OffsetDateTime::now_utc();
    // The share's owner, not the caller. A management token lets an
    // account-less uploader append to a member's share, and the bytes land
    // there,
    // so it is the owner's ceiling that has to allow them.
    let owner = match share.user_id {
        Some(id) => state.users.find_by_id(id).await?,
        None => None,
    };
    let (
        Ingested {
            storage,
            size_bytes,
            inline: _,
            segments,
        },
        _arriving,
    ) = ingest(
        &state,
        Ingest {
            id: ShareId::generate(),
            kind: ShareKind::File,
            owner: owner.as_ref(),
            now,
        },
        body,
    )
    .await?;

    state
        .shares
        .append_file(
            share.id,
            &NewFile {
                segments,
                filename,
                content_type: recorded_type(share.encryption.is_some(), &headers),
                sealed_name,
                scan: crate::scan::initial(&state, share.encryption.is_some()),
                size_bytes,
                storage,
            },
            now,
        )
        .await?;
    state.scan_wake.notify_one();

    let content = state.shares.content_of(&share).await?;
    Ok((
        StatusCode::CREATED,
        Json(ShareResponse::from_share(&share, &content, None)),
    )
        .into_response())
}

/// Metadata for `POST /shares/{code}/files`.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AddFileParams {
    /// What to call this file.
    #[param(example = "holiday.jpg")]
    pub filename: Option<String>,
    /// Its name and media type, sealed, when the share is encrypted. Replaces
    /// `filename` there.
    pub sealed_name: Option<String>,
}
