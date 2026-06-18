use serde_json::{Map, Value};
use vectoria_core::model::Product;

/// Convert an Algolia-format object into a vectoria-core Product.
///
/// `objectID` → `id` (required; returns None if absent).
/// Text fields (title/name/description/brand/category/tags) → `text` for BM25 + embedding.
/// The full object is stored verbatim as `metadata`.
pub fn object_to_product(obj: Map<String, Value>) -> Option<Product> {
    let id = obj.get("objectID")?.as_str()?.to_string();
    let text = extract_text(&obj);
    let metadata = Value::Object(obj);
    let mut p = Product::new(id, metadata);
    p.text = text;
    Some(p)
}

const TEXT_FIELDS: &[&str] = &[
    "title", "name", "description", "brand", "category", "tags", "text", "content",
];

fn extract_text(obj: &Map<String, Value>) -> Option<String> {
    let parts: Vec<&str> = TEXT_FIELDS
        .iter()
        .filter_map(|f| obj.get(*f)?.as_str())
        .collect();
    if parts.is_empty() { None } else { Some(parts.join(" ")) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_object_to_product_basic() {
        let obj = json!({
            "objectID": "p1",
            "title": "Nike Air Max",
            "brand": "Nike",
            "price": 120
        });
        let p = object_to_product(obj.as_object().unwrap().clone()).unwrap();
        assert_eq!(p.id, "p1");
        assert!(p.text.as_deref().unwrap().contains("Nike Air Max"));
        assert_eq!(p.metadata["price"], json!(120));
    }

    #[test]
    fn test_object_missing_object_id_returns_none() {
        let obj = json!({"title": "no id"});
        assert!(object_to_product(obj.as_object().unwrap().clone()).is_none());
    }

    #[test]
    fn test_text_extraction_order() {
        let obj = json!({
            "objectID": "p2",
            "title": "Trail Shoe",
            "brand": "Salomon",
            "category": "Footwear",
            "description": "All terrain grip"
        });
        let p = object_to_product(obj.as_object().unwrap().clone()).unwrap();
        let text = p.text.unwrap();
        assert!(text.contains("Trail Shoe"));
        assert!(text.contains("Salomon"));
        assert!(text.contains("Footwear"));
        assert!(text.contains("All terrain grip"));
    }
}
