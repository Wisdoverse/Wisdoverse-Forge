use serde_json::Value;
use std::io::Write;

/// Kubectl-style jsonpath evaluator translated to jq then executed via jaq.
/// Replaces `cli/internal/output/jsonpath.go:ApplyJSONPath`.
pub fn apply(data: &Value, expr: &str, w: &mut dyn Write) -> anyhow::Result<()> {
    let jq = jsonpath_to_jq(expr)?;
    let has_range = expr.contains("range");

    let mut buf = Vec::<u8>::new();
    super::jq::apply(data, &jq, &mut buf)?;
    let s = String::from_utf8(buf).unwrap_or_default();

    if has_range {
        w.write_all(s.as_bytes())?;
        return Ok(());
    }
    // Non-range: space-separate multiple results, drop trailing newlines.
    let parts: Vec<&str> = s.lines().filter(|l| !l.is_empty()).collect();
    let joined = parts.join(" ");
    w.write_all(joined.as_bytes())?;
    Ok(())
}

fn jsonpath_to_jq(expr: &str) -> anyhow::Result<String> {
    let expr = expr.trim();
    if expr.starts_with("{range") {
        return translate_range(expr);
    }
    let inner = strip_braces(expr);
    translate_path(inner)
}

fn strip_braces(s: &str) -> &str {
    let s = s.trim();
    if s.starts_with('{') && s.ends_with('}') { s[1..s.len() - 1].trim() } else { s }
}

fn translate_path(path: &str) -> anyhow::Result<String> {
    let path = path.trim();
    if path.is_empty() {
        return Ok(".".into());
    }
    let mut r = path.replace("[*]", "[]");
    if !r.starts_with('.') {
        r = format!(".{r}");
    }
    Ok(r)
}

fn translate_range(expr: &str) -> anyhow::Result<String> {
    // {range <path>}<body>{end}
    let rest = expr
        .strip_prefix("{range")
        .and_then(|s| s.find('}').map(|i| (&s[..i], &s[i + 1..])))
        .ok_or_else(|| anyhow::anyhow!("invalid range expression: {expr}"))?;
    let iter_path = rest.0.trim();
    let body_with_end = rest.1;
    let body =
        body_with_end.strip_suffix("{end}").ok_or_else(|| anyhow::anyhow!("invalid range expression: {expr}"))?;

    let jq_iter = translate_path(iter_path)?;

    // Scan body for {...} segments.
    let mut parts: Vec<String> = Vec::new();
    let mut cursor = 0usize;
    let bytes = body.as_bytes();
    while cursor < bytes.len() {
        if bytes[cursor] != b'{' {
            cursor += 1;
            continue;
        }
        let start = cursor + 1;
        let end = match body[start..].find('}') {
            Some(i) => start + i,
            None => return Err(anyhow::anyhow!("invalid range body: {body}")),
        };
        let inner = body[start..end].trim();
        if inner.starts_with('"') {
            parts.push(inner.to_string());
        } else {
            let field_jq = translate_path(inner)?;
            parts.push(format!("({field_jq} | tostring)"));
        }
        cursor = end + 1;
    }

    if parts.is_empty() {
        return Err(anyhow::anyhow!("empty range body: {expr}"));
    }

    Ok(format!("{} | {}", jq_iter, parts.join(" + ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn simple_field() {
        let mut buf = Vec::new();
        apply(&json!({"id":"a"}), "{.id}", &mut buf).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "a");
    }

    #[test]
    fn wildcard_iteration() {
        let mut buf = Vec::new();
        apply(&json!({"data":[{"id":"a"},{"id":"b"}]}), "{.data[*].id}", &mut buf).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "a b");
    }

    #[test]
    fn range_expression() {
        let mut buf = Vec::new();
        apply(
            &json!({"data":[{"id":"a","status":"idle"},{"id":"b","status":"working"}]}),
            r#"{range .data[*]}{.id}{"\t"}{.status}{"\n"}{end}"#,
            &mut buf,
        )
        .unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("a\tidle"));
        assert!(s.contains("b\tworking"));
    }
}
