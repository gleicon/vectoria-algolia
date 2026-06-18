use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use vectoria_core::model::{Hit, SearchMode, SearchRequest, SearchResponse};
use crate::filter_parser;

// ── Algolia request ───────────────────────────────────────────────────────────

/// Single-index query body (POST /1/indexes/{name}/query).
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AlgoliaQuery {
    #[serde(default)]
    pub query: String,
    #[serde(default = "default_hits_per_page")]
    pub hits_per_page: usize,
    #[serde(default)]
    pub page: usize,
    /// Comma-separated facet names to compute counts for (e.g. "brand,category").
    pub facets: Option<Value>,
    /// Algolia filter string: "brand:Nike AND price > 100".
    pub filters: Option<String>,
    /// Algolia numericFilters array: ["price >= 100", "price < 200"].
    pub numeric_filters: Option<Vec<String>>,
    /// "hybrid" | "semantic" | "bm25" — non-standard extension.
    pub search_mode: Option<String>,
}

fn default_hits_per_page() -> usize { 20 }

impl AlgoliaQuery {
    pub fn to_search_request(&self) -> SearchRequest {
        let limit = self.hits_per_page;
        let offset = self.page * limit;

        // Build Vectoria filters from Algolia filter string + numericFilters.
        let mut filters: HashMap<String, Value> = self
            .filters
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(filter_parser::parse)
            .unwrap_or_default();

        if let Some(nf) = &self.numeric_filters {
            for term in nf {
                filters.extend(filter_parser::parse(term));
            }
        }

        // Facet names for aggregation.
        let aggregate: Option<Vec<String>> = self.facets.as_ref().and_then(|v| match v {
            Value::Array(arr) => Some(
                arr.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect(),
            ),
            Value::String(s) if s == "*" => None,
            Value::String(s) => Some(s.split(',').map(|x| x.trim().to_string()).collect()),
            _ => None,
        });

        let mode = match self.search_mode.as_deref() {
            Some("semantic") => SearchMode::Semantic,
            Some("bm25")     => SearchMode::Bm25,
            _                => SearchMode::Hybrid,
        };

        SearchRequest {
            q: self.query.clone(),
            limit,
            offset,
            mode,
            filters: if filters.is_empty() { None } else { Some(filters) },
            aggregate,
            explain: false,
            rerank: false,
            ranking_weights: None,
        }
    }
}

// ── Algolia response ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlgoliaResponse {
    pub hits: Vec<AlgoliaHit>,
    pub nb_hits: u64,
    pub page: usize,
    pub nb_pages: usize,
    pub hits_per_page: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facets: Option<Map<String, Value>>,
    pub processing_time_ms: u64,
    pub query: String,
    pub query_id: String,
    pub index: String,
    /// Non-standard: carry Vectoria hybrid score through for debugging.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub exhaustive_nb_hits: bool,
}

#[derive(Debug, Serialize)]
pub struct AlgoliaHit {
    #[serde(rename = "objectID")]
    pub object_id: String,
    #[serde(rename = "_score")]
    pub score: f32,
    #[serde(flatten)]
    pub attributes: Map<String, Value>,
}

pub fn to_algolia_response(
    resp: SearchResponse,
    req: &AlgoliaQuery,
    index: &str,
    query_id: String,
) -> AlgoliaResponse {
    let hits_per_page = req.hits_per_page;
    let nb_pages = if hits_per_page == 0 {
        1
    } else {
        ((resp.total as usize).saturating_add(hits_per_page - 1)) / hits_per_page
    };

    let hits = resp.hits.into_iter().map(hit_to_algolia).collect();

    let facets = resp.aggregations.map(|agg| {
        agg.into_iter()
            .map(|(field, counts)| {
                let counts_obj: Map<String, Value> = counts
                    .into_iter()
                    .map(|(k, v)| (k, Value::Number(v.into())))
                    .collect();
                (field, Value::Object(counts_obj))
            })
            .collect()
    });

    AlgoliaResponse {
        hits,
        nb_hits: resp.total as u64,
        page: req.page,
        nb_pages,
        hits_per_page,
        facets,
        processing_time_ms: resp.processing_time_ms,
        query: resp.query,
        query_id,
        index: index.to_string(),
        exhaustive_nb_hits: true,
    }
}

fn hit_to_algolia(h: Hit) -> AlgoliaHit {
    // Flatten metadata object onto the hit — Algolia returns attributes at the top level.
    let attributes: Map<String, Value> = match h.metadata {
        Value::Object(o) => o,
        other => {
            let mut m = Map::new();
            m.insert("_raw".into(), other);
            m
        }
    };
    AlgoliaHit { object_id: h.id, score: h.score, attributes }
}

// ── Multi-search ──────────────────────────────────────────────────────────────

/// POST /1/indexes/*/queries body.
#[derive(Debug, Deserialize)]
pub struct MultiSearchBody {
    pub requests: Vec<MultiSearchRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiSearchRequest {
    pub index_name: String,
    /// Query params either as inline JSON fields or URL-encoded string.
    #[serde(flatten)]
    pub params_inline: Option<AlgoliaQuery>,
    /// Legacy: "query=shoes&hitsPerPage=10"
    pub params: Option<String>,
}

impl MultiSearchRequest {
    pub fn resolve(self) -> (String, AlgoliaQuery) {
        let index = self.index_name.clone();
        let query = if let Some(p) = self.params {
            serde_urlencoded::from_str::<AlgoliaQuery>(&p).unwrap_or_default()
        } else {
            self.params_inline.unwrap_or_default()
        };
        (index, query)
    }
}

#[derive(Debug, Serialize)]
pub struct MultiSearchResponse {
    pub results: Vec<AlgoliaResponse>,
}
