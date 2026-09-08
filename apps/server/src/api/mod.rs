//! Route assembly, middleware stack and the OpenAPI document.
//!
//! The OpenAPI document is derived from the handlers, not written by hand, and
//! `openapi.json` at the repository root is a build artefact of this
//! module (see the `tessera-openapi` binary). The frontend and the Discord bot
//! generate their clients from that file; the server emits no TypeScript.
//!
//! # Why the contract is OpenAPI and not a Rust-to-TypeScript exporter
//!
//! Rust is the single source of truth either way, since `utoipa` derives the
//! document from the same structs the handlers use, doc comments included. The
//! choice is only about the intermediate artefact: `openapi.json`, or `.ts`
//! files written into the frontend tree by something like `specta`.
//!
//! Three things settle it:
//!
//! - **The consumers are not all TypeScript.** A Discord bot, a CLI and ShareX read this too. One
//!   document generates a client for any of them; a TypeScript exporter serves exactly one.
//! - **Half the public surface is not a procedure.** `/s/{code}/raw` answers `text/plain`, and
//!   `/s/{code}/download` answers `206 Partial Content` with `Range`, `If-Range` and
//!   `Content-Disposition`. Those are HTTP contracts, not return types; a type exporter has nothing
//!   to say about them, and an RPC framework cannot express them at all.
//! - **The browsable docs need it.** Scalar renders this document, and that is what makes the API
//!   explorable by pasting values into a page.
//!
//! What a type exporter would buy is tighter modelling of data-carrying enums.
//! Nothing on this API is one today: every enum here is a plain string union in
//! the generated client, and that is what a caller wants. If that changes,
//! adding
//! `specta` alongside is possible, since both derive from the same structs and
//! a CI check catches a struct that grew one derive and not the other. Not
//! worth a second artefact before there is a type that needs it.

pub mod admin;
pub mod archive;
pub mod auth;
pub mod backends;
pub mod capabilities;
pub mod extract;
pub mod gate;
pub mod health;
pub mod meter;
pub mod oidc;
pub mod preferences;
pub mod preview;
pub mod public;
pub mod sessions;
pub mod settings;
pub mod shares;
pub mod timeout;
pub mod tokens;
pub mod uploads;

use std::{sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    http::{HeaderName, HeaderValue, Method, Request, header},
};
use tower_http::{
    catch_panic::CatchPanicLayer,
    compression::{
        CompressionLayer,
        predicate::{DefaultPredicate, NotForContentType, Predicate},
    },
    cors::{AllowOrigin, CorsLayer},
    request_id::{MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    trace::TraceLayer,
};
use ulid::Ulid;
use utoipa::OpenApi;
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_scalar::{Scalar, Servable as _};

use crate::{assets, config::Config, state::AppState};

/// One field of a partial update: absent, explicitly `null`, or a value.
///
/// `Option<T>` cannot say this. serde hands back `None` both for "the caller
/// did not mention it" and for "the caller sent `null`", which in a `PATCH` are
/// opposite instructions. Every field that can be cleared carries this instead.
pub(crate) type Patch<T> = Option<Option<T>>;

/// Deserialises a present field into [`Patch`], so that absent stays `None`.
///
/// `#[serde(default)]` supplies `None` when the key is missing; this runs only
/// when it is there. That is what makes `null` land as `Some(None)`.
pub(crate) fn patched<'de, D, T>(deserializer: D) -> Result<Patch<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    use serde::Deserialize as _;

    Option::<T>::deserialize(deserializer).map(Some)
}

/// Tag grouping infrastructure endpoints in the generated document.
pub const SYSTEM_TAG: &str = "system";

/// Tag grouping registration, sign-in and password endpoints.
pub const AUTH_TAG: &str = "auth";

/// Tag grouping API-token management.
pub const TOKENS_TAG: &str = "tokens";

/// Tag for the endpoints that show and end a person's own sign-ins.
pub const SESSIONS_TAG: &str = "sessions";

/// Tag for the endpoints an administrator uses on other people's accounts.
pub const ADMIN_TAG: &str = "admin";

