# Observability and SLOs

This runbook defines the service-level indicators (SLIs), service-level
objectives (SLOs), and alerting thresholds for a Wisdoverse Forge deployment.
The targets here are the defaults shipped in this repository; an operator
running Wisdoverse Forge may tighten them but should not loosen them without
recording the change next to their deployment config.

## SLIs

The Rust API and orchestrator emit Prometheus metrics through
`metrics_exporter_prometheus`. The default scrape endpoint is `/metrics` on
the orchestrator. The relevant metric families are:

| SLI                       | Metric                                                                    | Definition                                                                         |
| ------------------------- | ------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| API availability          | `http_requests_total{status=~"5..",service="api"}`                        | `1 - (5xx requests / total requests)` over a rolling window.                       |
| API latency (p95)         | `http_request_duration_seconds{service="api",quantile="0.95"}`            | Histogram quantile across all routes; excludes WebSocket upgrades and SSE streams. |
| Orchestrator success rate | `task_runs_completed_total{status="success"} / task_runs_completed_total` | Fraction of completed task runs that ended without an error status.                |
| Workflow start latency    | `temporal_workflow_start_seconds{quantile="0.95"}`                        | Time from `start_workflow` accepted to the workflow worker picking it up.          |
| NATS event delivery       | `nats_messages_delivered_total / nats_messages_published_total`           | Fraction of events successfully delivered to a consumer.                           |
| DB pool saturation        | `sqlx_connection_pool_in_use / sqlx_connection_pool_size`                 | Average over the SLO window.                                                       |
| WebSocket fanout backlog  | `ws_outbound_buffer_depth{quantile="0.99"}`                               | Backlog depth at the 99th percentile per connection.                               |

## SLO Targets

The defaults below assume a single-tenant self-hosted deployment serving
~10–50 operators. They are starting points, not contractual ceilings.

| Surface                   | Window  | Target         |
| ------------------------- | ------- | -------------- |
| API availability          | 30 days | 99.5%          |
| API p95 latency           | 30 days | 350 ms         |
| Orchestrator success rate | 30 days | 99.0%          |
| Workflow start p95        | 30 days | 1.5 s          |
| NATS event delivery       | 7 days  | 99.9%          |
| DB pool saturation        | 30 days | < 70% average  |
| WebSocket backlog p99     | 30 days | < 512 messages |

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

Alerts should be routed to the operator's on-call channel. The recommended
defaults:

| Condition                                        | Severity | Where it fires                                              |
| ------------------------------------------------ | -------- | ----------------------------------------------------------- |
| API 5xx rate > 1% for 5 min                      | Warning  | API availability SLI.                                       |
| API 5xx rate > 5% for 2 min                      | Critical | Same SLI; the page condition.                               |
| API p95 latency > 1 s for 10 min                 | Warning  | API latency SLI.                                            |
| Orchestrator workflow start p95 > 5 s for 10 min | Warning  | Workflow start SLI.                                         |
| Task run failure rate > 5% for 15 min            | Warning  | Orchestrator success rate SLI.                              |
| DB connection pool saturation > 90% for 5 min    | Critical | Pool saturation SLI; usually indicates a stuck transaction. |
| NATS event delivery < 99% for 5 min              | Warning  | NATS SLI; check Compose health of the NATS service.         |
| `/health` 200 lost for 60 s                      | Critical | Synthetic liveness check.                                   |

`/health` is the API liveness probe. `/api/health` is the deep readiness probe
that asserts PostgreSQL, Redis, and NATS reachability when configured. The
liveness probe should always page; the deep probe should warn first, then page
if it fails to recover within five minutes.

## On-Call Runbooks

When an alert fires, refer to the matching runbook:

- API 5xx rate: [docs/runbooks/runtime-validation.md](runtime-validation.md)
- NATS delivery: [docs/runbooks/nats-auth.md](nats-auth.md)
- Credential failure: [docs/runbooks/credential-sync.md](credential-sync.md)
- Context governance: [docs/runbooks/context-governance-audit.md](context-governance-audit.md)
- Orchestration: [docs/runbooks/orchestration.md](orchestration.md)
- Frontend deploy: [docs/runbooks/frontend-deploy.md](frontend-deploy.md)
- Disaster recovery: [docs/guides/disaster-recovery.md](../guides/disaster-recovery.md)

## Dashboards

The repository does not ship a Grafana JSON dashboard. Operators are expected
to build their own per-deployment dashboard from the metrics listed above.
A reference dashboard skeleton has the following panels:

- Request rate, error rate, p50/p95/p99 latency per route.
- Active WebSocket connections and outbound buffer depth distribution.
- Orchestrator workflow start latency and task run success rate.
- NATS publish/deliver rates by subject.
- SQLx pool size, in-use, and acquire-time histogram.
- Memory and CPU per service container.

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

DB invariant and replay burst alerts also fire immediately without a `for:`
window:

| Alert                                     | Trigger                                                       | Severity |
| ----------------------------------------- | ------------------------------------------------------------- | -------- |
| `AgentsCheckConstraintViolation`          | Any `agents_check_constraint_violations_total` increase in 5m | critical |
| `HostCliEnrollmentIdempotencyReplayBurst` | `agents_idempotency_replay_total` rate > 5/s for 5m           | warning  |

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
