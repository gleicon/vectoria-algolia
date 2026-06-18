use serde_json::Value;
use std::collections::HashMap;

/// Parse an Algolia filter string into a Vectoria filters map.
///
/// Supported syntax:
///   brand:Nike
///   in_stock:true
///   price > 100  |  price >= 100  |  price < 200  |  price <= 200
///   term1 AND term2 AND term3
///
/// OR and NOT are not supported — ignored silently.
/// Vectoria numeric range keys: price_min / price_max.
pub fn parse(filter_str: &str) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    for term in filter_str.split(" AND ") {
        parse_term(term.trim(), &mut out);
    }
    out
}

fn parse_term(term: &str, out: &mut HashMap<String, Value>) {
    // Numeric: price >= 100
    if let Some(rest) = term.strip_prefix("price ") {
        parse_numeric(rest.trim(), out);
        return;
    }
    // attribute:value
    if let Some(colon) = term.find(':') {
        let attr = term[..colon].trim().to_string();
        let raw = term[colon + 1..].trim().trim_matches('"');
        let val: Value = match raw {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            s => s
                .parse::<f64>()
                .map(|n| Value::Number(serde_json::Number::from_f64(n).unwrap()))
                .unwrap_or_else(|_| Value::String(s.to_string())),
        };
        out.insert(attr, val);
    }
}

fn parse_numeric(expr: &str, out: &mut HashMap<String, Value>) {
    // >=, >, <=, <
    let (op, num_str) = if let Some(s) = expr.strip_prefix(">=") {
        (">=", s.trim())
    } else if let Some(s) = expr.strip_prefix('>') {
        (">", s.trim())
    } else if let Some(s) = expr.strip_prefix("<=") {
        ("<=", s.trim())
    } else if let Some(s) = expr.strip_prefix('<') {
        ("<", s.trim())
    } else {
        return;
    };
    let Ok(n) = num_str.parse::<f64>() else { return };
    match op {
        ">=" => { out.insert("price_min".into(), Value::Number(serde_json::Number::from_f64(n).unwrap())); }
        ">"  => { out.insert("price_min".into(), Value::Number(serde_json::Number::from_f64(n + 1.0).unwrap())); }
        "<=" => { out.insert("price_max".into(), Value::Number(serde_json::Number::from_f64(n).unwrap())); }
        "<"  => { out.insert("price_max".into(), Value::Number(serde_json::Number::from_f64(n - 1.0).unwrap())); }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attribute_string() {
        let f = parse("brand:Nike");
        assert_eq!(f["brand"], Value::String("Nike".into()));
    }

    #[test]
    fn test_attribute_bool() {
        let f = parse("in_stock:true");
        assert_eq!(f["in_stock"], Value::Bool(true));
    }

    #[test]
    fn test_price_gte() {
        let f = parse("price >= 100");
        assert_eq!(f["price_min"].as_f64(), Some(100.0));
    }

    #[test]
    fn test_price_gt() {
        let f = parse("price > 100");
        assert_eq!(f["price_min"].as_f64(), Some(101.0));
    }

    #[test]
    fn test_price_lte() {
        let f = parse("price <= 200");
        assert_eq!(f["price_max"].as_f64(), Some(200.0));
    }

    #[test]
    fn test_combined_and() {
        let f = parse("brand:Adidas AND price >= 50 AND in_stock:true");
        assert_eq!(f["brand"], Value::String("Adidas".into()));
        assert_eq!(f["price_min"].as_f64(), Some(50.0));
        assert_eq!(f["in_stock"], Value::Bool(true));
    }

    #[test]
    fn test_empty() {
        let f = parse("");
        assert!(f.is_empty());
    }
}
