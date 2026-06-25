use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use crate::{AppState, ingest::object_to_product};

// ── Single object ─────────────────────────────────────────────────────────────

/// PUT /1/indexes/{index}/objects/{object_id}
/// Also handles POST /1/indexes/{index}/objects (objectID in body, path segment ignored as "_new")
pub async fn put_object(
    Path((index, object_id)): Path<(String, String)>,
    State(state): State<AppState>,
    Json(mut body): Json<Map<String, Value>>,
) -> impl IntoResponse {
    // objectID from path wins over body
    body.insert("objectID".into(), Value::String(object_id));

    let engine = {
        let reg = state.registry.read().await;
        reg.get(&index).cloned()
    };
    let Some(engine) = engine else {
        return (StatusCode::NOT_FOUND, Json(Value::String(format!("index '{index}' not found")))).into_response();
    };

    let Some(product) = object_to_product(body) else {
        return (StatusCode::BAD_REQUEST, Json(Value::String("missing objectID".into()))).into_response();
    };

    let object_id = product.id.clone();
    match engine.index(product).await {
        Ok(_) => Json(serde_json::json!({"objectID": object_id, "taskID": 1})).into_response(),
        Err(e) => {
            tracing::error!("index failed: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(Value::String("internal error".into()))).into_response()
        }
    }
}

// ── Batch ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BatchBody {
    pub requests: Vec<BatchRequest>,
}

#[derive(Debug, Deserialize)]
pub struct BatchRequest {
    pub action: String,
    pub body: Map<String, Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResponse {
    pub task_id: u64,
    pub object_i_ds: Vec<String>,
}

const MAX_BATCH_SIZE: usize = 5_000;

/// POST /1/indexes/{index}/batch
pub async fn batch(
    Path(index): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<BatchBody>,
) -> impl IntoResponse {
    if body.requests.len() > MAX_BATCH_SIZE {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "message": format!("batch too large: max {MAX_BATCH_SIZE} requests")
        }))).into_response();
    }

    let engine = {
        let reg = state.registry.read().await;
        reg.get(&index).cloned()
    };
    let Some(engine) = engine else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"message": format!("index '{index}' not found")}))).into_response();
    };

    let mut object_ids = Vec::with_capacity(body.requests.len());

    for req in body.requests {
        match req.action.as_str() {
            "addObject" | "updateObject" => {
                let Some(product) = object_to_product(req.body) else { continue };
                let id = product.id.clone();
                if let Err(e) = engine.index(product).await {
                    tracing::error!("batch index {id} failed: {e:#}");
                    return (StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"message": "internal error"}))).into_response();
                }
                object_ids.push(id);
            }
            "deleteObject" => {
                if let Some(id) = req.body.get("objectID").and_then(|v| v.as_str()) {
                    if let Err(e) = engine.delete(id).await {
                        tracing::warn!("batch delete {id} failed: {e:#}");
                    }
                    object_ids.push(id.to_string());
                }
            }
            other => tracing::warn!("unsupported batch action: {other}"),
        }
    }

    Json(serde_json::json!({"taskID": 1, "objectIDs": object_ids})).into_response()
}
