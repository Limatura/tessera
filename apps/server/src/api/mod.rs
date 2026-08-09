//! Route assembly, middleware stack and the OpenAPI document.
//!
//! The OpenAPI document is derived from the handlers rather than written by hand,
//! and `openapi.json` at the repository root is a build artefact of this module
//! (see the `tessera-openapi` binary). The frontend and the Discord bot generate
//! their clients from that file; the server emits no TypeScript.

pub mod auth;
pub mod capabilities;
pub mod health;
pub mod tokens;

use std::{sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    http::{HeaderName, HeaderValue, Method, Request, StatusCode, header},
};
use tower_http::{
    catch_panic::CatchPanicLayer,
    compression::CompressionLayer,
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
        .routes(routes!(auth::login))
        .routes(routes!(auth::logout))
        .routes(routes!(auth::session))
        .routes(routes!(auth::change_password))
        .routes(routes!(tokens::list, tokens::create))
        .routes(routes!(tokens::revoke))
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
    let contract = api.clone();
    let mut app = Router::new()
        .merge(routes)
        .route(
            OPENAPI_PATH,
            axum::routing::get(move || std::future::ready(Json(contract))),
        )
        .with_state(state);

    if config.features.docs {
        app = app.merge(Scalar::with_url(DOCS_PATH, api));
    }

    let request_id_header = HeaderName::from_static(REQUEST_ID_HEADER);

    app.fallback(assets::handler)
        // Innermost first: `.layer` wraps, so the last call is the outermost.
        // Compression sits closest to the handler and therefore compresses the
        // final body exactly once.
        .layer(CompressionLayer::new())
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
