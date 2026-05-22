# Aggregate Catalog

This catalog enumerates every DDD aggregate the API crate (`rust/crates/api`)
exposes today. Each row points at the repository module, the service module(s)
that orchestrate it, the domain module(s) that own its projections, and the
routes that serve it. See [ADR 0005](../adr/0005-aggregate-grouping.md) for
the grouping rule and [ddd-contract.md](ddd-contract.md) for the layer rules.

## Multi-Table Aggregates

| Aggregate         | Root tables                                                                        | Repository module                 | Services                                                                                                                                            | Domain modules                                                                                                                | Routes                                                                                                           |
| ----------------- | ---------------------------------------------------------------------------------- | --------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| Agent             | `agents`, `agent_events`, `agent_messages`                                         | `repositories/agent/`             | `services/agent`, `services/agent_workspace`, `services/agent_commands`, `services/mcp_agent`                                                       | `domain/agent`, `domain/agent_workspace`                                                                                      | `routes/agents`, `routes/containers`                                                                             |
| Context candidate | `context_candidates`, `context_feedback`, `context_approvals`                      | `repositories/context_candidate/` | `services/context`, `services/context_governance`, `services/context_preview`, `services/context_envelope`, `services/context_resolver`             | `domain/context`, `domain/context_governance`, `domain/context_envelope`, `domain/context_preview`, `domain/context_resolver` | `routes/context`, `routes/governance_audit`                                                                      |
| Credential        | `cli_credentials`, `api_keys`, `git_credentials`, `ssh_keys`                       | `repositories/credential/`        | `services/api_key`, `services/cli_credential`, `services/git_credential`, `services/ssh_key`, `services/credential_writer`, `services/llm_provider` | `domain/credential`, `domain/configuration`                                                                                   | `routes/api_keys`, `routes/cli_credentials`, `routes/git_credentials`, `routes/ssh_keys`, `routes/llm_providers` |
| Identity          | `organizations`, `teams`, `groups`                                                 | `repositories/identity/`          | `services/organization`, `services/team`, `services/group`, `services/auth`                                                                         | `domain/auth`, `domain/group`                                                                                                 | `routes/auth`, `routes/organizations`, `routes/teams`, `routes/groups`                                           |
| Orchestration     | `task_runs`, `task_contexts`, `context_links`, `run_context_injections`            | `repositories/orchestration/`     | `services/orchestration`, `services/task_context`, `services/turn`, `services/evidence_projection`                                                  | `domain/orchestration`, `domain/task_context`, `domain/turn`, `domain/evidence_projection`                                    | `routes/orchestration`, `routes/turns`, `routes/events`                                                          |
| Resource          | `resource_members`, `resource_permissions`, `resource_profiles`, legacy navigation | `repositories/resource/`          | `services/resource_member`, `services/resource_permission`, `services/resource_profile`, `services/legacy_navigation`                               | `domain/resource`, `domain/navigation`                                                                                        | `routes/resource_members`, `routes/resource_profiles`, `routes/legacy_navigation`                                |
| Skill             | `skills`, `skill_versions`                                                         | `repositories/skill/`             | `services/skill`                                                                                                                                    | `domain/skill`                                                                                                                | `routes/skills`                                                                                                  |
| User              | `users`, `user_llm_configs`                                                        | `repositories/user/`              | `services/user`, `services/auth`                                                                                                                    | `domain/user`, `domain/auth`                                                                                                  | `routes/auth`, `routes/users`                                                                                    |

## Single-Table Aggregates (Flat Modules)

