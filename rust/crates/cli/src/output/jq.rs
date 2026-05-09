use serde_json::Value;
use std::io::Write;

/// Evaluates `expr` against `data` and writes each result as one line.
/// Strings are printed raw (matches jq -r default). Others as JSON.
/// Replaces `cli/internal/output/jq.go:ApplyJQ`.
pub fn apply(data: &Value, expr: &str, w: &mut dyn Write) -> anyhow::Result<()> {
    use jaq_interpret::{Ctx, FilterT, ParseCtx, RcIter, Val};

    let mut defs = ParseCtx::new(Vec::new());
    defs.insert_natives(jaq_core::core());
    defs.insert_defs(jaq_std::std());
    let (parsed, errs) = jaq_parse::parse(expr, jaq_parse::main());
    if !errs.is_empty() {
        return Err(anyhow::anyhow!("jq parse: {}", errs[0]));
    }
    let parsed = parsed.ok_or_else(|| anyhow::anyhow!("jq parse: empty filter"))?;
    let filter = defs.compile(parsed);
    if !defs.errs.is_empty() {
        return Err(anyhow::anyhow!("jq compile: {}", defs.errs[0].0));
    }
    let inputs = RcIter::new(std::iter::empty());
    let input = Val::from(data.clone());
    for out in filter.run((Ctx::new([], &inputs), input)) {
        let v = out.map_err(|e| anyhow::anyhow!("jq eval: {e}"))?;
        let j: Value = v.into();
        match &j {
            Value::Null => writeln!(w, "null")?,
            Value::String(s) => writeln!(w, "{s}")?,
            _ => writeln!(w, "{}", serde_json::to_string(&j)?)?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn selects_field() {
        let mut buf = Vec::new();
        apply(&json!({"id":"a"}), ".id", &mut buf).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "a\n");
    }

    #[test]
    fn iterates_array() {
        let mut buf = Vec::new();
        apply(&json!([{"id":"a"},{"id":"b"}]), ".[].id", &mut buf).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "a\nb\n");
    }

    #[test]
    fn object_output_as_json() {
        let mut buf = Vec::new();
        apply(&json!({"x":1}), ".", &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(s.trim(), r#"{"x":1}"#);
    }
}