/// Tag grouping share creation and management.
pub const SHARES_TAG: &str = "shares";

/// Tag for the endpoints a person uses on their own preferences.
pub const PREFERENCES_TAG: &str = "preferences";

/// Name of the cookie-based security scheme in the generated document.
pub const SESSION_SCHEME: &str = "session";

/// Name of the bearer-token security scheme in the generated document.
pub const BEARER_SCHEME: &str = "bearer";

/// Header carrying the per-request correlation id.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Path the generated contract is served from.
pub const OPENAPI_PATH: &str = "/openapi.json";

/// Path the Swagger UI is mounted at.
pub const DOCS_PATH: &str = "/docs";

/// Prefix every versioned endpoint sits behind.
///
/// `/health` and `/docs` stay at the root, where orchestrators and people
/// expect them, and neither is part of the contract third parties generate
/// clients from. Everything that is carries a version, because the frontend, a
/// Discord bot and a CLI generate against it independently and cannot all be
/// updated at once.
pub const API_PREFIX: &str = "/api/v1";

/// Root of the generated OpenAPI document.
#[derive(Debug, OpenApi)]
#[openapi(
    info(
        title = "Tessera API",
        description = "Anonymous file courier. Upload a file or a note, hand out a retrieval \
                       code, and the payload destroys itself once it expires or runs out of \
                       retrievals.",
    ),
    tags(
        (name = SYSTEM_TAG, description = "Health and service metadata"),
        (name = AUTH_TAG, description = "Registration, sign-in and passwords"),
        (name = TOKENS_TAG, description = "API tokens for non-browser callers"),
        (name = SESSIONS_TAG, description = "Seeing and ending your own sign-ins"),
        (name = ADMIN_TAG, description = "Administering the instance"),
        (name = SHARES_TAG, description = "Creating and managing shares"),
        (name = PREFERENCES_TAG, description = "A person's own preferences"),
    ),
    modifiers(&SecurityAddon),
    components(schemas(crate::error::ErrorBody, crate::error::ErrorCode)),
)]
pub struct ApiDoc;

/// Declares the two ways a caller can prove who they are.
///
/// Written as a modifier, not in the attribute, because utoipa's derive has no
/// syntax for an `apiKey` scheme in a cookie. Both are declared so the
/// docs UI offers each: a browser gets the cookie after signing in, and a
/// script pastes a token into the bearer field.
#[derive(Debug)]
struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{
            ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme,
        };

        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            SESSION_SCHEME,
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                crate::auth::cookie::COOKIE_NAME,
                "Set by `POST /auth/login`. A browser sends it automatically; it is \
                 `HttpOnly`, so page scripts cannot read or set it.",
            ))),
        );
        components.add_security_scheme(
            BEARER_SCHEME,
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("tsk_*")
                    .description(Some(
                        "An API token from `POST /api/v1/tokens`, sent as \
                         `Authorization: Bearer tsk_…`. Shown once at creation.",
                    ))
                    .build(),
            ),
        );
    }
}

/// Builds the OpenAPI document without touching a database or a storage backend.
///
/// Used by the `tessera-openapi` binary and by the contract test, both of which
/// have to run in an empty container.
#[must_use]
pub fn openapi() -> utoipa::openapi::OpenApi {
    let (_, mut api) = api_routes().split_for_parts();
    // The package carries no `license` field, and utoipa renders that absence as
    // an empty licence name. Omitting the object is honest; `{"name": ""}` is not.
    // Set `package.license` in Cargo.toml once the project picks one and drop this.
    api.info.license = None;
    api
}

