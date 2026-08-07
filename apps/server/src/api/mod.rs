//! Route assembly, middleware stack and the OpenAPI document.
//!
//! The OpenAPI document is derived from the handlers rather than written by hand,
//! and `openapi.json` at the repository root is a build artefact of this module
//! (see the `tessera-openapi` binary). The frontend and the Discord bot generate
//! their clients from that file; the server emits no TypeScript.

pub mod health;

use std::time::Duration;

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
use utoipa_swagger_ui::SwaggerUi;

use crate::{assets, config::Config, state::AppState};

/// Tag grouping infrastructure endpoints in the generated document.
pub const SYSTEM_TAG: &str = "system";

/// Header carrying the per-request correlation id.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Path the generated contract is served from.
pub const OPENAPI_PATH: &str = "/openapi.json";

/// Path the Swagger UI is mounted at.
pub const DOCS_PATH: &str = "/docs";

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
    ),
    components(schemas(crate::error::ErrorBody, crate::error::ErrorCode)),
)]
pub struct ApiDoc;

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
    OpenApiRouter::with_openapi(ApiDoc::openapi()).routes(routes!(health::health))
}

/// Assembles the complete application: API routes, contract, docs, embedded SPA
/// and the middleware stack.
pub fn build(state: AppState) -> Router {
    let config = state.config.clone();
    let (routes, api) = api_routes().split_for_parts();

    let mut app = Router::new().merge(routes).with_state(state);

    if config.server.enable_docs {
        // No trailing-slash normalisation anywhere in this stack: the Swagger UI
        // redirects `/docs` to `/docs/`, so a layer that rewrote `/docs/` back to
        // `/docs` would produce an infinite redirect. See the regression test in
        // tests/api.rs.
        //
        // The Swagger UI owns `/openapi.json` when it is mounted, so that the
        // browsable docs and the machine contract can never disagree.
        app = app.merge(SwaggerUi::new(DOCS_PATH).url(OPENAPI_PATH, api));
    } else {
        // Docs off still has to serve the contract: client generators and the CI
        // drift check read it from a running instance.
        app = app.route(
            OPENAPI_PATH,
            axum::routing::get(move || std::future::ready(Json(api))),
        );
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

    let allow_origin = if origins.iter().any(|origin| origin == "*") {
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
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::ACCEPT,
            HeaderName::from_static(REQUEST_ID_HEADER),
        ])
        .expose_headers([HeaderName::from_static(REQUEST_ID_HEADER)])
        .max_age(Duration::from_hours(1))
}
