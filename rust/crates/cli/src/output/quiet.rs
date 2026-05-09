use serde_json::Value;
use std::io::Write;

/// Writes one value of `field` per line from `data`.
pub fn write(w: &mut dyn Write, data: &Value, field: &str) {
    for m in super::util::to_slice_of_maps(data) {
        if let Some(v) = m.get(field) {
            let _ = writeln!(w, "{}", super::util::to_string(v));
        }
    }
}