/// The contract as JSON, with utoipa's link keys spelt the way the
/// specification spells them.
///
/// utoipa's `Link` derives `Serialize` with no rename, so it emits
/// `operation_id`, `operation_ref` and `request_body` where OpenAPI names those
/// `operationId`, `operationRef` and `requestBody`. Anything reading the
/// document to the specification sees a link with no target and skips it,
/// losing the one statement here about how these operations connect: the code
/// `POST /shares` returns is what the three `{code}` operations take.
///
/// Repairing the keys is the narrow fix. Dropping links altogether would keep
/// the document valid by making it say less.
///
/// Both the served document and the file on disk go through this, so "the
/// contract" means one thing.
#[must_use]
pub fn openapi_json() -> serde_json::Value {
    /// `(what utoipa writes, what the specification calls it)`.
    const RENAMES: [(&str, &str); 3] = [
        ("operation_id", "operationId"),
        ("operation_ref", "operationRef"),
        ("request_body", "requestBody"),
    ];

    let mut document = serde_json::to_value(openapi())
        .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));

    let links = document
        .get_mut("paths")
        .and_then(serde_json::Value::as_object_mut)
        .into_iter()
        .flat_map(|paths| paths.values_mut())
        .filter_map(serde_json::Value::as_object_mut)
        .flat_map(|operations| operations.values_mut())
        .filter_map(|operation| operation.get_mut("responses"))
        .filter_map(serde_json::Value::as_object_mut)
        .flat_map(|responses| responses.values_mut())
        .filter_map(|response| response.get_mut("links"))
        .filter_map(serde_json::Value::as_object_mut)
        .flat_map(|links| links.values_mut())
        .filter_map(serde_json::Value::as_object_mut);

    for link in links {
        for (written, specified) in RENAMES {
            if let Some(value) = link.remove(written) {
                link.insert(specified.to_owned(), value);
            }
        }
    }

    document
}

/// The routed part of the service: everything that appears in the contract.
fn api_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(health::health))
        .nest(API_PREFIX, versioned_routes())
}

/// Endpoints under [`API_PREFIX`]. Nesting rewrites their paths in the
/// generated document too, so a handler declares the path it owns, not the
/// prefix.
///
/// Every endpoint is mounted on every instance, including ones whose feature is
/// switched off; those answer `404` from inside the handler. Mounting
/// conditionally would make the served document depend on the instance, and the
/// point of `/capabilities` is that it does not.
fn versioned_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(capabilities::capabilities))
        .routes(routes!(auth::register))
        .routes(routes!(auth::confirm_registration))
        .routes(routes!(auth::login))
        .routes(routes!(oidc::provider))
        .routes(routes!(oidc::start))
        .routes(routes!(oidc::callback))
        .routes(routes!(oidc::link))
        .routes(routes!(oidc::identities))
        .routes(routes!(oidc::unlink))
        .routes(routes!(oidc::backchannel_logout))
        .routes(routes!(auth::logout))
        .routes(routes!(auth::session))
        .routes(routes!(auth::change_password))
        .routes(routes!(auth::update_account, auth::delete_account))
        // Mounted even where no relay is configured, like every other switched
        // off feature: the handler answers 404 and the document stays the same
        // on every instance. `/capabilities` is what tells a client whether to
        // draw the link.
        .routes(routes!(auth::request_password_reset))
        .routes(routes!(auth::confirm_password_reset))
        .routes(routes!(tokens::list, tokens::create))
        .routes(routes!(tokens::revoke))
        .merge(account_routes())
        .merge(admin_routes())
        .merge(share_routes())
        .merge(upload_routes())
}

/// What a person does to their own account: their sign-ins and their
/// preferences.
///
/// Grouped for the same reason the three below are: [`versioned_routes`] should
/// stay readable as one page.
fn account_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(sessions::list, sessions::revoke_others))
        .routes(routes!(sessions::revoke))
        .routes(routes!(preferences::get, preferences::update))
}

/// The share endpoints.
///
/// Grouped for the same reason `admin_routes` and `upload_routes` are: five
/// more lines to scan past in [`versioned_routes`].
fn share_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(shares::list, shares::create))
        .routes(routes!(shares::describe, shares::patch, shares::delete))
        .routes(routes!(shares::unlock))
        .routes(routes!(shares::record_event))
        .routes(routes!(shares::add_file))
}

