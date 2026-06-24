use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextAdapterReport {
    pub adapter: String,
    pub applied_items: usize,
    pub degradation: Vec<String>,
}

pub mod claude;
pub mod codex;
pub mod gemini;
pub mod opencode;
