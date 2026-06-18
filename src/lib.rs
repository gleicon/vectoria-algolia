pub mod filter_parser;
pub mod ingest;
pub mod routes;
pub mod translate;

use std::{collections::HashMap, sync::Arc};
use axum::{Router, routing::{post, put}};
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use vectoria_core::SearchEngine;

pub type Registry = Arc<RwLock<HashMap<String, Arc<SearchEngine>>>>;

#[derive(Clone)]
pub struct AppState {
    pub registry: Registry,
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
        .layer(cors)
        .with_state(state)
}
