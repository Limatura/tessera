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
//! `multipart/form-data` is deliberately not accepted yet. A `fetch` with a
//! `File` body streams natively, so the only caller that would need multipart is
//! a plain HTML `<form>` — and supporting one would add a parser, a dependency
//! and a buffering path for a case nothing in this project has.

// `params(CreateShareParams)` makes utoipa emit a fully-qualified path to the
// type. The generated code lands outside the function it decorates, so the
// suppression has to be module-wide; dropping `params(..)` instead would take
// the query parameters out of the contract, which is the point of declaring
// them.
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
use object_store::{MultipartUpload, ObjectStoreExt as _, PutPayload, path::Path as ObjectPath};
use serde::{Deserialize, Serialize};
use tessera_core::{Password, Permission, RetrievalCode, RoleId, ShareId, UserId};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};

use crate::{
    api::{
        Patch, SHARES_TAG,
        extract::{ApiJson, ApiPath, ApiQuery},
        gate, patched,
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
            Change, Content, NewContent, NewFile, Share, ShareEvent, ShareKind, StorageLocation,
        },
        user::User,
    },
    error::{Error, Result, Tier},
    state::AppState,
};

/// Attempts to find an unused retrieval code before giving up.
///
/// Collisions are vanishingly rare at the default length, so more than a couple
/// of failures means the code space is genuinely crowded and the operator needs
/// to hear about it rather than have the server keep trying.
const CODE_ATTEMPTS: u8 = 8;

/// Longest `filename` and `title` a share may carry, in characters.
///
/// Tied to the MySQL column width. Without a check here the value reaches the
/// insert, and the three backends then disagree: SQLite and Postgres store it,
/// MySQL refuses the row. The caller sees `201` or `500` depending on which
/// database the operator chose — for the same request, which is the worst kind
/// of difference to leave in.
///
/// 191 rather than a round number because that is what utf8mb4 allows in an
/// indexable `VARCHAR`, and matching the column is what makes the limit real.
const MAX_TEXT_FIELD: usize = 191;

/// Longest `language` hint, matching its narrower column.
const MAX_LANGUAGE: usize = 64;

/// The request body of `POST /shares`: the content itself, as bytes.
///
/// A named type only so the contract can say `format: binary`. Declaring the
/// body as `Vec<u8>` makes utoipa emit an *array of integers*, which a docs UI
/// renders as a JSON array editor and a generated client types as `number[]` —
/// neither of which can carry a file.
///
/// An empty body is refused with a `400`, and that rule is *not* expressible
/// here: `min_length` is a field-level attribute, and the field's own schema is
/// overridden by the `value_type` above — which has to stay, or the body stops
/// being binary. So the constraint lives in the handler and is documented as a
/// response, and a generator reading this will believe an empty body is valid.
#[derive(Debug, ToSchema)]
#[schema(value_type = String, format = Binary)]
pub struct ShareBody(
    #[expect(dead_code, reason = "a schema marker; the body is read as a stream")] Vec<u8>,
);

/// Metadata for `POST /shares`, supplied as query parameters.
// `parameter_in = Query` is not a detail: without it `IntoParams` defaults to
// `Path`, and the document then tells every generated client that these belong
// in a path that has no placeholders for them — and that all five are required,
// when the handler treats every one as optional. A client built from that
// cannot construct a valid request at all.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct CreateShareParams {
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
    /// Relative on purpose: the server does not know what host it is reached
    /// through, and guessing wrong behind a reverse proxy produces links that
    /// do not work. Whoever calls this already knows the origin they called.
    #[schema(example = "/s/0ZPQ4M7K")]
    pub path: String,
    /// `text` or `file`.
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
    /// How many retrievals remain, or `null` for unlimited.
    pub uses_remaining: Option<u32>,
    /// How many times it has been retrieved.
    ///
    /// Counted by the server on the transfer path, so it is the number to
    /// trust — the two below are what a client told us.
    pub use_count: u32,
    /// How many times a client reported showing it.
    pub view_count: u32,
    /// How many times a client reported it being copied.
    pub copy_count: u32,
    /// When it was last retrieved, RFC 3339.
    ///
    /// The answer to the question somebody actually asks of their own share:
    /// has the person I sent this to picked it up yet?
    pub last_accessed_at: Option<String>,
    /// When it stops being served, RFC 3339, or `null` to live until deleted.
    pub expires_at: Option<String>,
    /// When it was created, RFC 3339.
    pub created_at: String,

    /// Token that lets an anonymous uploader delete this share.
    ///
    /// **Shown once, at creation, and only when nobody was signed in.** Only its
    /// digest is stored. Without it a mistaken anonymous upload could never be
    /// taken back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub management_token: Option<String>,
}

