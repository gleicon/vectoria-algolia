mod filter_parser;
mod routes;
mod translate;

use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use axum::{
    Router,
    routing::post,
};
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use vectoria_core::{SearchEngine, SearchEngineBuilder};

/// Shared map of index name → SearchEngine.
pub type Registry = Arc<RwLock<HashMap<String, Arc<SearchEngine>>>>;

#[derive(Clone)]
pub struct AppState {
    pub registry: Registry,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let port = std::env::var("PORT").unwrap_or_else(|_| "8108".into());
    // Default index name — matches the Algolia index name your frontend uses.
    let default_index = std::env::var("VECTORIA_INDEX").unwrap_or_else(|_| "products".into());

    let engine = SearchEngineBuilder::new().build().await?;
    let engine = Arc::new(engine);

    let mut map = HashMap::new();
    map.insert(default_index.clone(), engine);
    let registry: Registry = Arc::new(RwLock::new(map));

    let state = AppState { registry };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        // Single-index query — primary InstantSearch endpoint.
        .route("/1/indexes/{index}/query", post(routes::search::query))
        // Multi-search — used by InstantSearch when multiple widgets share a search.
        .route("/1/indexes/*/queries", post(routes::multi::multi_query))
        .layer(cors)
        .with_state(state);

    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    tracing::info!("vectoria-algolia listening on http://{addr}");
    tracing::info!("default index: {default_index}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
