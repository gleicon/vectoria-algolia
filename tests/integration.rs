use std::{collections::HashMap, sync::Arc};
use async_trait::async_trait;
use axum::{body::Body, http::{Request, StatusCode}};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tokio::sync::RwLock;
use tower::ServiceExt;
use vectoria_core::{embedding::EmbeddingProvider, SearchEngineBuilder};
use vectoria_algolia::{AppState, Registry, build_router};

// ── Stub embedding — no model download, hash-based vectors ───────────────────

struct StubEmbed(usize);

#[async_trait]
impl EmbeddingProvider for StubEmbed {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let mut v = vec![0.0f32; self.0];
        for (i, b) in text.bytes().enumerate() {
            v[i % self.0] += b as f32;
        }
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-8);
        Ok(v.into_iter().map(|x| x / norm).collect())
    }
    async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        let mut out = Vec::with_capacity(texts.len());
        for t in texts { out.push(self.embed(t).await?); }
        Ok(out)
    }
    fn dims(&self) -> usize { self.0 }
    fn model_id(&self) -> &str { "stub" }
}

// ── Test helpers ──────────────────────────────────────────────────────────────

async fn make_app() -> axum::Router {
    let embedding = Arc::new(StubEmbed(384));
    let engine = SearchEngineBuilder::new()
        .embedding(embedding)
        .build()
        .await
        .unwrap();
    let engine = Arc::new(engine);
    let mut map = HashMap::new();
    map.insert("products".to_string(), engine);
    let registry: Registry = Arc::new(RwLock::new(map));
    build_router(AppState { registry })
}

