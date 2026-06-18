pub mod filter_parser;
pub mod ingest;
pub mod routes;
pub mod translate;

use std::{collections::HashMap, sync::Arc};
use axum::{
    Router,
    extract::Request,
    http::{header, HeaderValue},
    middleware::{self, Next},
    response::Response,
    routing::{post, put},
};
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use vectoria_core::SearchEngine;

pub type Registry = Arc<RwLock<HashMap<String, Arc<SearchEngine>>>>;

#[derive(Clone)]
pub struct AppState {
    pub registry: Registry,
}

/// algoliasearch v5 sends POST requests without Content-Type or with text/plain
/// to avoid CORS preflight. Force application/json so axum's Json extractor accepts them.
async fn inject_json_content_type(mut req: Request, next: Next) -> Response {
    if req.method() == axum::http::Method::POST {
        let is_json = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.contains("application/json"))
            .unwrap_or(false);
        if !is_json {
            req.headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
        }
    }
    next.run(req).await
}

pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/1/indexes/{index}/query", post(routes::search::query))
        // Algolia multi-search: POST /1/indexes/*/queries
        // The path segment before /queries is conventionally "*" but ignored —
        // each request in the body carries its own indexName.
        .route("/1/indexes/{_}/queries", post(routes::multi::multi_query))
        .route("/1/indexes/{index}/objects/{object_id}", put(routes::objects::put_object))
        .route("/1/indexes/{index}/batch", post(routes::objects::batch))
        .layer(middleware::from_fn(inject_json_content_type))
        .layer(cors)
        .with_state(state)
}
