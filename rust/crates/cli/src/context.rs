use crate::client::Client;
use std::sync::Arc;

/// Shared runtime state built in the PreRun stage and passed to every
/// subcommand handler.
pub struct CliContext {
    pub client: Arc<Client>,
    pub format: String,
    pub jq: String,
    pub cancel: tokio_util::sync::CancellationToken,
}