/// One file in a share, as the API describes it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SharedFile {
    /// Its position in the share, from zero.
    ///
    /// Sent rather than left to the array's order: a client that sorts or
    /// filters the list still has the uploader's order to put it back in.
    pub position: u32,
    /// What it was uploaded as.
    #[schema(example = "holiday.jpg")]
    pub filename: Option<String>,
    /// How many bytes it holds.
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
        description = "The content itself. Send the file as the raw body — no wrapper, no encoding.",
        content_type = "application/octet-stream",
    ),
    responses(
        // The code in this response is the only way to reach the three
        // operations below — it is random by design, so nothing can derive one.
        // Saying so here is what lets a contract-driven test chain create into
        // describe, and what tells a generated client which field to carry.
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
        (status = 507, description = "Out of room — the body says whether that is the account's quota, its role's, or the instance's", body = crate::error::ErrorBody),
    ),
    security((), ("session" = []), ("bearer" = [])),
)]
pub async fn create(
    State(state): State<AppState>,
    MaybeUser(current): MaybeUser,
    ApiQuery(params): ApiQuery<CreateShareParams>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response> {
    if current.is_none() && !state.config().features.anonymous_upload {
        return Err(Error::Unauthorized);
    }
    // An anonymous caller has no permissions to check — the setting above is
    // the whole of that decision. A signed-in one does, and this is what makes
    // a token minted for a screenshot tool unable to do anything else.
    if let Some(user) = current.as_ref() {
        user.require(Permission::SharesCreate)?;
    }

    if params.max_uses == Some(0) {
        return Err(Error::bad_request(
            "`max_uses` must be at least 1; omit it for a share that never runs out",
        ));
    }
    let filename = checked_label(params.filename, "filename", MAX_TEXT_FIELD)?;
    let title = checked_label(params.title, "title", MAX_TEXT_FIELD)?;
    let language = checked_label(params.language, "language", MAX_LANGUAGE)?;

    let now = OffsetDateTime::now_utc();
    let expires_at = resolve_expiry(&state, params.expires_in_hours, now)?;
    let password_hash = match share_password(&headers)? {
        Some(password) => Some(password::hash(password, state.config().auth.hashing).await?),
        None => None,
    };

    let id = ShareId::generate();
    let kind = if filename.is_some() {
        ShareKind::File
    } else {
        ShareKind::Text
    };

    let Ingested {
        storage,
        size_bytes,
        inline,
    } = ingest(
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

    // A share of nothing has nothing to hand out, and the retrieval code it
    // would occupy is not free.
    if size_bytes == 0 {
        return Err(Error::bad_request("the request body is empty"));
    }

    // Anonymous uploads get a token so their author can take one back. A signed-in
    // uploader already has one — their account.
    let management_secret = current.is_none().then(Secret::generate_session);

    let share = Share {
        id,
        // Replaced on each attempt below; the id and the object key do not
        // change, so a retry costs one insert rather than a re-upload.
        code: RetrievalCode::parse("0000").unwrap_or_else(|_| unreachable!("four zeroes parse")),
        user_id: current.as_ref().map(|user| user.user.id),
        // Already in hand for a signed-in caller, and the guest role for an
        // anonymous one. Neither touches the database.
        role_id: charged_role_of(current.as_ref().map(|current| &current.user)),
        kind,
        password_hash,
        use_limit: params.max_uses,
        use_count: 0,
        expires_at,
        disabled_at: None,
        title,
        og_config: None,
        management_token_hash: management_secret.as_ref().map(Secret::digest),
        view_count: 0,
        copy_count: 0,
        bytes_served: 0,
        last_accessed_at: None,
        created_at: now,
        updated_at: now,
    };

    let content = match kind {
        ShareKind::File => NewContent::File(NewFile {
            filename,
            // What the uploader claimed, recorded so a future feature can use it
            // — and never echoed, because serving an attacker-chosen media type
            // back is how a `text/html` upload becomes stored XSS on our own
            // origin.
            content_type: claimed_content_type(&headers),
            size_bytes,
            storage,
        }),
        ShareKind::Text => NewContent::Text {
            language,
            // Text is its own row, so it is always here by the time a share
            // exists — `ingest` refuses anything it could not hold.
            body: inline.unwrap_or_default(),
        },
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

/// Which slice of a share listing to return.
///
/// Shared with the administrative listing, which pages the same way over a
/// different response body.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
// Same reason as `CreateShareParams`: a query the handler does not understand
// is a caller who believes something that is not true, and answering `200` to
// it teaches them it worked.
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
/// Two reasons, and they agree. `sea-query`'s encoders convert both the limit
/// and the offset to `i64` and `unwrap()` the conversion, so anything above
/// `i64::MAX` panics inside the driver — a `500` no handler can catch,
/// reachable by anybody who can read a listing. And the document says `int64`,
/// because that is what utoipa emits for these; a value the schema does not
/// admit should be refused rather than quietly interpreted.
///
/// Found by the contract fuzzer, which is the only reader that would ever send
/// one.
const MAX_PAGE_FIGURE: u64 = i64::MAX.unsigned_abs();

/// Resolves a page request into a limit and an offset.
///
/// The bounds live in one function rather than at each call site — a route
/// that forgot one would be either the unbounded query this replaced or the
/// panic above.
///
/// # Why one bound refuses and the other clamps
///
/// They are different kinds of wrong. Asking for more rows than the page cap is
/// a reasonable request the server is entitled to answer with fewer, which is
/// what every paged API does. A figure the document does
/// not admit at all is not a request anybody meant, and answering it with a
/// page would report something for a question that was never asked.
///
/// # Errors
///
/// `400` when either figure is larger than the largest this API accepts.
pub fn page_bounds(params: &SharePageParams) -> Result<(u64, u64)> {
    for (name, value) in [("limit", params.limit), ("offset", params.offset)] {
        if value.is_some_and(|value| value > MAX_PAGE_FIGURE) {
            return Err(Error::bad_request(format!(
                "`{name}` must be at most {MAX_PAGE_FIGURE}"
            )));
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

/// A page of the caller's shares.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SharePage {
    /// The shares on this page.
    pub shares: Vec<ShareResponse>,
    /// How many the caller has in total, so a client can render pagination.
    pub total: u64,
}

/// Lists the caller's shares, newest first.
///
/// # Why this is paged
///
/// It was not, on the reasoning that one person's shares are bounded by what
/// one person uploaded. That holds for somebody using the web page and not for
/// anybody driving the API from a script: an account with fifty thousand
/// shares made every visit to the listing read fifty thousand rows and build
/// fifty thousand responses, and nothing in the request said so.
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
    // One query for the whole page rather than one per share: see
    // `contents_for`.
    let contents = state.shares.contents_for(&shares).await?;
    Ok(Json(SharePage {
        shares: shares
            .iter()
            .filter_map(|share| {
                contents
                    .get(&share.id)
                    .map(|content| ShareResponse::from_share(share, content, None))
            })
            .collect(),
        total: state.shares.count_for_user(current.user.id).await?,
    }))
}

/// Deletes a share, and whatever it put in the blob store.
///
/// Accepts either the owning account or, for an anonymous share, the management
/// token from creation — sent as `X-Share-Management-Token`.
///
/// # Errors
///
/// `404` when no share of the caller's has that code. Deliberately the same
/// answer as "somebody else's": distinguishing them would confirm which codes
/// exist.
#[utoipa::path(
    delete,
    path = "/shares/{code}",
    operation_id = "delete_share",
    tag = SHARES_TAG,
    params(("code" = String, Path, description = "The retrieval code. Normalised before it is looked up, so O reads as 0, I and L read as 1, case is folded, and spaces and hyphens are dropped — `0ZPQ4M7K`, `ozpq-4m7k` and `OZPQ4M7K` are the same share.")),
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

    // Owning it is not enough; the credential in hand has to carry the
    // permission too. A token given only `shares:create` uploads on its owner's
    // behalf and cannot then delete what the owner has.
    let owns = current.as_ref().is_some_and(|user| {
        share.user_id == Some(user.user.id) && user.permissions.allows(Permission::SharesDelete)
    });
    let holds_token = share.management_token_hash.as_ref().is_some_and(|stored| {
        management_token(&headers).is_some_and(|presented| presented.digest() == *stored)
    });
    if !owns && !holds_token {
        return Err(Error::NotFound);
    }

    // The object first. A row without its object is a broken share; an object
    // without its row is an orphan the sweep can still find by listing, so if
    // only one of the two can happen, this is the better order.
    // Every object, not the first: a share can hold several.
    for key in state.shares.object_keys_of(share.id).await? {
        state
            .storage
            .delete(&ObjectPath::from(key.as_str()))
            .await?;
    }
    state.shares.delete(share.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Body of `PATCH /shares/{code}`.
///
/// Every field is optional and omitting one leaves that setting alone. Sending
/// `null` clears it — absent and null are different instructions here.
///
/// The password is deliberately not here. It travels in `X-Share-Password`, the
/// same header the rest of this API uses, because a body is not the problem —
/// the problem is every other place a value can end up, and a header is the one
/// channel that stays out of the query string, the access log and the
/// `Referer`. What is here is only the instruction to remove it, which is not a
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
    /// below `use_count` retires the share rather than reviving it.
    #[serde(default, deserialize_with = "patched")]
    #[schema(value_type = Option<u32>, example = 1)]
    pub max_uses: Patch<u32>,

    /// Removes the share's password.
    ///
    /// Setting one is `X-Share-Password`; this is the other direction. Sending
    /// both is refused rather than guessed at.
    #[serde(default)]
    pub remove_password: bool,
}

/// Changes a share's settings after it exists.
///
/// # Why this exists
///
/// Everything here could be decided at creation and nothing could be changed
/// afterwards, which is the state this replaces. It does not survive contact
/// with how people actually share a file: the link goes out, and then the
/// reason for the expiry changes — the meeting moved, the recipient asks for
/// another day, the wrong person was in the thread and the password has to
/// change now rather than by deleting and re-uploading.
///
/// # Who may
///
/// The same two answers as `DELETE`: the owning account, or whoever holds the
/// management token an anonymous upload was given. The permission checked is
/// `shares:delete` — there is no `shares:update` today, and gating a change
/// behind the right to remove the thing outright is the conservative reading
/// while there is not.
///
/// # Errors
///
/// `400` for a value the instance will not accept, `404` when no share of the
/// caller's has that code — deliberately the same answer as somebody else's,
/// because distinguishing them would confirm which codes exist.
#[utoipa::path(
    patch,
    path = "/shares/{code}",
    operation_id = "update_share",
    tag = SHARES_TAG,
    params(
        ("code" = String, Path, description = "The retrieval code. Normalised before it is looked up, so O reads as 0, I and L read as 1, case is folded, and spaces and hyphens are dropped — `0ZPQ4M7K`, `ozpq-4m7k` and `OZPQ4M7K` are the same share."),
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

    // Byte for byte the check `delete` makes, and for the same reason: owning
    // the share is not enough if the credential in hand does not carry the
    // permission.
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
        return Err(Error::bad_request(
            "send `X-Share-Password` to set a password or `remove_password` to clear it, not both",
        ));
    }

    let now = OffsetDateTime::now_utc();
    let expires_at = match body.expires_in_hours {
        None => Change::Keep,
        // Clearing the expiry would put the share outside whatever ceiling the
        // instance set, through a route that was never meant to grant that. An
        // instance with no ceiling has nothing to bypass.
        Some(None) => {
            if state.config().share.max_expiry().is_some() {
                return Err(Error::bad_request(format!(
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
            return Err(Error::bad_request(
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

    // Read back rather than patching the copy in hand: the response then says
    // what the database holds, which is the only version that matters when the
    // caller sends the next one.
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
/// The uploader's role, or `guest` when nobody was signed in. Anonymous
/// uploads carry a role rather than NULL because `user_id IS NULL` already
/// means two things — nobody signed in, and the account was deleted since —
/// so it cannot answer "was this anonymous".
///
/// Read at upload time and stored, not looked up when the quota is checked:
/// moving somebody to another role later must not rewrite which quota their
/// old shares counted against.
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
/// The extractor has read the row to authenticate the request, so asking the
/// database again for the role it just fetched would be a second query for a
/// value already in hand.
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
}

/// Reads the body, keeping it in the database if it is small and streaming it to
/// the blob store if it is not.
///
/// # Why it is not `bytes().await`
///
/// Reading the whole body first is one line and costs the body's size in memory.
/// At the default limit that is 100 MiB per upload in flight, which on the small
/// machines this software is aimed at is the difference between working and
/// being killed by the OOM reaper. So the body is consumed a frame at a time:
/// buffered while it might still be small enough to inline, and pushed to the
/// store in fixed blocks once it is not.
///
/// # Why the size limit is enforced here
///
/// `DefaultBodyLimit` is not a limit on the connection — it is a value that
/// *extractors* consult, so `Bytes`, `String` and `Json` respect it and a
/// handler that takes the `Body` and reads frames itself does not. Reading the
/// body by hand means owning the limit by hand; without this check the layer in
/// `api::build` silently does nothing for this route.
///
/// # Errors
///
/// Returns [`Error`] if the body cannot be read or the store will not take it.
async fn ingest(state: &AppState, request: Ingest<'_>, body: Body) -> Result<Ingested> {
    let id = request.id;
    // Read before the body is consumed. `Content-Length` is what usually
    // supplies it, but taking the hint rather than the header means an
    // in-memory body reports its size too — and a chunked one reports nothing,
    // which is the honest answer to "I will not tell you how much I am
    // sending".
    let expected_bytes = <Body as http_body::Body>::size_hint(&body).exact();
    let mut upload: Option<Box<dyn MultipartUpload>> = None;
    let result = ingest_into(state, request, expected_bytes, body, &mut upload).await;

    if let Some(upload) = upload.as_mut() {
        // Only reachable when something failed: the success path takes the
        // upload out of the slot after completing it. Abandoning a multipart
        // upload leaves parts behind that a store will bill for and that nothing
        // else will ever look for, so the failure path has to say so explicitly.
        if let Err(error) = upload.abort().await {
            tracing::warn!(?error, %id, "could not abort an incomplete upload");
        }
    }
    result
}

/// What is left before the first ceiling refuses, counting uploads still
/// arriving.
///
/// An open upload session has reserved the room it declared, so it spends quota
/// before a byte of it arrives. Without that, two callers can each be promised
/// the same free space and the instance goes over its own limit.
///
/// Read once per upload rather than held in a counter. An upload is not a hot
/// path, and a counter would have to survive every failed ingest, every delete
/// and every sweep — the first time it drifted, the instance would either
/// refuse uploads it had room for or accept ones it did not, and nothing would
/// say which.
///
/// # This is a guard, not an accountant
///
/// Uploads in flight at the same time each read this before any of them has a
/// row to be counted, so together they can land past the line. The overshoot is
/// bounded by the number of concurrent uploads times `server.max_body_bytes`,
/// and the next upload after them is refused.
///
/// Closing that window would mean reserving space up front and releasing it on
/// every failure path, or completing the upload and then deleting the object
/// when the total turns out to be over. Both add a way to lose bytes that
/// should have been kept, to make a limit whose entire job is to stop unbounded
/// growth slightly more exact. Set the figure with the same slack you would
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
/// For telling a caller how much room *they* have. How full the disk is is an
/// instance-wide figure and is told to whoever may read instance-wide figures;
/// somebody's own quota is theirs to know, and answering it with the disk's
/// free space would tell every signed-in account how full the machine is.
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
/// Later ceilings win a tie so the answer names the nearest one — being told
/// "your quota is full" is actionable in a way that "the instance is full" is
/// not, and when both are equally full both are true.
async fn tightest(state: &AppState, ceilings: Vec<Ceiling>) -> Result<Room> {
    let mut tightest = Room {
        bytes: u64::MAX,
        tier: Tier::Instance,
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
/// [`Error::InsufficientStorage`] when nothing has room for the whole file.
/// Deliberately not a `413`: the file is not too big in itself, the pool is too
/// full to take it, and a smaller one — or the same one later — may well fit.
pub(crate) async fn place(state: &AppState, size: u64) -> Result<String> {
    place_where(state, size, |_| true).await
}

/// The same, restricted to backends that can hold a resumable upload open.
///
/// A backend with no multipart view cannot, and neither can one that is not the
/// default — the partial file the appending sink writes only exists beside the
/// directory `storage.url` names. Placing a resumable upload on one anyway
/// answers `400 this instance's store cannot hold a resumable upload` while
/// another backend was sitting there able to take it.
///
/// # Errors
///
/// [`Error::InsufficientStorage`] when nothing suitable has room.
pub(crate) async fn place_resumable(state: &AppState, size: u64) -> Result<String> {
    place_where(state, size, |name| {
        state.backend_multipart(name).is_some() || name == crate::storage::DEFAULT_BACKEND
    })
    .await
}

/// The same, restricted to backends that can be written a piece at a time.
///
/// A file past `share.inline_threshold_bytes` is streamed out with
/// `put_multipart` while it is still arriving, so a backend without it cannot
/// take one however much room it has. Filtering here rather than failing at the
/// write is the difference between "this file is too large for anywhere we can
/// put it" and a `500` raised halfway through the body — see
/// [`Stores::put_multipart`](crate::storage::Stores::put_multipart).
///
/// # Errors
///
/// [`Error::PayloadTooLarge`] when the only backends with room cannot take a
/// streamed write, [`Error::InsufficientStorage`] when none had room anyway.
async fn place_streamed(state: &AppState, size: u64) -> Result<String> {
    // Loaded once, outside the predicate: asking the `ArcSwap` per candidate
    // would let a reload land mid-decision and judge two candidates against
    // two different sets of backends.
    let pool = state.backends.load();
    place_where(state, size, |name| {
        pool.open
            .get(name)
            .is_some_and(|stores| stores.put_multipart)
    })
    .await
}

async fn place_where(
    state: &AppState,
    size: u64,
    suitable: impl Fn(&str) -> bool,
) -> Result<String> {
    let pool = state.backends.load();
    let used = state.shares.bytes_per_backend().await?;
    let all =
        crate::storage::candidates(&state.config().storage, &pool.configured, &pool.open, &used);
    let candidates: Vec<crate::storage::placement::Candidate> = all
        .iter()
        .filter(|candidate| suitable(&candidate.name))
        .cloned()
        .collect();

    if let Some(chosen) = crate::storage::placement::choose(&candidates, size) {
        return Ok(chosen.name.clone());
    }

    // Two different failures wear the same shape here, and telling them apart is
    // the whole point: "every backend is full" is something an operator fixes by
    // adding room, while "the backends with room cannot take a file written this
    // way" is not about room at all and adding more of it changes nothing.
    Err(if crate::storage::placement::choose(&all, size).is_some() {
        Error::PayloadTooLarge
    } else {
        Error::InsufficientStorage(Tier::Instance)
    })
}

/// Places the file and opens a multipart upload on the backend chosen.
///
/// Placed before the first byte goes anywhere. Choosing after the upload
/// started would mean recording a name the bytes are not at — which is the
/// failure this whole change exists to prevent.
///
/// # Errors
///
/// Returns [`Error`] if nothing has room, or the chosen backend will not open.
async fn start_object(
    state: &AppState,
    id: ShareId,
    now: OffsetDateTime,
    expected_bytes: Option<u64>,
) -> Result<(String, String, Box<dyn MultipartUpload>)> {
    let object_key = object_key(id, now);
    let chosen = place_streamed(state, expected_bytes.unwrap_or(0)).await?;
    let store = state.backend(&chosen).ok_or(Error::Unavailable)?;
    let started = store
        .put_multipart(&ObjectPath::from(object_key.as_str()))
        .await?;
    Ok((chosen, object_key, started))
}

/// How much room one named backend has, or [`u64::MAX`] when nothing limits it.
///
/// # Errors
///
/// Returns [`Error`] if the usage cannot be read.
async fn backend_free(state: &AppState, name: &str) -> Result<u64> {
    let pool = state.backends.load();
    let used = state.shares.bytes_per_backend().await?;
    Ok(
        crate::storage::candidates(&state.config().storage, &pool.configured, &pool.open, &used)
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
/// five holding 100 MB each cannot take 200 MB — and reporting 500 MB would be
/// a figure that explains nothing when the upload is refused.
///
/// # Errors
///
/// Returns [`Error`] if the usage cannot be read.
pub(crate) async fn largest_placeable(state: &AppState) -> Result<Option<u64>> {
    let pool = state.backends.load();
    let used = state.shares.bytes_per_backend().await?;
    Ok(crate::storage::placement::largest_acceptable(
        &crate::storage::candidates(&state.config().storage, &pool.configured, &pool.open, &used),
    ))
}

/// The largest single file this caller may send.
///
/// Nearest wins, same as the storage quota: the account's own figure, then its
/// role's, then `server.max_body_bytes`. `0` at a layer means no limit *there*
/// and hands the question on to the instance, which always applies —
/// `DefaultBodyLimit` is baked into the router and never sees these.
///
/// Not a [`Tier`], because a refusal here is a `413` and there is nothing the
/// caller can do about which layer said so: the file is too big and will stay
/// too big. That is the difference from a `507`, where knowing whose quota is
/// full tells them whether deleting their own shares would help.
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
/// Two at most, and they are `AND`ed rather than resolved to one winner:
///
/// * `storage.max_total_bytes`, which is the disk. Nothing may pass it — a role granted "no
///   ceiling" means no ceiling *of its own*, not permission to fill the machine, because the person
///   who set the role is often not the person who owns the disk.
/// * The caller's own figure, taken from their account if it has one and from their role otherwise.
///   Nearest wins, so raising one person does not need a role invented for them.
///
/// A tier that says `0` states there is no ceiling *there* and drops out. A tier
/// that says nothing at all asks the next one, which is why the columns are
/// nullable — see the migration.
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

    // Anonymous work is charged to the guest role, so an instance that allows
    // it can still bound it — otherwise the only figure reaching a caller with
    // no account would be the instance's, and "let strangers upload, but not
    // more than twenty gigabytes of it" would be unsayable.
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
/// Returns [`Error::BadRequest`] if it is too long or holds a control
/// character.
pub(crate) fn checked_filename(value: Option<String>) -> Result<Option<String>> {
    checked_label(value, "filename", MAX_TEXT_FIELD)
}

/// What one ingest is: where the bytes go, what they are, and whose ceiling
/// has to allow them.
struct Ingest<'a> {
    /// Names the object, not the share. A share holds several files and each
    /// needs its own key — deriving it from the share meant the second upload
    /// wrote over the first, silently, and the download sent one file's bytes
    /// three times. Measured before it was fixed: two rows, one object.
    id: ShareId,
    /// Decides whether the bytes become an object or a row.
    kind: ShareKind,
    /// Whose ceilings apply. Not always the caller: a management token lets
    /// somebody with no account append to a member's share.
    owner: Option<&'a User>,
    /// One reading of the clock for the whole request.
    now: OffsetDateTime,
}

async fn ingest_into(
    state: &AppState,
    request: Ingest<'_>,
    // What the body says it will be, when it says. Chooses the backend before
    // the first byte is written — without it the placer can only be asked for
    // "somewhere with any room", which will happily pick one the file then does
    // not fit in, and by then parts have already been written there.
    expected_bytes: Option<u64>,
    body: Body,
    upload: &mut Option<Box<dyn MultipartUpload>>,
) -> Result<Ingested> {
    let Ingest {
        id,
        kind,
        owner,
        now,
    } = request;
    // Where the bytes end up is decided by what they are, not by how many of
    // them there are. A file is an object whatever its size, so an operator
    // looking for an upload finds it in one place; text is a row, so reading a
    // paste is one query.
    //
    // The threshold still earns its keep: it is how much of a file is buffered
    // before the multipart upload starts, which is a memory decision rather than
    // a storage one.
    let stream_to_object = kind == ShareKind::File;
    let text_limit = state.config().share.max_text_bytes;
    let threshold = state.config().share.inline_threshold_bytes;
    let part_size = state.config().server.part_size_bytes;
    // The caller's own limit, which is `server.max_body_bytes` or something
    // narrower. Read here rather than in the handler so both the direct and the
    // resumable paths go through one definition of "too big for you".
    let limit = max_file_bytes(state, owner).await?;
    // What is left of the instance's allowance, checked as the body arrives
    // rather than only up front: the caller need not send a `Content-Length`,
    // and a chunked upload that would take us past the line should stop at the
    // line instead of after it.
    let room = remaining_room(state, owner).await?;
    let mut body = body;
    let mut buffered: Vec<u8> = Vec::new();
    let mut size_bytes: u64 = 0;
    let mut key: Option<(String, String)> = None;
    // Until a backend is chosen there is nothing to exceed.
    let mut backend_limit = u64::MAX;

    let stall = state.config().server.upload_stall();

    while let Some(frame) = next_frame(&mut body, stall).await? {
        let Ok(chunk) = frame.into_data() else {
            // Trailers carry no content; a body that is only trailers is empty.
            continue;
        };
        size_bytes = size_bytes.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
        if size_bytes > limit {
            return Err(Error::PayloadTooLarge);
        }
        // Checked as it arrives rather than at the end: a paste ten times the
        // limit should stop at the limit, not be read in full and then refused.
        if !stream_to_object && size_bytes > u64::try_from(text_limit).unwrap_or(u64::MAX) {
            return Err(Error::bad_request(
                "that is larger than this instance keeps as text; send it as a file instead",
            ));
        }
        if size_bytes > room.bytes {
            return Err(Error::InsufficientStorage(room.tier));
        }
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
            let (chosen, object_key, started) =
                start_object(state, id, now, expected_bytes).await?;
            backend_limit = backend_free(state, &chosen).await?;
            key = Some((chosen, object_key));
            *upload = Some(started);
        }
        // The chosen backend's own ceiling, checked as the bytes arrive for the
        // same reason the quota is: a body that would take it past the line
        // should stop at the line rather than after it — and past the line here
        // is the provider refusing mid-upload, which is worse than a 507.
        if size_bytes > backend_limit {
            return Err(Error::InsufficientStorage(Tier::Instance));
        }
        if let Some(upload) = upload.as_mut() {
            flush_full_parts(upload.as_mut(), &mut buffered, part_size).await?;
        }
    }

    match key {
        Some((backend, key)) => {
            let mut started = upload.take().ok_or_else(|| {
                Error::bad_request("upload state was lost part-way through the body")
            })?;
            if !buffered.is_empty() {
                started
                    .put_part(PutPayload::from(std::mem::take(&mut buffered)))
                    .await?;
            }
            started.complete().await?;
            Ok(Ingested {
                storage: StorageLocation::Object { backend, key },
                size_bytes,
                inline: None,
            })
        }
        // A file small enough that the multipart upload never started. It still
        // becomes an object — one `put` rather than a multipart, because a
        // multipart upload of nine bytes is three round trips to say one thing.
        None if stream_to_object => {
            let object_key = object_key(id, now);
            // The whole file is in hand, so the size is known before a byte is
            // written and the placer can be asked. The streaming path above
            // cannot do this — it has already started writing by the time it
            // knows how much there is.
            let backend = place(state, size_bytes).await?;
            let store = state.backend(&backend).ok_or(Error::Unavailable)?;
            store
                .put(
                    &ObjectPath::from(object_key.as_str()),
                    PutPayload::from(buffered),
                )
                .await?;
            Ok(Ingested {
                storage: StorageLocation::Object {
                    backend,
                    key: object_key,
                },
                size_bytes,
                inline: None,
            })
        }
        None => Ok(Ingested {
            storage: StorageLocation::Inline,
            size_bytes,
            inline: Some(buffered),
        }),
    }
}

/// The next piece of a body's data, or `None` at its end.
///
/// Trailers carry no content, so they are skipped rather than returned. The
/// wait is bounded the same way `next_frame` bounds it — see there.
///
/// # Errors
///
/// Returns [`Error::Timeout`] when nothing arrives for `stall`, or
/// [`Error::BadRequest`] when the body ends part-way.
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
/// way through, and a client that stopped sending is not answering at all. This
/// is the only place that can tell those apart — by the time a request-wide
/// budget expires, both look the same.
///
/// See `api::timeout` for why the wall-clock budget is not applied here.
async fn next_frame(body: &mut Body, stall: Duration) -> Result<Option<http_body::Frame<Bytes>>> {
    match tokio::time::timeout(stall, body.frame()).await {
        Ok(Some(frame)) => frame
            .map(Some)
            .map_err(|error| Error::bad_request(format!("body ended early: {error}"))),
        Ok(None) => Ok(None),
        Err(_) => Err(Error::Timeout),
    }
}

/// Pushes as many whole blocks as the buffer holds, leaving the remainder.
///
/// `part_size` is `server.part_size_bytes`. It is fixed rather than following
/// whatever chunk sizes the network delivered, because most object stores
/// require every part but the last to be at least 5 MiB and some require them
/// all to be the same size.
///
/// **It is also the memory an in-flight upload holds** — one block per upload,
/// not one per connection, because a block goes out as soon as it fills.
async fn flush_full_parts(
    upload: &mut dyn MultipartUpload,
    buffered: &mut Vec<u8>,
    part_size: usize,
) -> Result<()> {
    while buffered.len() >= part_size {
        let rest = buffered.split_off(part_size);
        let part = std::mem::replace(buffered, rest);
        upload.put_part(PutPayload::from(Bytes::from(part))).await?;
    }
    Ok(())
}

/// `{yyyy}/{mm}/{id}`.
///
/// Sharded by date rather than flat: a personal instance running for years would
/// otherwise put every object in one directory, and a local filesystem handles
/// that badly. Sharding on the identifier's leading characters would not help —
/// a ULID starts with a timestamp, so those characters are almost the same for
/// everything written in the same era. Dates are also what an operator wants to
/// see when deciding what to back up or what is old.
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
/// only writes the rows and picks a code. Everything a `POST /shares` decides
/// from its query — password, expiry, use limit — is not available to an upload
/// session, so those take their defaults and the uploader adjusts the share
/// afterwards.
///
/// # Errors
///
/// Returns [`Error`] if no unused code can be found or the insert fails.
pub(crate) async fn create_from_file(
    state: &AppState,
    user_id: Option<UserId>,
    file: NewFile,
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
        password_hash: None,
        use_limit: None,
        use_count: 0,
        expires_at: resolve_expiry(state, None, now)?,
        disabled_at: None,
        title: None,
        og_config: None,
        management_token_hash: management_secret.as_ref().map(Secret::digest),
        view_count: 0,
        copy_count: 0,
        bytes_served: 0,
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
            Ok(()) => return Ok(share),
            Err(error) => {
                // Distinguish "that code is taken" from a database that is
                // unwell by looking, rather than assuming every failure is a
                // collision — that would retry through an outage and then
                // report it as a conflict.
                if state.shares.find_by_code(&share.code).await?.is_none() {
                    return Err(error.into());
                }
            }
        }
    }
    Err(Error::Conflict(
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
/// Control characters are refused rather than stripped. `filename` is echoed in
/// `Content-Disposition` when the share is downloaded, and a value containing
/// CR or LF would end the header and let the caller append one of their own.
/// Stripping would work, but silently changing what somebody named their file is
/// worse than telling them it is not a name.
fn checked_label(value: Option<String>, field: &'static str, max: usize) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > max {
        return Err(Error::bad_request(format!(
            "`{field}` must be at most {max} characters"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(Error::bad_request(format!(
            "`{field}` must not contain control characters"
        )));
    }
    Ok(Some(value))
}

/// Reads the optional share password from `X-Share-Password`.
///
/// A header rather than a query parameter: query strings end up in access logs,
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
        // Most shares arrive without a lifetime, so this is the figure that
        // decides how much an instance holds. It used to be the ceiling itself,
        // which meant the *longest* life anyone was allowed was also what
        // everyone got by default — thirty days of uploads held to serve links
        // that are almost all collected within hours.
        return Ok(state
            .config()
            .share
            .default_expiry()
            .map(|limit| now.saturating_add(duration_of(limit))));
    };

    if hours == 0 {
        return Err(Error::bad_request(
            "`expires_in_hours` must be at least 1; omit it for the instance default",
        ));
    }
    let requested = std::time::Duration::from_secs(hours.saturating_mul(3600));
    if ceiling.is_some_and(|limit| requested > limit) {
        return Err(Error::bad_request(format!(
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
    /// `i64::MAX` panicked inside the driver and came back as a `500` — from a
    /// query parameter, on a route any signed-in caller can reach. Found by the
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

    /// The cap is the whole reason the listing is bounded.
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
/// is unlocked. A file name and a size are a description of the content, and a
/// password-protected share should not describe itself to somebody who has not
/// presented the password — the same reason its link preview must not.
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
    /// Whether the fields below are filled in.
    pub unlocked: bool,

    /// Display name.
    pub title: Option<String>,
    /// Every file this share holds. Empty for text and for a locked share.
    pub files: Vec<SharedFile>,
    /// Syntax-highlighting hint.
    pub language: Option<String>,
    /// Size of the content in bytes.
    pub size_bytes: Option<u64>,
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
            unlocked: false,
            title: None,
            files: Vec::new(),
            language: None,
            size_bytes: None,
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
            files: SharedFile::list(content),
            language: match content {
                Content::Text(text) => text.language.clone(),
                Content::Files(_) | Content::Gone => None,
            },
            size_bytes: Some(content.size_bytes()),
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
        ("code" = String, Path, description = "The retrieval code. Normalised before it is looked up, so O reads as 0, I and L read as 1, case is folded, and spaces and hyphens are dropped — `0ZPQ4M7K`, `ozpq-4m7k` and `OZPQ4M7K` are the same share."),
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
    // rather than a dead end. What it answers is only "this exists and wants a
    // password".
    if headers.get(gate::PASSWORD_HEADER).is_none() {
        return Ok(Json(PublicShareResponse::locked(&share)));
    }
    gate::unlock(&state, &share, &headers, &source).await?;
    // Read only once the password is right: a locked share answers without
    // touching its content, so a wrong guess costs a gate check and nothing
    // more.
    let content = state.shares.content_of(&share).await?;
    Ok(Json(PublicShareResponse::unlocked(&share, &content)))
}

/// Spends one retrieval and returns a ticket for the transfer.
///
/// # Why this is a separate step
///
/// One authorisation costs one retrieval; the ticket it returns is then good for
/// every request the transfer needs — each `Range`, each resumption, each retry.
/// Counting requests instead would let a resumed download spend three uses of a
/// share that allows three.
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
        ("code" = String, Path, description = "The retrieval code. Normalised before it is looked up, so O reads as 0, I and L read as 1, case is folded, and spaces and hyphens are dropped — `0ZPQ4M7K`, `ozpq-4m7k` and `OZPQ4M7K` are the same share."),
        ("X-Share-Password" = Option<String>, Header, description = "The share's password, if it has one"),
    ),
    responses(
        (status = 200, description = "Authorised", body = TicketResponse),
        (status = 401, description = "Wrong password", body = crate::error::ErrorBody),
        (status = 404, description = "No such code", body = crate::error::ErrorBody),
        (status = 410, description = "Expired or exhausted", body = crate::error::ErrorBody),
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
    /// Retrieval is deliberately not one of these — see [`record_event`].
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
/// These two are the opposite case: nothing reaches the server when somebody
/// reads a note on screen or presses a copy button, so a client is the only
/// thing that can see them at all. The price is that anybody who can call this
/// can inflate them, which is tolerable precisely because nothing depends on
/// them.
///
/// # Why it does not say whether the share exists
///
/// `204` whatever happens, including for a code that names nothing. Answering
/// `404` would make an unauthenticated endpoint into a way to ask which codes
/// are real — the same oracle the retrieval throttle exists to close, reached
/// through a door nobody was watching.
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
        // Named rather than listed as "invalid": a client sending `download`
        // has misread what this endpoint is for, and saying so is more use than
        // a list of two words.
        "download" | "retrieval" => {
            return Err(Error::bad_request(
                "retrievals are counted by the server on the transfer path, not reported here",
            ));
        }
        _ => return Err(Error::bad_request("`kind` must be `view` or `copy`")),
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
/// One request carrying five files is one request that loses five when it drops
/// at the fourth — which is the case somebody uploading a folder of photographs
/// on a home connection hits, not a theoretical one. A file that fails here is
/// retried on its own.
///
/// # Why the share exists before its second file
///
/// There is no draft state. A share is created by its first file and grown by
/// the rest, so nothing has to remember to finish, and an upload interrupted
/// half way leaves a share holding what did arrive rather than nothing at all.
///
/// The cost is that somebody who opened the link during the upload would see
/// part of it. The link is handed out after the upload, by the person doing it.
///
/// # Errors
///
/// `403` when the caller cannot show it is theirs, `404` when no share has that
/// code, `409` when the share holds text, `413` when the body is too large,
/// `507` when something is out of room — the share owner's quota, their role's,
/// or the instance's. The body says which.
#[utoipa::path(
    post,
    path = "/shares/{code}/files",
    operation_id = "add_share_file",
    tag = SHARES_TAG,
    params(("code" = String, Path, description = "The share to add to"), AddFileParams),
    request_body(content = Vec<u8>, description = "The file", content_type = "application/octet-stream"),
    responses(
        (status = 201, description = "The share as it now stands", body = ShareResponse),
        (status = 403, description = "Not yours", body = crate::error::ErrorBody),
        (status = 404, description = "No such share", body = crate::error::ErrorBody),
        (status = 409, description = "That share holds text", body = crate::error::ErrorBody),
        (status = 413, description = "Body exceeds the configured limit", body = crate::error::ErrorBody),
        (status = 507, description = "Out of room — the body says whether that is the account's quota, its role's, or the instance's", body = crate::error::ErrorBody),
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

    // Adding to somebody else's share would let a stranger put content behind
    // their link. The same two proofs the delete route accepts: the management
    // token the uploader was handed, or being the account that owns it.
    let owns = current.as_ref().is_some_and(|user| {
        share.user_id == Some(user.user.id) && user.permissions.allows(Permission::SharesCreate)
    });
    let holds_token = share.management_token_hash.as_ref().is_some_and(|stored| {
        management_token(&headers).is_some_and(|presented| presented.digest() == *stored)
    });
    if !owns && !holds_token {
        // The same answer as a share that is not there, for the same reason the
        // delete route gives it: otherwise this endpoint says which codes exist.
        return Err(Error::NotFound);
    }

    if share.kind == ShareKind::Text {
        return Err(Error::Conflict(
            "that share holds text; a text share holds one thing".to_owned(),
        ));
    }

    let filename = checked_label(params.filename, "filename", MAX_TEXT_FIELD)?;
    let now = OffsetDateTime::now_utc();
    // The share's owner, not the caller: a management token lets somebody with
    // no account append to a member's share, and the bytes land on that share,
    // so it is the owner's ceiling that has to allow them.
    let owner = match share.user_id {
        Some(id) => state.users.find_by_id(id).await?,
        None => None,
    };
    let Ingested {
        storage,
        size_bytes,
        inline: _,
    } = ingest(
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
                filename,
                content_type: claimed_content_type(&headers),
                size_bytes,
                storage,
            },
            now,
        )
        .await?;

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
}
