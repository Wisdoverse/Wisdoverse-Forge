//! Deliberate maintenance intake and read-side lineage. No I/O or database rows.

use agentforge_core::{AppError, AppResult, ErrorKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::orchestration::TaskTitle;
use super::self_fix::SelfFixRepositorySetup;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MaintenanceRequestInput {
    pub group_id: Uuid,
    pub title: String,
    pub brief: String,
    pub source: MaintenanceSourceInput,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum MaintenanceSourceInput {
    Request { reference: String },
    PullRequest { number: i32 },
}

pub(crate) struct PreparedMaintenanceRequest {
    pub group_id: Uuid,
    pub title: String,
    pub brief: String,
    pub source: MaintenanceSourceIdentity,
}

pub(crate) struct MaintenanceSourceIdentity {
    pub kind: &'static str,
    pub reference: String,
    pub pr_number: Option<i32>,
}

impl MaintenanceRequestInput {
    pub(crate) fn prepare(self) -> AppResult<PreparedMaintenanceRequest> {
        let title = self.title.trim().to_owned();
        TaskTitle::validate(&title)?;
        let brief = self.brief.trim().to_owned();
        if self.group_id.is_nil() || brief.is_empty() || brief.len() > 16_000 {
            return Err(invalid_request());
        }
        let source = match self.source {
            MaintenanceSourceInput::Request { reference } => {
                let reference = reference.trim().to_ascii_lowercase();
                if reference.len() > 128
                    || !reference.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
                    || !reference.bytes().all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
                {
                    return Err(invalid_request());
                }
                MaintenanceSourceIdentity { kind: "request", reference, pr_number: None }
            }
            MaintenanceSourceInput::PullRequest { number } if number > 0 => MaintenanceSourceIdentity {
                kind: "pull_request",
                reference: number.to_string(),
                pr_number: Some(number),
            },
            MaintenanceSourceInput::PullRequest { .. } => return Err(invalid_request()),
        };
        Ok(PreparedMaintenanceRequest { group_id: self.group_id, title, brief, source })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaintenanceSubmission {
    pub request_id: Uuid,
    pub task_id: Uuid,
    pub reused: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaintenancePullRequest {
    pub number: i32,
    pub url: String,
    pub state: String,
    pub head_sha: String,
    pub base_branch: String,
}

/// A narrow wire adapter: never imports PR bodies, tokens or provider URLs.
#[derive(Deserialize)]
pub(crate) struct GithubMaintenancePullRequest {
    number: i32,
    state: String,
    merged: bool,
    head: GithubHead,
    base: GithubBase,
}

#[derive(Deserialize)]
struct GithubHead {
    sha: String,
}

#[derive(Deserialize)]
struct GithubBase {
    #[serde(rename = "ref")]
    branch: String,
    repo: GithubRepository,
}

#[derive(Deserialize)]
struct GithubRepository {
    full_name: String,
}

impl GithubMaintenancePullRequest {
    pub(crate) fn snapshot(self, repository: &str, expected_number: i32) -> AppResult<MaintenancePullRequest> {
        if self.number != expected_number
            || !self.base.repo.full_name.eq_ignore_ascii_case(repository)
            || !matches!(self.state.as_str(), "open" | "closed")
            || (self.merged && self.state != "closed")
            || self.base.branch.is_empty()
            || !matches!(self.head.sha.len(), 40 | 64)
            || !self.head.sha.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(source_unavailable());
        }
        Ok(MaintenancePullRequest {
            number: self.number,
            url: pull_request_url(repository, self.number),
            state: if self.merged { "merged".into() } else { self.state },
            head_sha: self.head.sha.to_ascii_lowercase(),
            base_branch: self.base.branch,
        })
    }
}

pub(crate) fn ensure_repairable(source: &MaintenancePullRequest, default_branch: &str) -> AppResult<()> {
    if source.state != "open" {
        return Err(ErrorKind::ValidationWithCode {
            code: "errors.maintenance.source_closed",
            message: "This pull request is closed or merged. Choose an open pull request, or open its existing maintenance task.".into(),
        }.into());
    }
    if source.base_branch != default_branch {
        return Err(ErrorKind::ValidationWithCode {
            code: "errors.maintenance.source_base",
            message: "This pull request targets a different branch. This maintenance path supports the approved repository's default branch; use the ordinary task workflow for other branches.".into(),
        }.into());
    }
    Ok(())
}

pub(crate) fn maintenance_task_params(
    request: &PreparedMaintenanceRequest,
    setup: &SelfFixRepositorySetup,
    source: Option<&MaintenancePullRequest>,
) -> Value {
    json!({
        "task": request.title,
        "message": request.brief,
        "maintenanceSource": {
            "repository": setup.repository.to_ascii_lowercase(),
            "defaultBranch": setup.default_branch,
            "startingSha": setup.base_sha,
            "kind": request.source.kind,
            "reference": request.source.reference,
            "url": source.map(|pr| &pr.url),
            "submittedHeadSha": source.map(|pr| &pr.head_sha),
        },
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaintenanceSource {
    pub kind: String,
    pub reference: String,
    pub url: Option<String>,
    pub submitted_head_sha: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaintenanceExecution {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub state: String,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaintenanceObservation {
    /// observed | unavailable | not_applicable | not_created
    pub status: &'static str,
    pub checked_at: Option<String>,
    pub snapshot: Option<MaintenancePullRequest>,
    pub head_changed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaintenanceTrace {
    pub request_id: Uuid,
    pub task_id: Uuid,
    pub task_state: String,
    pub attempt: i32,
    pub repository: String,
    pub default_branch: String,
    pub starting_sha: String,
    pub created_at: String,
    pub source: MaintenanceSource,
    pub executions: Vec<MaintenanceExecution>,
    pub rebuild_base_sha: Option<String>,
    pub recorded_pr_head_sha: Option<String>,
    pub produced_pr_url: Option<String>,
    pub source_state: MaintenanceObservation,
    pub produced_state: MaintenanceObservation,
}

pub(crate) fn pull_request_url(repository: &str, number: i32) -> String {
    format!("https://github.com/{repository}/pull/{number}")
}

pub(crate) fn invalid_request() -> AppError {
    ErrorKind::ValidationWithCode {
        code: "errors.maintenance.invalid_request",
        message: "Choose a place for new tasks, provide a title and brief, and use a request reference or a positive pull request number.".into(),
    }.into()
}

pub(crate) fn destination_unavailable() -> AppError {
    ErrorKind::ValidationWithCode {
        code: "errors.maintenance.destination_unavailable",
        message:
            "Choose an active place for new tasks in this organization. Open project settings to create one if needed."
                .into(),
    }
    .into()
}

pub(crate) fn source_unavailable() -> AppError {
    ErrorKind::ValidationWithCode {
        code: "errors.maintenance.source_unavailable",
        message: "The pull request could not be verified in the approved repository. Check its number and the GitHub App access, then retry.".into(),
    }.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(source: Value) -> Value {
        json!({"groupId": Uuid::new_v4(), "title": " Repair dependency ", "brief": " Explain and verify ", "source": source})
    }

    #[test]
    fn request_reference_is_canonical_without_rewriting_brief() {
        let request: MaintenanceRequestInput =
            serde_json::from_value(input(json!({"kind":"request", "reference":" DEP-2026.10 "}))).unwrap();
        let prepared = request.prepare().unwrap();
        assert_eq!(prepared.source.reference, "dep-2026.10");
        assert_eq!(prepared.title, "Repair dependency");
        assert_eq!(prepared.brief, "Explain and verify");
    }

    #[test]
    fn invalid_references_and_nonpositive_prs_are_rejected() {
        for reference in ["", "../escape", "request with spaces", "https://example.com", "请求", &"a".repeat(129)] {
            let request: MaintenanceRequestInput =
                serde_json::from_value(input(json!({"kind":"request", "reference":reference}))).unwrap();
            assert!(request.prepare().is_err(), "{reference}");
        }
        for number in [0, -1] {
            let request: MaintenanceRequestInput =
                serde_json::from_value(input(json!({"kind":"pull_request", "number":number}))).unwrap();
            assert!(request.prepare().is_err());
        }
    }

    #[test]
    fn callers_cannot_override_repository_or_task_privileges() {
        let mut value = input(json!({"kind":"pull_request", "number":42}));
        value["selfFix"] = json!(false);
        assert!(serde_json::from_value::<MaintenanceRequestInput>(value).is_err());
        let value = input(json!({"kind":"pull_request", "number":42, "url":"https://example.com"}));
        assert!(serde_json::from_value::<MaintenanceRequestInput>(value).is_err());
    }

    #[test]
    fn missing_destination_and_empty_or_oversized_brief_are_rejected() {
        for brief in [" ".to_owned(), "a".repeat(16_001)] {
            let mut value = input(json!({"kind":"request", "reference":"repair"}));
            value["brief"] = json!(brief);
            let request: MaintenanceRequestInput = serde_json::from_value(value).unwrap();
            assert!(request.prepare().is_err());
        }
        let mut value = input(json!({"kind":"request", "reference":"repair"}));
        value["groupId"] = json!(Uuid::nil());
        assert!(serde_json::from_value::<MaintenanceRequestInput>(value).unwrap().prepare().is_err());
    }

    fn provider_pr() -> Value {
        json!({"number":42, "state":"open", "merged":false, "head":{"sha":"A".repeat(40)},
            "base":{"ref":"main", "repo":{"full_name":"Acme/Widgets"}}, "body":"untrusted content", "html_url":"https://example.com"})
    }

    #[test]
    fn snapshot_uses_approved_repository_and_normalizes_revision() {
        let pr = serde_json::from_value::<GithubMaintenancePullRequest>(provider_pr())
            .unwrap()
            .snapshot("acme/widgets", 42)
            .unwrap();
        assert_eq!(pr.url, "https://github.com/acme/widgets/pull/42");
        assert_eq!(pr.head_sha, "a".repeat(40));
        assert!(ensure_repairable(&pr, "main").is_ok());
        assert!(ensure_repairable(&pr, "develop").is_err());
    }

    #[test]
    fn foreign_or_malformed_provider_identity_fails_closed() {
        for (field, replacement) in [("number", json!(43)), ("state", json!("unknown")), ("merged", json!(true))] {
            let mut value = provider_pr();
            value[field] = replacement;
            assert!(
                serde_json::from_value::<GithubMaintenancePullRequest>(value)
                    .unwrap()
                    .snapshot("acme/widgets", 42)
                    .is_err()
            );
        }
        let mut value = provider_pr();
        value["head"]["sha"] = json!("not-a-revision");
        assert!(
            serde_json::from_value::<GithubMaintenancePullRequest>(value)
                .unwrap()
                .snapshot("acme/widgets", 42)
                .is_err()
        );
        assert!(
            serde_json::from_value::<GithubMaintenancePullRequest>(provider_pr())
                .unwrap()
                .snapshot("acme/other", 42)
                .is_err()
        );
    }

    #[test]
    fn merged_and_closed_sources_are_observable_but_not_new_work() {
        for merged in [false, true] {
            let mut value = provider_pr();
            value["state"] = json!("closed");
            value["merged"] = json!(merged);
            let pr = serde_json::from_value::<GithubMaintenancePullRequest>(value)
                .unwrap()
                .snapshot("acme/widgets", 42)
                .unwrap();
            assert_eq!(pr.state, if merged { "merged" } else { "closed" });
            assert!(ensure_repairable(&pr, "main").is_err());
        }
    }
}
