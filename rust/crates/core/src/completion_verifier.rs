//! Opt-in task completion verifier (#793 / #875).
//!
//! A task that reports completion is normally taken at its word: the agent says
//! "done", the task goes `completed`. This module adds an OPT-IN contract carried
//! in the task's free-form `params` JSONB (`params.expectedResult`) and a pure
//! verifier that checks the agent's reported result against it. The result
//! consumer calls [`CompletionVerifier::verify`] at completion time; on a miss it
//! holds the task in `blocked / waiting_verification` instead of `completed` so an
//! operator sees the suspect result rather than a silent false success.
//!
//! This lives in `core` (not `api`) on purpose: the NATS result consumer in the
//! `jobs` crate must reach it, and `jobs` depends on `core`, not `api`.
//!
//! Scope (v1, deliberately minimal): a single `contains` substring check. A
//! design review proved `exitCode` is degenerate on the only verified outcome (a
//! NATS `Completed` result stores `{stdout}` with no exit-code key), so structured
//! checks are deferred rather than shipped half-defined.
//!
//! ```json
//! { "expectedResult": { "contains": "tests passed" } }
//! ```

use serde::Deserialize;
use serde_json::Value;

/// The opt-in completion contract parsed from a task's `params.expectedResult`.
///
/// Only fields this version understands are deserialized; unknown sub-keys are
/// ignored so a future field (e.g. `exitCode`, `regex`) added by a newer producer
/// does not break parsing for an older consumer.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ExpectedResult {
    /// The agent's result, serialized to a string, must contain this substring
    /// (case-sensitive). `None` (absent or null) means "no substring check".
    #[serde(default)]
    pub contains: Option<String>,
}

impl ExpectedResult {
    /// Parse the contract from a task's `params` object.
    ///
    /// Returns `None` — meaning "no verification, behave exactly as before" — when:
    /// - `params` is `None`,
    /// - `params.expectedResult` is absent,
    /// - `expectedResult` fails to deserialize into the known shape, or
    /// - the parsed contract carries nothing actionable (`contains` is null/empty).
    ///
    /// The empty-string guard matters: `{ "contains": "" }` would otherwise be a
    /// substring of every result and silently pass everything, which is worse than
    /// no contract. We treat it as "no contract".
    pub fn from_params(params: Option<&Value>) -> Option<ExpectedResult> {
        let expected_value = params?.get("expectedResult")?;
        // ponytail: tolerate a malformed `expectedResult` by skipping verification
        // rather than failing the completion — an opt-in safety net must never make
        // a well-formed completion worse.
        //
        // #793/#875 FIX 6: but a PRESENT-yet-broken contract (wrong shape, or
        // `contains` of a non-string type) is a producer mistake that silently
        // disables a contract they intended to enforce. Distinguish it from the
        // legitimate "absent" case (handled by the `?` above) and log it so the
        // broken contract is discoverable. We still fail open (return None).
        let expected: ExpectedResult = match serde_json::from_value(expected_value.clone()) {
            Ok(expected) => expected,
            Err(err) => {
                tracing::warn!(
                    expected_result = %expected_value,
                    error = %err,
                    "params.expectedResult is present but malformed; skipping completion verification (fail-open). \
                     Fix the contract shape (expected an object with an optional string `contains`)."
                );
                return None;
            }
        };

        let has_contains = expected.contains.as_deref().is_some_and(|s| !s.is_empty());
        if has_contains { Some(expected) } else { None }
    }
}

/// Pure verifier for completion contracts. Stateless; safe to call from any crate.
pub struct CompletionVerifier;

