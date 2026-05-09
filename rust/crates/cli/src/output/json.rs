use super::Pagination;
use serde::Serialize;
use serde_json::Value;
use std::io::Write;

#[derive(Serialize)]
struct Envelope<'a> {
    ok: bool,
    data: &'a Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pagination: Option<&'a Pagination>,
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    ok: bool,
    error: &'a str,
    message: &'a str,
}

pub fn write_envelope(w: &mut dyn Write, data: &Value, pag: Option<&Pagination>) -> anyhow::Result<()> {
    let env = Envelope { ok: true, data, pagination: pag };
    let bytes = serde_json::to_vec_pretty(&env)?;
    w.write_all(&bytes)?;
    w.write_all(b"\n")?;
    Ok(())
}

pub fn write_error(w: &mut dyn Write, code: &str, message: &str) -> anyhow::Result<()> {
    let env = ErrorEnvelope { ok: false, error: code, message };
    let bytes = serde_json::to_vec_pretty(&env)?;
    w.write_all(&bytes)?;
    w.write_all(b"\n")?;
    Ok(())
}
