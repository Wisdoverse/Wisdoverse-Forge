use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output::{self, Column};
use serde_json::Value;
use std::io::Write;

const COLUMNS: &[Column] = &[
    Column { header: "ID", field: "id" },
    Column { header: "EMAIL", field: "email" },
    Column { header: "NAME", field: "name" },
];

pub async fn run(ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let result = ctx
        .client
        .do_request(reqwest::Method::GET, "/api/v1/users/me", None, ResponseKind::Auto)
        .await?
        .unwrap_or(Value::Null);
    output::format(stdout, &ctx.format, COLUMNS, &result, None).map_err(|e| CliError::Other(e.to_string()))
}