| Aggregate          | Root table                    | Repository module                    | Service module                                   | Domain module                            | Routes                    |
| ------------------ | ----------------------------- | ------------------------------------ | ------------------------------------------------ | ---------------------------------------- | ------------------------- |
| Admin              | various read-only views       | `repositories/admin.rs`              | `services/admin`                                 | `domain/admin`                           | `routes/admin`            |
| Analytics          | `usage_analytics_*`           | `repositories/analytics.rs`          | `services/analytics`, `services/usage_analytics` | `domain/usage_analytics`                 | `routes/analytics`        |
| Attachment         | `attachments`                 | `repositories/attachment.rs`         | `services/attachment`                            | `domain/attachment`                      | `routes/attachments`      |
| Audit              | `audit_events`                | `repositories/audit.rs`              | `services/audit`                                 | `domain/observability`                   | `routes/audit`            |
| Billing            | `billing_*`                   | `repositories/billing.rs`            | `services/billing/`                              | `domain/billing`                         | `routes/billing`          |
| Context preview    | `context_previews`            | `repositories/context_preview.rs`    | `services/context_preview`                       | `domain/context_preview`                 | `routes/context`          |
| Dev environment    | `dev_environments`            | `repositories/dev_environment.rs`    | `services/dev_environment`                       | `domain/dev_environment`                 | `routes/dev_environments` |
| Favorite           | `favorites`                   | `repositories/favorite.rs`           | `services/favorite`                              | shared in `domain/resource`              | `routes/favorites`        |
| Feature flag       | `feature_flags`               | `repositories/feature_flag.rs`       | `services/feature_flag`                          | (no projection)                          | `routes/feature_flags`    |
| Governance audit   | `governance_audit_*`          | `repositories/governance_audit.rs`   | `services/context_governance`                    | `domain/context_governance`              | `routes/governance_audit` |
| License            | `licenses`                    | `repositories/license.rs`            | `services/license`                               | `domain/license`                         | `routes/licenses`         |
| Memory             | `memories`                    | `repositories/memory.rs`             | `services/memory`                                | `domain/memory`                          | `routes/memory`           |
| Plugin             | `plugins`                     | `repositories/plugin.rs`             | `services/plugin`                                | (no projection)                          | `routes/plugins`          |
| Project            | `projects`                    | `repositories/project.rs`            | `services/project`                               | (uses `domain/resource`)                 | `routes/projects`         |
| Prompt             | `prompts`, `prompt_library_*` | `repositories/prompt.rs`             | `services/prompt`, `services/prompt_library`     | `domain/prompt`, `domain/prompt_library` | `routes/prompts`          |
| Quota              | `quota_usage`                 | `repositories/quota.rs`              | `services/quota`                                 | (no projection)                          | `routes/quota`            |
| Runtime capability | `runtime_capabilities`        | `repositories/runtime_capability.rs` | `services/runtime_capability_registry`           | `domain/runtime_capability`              | (consumed internally)     |
| Setting            | `settings`                    | `repositories/setting.rs`            | `services/setting`                               | `domain/configuration`                   | `routes/settings`         |
| Tile               | `tiles`                       | `repositories/tile.rs`               | `services/tile`                                  | (no projection)                          | `routes/tiles`            |
| Voice              | `voice_providers`             | `repositories/voice.rs`              | `services/voice`                                 | `domain/voice`                           | `routes/voice`            |
| Workspace          | `workspaces`                  | `repositories/workspace.rs`          | `services/workspace`                             | (uses `domain/resource`)                 | `routes/workspaces`       |

## Cross-Aggregate Services

A small number of services span aggregate boundaries by composing multiple
repositories. These are the only places where one aggregate can authorize or
load another:

| Service                  | Composes                                                                                                                       | Why                                                                                                                                                           |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `services/auth`          | `identity::OrganizationRepository`, `identity::TeamRepository`, `workspace::WorkspaceRepository`, `project::ProjectRepository` | Authorizing a session context switch into a new org/workspace/team/project requires checking membership across four aggregates before issuing a new JWT pair. |
| `services/orchestration` | `orchestration::*`, plus reads from `agent::*` and `context_candidate::*`                                                      | A task run can reference an agent run and a context candidate; the orchestrator service coordinates their evidence.                                           |

When a new cross-aggregate flow appears, prefer a new service module rather
than reaching into another aggregate's internals from inside one of its own
services.

## Drift

If this catalog disagrees with the code, the code wins. Update this file in
the same PR that adds or moves an aggregate.
