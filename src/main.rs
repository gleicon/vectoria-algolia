use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::sync::RwLock;
use tower_http::services::ServeDir;
use vectoria_core::SearchEngineBuilder;
use vectoria_algolia::{AppState, Registry, build_router};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let port = std::env::var("PORT").unwrap_or_else(|_| "8108".into());
    let default_index = std::env::var("VECTORIA_INDEX").unwrap_or_else(|_| "products".into());

    let engine = SearchEngineBuilder::new().build().await?;
    let engine = Arc::new(engine);

    let mut map = HashMap::new();
    map.insert(default_index.clone(), engine);
    let registry: Registry = Arc::new(RwLock::new(map));

    let state = AppState { registry };
    let mut app = build_router(state);

    let static_dir = std::env::var("STATIC_DIR").ok().or_else(|| {
        // Local dev convenience: serve demo/dist if it exists and STATIC_DIR isn't set.
        let candidate = "./demo/dist";
        std::path::Path::new(candidate)
            .join("index.html")
            .exists()
            .then(|| candidate.to_string())
    });
    if let Some(dir) = static_dir {
        tracing::info!("serving static files from {dir}");
        app = app.fallback_service(ServeDir::new(dir));
    }

    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    tracing::info!("vectoria-algolia listening on http://{addr}");
    tracing::info!("default index: {default_index}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
