//! Route assembly, middleware stack and the OpenAPI document.
//!
//! The OpenAPI document is derived from the handlers rather than written by hand,
//! and `openapi.json` at the repository root is a build artefact of this module
//! (see the `tessera-openapi` binary). The frontend and the Discord bot generate
//! their clients from that file; the server emits no TypeScript.
//!
//! # Why the contract is OpenAPI and not a Rust-to-TypeScript exporter
//!
//! Rust is already the single source of truth either way: `utoipa` derives the
//! document from the same structs the handlers use, doc comments included. The
//! choice is only about the intermediate artefact — `openapi.json`, or `.ts`
//! files written straight into the frontend tree by something like `specta`.
//!
//! Three things settle it:
//!
//! - **The consumers are not all TypeScript.** A Discord bot, a CLI and ShareX read this too. One
//!   document generates a client for any of them; a TypeScript exporter serves exactly one.
//! - **Half the public surface is not a procedure.** `/s/{code}/raw` answers `text/plain`, and
//!   `/s/{code}/download` answers `206 Partial Content` with `Range`, `If-Range` and
//!   `Content-Disposition`. Those are HTTP contracts, not return types; a type exporter has nothing
//!   to say about them, and an RPC framework cannot express them at all.
//! - **The browsable docs need it.** Scalar renders this document, which is what makes the API
//!   explorable by pasting values into a page.
//!
//! What a type exporter would buy is tighter modelling of data-carrying enums.
//! Nothing on this API is one today — every enum here is a plain string union in
//! the generated client, which is what a caller wants. If that changes, adding
//! `specta` alongside is possible: both would derive from the same structs, so
//! they cannot disagree about a type, and a CI check catches a struct that grew
//! one derive and not the other. It is not worth the second artefact before
//! there is a type that needs it.

pub mod admin;
pub mod auth;
pub mod capabilities;
pub mod extract;
pub mod gate;
pub mod health;
pub mod preview;
pub mod public;
pub mod sessions;
pub mod shares;
pub mod tokens;

use std::{sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    http::{HeaderName, HeaderValue, Method, Request, StatusCode, header},
};
use tower_http::{
    catch_panic::CatchPanicLayer,
    compression::{
        CompressionLayer,
        predicate::{DefaultPredicate, NotForContentType, Predicate},
    },
    cors::{AllowOrigin, CorsLayer},
    request_id::{MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use ulid::Ulid;
use utoipa::OpenApi;
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_scalar::{Scalar, Servable as _};

use crate::{assets, config::Config, state::AppState};

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
/// `/health` and `/docs` stay at the root: orchestrators and humans expect them
/// there, and neither is part of the contract third parties generate clients
/// from. Everything that *is* carries a version, because the frontend, a Discord
/// bot and a CLI will each generate against it independently and cannot all be
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
    ),
    modifiers(&SecurityAddon),
    components(schemas(crate::error::ErrorBody, crate::error::ErrorCode)),
)]
pub struct ApiDoc;

/// Declares the two ways a caller can prove who they are.
///
/// Written as a modifier rather than in the attribute because utoipa's derive has
/// no syntax for an `apiKey` scheme in a cookie. Both are declared so the docs UI
/// offers each: a browser gets the cookie for free after signing in, while a
/// script pastes a token into the bearer field and can exercise the same
/// endpoints from the same page.
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
/// `operation_id`, `operation_ref` and `request_body`, while OpenAPI names those
/// `operationId`, `operationRef` and `requestBody`. Anything reading the
/// document to the specification therefore sees a link with no target and skips
/// it — which costs the one statement in here about how these operations
/// connect: that the code `POST /shares` returns is what the three `{code}`
/// operations take.
///
/// Repairing the keys is the narrow fix. The alternative, dropping links
/// altogether, would keep the document valid by making it say less.
///
/// This is what both the served document and the file on disk go through, so
/// that "the contract" means one thing.
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

