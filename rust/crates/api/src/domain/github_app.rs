//! Pure GitHub REST/GraphQL request-body builders and typed error contracts for
//! the self-fix GitHub App client.
//!
//! These live in `domain` so the `services::github_app` HTTP adapter holds no
//! `json!` payload construction and no `ErrorKind` policy of its own (the DDD
//! boundary the `route_ddd_boundary_test` enforces): the service composes HTTP
//! requests from these builders and maps every failure through these helpers.
//!
//! Every error helper is attacker-independent — it carries only static endpoint
//! shapes and numeric statuses, never a token, header, request body, or response
//! body — so a failure can be surfaced to clients without leaking credentials.

use agentforge_core::{AppError, AppResult, ErrorKind};
use serde_json::{Value, json};

/// One discovered repository base. Keep its name and revision together so the
/// rebuild and draft PR cannot silently target different branches.
#[derive(Debug, Clone)]
pub struct RepositoryBase {
    pub branch: String,
    pub sha: String,
}

/// Validate Git's branch-name rules before using metadata in a URL or command.
/// This also rejects empty/unborn defaults rather than falling back to `main`.
pub(crate) fn valid_default_branch(branch: &str) -> bool {
    !branch.is_empty()
        && !branch.starts_with('-')
        && !branch.ends_with('.')
        && !branch.contains("..")
        && !branch.contains("@{")
        && !branch.chars().any(|c| c.is_control() || " ~^:?*[\\".contains(c))
        && branch.split('/').all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock"))
}

pub(crate) fn repository_base(branch: String, sha: String) -> AppResult<RepositoryBase> {
    if !valid_default_branch(&branch) || sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(request_failed("resolve repository default branch"));
    }
    Ok(RepositoryBase { branch, sha })
}

pub(crate) fn pr_base_changed() -> AppError {
    ErrorKind::Conflict(
        "self-fix: existing pull request targets a different base branch; review it on GitHub before retrying".into(),
    )
    .into()
}

/// Body for `POST /repos/{repo}/pulls` — open a DRAFT pull request.
#[allow(dead_code)]
pub(crate) fn create_pull_request_body(title: &str, body: &str, head_branch: &str, base: &str) -> Value {
    json!({ "title": title, "body": body, "head": head_branch, "base": base, "draft": true })
}

/// GraphQL mutation body that flips a draft PR to ready-for-review.
#[allow(dead_code)]
pub(crate) fn mark_ready_mutation_body(node_id: &str) -> Value {
    json!({
        "query": "mutation($id:ID!){markPullRequestReadyForReview(input:{pullRequestId:$id}){pullRequest{isDraft}}}",
        "variables": { "id": node_id },
    })
}

/// Body for `PUT /repos/{repo}/pulls/{n}/merge` — squash-merge at an expected head.
#[allow(dead_code)]
pub(crate) fn merge_squash_body(expected_head: &str) -> Value {
    json!({ "sha": expected_head, "merge_method": "squash" })
}

/// Body for `POST /repos/{repo}/issues/{n}/comments` — post a PR comment.
#[allow(dead_code)]
pub(crate) fn comment_body(body: &str) -> Value {
    json!({ "body": body })
}

/// Signing the app JWT failed. The underlying error can carry key-material
/// context, so it is intentionally dropped — only this static message surfaces.
#[allow(dead_code)]
pub(crate) fn sign_jwt_failed() -> AppError {
    ErrorKind::Unavailable("github: failed to sign app JWT".into()).into()
}

/// The PR is not mergeable (GitHub `405`) — e.g. still a draft or blocked by
/// branch protection.
#[allow(dead_code)]
pub(crate) fn not_mergeable() -> AppError {
    ErrorKind::Conflict("pull request not mergeable".into()).into()
}

/// A transport failure reaching GitHub, labelled only by the static endpoint shape.
#[allow(dead_code)]
pub(crate) fn request_failed(endpoint_label: &str) -> AppError {
    ErrorKind::Unavailable(format!("github: request failed: {endpoint_label}")).into()
}

/// A non-2xx GitHub response, labelled by numeric status + static endpoint shape.
#[allow(dead_code)]
pub(crate) fn status_failed(status_code: u16, endpoint_label: &str) -> AppError {
    ErrorKind::Unavailable(format!("github {status_code}: {endpoint_label}")).into()
}
