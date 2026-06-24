use serde::Serialize;
use serde_json::Value;
use std::io::Write;

pub mod jq;
pub mod json;
pub mod jsonpath;
pub mod quiet;
pub mod table;
pub mod util;
pub mod yaml;

#[derive(Debug, Clone)]
pub struct Column {
    pub header: &'static str,
    pub field: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Pagination {
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

/// Dispatches to the right formatter.
/// Matches `cli/internal/output/formatter.go:Format`.
pub fn format(
    w: &mut dyn Write,
    format: &str,
    cols: &[Column],
    data: &Value,
    pagination: Option<&Pagination>,
) -> anyhow::Result<()> {
    if let Some(expr) = format.strip_prefix("jsonpath=") {
        return jsonpath::apply(data, expr, w);
    }
    match format {
        "json" => json::write_envelope(w, data, pagination),
        "yaml" => yaml::write_envelope(w, data, pagination),
        "quiet" => {
            quiet::write(w, data, "id");
            Ok(())
        }
        _ => {
            let use_color = !no_color();
            table::write(w, cols, data, use_color);
            Ok(())
        }
    }
}

/// Like `format` but applies a jq expression bypassing all other formatting.
pub fn format_with_jq(
    w: &mut dyn Write,
    fmt: &str,
    cols: &[Column],
    data: &Value,
    pagination: Option<&Pagination>,
    jq_expr: &str,
) -> anyhow::Result<()> {
    if !jq_expr.is_empty() {
        return jq::apply(data, jq_expr, w);
    }
    format(w, fmt, cols, data, pagination)
}

/// Structured formats keep the envelope; table output uses `text`.
/// Matches `cli/internal/output/formatter.go:FormatAction`.
pub fn format_action(w: &mut dyn Write, fmt: &str, text: &str, data: &Value) -> anyhow::Result<()> {
    if let Some(expr) = fmt.strip_prefix("jsonpath=") {
        return jsonpath::apply(data, expr, w);
    }
    match fmt {
        "json" | "yaml" | "quiet" => format(w, fmt, &[], data, None),
        _ => {
            writeln!(w, "{text}")?;
            Ok(())
        }
    }
}

/// Matches `cli/internal/output/formatter.go:noColor`.
pub fn no_color() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return true;
    }
    if std::env::var("TERM").as_deref() == Ok("dumb") {
        return true;
    }
    !is_terminal::IsTerminal::is_terminal(&std::io::stdout())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_envelope_has_ok_data_pagination() {
        let mut buf = Vec::new();
        let data = json!([{"id":"a"},{"id":"b"}]);
        let pag = Pagination { total: 2, limit: 50, offset: 0 };
        format(&mut buf, "json", &[], &data, Some(&pag)).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\"ok\": true"));
        assert!(s.contains("\"data\""));
        assert!(s.contains("\"pagination\""));
    }

    #[test]
    fn quiet_outputs_ids() {
        let mut buf = Vec::new();
        format(&mut buf, "quiet", &[], &json!([{"id":"a"},{"id":"b"}]), None).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "a\nb\n");
    }

    #[test]
    fn action_table_mode_prints_text() {
        let mut buf = Vec::new();
        format_action(&mut buf, "table", "Agent x deleted.", &json!({"id":"x"})).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "Agent x deleted.\n");
    }
}
