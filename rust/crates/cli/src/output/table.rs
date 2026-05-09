use super::Column;
use super::util::{to_slice_of_maps, to_string};
use serde_json::Value;
use std::io::Write;
use tabwriter::TabWriter;

const RESET: &str = "\x1b[0m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const CYAN: &str = "\x1b[36m";
const GRAY: &str = "\x1b[90m";

fn status_color(status: &str) -> &'static str {
    match status.to_lowercase().as_str() {
        "idle" => GREEN,
        "working" => YELLOW,
        "waiting" | "attention" => CYAN,
        "offline" => RED,
        _ => GRAY,
    }
}

/// Writes data as a tab-aligned table. Matches `cli/internal/output/table.go:FormatTable`.
pub fn write(w: &mut dyn Write, cols: &[Column], data: &Value, use_color: bool) {
    let mut tw = TabWriter::new(Vec::<u8>::new()).padding(2);

    let headers: Vec<&str> = cols.iter().map(|c| c.header).collect();
    let _ = writeln!(tw, "{}", headers.join("\t"));

    for item in to_slice_of_maps(data) {
        let cells: Vec<String> = cols
            .iter()
            .map(|c| {
                let v = item.get(c.field).cloned().unwrap_or(Value::Null);
                let s = to_string(&v);
                if use_color && c.field == "status" { format!("{}{}{}", status_color(&s), s, RESET) } else { s }
            })
            .collect();
        let _ = writeln!(tw, "{}", cells.join("\t"));
    }

    let _ = tw.flush();
    let inner = tw.into_inner().unwrap_or_default();
    let _ = w.write_all(&inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_headers_and_cells() {
        let cols = vec![Column { header: "ID", field: "id" }, Column { header: "STATUS", field: "status" }];
        let data = json!([{"id":"a","status":"idle"}]);
        let mut buf = Vec::new();
        write(&mut buf, &cols, &data, false);
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("ID"));
        assert!(s.contains("STATUS"));
        assert!(s.contains("idle"));
        assert!(!s.contains("\x1b["));
    }

    #[test]
    fn colors_status_when_enabled() {
        let cols = vec![Column { header: "STATUS", field: "status" }];
        let data = json!([{"status":"working"}]);
        let mut buf = Vec::new();
        write(&mut buf, &cols, &data, true);
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\x1b[33m"));
        assert!(s.contains("\x1b[0m"));
    }
}