impl CompletionVerifier {
    /// Check an agent's reported `result` against the `expected` contract.
    ///
    /// `Ok(())` means the result satisfies the contract (or the contract carries
    /// no check). `Err(reason)` carries a human-readable explanation suitable for
    /// surfacing to an operator on the held task.
    ///
    /// For the `contains` check the entire `result` is serialized to a JSON string
    /// and a case-sensitive substring match is performed. For a NATS `Completed`
    /// result `{ "stdout": "…" }` this answers "did stdout actually mention X".
    pub fn verify(expected: &ExpectedResult, result: &Value) -> Result<(), String> {
        if let Some(sub) = expected.contains.as_deref() {
            // `to_string` on a `Value` is infallible (no custom Serialize that can
            // error), so this never panics.
            let serialized = serde_json::to_string(result).unwrap_or_default();
            if !serialized.contains(sub) {
                return Err(format!("result is missing the expected substring {sub:?}"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn contains_match_is_ok() {
        let expected = ExpectedResult { contains: Some("tests passed".to_string()) };
        let result = json!({ "stdout": "all 42 tests passed in 1.2s" });
        assert!(CompletionVerifier::verify(&expected, &result).is_ok());
    }

    #[test]
    fn contains_mismatch_is_err_with_reason() {
        let expected = ExpectedResult { contains: Some("tests passed".to_string()) };
        let result = json!({ "stdout": "build failed" });
        let err = CompletionVerifier::verify(&expected, &result).unwrap_err();
        assert!(err.contains("tests passed"), "reason should name the missing substring: {err}");
    }

    #[test]
    fn none_contains_always_ok() {
        let expected = ExpectedResult { contains: None };
        let result = json!({ "stdout": "anything" });
        assert!(CompletionVerifier::verify(&expected, &result).is_ok());
    }

    #[test]
    fn absent_contract_yields_none() {
        // params present but no expectedResult key.
        assert_eq!(ExpectedResult::from_params(Some(&json!({ "task": "do the thing" }))), None);
        // params entirely absent.
        assert_eq!(ExpectedResult::from_params(None), None);
    }

    #[test]
    fn null_or_empty_contains_yields_none() {
        // explicit null `contains`.
        assert_eq!(ExpectedResult::from_params(Some(&json!({ "expectedResult": { "contains": null } }))), None);
        // empty-string `contains` (would otherwise match everything).
        assert_eq!(ExpectedResult::from_params(Some(&json!({ "expectedResult": { "contains": "" } }))), None);
        // expectedResult present but with no contains at all.
        assert_eq!(ExpectedResult::from_params(Some(&json!({ "expectedResult": {} }))), None);
    }

    #[test]
    fn valid_contract_round_trips_through_from_params() {
        let parsed =
            ExpectedResult::from_params(Some(&json!({ "expectedResult": { "contains": "deployed" } }))).unwrap();
        assert_eq!(parsed, ExpectedResult { contains: Some("deployed".to_string()) });
    }

    #[test]
    fn unknown_sub_keys_are_tolerated() {
        // A future producer adds `exitCode`/`regex`; the older parser must ignore
        // them and still honor `contains`.
        let parsed = ExpectedResult::from_params(Some(&json!({
            "expectedResult": { "contains": "ok", "exitCode": 0, "regex": "^ok$" }
        })))
        .expect("contains must still parse despite unknown sub-keys");
        assert_eq!(parsed.contains.as_deref(), Some("ok"));
    }

    #[test]
    fn malformed_expected_result_is_treated_as_no_contract() {
        // `expectedResult` is not an object → skip verification rather than error.
        // (#793/#875 FIX 6: this now also logs a warn, but the contract is to keep
        // failing open with None.)
        assert_eq!(ExpectedResult::from_params(Some(&json!({ "expectedResult": "nope" }))), None);
    }

    #[test]
    fn wrong_typed_contains_is_malformed_and_yields_none() {
        // #793/#875 FIX 6: `contains` of a non-string type is a producer mistake.
        // It fails to deserialize into `Option<String>`, so it is treated as a
        // malformed (not absent) contract: fail open with None (and a logged warn).
        assert_eq!(ExpectedResult::from_params(Some(&json!({ "expectedResult": { "contains": 42 } }))), None);
        assert_eq!(ExpectedResult::from_params(Some(&json!({ "expectedResult": { "contains": ["a"] } }))), None);
    }
}
