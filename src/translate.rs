use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use vectoria_core::model::{Hit, SearchMode, SearchRequest, SearchResponse};
use crate::filter_parser;

fn default_highlight_pre_tag() -> String {
    "<ais-highlight-0000000000>".to_string()
}
fn default_highlight_post_tag() -> String {
    "</ais-highlight-0000000000>".to_string()
}

/// Wrap every occurrence of any token in `text` with pre/post tags.
/// Returns (highlighted_value, matched_words, match_level).
fn highlight_text(
    text: &str,
    tokens: &[&str],
    pre: &str,
    post: &str,
) -> (String, Vec<String>, &'static str) {
    if tokens.is_empty() || text.is_empty() {
        return (text.to_string(), vec![], "none");
    }
    let lower = text.to_lowercase();
    // Collect byte-offset spans for each matched token.
    let mut spans: Vec<(usize, usize)> = vec![];
    let mut matched: Vec<String> = vec![];
    for token in tokens {
        let tl = token.to_lowercase();
        if tl.is_empty() {
            continue;
        }
        let mut start = 0usize;
        while let Some(pos) = lower[start..].find(tl.as_str()) {
            let abs = start + pos;
            spans.push((abs, abs + tl.len()));
            if !matched.contains(&tl) {
                matched.push(tl.clone());
            }
            start = abs + tl.len();
        }
    }
    if spans.is_empty() {
        return (text.to_string(), vec![], "none");
    }
    // Merge overlapping spans.
    spans.sort_unstable_by_key(|s| s.0);
    let mut merged: Vec<(usize, usize)> = vec![];
    for (s, e) in spans {
        if let Some(last) = merged.last_mut() {
            if s <= last.1 {
                last.1 = last.1.max(e);
                continue;
            }
        }
        merged.push((s, e));
    }
    // Build highlighted string.
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len() + merged.len() * (pre.len() + post.len()));
    let mut cursor = 0usize;
    for (s, e) in &merged {
        out.push_str(&text[cursor..*s]);
        out.push_str(pre);
        // Use original case from source, not lowercased.
        out.push_str(std::str::from_utf8(&bytes[*s..*e]).unwrap_or(""));
        out.push_str(post);
        cursor = *e;
    }
    out.push_str(&text[cursor..]);

    let total_chars = text.chars().count();
    let highlighted_chars: usize = merged.iter().map(|(s, e)| e - s).sum();
    let level = if highlighted_chars >= total_chars { "full" } else { "partial" };
    (out, matched, level)
}

fn build_highlight_result(
    attributes: &Map<String, Value>,
    tokens: &[&str],
    pre: &str,
    post: &str,
) -> Map<String, Value> {
    let mut result = Map::new();
    for (key, val) in attributes {
        if let Value::String(s) = val {
            let (highlighted, matched_words, level) = highlight_text(s, tokens, pre, post);
            let fully = level == "full";
            let entry = serde_json::json!({
                "value": highlighted,
                "matchLevel": level,
                "matchedWords": matched_words,
                "fullyHighlighted": fully,
            });
            result.insert(key.clone(), entry);
        }
    }
    result
}

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
    /// RefinementList sends [["attr:val"]] or [["attr:a","attr:b"]] for OR.
    /// Outer array = AND, inner array = OR across same attribute.
    pub facet_filters: Option<Value>,
    /// "hybrid" | "semantic" | "bm25" — non-standard extension.
    pub search_mode: Option<String>,
    #[serde(default = "default_highlight_pre_tag")]
    pub highlight_pre_tag: String,
    #[serde(default = "default_highlight_post_tag")]
    pub highlight_post_tag: String,
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

        // facetFilters: [["attr:val"], ["attr:a","attr:b"]]
        // Outer = AND groups; inner = OR within one attribute.
        // We take the first value per attribute (single-select common case).
        if let Some(ff) = &self.facet_filters {
            let groups: Vec<&Value> = match ff {
                Value::Array(arr) => arr.iter().collect(),
                other => vec![other],
            };
            for group in groups {
                let terms: Vec<&str> = match group {
                    Value::Array(arr) => arr.iter().filter_map(|v| v.as_str()).collect(),
                    Value::String(s) => vec![s.as_str()],
                    _ => continue,
                };
                if let Some(first) = terms.first() {
                    filters.extend(filter_parser::parse(first));
                }
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
    #[serde(rename = "_highlightResult")]
    pub highlight_result: Map<String, Value>,
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

    let tokens: Vec<&str> = req.query.split_whitespace().collect();
    let hits = resp
        .hits
        .into_iter()
        .map(|h| hit_to_algolia(h, &tokens, &req.highlight_pre_tag, &req.highlight_post_tag))
        .collect();

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

fn hit_to_algolia(h: Hit, tokens: &[&str], pre: &str, post: &str) -> AlgoliaHit {
    let attributes: Map<String, Value> = match h.metadata {
        Value::Object(o) => o,
        other => {
            let mut m = Map::new();
            m.insert("_raw".into(), other);
            m
        }
    };
    let highlight_result = build_highlight_result(&attributes, tokens, pre, post);
    AlgoliaHit { object_id: h.id, score: h.score, highlight_result, attributes }
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
