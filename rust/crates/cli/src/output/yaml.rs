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

pub fn write_envelope(w: &mut dyn Write, data: &Value, pag: Option<&Pagination>) -> anyhow::Result<()> {
    let env = Envelope { ok: true, data, pagination: pag };
    serde_yaml::to_writer(&mut *w, &env)?;
    Ok(())
}
