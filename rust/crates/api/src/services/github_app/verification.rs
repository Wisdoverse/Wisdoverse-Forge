//! Bounded, read-only observations for reports; the merge gate is independent.

use agentforge_core::AppResult;
use serde::Deserialize;

use super::GithubAppClient;
use crate::domain::maintenance_delivery::{GithubVerification, ObservedCheck, delivery_unavailable};

#[derive(Deserialize)]
struct CheckPage {
    total_count: usize,
    check_runs: Vec<CheckRun>,
}
#[derive(Deserialize)]
struct CheckRun {
    name: String,
    head_sha: String,
    status: String,
    conclusion: Option<String>,
}
#[derive(Deserialize)]
struct CommitStatus {
    context: String,
    state: String,
}

impl GithubAppClient {
    pub(crate) async fn verification_checks(&self, number: i32, revision: &str) -> AppResult<GithubVerification> {
        let pr = self.maintenance_pull_request(number).await?;
        let mut result = GithubVerification {
            status: if pr.head_sha == revision { "observed" } else { "head_changed" }.into(),
            checked_at: Some(chrono::Utc::now().to_rfc3339()),
            revision: Some(pr.head_sha.clone()),
            complete: pr.head_sha == revision,
            checks: vec![],
        };
        if pr.head_sha != revision {
            return Ok(result);
        }
        let root = format!("{}/repos/{}/commits/{revision}", Self::api_base(), self.cfg.repo);
        for page in 1..=5 {
            let response = self
                .authed(reqwest::Method::GET, format!("{root}/check-runs?filter=latest&per_page=100&page={page}"))
                .await?
                .send()
                .await
                .map_err(|_| delivery_unavailable())?;
            if !response.status().is_success() {
                return Err(delivery_unavailable());
            }
            let body: CheckPage = response.json().await.map_err(|_| delivery_unavailable())?;
            if body.check_runs.len() > 100 || body.check_runs.iter().any(|c| c.head_sha != revision) {
                return Err(delivery_unavailable());
            }
            let count = body.check_runs.len();
            result.checks.extend(body.check_runs.into_iter().map(|c| ObservedCheck {
                kind: "check_run".into(),
                name: c.name.chars().take(256).collect(),
                state: c.status.chars().take(64).collect(),
                conclusion: c.conclusion.map(|s| s.chars().take(64).collect()),
            }));
            if count < 100 || body.total_count <= page * 100 {
                break;
            }
            if page == 5 {
                result.complete = false;
            }
        }
        let mut contexts = std::collections::HashSet::new();
        for page in 1..=5 {
            let response = self
                .authed(reqwest::Method::GET, format!("{root}/statuses?per_page=100&page={page}"))
                .await?
                .send()
                .await
                .map_err(|_| delivery_unavailable())?;
            if !response.status().is_success() {
                return Err(delivery_unavailable());
            }
            let body: Vec<CommitStatus> = response.json().await.map_err(|_| delivery_unavailable())?;
            if body.len() > 100 {
                return Err(delivery_unavailable());
            }
            let count = body.len();
            for status in body {
                if contexts.insert(status.context.clone()) {
                    result.checks.push(ObservedCheck {
                        kind: "commit_status".into(),
                        name: status.context.chars().take(256).collect(),
                        state: status.state.chars().take(64).collect(),
                        conclusion: None,
                    });
                }
            }
            if count < 100 {
                break;
            }
            if page == 5 {
                result.complete = false;
            }
        }
        // Refresh the head after collecting checks so moving PRs fail closed.
        let fresh = self.maintenance_pull_request(number).await?;
        if fresh.head_sha != revision {
            result.status = "head_changed".into();
            result.revision = Some(fresh.head_sha);
            result.checks.clear();
            result.complete = false;
        }
        result.checked_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(result)
    }
}