/// Everything under `/admin`.
///
/// Its own function because it is its own surface: an operator can switch it
/// off with `features.admin`, and its frontend is a separate application that
/// may never be deployed. Keeping the list here also leaves
/// [`versioned_routes`] readable as one page.
fn admin_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(admin::list_users))
        .routes(routes!(admin::update_user, admin::delete_user))
        .routes(routes!(admin::list_invites, admin::create_invite))
        .routes(routes!(admin::revoke_invite))
        .routes(routes!(admin::stats))
        .routes(routes!(admin::list_roles, admin::create_role))
        .routes(routes!(admin::update_role, admin::delete_role))
        .routes(routes!(admin::list_permissions))
        .routes(routes!(admin::set_user_roles))
        .routes(routes!(admin::list_shares))
        .routes(routes!(admin::delete_share))
        .routes(routes!(admin::share_access_log))
        .routes(routes!(backends::list, backends::add))
        .routes(routes!(backends::remove))
        .routes(routes!(backends::set_default))
        .routes(routes!(settings::list, settings::update))
        .routes(routes!(settings::preview))
        .routes(routes!(admin::remove_user_shares))
}

/// The resumable-upload endpoints.
///
/// Kept out of [`versioned_routes`] for the same reason `admin_routes` is: that
/// function should read as one page.
fn upload_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(uploads::create))
        .routes(routes!(uploads::offset, uploads::append))
}

/// Assembles the complete application: API routes, contract, docs, embedded SPA
/// and the middleware stack.
pub fn build(state: AppState) -> Router {
    // A snapshot, by design. Everything read from it below is baked into a
    // middleware layer or a mounted route, so those keys are
    // on `settings::FIXED_AT_BOOT` and the panel does not offer them.
    let config = Arc::clone(&state.config());
    // Cloned before the router takes ownership: the meter is a `from_fn_with_state`
    // layer applied after `with_state`, so it needs its own handle.
    let metering_state = state.clone();
    let timeout_state = state.clone();
    let (routes, api) = api_routes().split_for_parts();

    // The contract is served unconditionally. Turning the browsable docs off is
    // about not shipping a UI, not hiding the API: client generators and the CI
    // drift check both read this, and the spec is in the repository anyway.
    let contract = openapi_json();
    let mut app = Router::new()
        .merge(routes)
        // Outside the contract by design: these are where a link points, not
        // an API. See `public` for why that freezes their shape.
        .nest(public::SHARE_PREFIX, public::routes())
        .route(
            OPENAPI_PATH,
            axum::routing::get(move || std::future::ready(Json(contract))),
        )
        // Attached before the state is applied because the fallback needs it:
        // `/s/{code}` is served from the bundle like any client-side route, but
        // with the share's link preview spliced into the HTML on the way out.
        .fallback(assets::handler)
        .with_state(state);

    if config.features.docs {
        app = app.merge(Scalar::with_url(DOCS_PATH, api));
    } else {
        // Answered here, not left to the fallback. Without this, `/docs` is a
        // path the router does not know, so it reaches the SPA and comes back
        // as the "page not found" a typo produces, with no way to tell "the
        // docs are off" from "I spelt it wrong".
        //
        // Still a 404, because from outside there is no docs page here. What
        // changes is that the body says so, and says where the contract is.
        app = app.route(DOCS_PATH, axum::routing::get(docs_are_off));
    }

    let request_id_header = HeaderName::from_static(REQUEST_ID_HEADER);

    app
        // Innermost first: `.layer` wraps, so the last call is the outermost.
        // Compression sits closest to the handler and therefore compresses the
        // final body exactly once.
        .layer(CompressionLayer::new().compress_when(compression_predicate()))
        // Outside compression, so it counts the bytes that go on the wire
        // and not the ones the handler produced. Measured: for a 64 KiB run of
        // one character those differ by a factor of six hundred.
        .layer(axum::middleware::from_fn_with_state(
            metering_state,
            meter::count_bytes_served,
        ))
        .layer(DefaultBodyLimit::max(config.server.max_body_bytes))
        .layer(cors(&config))
        // Reads the budget per request instead of holding the value it was
        // built with, and exempts the routes that receive a file, where a
        // wall-clock budget answers the wrong question. See `timeout`.
        .layer(axum::middleware::from_fn_with_state(
            timeout_state,
            timeout::enforce,
        ))
        // Outside the timeout, so a panicking handler becomes a 500 and not a
        // dropped connection.
        .layer(CatchPanicLayer::new())
        .layer(PropagateRequestIdLayer::new(request_id_header.clone()))
        .layer(TraceLayer::new_for_http().make_span_with(make_span))
        // Outermost: stamped before anything else, so every log line and every
        // error response for this request can be correlated.
        .layer(SetRequestIdLayer::new(request_id_header, UlidRequestId))
}

