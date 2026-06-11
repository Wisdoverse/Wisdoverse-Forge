use serde_json::Value;
use std::io::Write;

/// Evaluates `expr` against `data` and writes each result as one line.
/// Strings are printed raw (matches jq -r default). Others as JSON.
/// Replaces `cli/internal/output/jq.go:ApplyJQ`.
pub fn apply(data: &Value, expr: &str, w: &mut dyn Write) -> anyhow::Result<()> {
    use jaq_core::load::{Arena, File, Loader};
    use jaq_core::{Compiler, Ctx, RcIter};
    use jaq_json::Val;

    // jaq 2.x pipeline: load (lex+parse against the std defs) then compile
    // with the native funs. jaq-json owns the serde_json `Val` bridge.
    let program = File { code: expr, path: () };
    let loader = Loader::new(jaq_std::defs().chain(jaq_json::defs()));
    let arena = Arena::default();
    let modules =
        loader.load(&arena, program).map_err(|errs| anyhow::anyhow!("jq parse: {}", load_errors_to_string(&errs)))?;
    let filter = Compiler::default()
        .with_funs(jaq_std::funs().chain(jaq_json::funs()))
        .compile(modules)
        .map_err(|errs| anyhow::anyhow!("jq compile: {}", compile_errors_to_string(&errs)))?;

    let inputs = RcIter::new(core::iter::empty());
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

/// Render jaq load (lex/parse) errors as a single operator-readable line.
/// The structured spans reference the arena-backed source; for a one-line
/// CLI error the failing filter text plus the error kind is enough.
fn load_errors_to_string(errs: &jaq_core::load::Errors<&str, ()>) -> String {
    errs.iter().map(|(file, err)| format!("{:?} in filter `{}`", err, file.code)).collect::<Vec<_>>().join("; ")
}

fn compile_errors_to_string(errs: &jaq_core::compile::Errors<&str, ()>) -> String {
    errs.iter()
        .flat_map(|(file, errs)| errs.iter().map(move |err| format!("{:?} in filter `{}`", err, file.code)))
        .collect::<Vec<_>>()
        .join("; ")
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

    #[test]
    fn std_defs_are_available() {
        // `map/1` comes from the jq standard library, not the native core —
        // guards the defs/funs wiring after the 2.x migration.
        let mut buf = Vec::new();
        apply(&json!([1, 2]), "map(. + 1)", &mut buf).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap().trim(), "[2,3]");
    }

    #[test]
    fn parse_error_is_reported() {
        let mut buf = Vec::new();
        let err = apply(&json!({}), ".foo[", &mut buf).unwrap_err();
        assert!(err.to_string().starts_with("jq parse:"), "got: {err}");
    }
}