async fn json_body(resp: axum::response::Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn put_req(path: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

fn post_req(path: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

// ── Indexing tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_put_object_returns_201_like() {
    let app = make_app().await;
    let resp = app
        .oneshot(put_req(
            "/1/indexes/products/objects/p1",
            json!({"title": "Nike Air Max", "brand": "Nike", "price": 120}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["objectID"], "p1");
}

#[tokio::test]
async fn test_put_object_unknown_index_returns_404() {
    let app = make_app().await;
    let resp = app
        .oneshot(put_req(
            "/1/indexes/nonexistent/objects/p1",
            json!({"title": "test"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_batch_indexes_multiple_objects() {
    let app = make_app().await;
    let resp = app
        .oneshot(post_req(
            "/1/indexes/products/batch",
            json!({
                "requests": [
                    {"action": "addObject", "body": {"objectID": "b1", "title": "Running Shoe", "brand": "Adidas", "category": "Footwear", "price": 150}},
                    {"action": "addObject", "body": {"objectID": "b2", "title": "Yoga Mat", "brand": "Lululemon", "category": "Sports", "price": 80}},
                    {"action": "addObject", "body": {"objectID": "b3", "title": "Headphones", "brand": "Sony", "category": "Electronics", "price": 300}}
                ]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    let ids = body["objectIDs"].as_array().unwrap();
    assert_eq!(ids.len(), 3);
    assert!(ids.contains(&json!("b1")));
}

// ── Search tests ──────────────────────────────────────────────────────────────

async fn seeded_app() -> axum::Router {
    let app = make_app().await;
    // Seed products via batch
    let _ = app
        .clone()
        .oneshot(post_req(
            "/1/indexes/products/batch",
            json!({
                "requests": [
                    {"action": "addObject", "body": {"objectID": "shoe1", "title": "Nike Running Shoe Lightweight", "brand": "Nike", "category": "Footwear", "price": 120, "in_stock": true}},
                    {"action": "addObject", "body": {"objectID": "shoe2", "title": "Adidas Ultraboost Running", "brand": "Adidas", "category": "Footwear", "price": 180, "in_stock": true}},
                    {"action": "addObject", "body": {"objectID": "mat1", "title": "Yoga Mat Premium Non-Slip", "brand": "Lululemon", "category": "Sports", "price": 78, "in_stock": true}},
                    {"action": "addObject", "body": {"objectID": "hdp1", "title": "Sony WH-1000XM5 Headphones Wireless", "brand": "Sony", "category": "Electronics", "price": 350, "in_stock": false}}
                ]
            }),
        ))
        .await
        .unwrap();
    app
}

#[tokio::test]
async fn test_query_returns_hits() {
    let app = seeded_app().await;
    let resp = app
        .oneshot(post_req(
            "/1/indexes/products/query",
            json!({"query": "running shoe", "hitsPerPage": 10}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert!(body["nbHits"].as_u64().unwrap_or(0) > 0);
    let hits = body["hits"].as_array().unwrap();
    assert!(!hits.is_empty());
    // Hits must have objectID
    assert!(hits[0].get("objectID").is_some());
}

#[tokio::test]
async fn test_query_unknown_index_returns_404() {
    let app = make_app().await;
    let resp = app
        .oneshot(post_req(
            "/1/indexes/noindex/query",
            json!({"query": "shoes"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_query_pagination_fields() {
    let app = seeded_app().await;
    let resp = app
        .oneshot(post_req(
            "/1/indexes/products/query",
            json!({"query": "", "hitsPerPage": 2, "page": 0}),
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    assert!(body["page"].is_number());
    assert!(body["nbPages"].is_number());
    assert!(body["hitsPerPage"].is_number());
    assert_eq!(body["hitsPerPage"].as_u64().unwrap(), 2);
}

#[tokio::test]
async fn test_query_facets_returned() {
    let app = seeded_app().await;
    let resp = app
        .oneshot(post_req(
            "/1/indexes/products/query",
            json!({"query": "", "hitsPerPage": 10, "facets": ["category", "brand"]}),
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    let facets = &body["facets"];
    assert!(facets.is_object(), "facets must be an object");
    assert!(facets["category"].is_object(), "facets.category must have per-value counts");
    assert!(facets["brand"].is_object(), "facets.brand must have per-value counts");
}

#[tokio::test]
async fn test_query_filter_by_category() {
    let app = seeded_app().await;
    let resp = app
        .oneshot(post_req(
            "/1/indexes/products/query",
            json!({"query": "", "hitsPerPage": 10, "filters": "category:Electronics"}),
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    let hits = body["hits"].as_array().unwrap();
    for hit in hits {
        assert_eq!(hit["category"].as_str().unwrap(), "Electronics");
    }
}

#[tokio::test]
async fn test_query_filter_price_range() {
    let app = seeded_app().await;
    let resp = app
        .oneshot(post_req(
            "/1/indexes/products/query",
            json!({"query": "", "hitsPerPage": 10, "filters": "price >= 100 AND price <= 200"}),
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    let hits = body["hits"].as_array().unwrap();
    for hit in hits {
        let price = hit["price"].as_f64().unwrap();
        assert!(price >= 100.0 && price <= 200.0);
    }
}

// ── Highlight tests ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_highlight_result_present_on_hits() {
    let app = seeded_app().await;
    let resp = app
        .oneshot(post_req(
            "/1/indexes/products/query",
            json!({"query": "running", "hitsPerPage": 5}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    let hits = body["hits"].as_array().unwrap();
    assert!(!hits.is_empty(), "need at least one hit to verify highlight");
    let hit = &hits[0];
    let hr = hit.get("_highlightResult").expect("_highlightResult must be present");
    // title must have a highlight entry
    let title_hl = hr.get("title").expect("_highlightResult.title must exist");
    assert!(title_hl.get("value").is_some(), "highlight.value must exist");
    assert!(title_hl.get("matchLevel").is_some(), "highlight.matchLevel must exist");
    assert!(title_hl.get("matchedWords").is_some(), "highlight.matchedWords must exist");
}

#[tokio::test]
async fn test_highlight_tags_wrap_matched_text() {
    let app = seeded_app().await;
    let resp = app
        .oneshot(post_req(
            "/1/indexes/products/query",
            json!({"query": "running", "hitsPerPage": 10}),
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    let hits = body["hits"].as_array().unwrap();
    // Find a hit whose title contains "Running" — should have AIS tags in its highlight
    let tagged = hits.iter().find(|h| {
        h["_highlightResult"]["title"]["value"]
            .as_str()
            .map(|v| v.contains("<ais-highlight-0000000000>"))
            .unwrap_or(false)
    });
    assert!(tagged.is_some(), "at least one hit title should contain AIS highlight tags");
}

#[tokio::test]
async fn test_highlight_empty_query_none_match_level() {
    let app = seeded_app().await;
    let resp = app
        .oneshot(post_req(
            "/1/indexes/products/query",
            json!({"query": "", "hitsPerPage": 5}),
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    let hits = body["hits"].as_array().unwrap();
    if let Some(hit) = hits.first() {
        let level = hit["_highlightResult"]["title"]["matchLevel"].as_str().unwrap_or("none");
        assert_eq!(level, "none", "empty query → matchLevel should be none");
    }
}

// ── facetFilters tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_facet_filters_restrict_results() {
    let app = seeded_app().await;
    let resp = app
        .oneshot(post_req(
            "/1/indexes/products/query",
            json!({"query": "", "hitsPerPage": 10, "facetFilters": [["category:Electronics"]]}),
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    let hits = body["hits"].as_array().unwrap();
    assert!(!hits.is_empty(), "should return hits for Electronics facetFilter");
    for hit in hits {
        assert_eq!(
            hit["category"].as_str().unwrap_or(""),
            "Electronics",
            "facetFilters must restrict to Electronics"
        );
    }
}

// ── Multi-search tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_multi_query_returns_results_array() {
    let app = seeded_app().await;
    let resp = app
        .oneshot(post_req(
            "/1/indexes/_/queries",
            json!({
                "requests": [
                    {"indexName": "products", "query": "shoe", "hitsPerPage": 5},
                    {"indexName": "products", "query": "yoga", "hitsPerPage": 5}
                ]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 2, "two requests → two results");
    assert!(results[0].get("hits").is_some());
    assert!(results[1].get("hits").is_some());
}

#[tokio::test]
async fn test_disjunctive_facet_pattern() {
    // InstantSearch disjunctive faceting sends N+1 queries:
    //   request[0]: main query with all active filters (returns hits)
    //   request[1]: same query WITHOUT the one facet being refined (returns independent counts)
    // Verify that the two results are independent — filtered has fewer hits.
    let app = seeded_app().await;
    let resp = app
        .oneshot(post_req(
            "/1/indexes/_/queries",
            json!({
                "requests": [
                    // main: filtered to Electronics
                    {
                        "indexName": "products",
                        "query": "",
                        "hitsPerPage": 10,
                        "facets": ["category"],
                        "facetFilters": [["category:Electronics"]]
                    },
                    // disjunctive: no category filter — returns all, for sidebar counts
                    {
                        "indexName": "products",
                        "query": "",
                        "hitsPerPage": 10,
                        "facets": ["category"]
                    }
                ]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);

    let filtered_nb  = results[0]["nbHits"].as_u64().unwrap_or(0);
    let unfiltered_nb = results[1]["nbHits"].as_u64().unwrap_or(0);
    assert!(
        filtered_nb <= unfiltered_nb,
        "filtered result ({filtered_nb}) must have ≤ hits than unfiltered ({unfiltered_nb})"
    );

    // The unfiltered result should include facet counts for all categories
    let facets = &results[1]["facets"];
    assert!(
        facets.is_object() && facets["category"].is_object(),
        "disjunctive sub-query must return category facet counts"
    );
}

#[tokio::test]
async fn test_multi_query_unknown_index_returns_404() {
    let app = make_app().await;
    let resp = app
        .oneshot(post_req(
            "/1/indexes/_/queries",
            json!({"requests": [{"indexName": "doesnotexist", "query": "x"}]}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ── numericFilters / URL-encoded params ──────────────────────────────────────

#[tokio::test]
async fn test_numeric_filters_price_range() {
    let app = seeded_app().await;
    let resp = app
        .oneshot(post_req(
            "/1/indexes/products/query",
            json!({"query": "", "hitsPerPage": 10, "numericFilters": ["price >= 100", "price <= 200"]}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    let hits = body["hits"].as_array().unwrap();
    assert!(!hits.is_empty(), "numericFilters should return matching hits");
    for hit in hits {
        let price = hit["price"].as_f64().unwrap();
        assert!(price >= 100.0 && price <= 200.0);
    }
}

#[tokio::test]
async fn test_multi_search_url_encoded_params() {
    let app = seeded_app().await;
    let resp = app
        .oneshot(post_req(
            "/1/indexes/_/queries",
            json!({
                "requests": [
                    {"indexName": "products", "params": "query=running&hitsPerPage=5"}
                ]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert!(!results[0]["hits"].as_array().unwrap().is_empty());
    assert_eq!(results[0]["hitsPerPage"].as_u64().unwrap(), 5);
}
