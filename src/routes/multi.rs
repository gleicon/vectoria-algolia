use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};
use uuid::Uuid;
use crate::{AppState, translate::{MultiSearchBody, MultiSearchResponse, to_algolia_response}};

/// POST /1/indexes/*/queries
pub async fn multi_query(
    State(state): State<AppState>,
    Json(body): Json<MultiSearchBody>,
) -> impl IntoResponse {
    let mut results = Vec::with_capacity(body.requests.len());

    for item in body.requests {
        let (index, query) = item.resolve();
        let engine = {
            let reg = state.registry.read().await;
            reg.get(&index).cloned()
        };
        let Some(engine) = engine else {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"message": format!("index '{}' not found", index)})),
            ).into_response();
        };

        let req = query.to_search_request();
        match engine.search(req).await {
            Ok(resp) => {
                let query_id = Uuid::new_v4().to_string();
                results.push(to_algolia_response(resp, &query, &index, query_id));
            }
            Err(e) => {
                tracing::error!("multi-search failed on index '{index}': {e:#}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"message": e.to_string()})),
                ).into_response();
            }
        }
    }

    Json(MultiSearchResponse { results }).into_response()
}
