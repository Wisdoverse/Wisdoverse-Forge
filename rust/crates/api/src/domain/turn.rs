//! Turn read-model pagination and cursor policies.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

use agentforge_core::{AppResult, ErrorKind};

const DEFAULT_TURN_LIMIT: i64 = 50;
const MAX_TURN_LIMIT: i64 = 100;

/// Validated page size for turn read-model pagination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TurnListPage {
    limit: usize,
}

impl TurnListPage {
    pub(crate) const DEFAULT_LIMIT: i64 = DEFAULT_TURN_LIMIT;

    pub(crate) fn new(limit: i64) -> Self {
        Self { limit: limit.clamp(1, MAX_TURN_LIMIT) as usize }
    }

    pub(crate) fn start_index(self, eligible_count: usize) -> usize {
        eligible_count.saturating_sub(self.limit)
    }

    pub(crate) fn has_more(self, eligible_count: usize) -> bool {
        self.start_index(eligible_count) > 0
    }
}

/// Stable cursor for fetching turns that come before the current page.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct TurnCursor {
    started_at: i64,
    id: String,
}

impl TurnCursor {
    pub(crate) fn new(started_at: i64, id: impl Into<String>) -> Self {
        Self { started_at, id: id.into() }
    }

    pub(crate) fn is_turn_before(&self, started_at: i64, id: &str) -> bool {
        started_at < self.started_at || (started_at == self.started_at && id < self.id.as_str())
    }

    pub(crate) fn encode(&self) -> AppResult<String> {
        let bytes = serde_json::to_vec(self)
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("encode turn cursor: {err}")))?;
        Ok(URL_SAFE_NO_PAD.encode(bytes))
    }

    pub(crate) fn decode(raw: &str) -> AppResult<Self> {
        let bytes =
            URL_SAFE_NO_PAD.decode(raw).map_err(|_| ErrorKind::Validation("invalid turn cursor".to_string()))?;
        serde_json::from_slice::<Self>(&bytes)
            .map_err(|_| ErrorKind::Validation("invalid turn cursor".to_string()).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_list_page_clamps_limit() {
        assert_eq!(TurnListPage::new(0).start_index(10), 9);
        assert_eq!(TurnListPage::new(50).start_index(60), 10);
        assert_eq!(TurnListPage::new(500).start_index(200), 100);
    }

    #[test]
    fn turn_list_page_computes_tail_window() {
        let page = TurnListPage::new(3);

        assert_eq!(page.start_index(10), 7);
        assert!(page.has_more(10));
        assert_eq!(page.start_index(2), 0);
        assert!(!page.has_more(2));
    }

    #[test]
    fn turn_cursor_round_trips() {
        let raw = TurnCursor::new(123, "turn-1").encode().unwrap();
        let decoded = TurnCursor::decode(&raw).unwrap();

        assert_eq!(decoded, TurnCursor::new(123, "turn-1"));
    }

    #[test]
    fn turn_cursor_rejects_invalid_payloads() {
        assert!(TurnCursor::decode("not-base64").is_err());
        assert!(TurnCursor::decode(&URL_SAFE_NO_PAD.encode(b"{\"started_at\":1}")).is_err());
    }

    #[test]
    fn turn_cursor_compares_timestamp_then_id() {
        let cursor = TurnCursor::new(100, "turn-b");

        assert!(cursor.is_turn_before(99, "turn-z"));
        assert!(cursor.is_turn_before(100, "turn-a"));
        assert!(!cursor.is_turn_before(100, "turn-b"));
        assert!(!cursor.is_turn_before(101, "turn-a"));
    }
}
