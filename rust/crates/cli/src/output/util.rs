use serde_json::{Map, Value};

/// Normalises a `Value` into `Vec<Map<String, Value>>` matching Go's
/// `toSliceOfMaps`. Scalars/arrays of scalars are lost.
pub fn to_slice_of_maps(v: &Value) -> Vec<Map<String, Value>> {
    match v {
        Value::Array(arr) => arr
            .iter()
            .filter_map(|x| match x {
                Value::Object(m) => Some(m.clone()),
                _ => None,
            })
            .collect(),
        Value::Object(m) => vec![m.clone()],
        _ => Vec::new(),
    }
}

/// Go-like string conversion for arbitrary Value cells.
pub fn to_string(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(v).unwrap_or_default(),
    }
}
