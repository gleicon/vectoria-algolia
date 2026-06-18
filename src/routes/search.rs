use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use uuid::Uuid;
use crate::{AppState, translate::{AlgoliaQuery, to_algolia_response}};

/// POST /1/indexes/{index}/query
pub async fn query(
    Path(index): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<AlgoliaQuery>,
) -> impl IntoResponse {
    let engine = {
        let reg = state.registry.read().await;
        reg.get(&index).cloned()
    };
    let Some(engine) = engine else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"message": "index not found"}))).into_response();
    };

    let req = body.to_search_request();
    match engine.search(req).await {
        Ok(resp) => {
            let query_id = Uuid::new_v4().to_string();
            let algolia = to_algolia_response(resp, &body, &index, query_id);
            Json(algolia).into_response()
        }
        Err(e) => {
            tracing::error!("search failed: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"message": e.to_string()}))).into_response()
        }
    }
}