/// Endpoints under [`API_PREFIX`]. Nesting rewrites their paths in the generated
/// document too, so a handler declares the path it owns and not the prefix.
/// Every endpoint is mounted on every instance, including ones whose feature is
/// switched off — those answer `404` from inside the handler instead. Mounting
/// conditionally would make the served document depend on the instance, and the
/// whole point of `/capabilities` is that it does not.
fn versioned_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(capabilities::capabilities))
        .routes(routes!(auth::register))
        .routes(routes!(auth::confirm_registration))
        .routes(routes!(auth::login))
        .routes(routes!(auth::logout))
        .routes(routes!(auth::session))
        .routes(routes!(auth::change_password))
        // Mounted even where no relay is configured, like every other switched
        // off feature: the handler answers 404 and the document stays the same
        // on every instance. `/capabilities` is what tells a client whether to
        // draw the link.
        .routes(routes!(auth::request_password_reset))
        .routes(routes!(auth::confirm_password_reset))
        .routes(routes!(tokens::list, tokens::create))
        .routes(routes!(tokens::revoke))
        .routes(routes!(sessions::list, sessions::revoke_others))
        .routes(routes!(sessions::revoke))
        .merge(admin_routes())
        .routes(routes!(shares::list, shares::create))
        .routes(routes!(shares::describe, shares::delete))
        .routes(routes!(shares::unlock))
}

/// Everything under `/admin`.
///
/// Its own function because it is its own surface — an operator can switch it
/// off with `features.admin`, and the frontend for it is a separate application
/// that somebody may never deploy. Keeping the list here rather than inline also
/// keeps [`versioned_routes`] readable as one page.
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
}

/// Assembles the complete application: API routes, contract, docs, embedded SPA
/// and the middleware stack.
pub fn build(state: AppState) -> Router {
    let config = Arc::clone(&state.config);
    let (routes, api) = api_routes().split_for_parts();

    // The contract is served unconditionally. Turning the browsable docs off is
    // about not shipping a UI, not about hiding the API — client generators and
    // the CI drift check both read this, and the spec is in the repository
    // anyway.
    let contract = openapi_json();
    let mut app = Router::new()
        .merge(routes)
        // Outside the contract on purpose: these are where a link points, not
        // an API. See `public` for why that means their shape is frozen.
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
    }

    let request_id_header = HeaderName::from_static(REQUEST_ID_HEADER);

    app
        // Innermost first: `.layer` wraps, so the last call is the outermost.
        // Compression sits closest to the handler and therefore compresses the
        // final body exactly once.
        .layer(CompressionLayer::new().compress_when(compression_predicate()))
        .layer(DefaultBodyLimit::max(config.server.max_body_bytes))
        .layer(cors(&config))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            config.server.request_timeout(),
        ))
        // Outside the timeout, so a panicking handler becomes a 500 rather than a
        // dropped connection.
        .layer(CatchPanicLayer::new())
        .layer(PropagateRequestIdLayer::new(request_id_header.clone()))
        .layer(TraceLayer::new_for_http().make_span_with(make_span))
        // Outermost: stamped before anything else, so every log line and every
        // error response for this request can be correlated.
        .layer(SetRequestIdLayer::new(request_id_header, UlidRequestId))
}

/// What is worth compressing.
///
/// Public so the test suite can assert on it directly; the alternative is
/// inferring the policy from response headers, which tests the plumbing rather
/// than the decision.
///
/// The default already skips gRPC, images and server-sent events. Added here is
/// everything that arrives as opaque or already-compressed bytes, which matters
/// for one route in particular.
///
/// `GET /s/{code}/download` always sends `application/octet-stream` — not the
/// media type the uploader claimed, which is never echoed. That fixed type is
/// what makes a single exclusion cover every download, whatever was uploaded.
///
/// Compressing a download would be worse than pointless. A ZIP or an MP4 does
/// not get smaller, and compressing the response drops `Content-Length` in
/// favour of chunked encoding — so the browser loses the progress bar and the
/// estimated time remaining on exactly the transfers where a person is watching
/// them. Range responses are already exempt: tower-http skips anything carrying
/// `Content-Range`, so only the first, unranged request was ever at risk.
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

/// Correlation ids are ULIDs rather than UUIDv4: equivalent collision resistance
/// for this purpose, but lexicographically sortable, so grepping a log file
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
        // origin, so a front-end running on its own dev server can log in and
        // then find itself logged out on the next request. Named origins only:
        // the Fetch standard refuses credentials alongside `*`, and a browser
        // would drop the whole response rather than tell anyone why.
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
