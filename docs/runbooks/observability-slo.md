# Observability and SLOs

This runbook defines the service-level indicators (SLIs), service-level
objectives (SLOs), and alerting thresholds for a Wisdoverse Forge deployment.
The targets here are the defaults shipped in this repository; an operator
running Wisdoverse Forge may tighten them but should not loosen them without
recording the change next to their deployment config.

## Start Here

What Forge measures and alerts on today:

- API availability and latency, from the shared HTTP metrics.
- Orchestrator HTTP errors and latency, and the orchestration outbox lag and backlog.
- CLI auth proxy OAuth client failures and self-fix delivery failures.
- The agents-runtime endpoints (create, Host CLI enroll, restart).

The alert rules live in `ops/prometheus/alerts.yml`,
`ops/prometheus/agents-runtime.yml` and, for orchestration production
reliability, `docker/orchestration-alerts.yml`. Add all three files to
`rule_files` in your Prometheus configuration.
[Alerting Thresholds](#alerting-thresholds) lists every rule. Three rules in
`ops/prometheus/alerts.yml` need an exporter that Forge does not ship; the table
says which. Some indicators that earlier versions of this page described are
not measured yet; see [Not Implemented Indicators](#not-implemented-indicators).

To confirm that this page and the alert rules only name metrics the code emits,
run this from the repository root (requires Node.js and `npm ci`):

```bash
npm run metrics:contract
```

Success prints `[metrics-contract] ... names + labels resolve.` A failure lists
each metric that no code emits, with its file and line. CI runs the same check.

## SLIs

The Rust API and orchestrator each emit Prometheus metrics through
`metrics_exporter_prometheus`. Both expose a top-level `/metrics` scrape
endpoint — the API on its HTTP port (default `:4003`) and the orchestrator on
its own (default `:4010`) — so configure a Prometheus scrape job for each
target. Both endpoints are access-gated and are not world-readable: the API
endpoint requires a platform-admin principal, and the orchestrator endpoint
requires the internal operator token (`ORCHESTRATOR_INTERNAL_TOKEN`) — the
credential the scraper carries in its `Authorization: Bearer` header.

The two services share the HTTP metric names `http_requests_total` and
`http_request_duration_seconds`. To keep them apart in a shared Prometheus, the
orchestrator stamps every series with `service="orchestrator"`; the API emits no
`service` label. So API-only queries select `service!="orchestrator"` (the
absent label matches a negative matcher) and orchestrator-only queries select
`service="orchestrator"`. The shipped alert rules in `ops/prometheus/alerts.yml`
follow exactly this split. The relevant metric families are:

| SLI               | Metric                                                                                                                            | Definition                                                                                                                                                   | Alert rules                                                        |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------ |
| API availability  | `http_requests_total{status=~"5..",status!="501",service!="orchestrator"}`                                                        | `1 - (5xx requests / total requests)` over a rolling window, with status `501` excluded from both counts.                                                    | `API5xxRatioAtOrAboveSLO`, `APIHigh5xxRate`, `APIErrorBudgetBurn`  |
| API latency (p95) | `histogram_quantile(0.95, sum by (le) (rate(http_request_duration_seconds_bucket{path=~"/api/v1/agents.*", method="POST"}[5m])))` | p95 over the agents-runtime write paths, computed from the histogram `_bucket` series (labels `method`/`path`); excludes WebSocket upgrades and SSE streams. | `APILatencyHighP95`, `APILatencyHighP99` (all API paths, per path) |

Orchestrator success rate, workflow start latency, NATS event delivery, DB pool
saturation and WebSocket fanout backlog have no metric yet. See
[Not Implemented Indicators](#not-implemented-indicators).

A deliberate `501 Not Implemented` reply marks an endpoint that has no implementation yet.
It does not measure the availability of implemented service, so the availability SLI,
its alerts and the overview dashboard exclude `status="501"` from both the error count and the request count.

### Not Implemented Responses

The `APINotImplementedResponses` alert (severity `ticket`) fires when any API route answered `501` in the last 15 minutes.
Its `path` label names the route template, for example `/api/v1/voice/transcribe`.
Because 501s are excluded from the availability ratio, this alert is the only signal that a route answers 501.

1. Read the `path` label on the alert.
2. If the path is `/api/v1/voice/transcribe` or `/api/v1/agents/{id}/git`, it is a call to a deprecated stub endpoint.
   Find the caller from your reverse proxy access log or the API [logs](#logs) for that route.
   Move the caller off the endpoint before its removal date (see [Preview Boundaries](runtime-validation.md#preview-boundaries)).
3. For any other path, treat it as a regression of an implemented route: find the change that made the route return 501 and fix or revert it.

Success looks like the alert resolving on its own once no route has answered 501 for 15 minutes.

## SLO Targets

The defaults below assume a single-tenant self-hosted deployment serving
~10–50 operators. They are starting points, not contractual ceilings.

| Surface          | Window  | Target  |
| ---------------- | ------- | ------- |
| API availability | 30 days | > 99.5% |
| API p95 latency  | 30 days | 350 ms  |

## API Availability

Use this path to measure the global API 5xx objective. Do not treat an alert
threshold as the objective.

### Before you start

Confirm each prerequisite before you measure the objective:

- Prometheus is available and has a configured Prometheus data source in Grafana.
- Authenticated scrape jobs collect `/metrics` from the API and orchestrator.
- The scrape jobs have continuous coverage for the full 30-day window.
- The window has nonzero API request traffic.

The API scrape uses platform-admin authentication. The orchestrator scrape uses
the internal operator token. The orchestrator adds `service="orchestrator"` to
the shared HTTP metrics. API series have no `service` label.

### Measure the 30-day objective

1. In Grafana, select **Dashboards → Import → Upload JSON file**.
2. Upload `ops/grafana/dashboards/agentforge-overview.json`.
3. Select the configured Prometheus data source.
4. Set the dashboard time range to **Last 30 days**.
5. Open the **Observed API 5xx (30-day lookback)** panel.
6. Confirm that the panel uses the API scope and excludes the orchestrator.
7. Confirm continuous scrape coverage across the full window.
8. Confirm that the window contains API requests.
9. Compare the observed global API 5xx ratio with `0.005`.

The API scope includes series without a `service` label. It excludes series
with `service="orchestrator"`. The objective passes only when the observed
30-day ratio is strictly below `0.005` and all prerequisites hold. This is the
same as API availability strictly above `99.5%`.

The global panel covers all API paths regardless of the dashboard path filter.

If requests exist but no API 5xx series exists, the panel reports an observed
ratio of zero. This is normal when the API has no 5xx responses. An empty panel
or zero request traffic does not prove the objective. Sparse data can produce
an extrapolated rate. A new deployment or an incomplete window does not prove
the objective either. Retention length alone does not prove continuous scrape
coverage.

### Rule validation

Rule validation requires Docker and Node.js.
From the repository root, run:

```bash
node scripts/check-prometheus-rules.mjs
```

The command evaluates both shipped rule files and the overview queries with synthetic samples.
Successful runs report `SUCCESS`.
These results do not prove production scrape coverage or the observed objective.
For query behavior, see the [Prometheus function reference](https://prometheus.io/docs/prometheus/latest/querying/functions/).

## Error Budget Policy

When a 30-day SLO is exceeded:

1. The next operator-facing deploy is blocked until a postmortem is filed.
2. Feature merges into `main` continue, but releases pause.
3. Recovery work is prioritized until the budget is restored over the next
   rolling window.

Operators may relax this policy for their own deployment, but the postmortem
expectation stays — the repository's default is that consumed budget is
_always_ explained.

## Alerting Thresholds

Alerts should be routed to the operator's on-call channel. Every rule shipped
in `ops/prometheus/alerts.yml` is listed below. The **For** column is the rule's
pending period: the condition must stay true that long before the alert fires;
`0m` fires on the first evaluation. The agents-runtime rules are in
[Agents Runtime SLOs](#agents-runtime-slos).

| Alert rule                          | Condition                                                                 | For | Severity | Notes                                                                                                                                                                                                           |
| ----------------------------------- | ------------------------------------------------------------------------- | --- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `APIHigh5xxRate`                    | Per-path API 5xx ratio > 1% over 5 min                                    | 5m  | Page     | API availability SLI.                                                                                                                                                                                           |
| `APIErrorBudgetBurn`                | Global API 5xx ratio > 1.4% over both 1h and 5m                           | 2m  | Page     | API availability SLI.                                                                                                                                                                                           |
| `API5xxRatioAtOrAboveSLO`           | Global API 5xx ratio >= 0.5% over 30 days, with nonzero traffic           | 1h  | Warning  | API availability SLI.                                                                                                                                                                                           |
| `APINotImplementedResponses`        | Any API 501 response in the last 15 min                                   | 0m  | Ticket   | Fires per route; see [Not Implemented Responses](#not-implemented-responses).                                                                                                                                   |
| `APILatencyHighP95`                 | Per-path API p95 latency > 1 s                                            | 10m | Page     | API latency SLI.                                                                                                                                                                                                |
| `APILatencyHighP99`                 | Per-path API p99 latency > 3 s                                            | 10m | Ticket   | API latency SLI.                                                                                                                                                                                                |
| `OrchestratorHigh5xxRate`           | Per-path orchestrator 5xx ratio > 1% over 5 min                           | 5m  | Page     | Orchestrator HTTP traffic only (`service="orchestrator"`).                                                                                                                                                      |
| `OrchestratorLatencyHighP95`        | Per-path orchestrator p95 latency > 1 s                                   | 10m | Ticket   | Orchestrator HTTP traffic only.                                                                                                                                                                                 |
| `OrchestrationLagHigh`              | Oldest unpublished orchestration outbox row older than 60 s               | 5m  | Page     | See [Orchestration](orchestration.md).                                                                                                                                                                          |
| `OrchestrationOutboxBacklogHigh`    | More than 1000 unpublished outbox rows                                    | 10m | Ticket   | See [Orchestration](orchestration.md).                                                                                                                                                                          |
| `ContainerRestartLoop`              | API or orchestrator container restarted more than 3 times in 10 min       | 0m  | Page     | Needs a Docker exporter that publishes `container_restart_count` (cAdvisor and kube-state-metrics use other names). Forge does not ship one; without it this alert never fires.                                 |
| `HealthCheckFailing`                | Blackbox probe in the scrape job `staging-health` fails                   | 3m  | Page     | Needs the Prometheus blackbox exporter and a scrape job named `staging-health`. Forge ships neither; without them this alert never fires.                                                                       |
| `APIMemoryHigh`                     | API resident memory > 80% of machine memory                               | 15m | Ticket   | Needs cAdvisor for `machine_memory_bytes` and an exporter that publishes `process_resident_memory_bytes` for the job `agentforge-server`; the Forge services emit neither. Without them this alert never fires. |
| `CliAuthProxyInvalidClientFailures` | OAuth provider rejected the CLI auth proxy client at least once in 10 min | 1m  | Critical | See [credential sync](credential-sync.md#cli-auth-proxy-invalid-client).                                                                                                                                        |
| `SelfFixPrPermanentFailure`         | A self-fix PR open failed permanently in the last 15 min                  | 0m  | Warning  | See [Self-Fix Delivery](#self-fix-delivery).                                                                                                                                                                    |
| `SelfFixMergeFailed`                | A self-fix merge attempt failed in the last 15 min                        | 0m  | Warning  | See [Self-Fix Delivery](#self-fix-delivery).                                                                                                                                                                    |
| `SelfFixMergeAttemptsExhausted`     | A self-fix approve hit the merge attempt cap in the last 15 min           | 0m  | Warning  | See [Self-Fix Delivery](#self-fix-delivery).                                                                                                                                                                    |

The orchestration production rules in `docker/orchestration-alerts.yml`
(`OrchestrationAssignmentOutboxBacklog`, `OrchestrationAssignmentOutboxStalled`,
`OrchestrationExpiredWorkingLeases`, `OrchestrationStaleParticipants`,
`OrchestrationBusyParticipantWithoutWork`,
`OrchestrationWorkingTaskWithoutBusyParticipant`, `OrchestrationResultApplyLag`,
`OrchestrationResultUnauthorizedSpike`, `OrchestrationOutboxPublishErrors`) are
documented in the [Orchestration runbook](orchestration.md#alert-route-check).

Both error-budget burn windows use a 1.4% threshold. This is 2.8 times the
global 0.5% error budget.

`/health` is the API liveness probe. `/api/health` is the deep readiness probe
that asserts PostgreSQL, Redis, and NATS reachability when configured. The
liveness probe should always page; the deep probe should warn first, then page
if it fails to recover within five minutes.

## On-Call Runbooks

When an alert fires, refer to the matching runbook:

- API 5xx rate: [API Availability](observability-slo.md#api-availability)
- NATS authentication or connection problems (no NATS delivery alert exists yet): [docs/runbooks/nats-auth.md](nats-auth.md)
- Credential failure: [docs/runbooks/credential-sync.md](credential-sync.md)
- Context governance: [docs/runbooks/context-governance-audit.md](context-governance-audit.md)
- Orchestration: [docs/runbooks/orchestration.md](orchestration.md)
- Self-fix PR or merge failure: [Self-Fix Delivery](#self-fix-delivery)
- Frontend deploy: [docs/runbooks/frontend-deploy.md](frontend-deploy.md)
- Disaster recovery: [docs/guides/disaster-recovery.md](../guides/disaster-recovery.md)

## Dashboards

The repository ships a Grafana dashboard for the agents-runtime SLOs at
`ops/grafana/dashboards/agents-runtime.json` (see [Agents Runtime
SLOs](#agents-runtime-slos) below for import steps). For metrics outside that
scope, operators are expected to build their own per-deployment dashboard from
the metrics listed above. A reference dashboard skeleton has the following
panels:

- Request rate, error rate, p50/p95/p99 latency per route.
- Orchestration outbox backlog, oldest unpublished age, and publish errors.
- Memory and CPU per service container, from your container exporter.

The indicators under [Not Implemented Indicators](#not-implemented-indicators)
have no metric, so they cannot have a panel yet. The overview dashboard's memory
and container restart panels use the external metrics named in
[Alerting Thresholds](#alerting-thresholds) and stay empty without those exporters.

Dashboard URLs and credentials live in the operator's private infra repo, not
in this repository.

## Agents Runtime SLOs

Added 2026-05-28, wired by PR chore/dashboards-slo-alerts. Counters emitted by
the agents-runtime service (#447).

| Endpoint                           | p95 SLO | Success SLO | Window | Alert rules                                                       |
| ---------------------------------- | ------- | ----------- | ------ | ----------------------------------------------------------------- |
| `POST /api/v1/agents`              | < 500ms | > 99.5%     | 28d    | `AgentCreateP95Slow`, `AgentCreateSuccessRatioBelowSLO`           |
| `POST /api/v1/agents/local-enroll` | < 800ms | > 99.5%     | 28d    | `HostCliEnrollP95Slow`, `HostCliEnrollSuccessRatioBelowSLO`       |
| `POST /api/v1/agents/:id/restart`  | < 2s    | > 99.0%     | 28d    | `ContainerRestartP95Slow`, `ContainerRestartSuccessRatioBelowSLO` |

The p95 alerts fire after the threshold is exceeded for 10 minutes. The success
ratio alerts fire after the 28-day ratio stays below the target for 1 hour. All
six are severity `warning`.

Two more rules watch the DB invariant and replay bursts:

| Alert                                     | Trigger                                                       | For                | Severity |
| ----------------------------------------- | ------------------------------------------------------------- | ------------------ | -------- |
| `AgentsCheckConstraintViolation`          | Any `agents_check_constraint_violations_total` increase in 5m | 0s (fires at once) | critical |
| `HostCliEnrollmentIdempotencyReplayBurst` | `agents_idempotency_replay_total` rate > 5/s                  | 5m                 | warning  |

Alert rules file: `ops/prometheus/agents-runtime.yml`

Dashboard: `ops/grafana/dashboards/agents-runtime.json` (uid `agents-runtime`)

Import the dashboard via **Grafana → Dashboards → Import → Upload JSON file**.
The dashboard uses the `$datasource` template variable; select your Prometheus
data source on import.

### Tuning thresholds

If the p95 or success-ratio alerts fire too frequently during normal operation,
adjust the `for:` window (latency alerts) or the ratio threshold (success
alerts) in `ops/prometheus/agents-runtime.yml` and redeploy the Prometheus
rules. Do not loosen the 28-day success SLO below 99.0% without recording the
change in your deployment config.

## Self-Fix Delivery

These alerts cover the [self-fix loop](../guides/self-fix-loop.md). All three
live in `ops/prometheus/alerts.yml` (group `agentforge.self-fix`) and fire on
any increase in a 15-minute window.

| Alert                           | Trigger                                                    | Meaning                                                                                                                                                                                       |
| ------------------------------- | ---------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SelfFixPrPermanentFailure`     | `agentforge_self_fix_pr_total{outcome="permanent_failed"}` | The PR Bridge hit a configuration, policy, or validation error. The job was dropped without retry, so the task has no pull request.                                                           |
| `SelfFixMergeFailed`            | `agentforge_self_fix_merge_total{outcome="failed"}`        | The merge attempt did not complete cleanly; the merge may already have happened. The server merges before it posts the audit comment, so a failed comment also reports `failed`.              |
| `SelfFixMergeAttemptsExhausted` | `agentforge_self_fix_merge_total{outcome="exhausted"}`     | An approve call found the task at the merge attempt cap (`SELF_FIX_MAX_MERGE_ATTEMPTS`, default 5). This call merged nothing and set `changes_requested`. An earlier attempt may have merged. |

CI-red, head-moved, and sensitive-path refusals use other `outcome` labels and
do not fire these alerts.

Shortest safe first step:

1. For `SelfFixPrPermanentFailure`, search the API logs for the failure reason:

   ```bash
   docker compose -f docker/compose.yml logs agentforge-server | grep "self-fix PR open failed permanently"
   ```

   Fix the reported cause, usually the four `GITHUB_APP_*` variables or the
   GitHub App permissions listed in the
   [self-fix guide](../guides/self-fix-loop.md#what-you-need-first). Restart
   the API after changing configuration. The dropped job is not retried
   automatically.

2. For `SelfFixMergeFailed`, first check the pull request's actual state on
   GitHub. Open the PR link from the task's **Review** tab.
   - If GitHub shows the PR as merged, do not merge again. Confirm the audit
     comment naming the approver is present, and add it by hand if it is
     missing. While the task is under the attempt cap, pressing
     **Approve & merge** again records it as **Merged** without a second merge.
   - If the PR is still open, read the error in the **Review** tab. Confirm
     GitHub is reachable and the GitHub App can still merge pull requests on
     the repository. Then press **Refresh** and approve again.

   Every approve counts against the attempt cap.

3. For `SelfFixMergeAttemptsExhausted`, stop approving. The cap is checked
   before anything else, and the platform has no way to reset it, so
   **Refresh** and **Approve & merge** cannot bypass it. Each further approve
   is refused and fires this alert again. Finish the change on GitHub instead.
   First check whether the PR is already merged, because an earlier attempt
   may have merged it. If the PR is still open, a maintainer reviews it there
   and either merges it under the normal branch protection or closes it.

Success looks like this: the counter stops increasing and the alert resolves
after the 15-minute window passes. For PR failures, the next self-fix task opens
a draft pull request. For merge failures, the task's **Review** tab reads
**Merged**. For an exhausted task, the PR is merged or closed on GitHub.

## Not Implemented Indicators

Earlier versions of this page described the indicators below. No Forge service
emits their metrics, so they have no SLO target, no alert rule and no dashboard
panel. Do not build alerts on these names: they would never fire.

| Indicator                 | Metric named earlier                                             | What you can use today                                                                                                                       |
| ------------------------- | ---------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| Orchestrator success rate | `task_runs_completed_total`                                      | Nothing yet.                                                                                                                                 |
| Workflow start latency    | `temporal_workflow_start_seconds`                                | Nothing yet. The assignment publish time in [Emitted Without Alert Rules](#emitted-without-alert-rules) covers only the publish step.        |
| NATS event delivery       | `nats_messages_delivered_total`, `nats_messages_published_total` | `OrchestrationLagHigh` and `OrchestrationOutboxBacklogHigh` cover the orchestrator's publish to NATS. Delivery to consumers is not measured. |
| DB pool saturation        | `sqlx_connection_pool_in_use`, `sqlx_connection_pool_size`       | Nothing yet.                                                                                                                                 |
| WebSocket fanout backlog  | `ws_outbound_buffer_depth`                                       | Nothing yet.                                                                                                                                 |

`npm run metrics:contract` allows a never-emitted metric name only in this
section, and fails if a name listed here starts being emitted. When one of
these indicators is implemented, move it back to [SLIs](#slis) with its alert.

## Emitted Without Alert Rules

These orchestration metrics are emitted and shown on
`ops/grafana/orchestration-release-gate-dashboard.json`, but no shipped rule
alerts on them:

- `agentforge_orchestration_outbox_published_total`: assignment outbox rows published to NATS JetStream and acknowledged.
- `agentforge_orchestration_assignment_publish_seconds`: time to publish one assignment and receive the acknowledgement.

The second one is recorded as a histogram without configured buckets, so the
exporter publishes it as a summary: one series per quantile (for example
`agentforge_orchestration_assignment_publish_seconds{quantile="0.95"}`, over
roughly the last minute) plus `_sum` and `_count`. It has no `_bucket` series,
so `histogram_quantile()` does not work on it. Only
`http_request_duration_seconds` has buckets.

## Logs

The API and orchestrator emit structured JSON logs via `tracing`. The default
format includes `service`, `request_id`, `org_id`, `user_id`, `route`, and
`error.chain` when an error propagates. Logs that originate from a privileged
trusted source (sidecar hooks, NATS auth callouts) include
`source = "<source>"`.

Sensitive fields are never logged:

- Password hashes.
- API keys, encrypted tokens, refresh tokens.
- LLM provider secrets and decrypted prompt content.
- Personal identifiers beyond `user_id` UUID and the email domain.

When emitting logs from a new code path, run `cargo test -p agentforge-api --lib`
which includes redaction regression tests against `cli_auth_proxy` and
`auth_callout`. Add a corresponding test if the new path handles a secret.