/// The answer at `/docs` when `features.docs` is off.
///
/// Plain text, not the JSON every other error uses. This is a path typed into a
/// browser, and a sentence reads better there than a machine-readable
/// envelope.
async fn docs_are_off() -> impl axum::response::IntoResponse {
    (
        axum::http::StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        format!(
            "The browsable API documentation is turned off on this instance \
             (`features.docs`).\n\nThe contract itself is always served, at \
             {OPENAPI_PATH} — point a client generator or a local viewer at \
             that.\n"
        ),
    )
}

/// What is worth compressing.
///
/// Public so the test suite can assert on it directly; inferring the policy
/// from response headers would test the plumbing, not the decision.
///
/// The default already skips gRPC, images and server-sent events. Added here is
/// everything arriving as opaque or already-compressed bytes.
///
/// `GET /s/{code}/download` always sends `application/octet-stream`, never the
/// media type the uploader claimed, and that fixed type is what lets one
/// exclusion cover every download.
///
/// Compressing a download is worse than pointless. A ZIP or an MP4 does not get
/// smaller, and compressing drops `Content-Length` in favour of chunked
/// encoding, so the browser loses the progress bar and the estimated time on
/// exactly the transfers being watched. Range responses are already exempt,
/// since tower-http skips anything carrying `Content-Range`.
#[must_use]
pub fn compression_predicate() -> impl Predicate {
    DefaultPredicate::new()
        .and(NotForContentType::const_new("application/octet-stream"))
        .and(NotForContentType::const_new("application/zip"))
        .and(NotForContentType::const_new("application/gzip"))
        .and(NotForContentType::const_new("video/"))
        .and(NotForContentType::const_new("audio/"))
        .and(NotForContentType::const_new("font/"))
}

/// Correlation ids are ULIDs, not UUIDv4. Equivalent collision resistance for
/// this purpose, but lexicographically sortable, so grepping a log file
/// yields chronological order for free.
#[derive(Debug, Clone, Copy)]
struct UlidRequestId;

impl MakeRequestId for UlidRequestId {
    fn make_request_id<B>(&mut self, _request: &Request<B>) -> Option<RequestId> {
        HeaderValue::from_str(&Ulid::generate().to_string())
            .ok()
            .map(RequestId::new)
    }
}

fn make_span<B>(request: &Request<B>) -> tracing::Span {
    let request_id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-");
    tracing::info_span!(
        "http",
        method = %request.method(),
        path = %request.uri().path(),
        request_id = %request_id,
    )
}

fn cors(config: &Config) -> CorsLayer {
    let origins = &config.server.cors_allowed_origins;
    if origins.is_empty() {
        // Same-origin only. In the single-binary deployment the SPA is served by
        // this very process, so no cross-origin grant is needed and the safest
        // default is to issue no CORS headers at all.
        return CorsLayer::new();
    }

    let wildcard = origins.iter().any(|origin| origin == "*");
    let allow_origin = if wildcard {
        AllowOrigin::any()
    } else {
        AllowOrigin::list(
            origins
                .iter()
                .filter_map(|origin| HeaderValue::from_str(origin).ok()),
        )
    };

    CorsLayer::new()
        .allow_origin(allow_origin)
        // Without this a browser will not send the session cookie to another
        // origin, so a front-end on its own dev server logs in and finds itself
        // logged out on the next request. Named origins only: the Fetch
        // standard refuses credentials alongside `*`, and a browser drops the
        // whole response instead of saying why.
        .allow_credentials(!wildcard)
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::ACCEPT,
            HeaderName::from_static(REQUEST_ID_HEADER),
        ])
        .expose_headers([HeaderName::from_static(REQUEST_ID_HEADER)])
        .max_age(Duration::from_hours(1))
}
