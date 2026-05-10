# Orchestration Production Runbook

This runbook is Docker Compose-first and matches the deployment files kept in
this repository.

## Scope

Applies to the orchestration task path:

- participant heartbeat and stale/offline recovery
- deterministic task dispatch and lease expiry
- durable assignment outbox and sidecar assignment inbox
- durable sidecar result publish and backend result inbox dedupe
- realtime board projection and polling fallback

## Quick Triage

Check the Compose stack:

```bash
docker compose -f docker/compose.yml --profile external ps
```

Check server health and logs:

```bash
curl -fsS http://127.0.0.1:4003/health
docker compose -f docker/compose.yml --profile external logs --since=15m agentforge-server \
  | rg 'orchestration|participant|outbox|lease|result'
```

Check NATS and JetStream:

```bash
curl -fsS 'http://127.0.0.1:8222/jsz?accounts=true&streams=true&consumers=true&config=true' \
  | jq '.account_details[].stream_detail[] | select(.name | test("ORCHESTRATION"))'
docker compose -f docker/compose.yml --profile external logs --since=15m nats \
  | rg 'ERR|WRN|auth|JetStream'
```

### Local Real-CLI E2E Smoke

Use this only for local `prod-ext` or an explicitly approved canary target. The
runner copies the selected operator's CLI credential directories into a
temporary `0700` HOME and fails if cleanup cannot remove the temporary
credential copy. By default it runs `/usr/local/bin/agentforge-sidecar` inside
the production agent image (`agentforge-agent:<tool>`), so the CLI under test is
the container-baked CLI, not the host `codex` or `claude` binary.

```bash
E2E_DATABASE_URL='postgres://...' \
E2E_PASSWORD='...' \
BASE_URL='http://127.0.0.1:4007' \
npm run test:e2e:orchestration:real-cli
```

Supported overrides:

- `ORCHESTRATION_REAL_CLI_TOOL=codex|claude`
- `ORCHESTRATION_REAL_CLI_EXECUTION_MODE=container|host` (default:
  `container`)
- `AGENTFORGE_SIDECAR_CONTAINER_IMAGE=agentforge-agent:codex`
- `ORCHESTRATION_REAL_CLI_SOURCE_HOME=/secure/operator/home`
- `ORCHESTRATION_REAL_CLI_E2E_TIMEOUT=300s`
- `E2E_EMAIL=dev@example.com`
- `E2E_PASSWORD` must be provided explicitly for the selected test account.

Host mode is available only for local debugging:

```bash
cargo build -p agentforge-sidecar
ORCHESTRATION_REAL_CLI_EXECUTION_MODE=host \
AGENTFORGE_SIDECAR_BIN="$PWD/rust/target/debug/agentforge-sidecar" \
E2E_DATABASE_URL='postgres://...' \
E2E_PASSWORD='...' \
npm run test:e2e:orchestration:real-cli
```

Do not mount or commit raw `.codex` / `.claude` directories into repository
paths or images. For orchestrated environments, use the same principle with a
short-lived Secret mounted into an ephemeral test workload, not a long-lived
shared volume.

Scrape metrics through the same authenticated path Prometheus uses:

```bash
curl -fsS -H "Authorization: Bearer $ADMIN_JWT" http://127.0.0.1:4003/metrics \
  | rg 'agentforge_orchestration_(outbox|stale|expired|result|inbox|working|busy)'
```

### Release-Gate Dashboard

Import `ops/grafana/orchestration-release-gate-dashboard.json` as the
`orchestration-release-gate` Grafana dashboard before opening the 24h staging
soak window. Configure `DS_PROMETHEUS` to a target-isolated datasource for the
environment under review. Do not use a shared datasource that mixes staging,
canary, and production series unless SRE has provided target-isolated recording
rules for the same metric contract.

The dashboard covers the release-gate signals that must be trended through soak
and canary:

- dispatch publish P95 latency
- result apply P95 latency
- assignment outbox backlog and oldest unpublished age
- stale participants
- expired working leases
- busy participants without matching work
- working tasks without a busy participant
- unauthorized result rate
- result, outbox, and metrics collector error rates
- required metric presence; any missing required metric blocks the release-gate
  review

Attach the live dashboard URL or a timestamped screenshot set to the release
gate evidence pack. The dashboard JSON only proves the panel contract is ready;
it does not replace live import, alert routing, 24h trend review, canary review,
rollback drill evidence, or owner signoff.

### Alert Route Check

Use the alert route checker after SRE loads `docker/orchestration-alerts.yml`
into Prometheus and after the target Alertmanager route is ready. The script
verifies that all orchestration alert rules are present in Prometheus with the
expected route-critical labels, severities, runbook annotations, durations, and
metric expressions. It also verifies that a sanitized route-contract JSON file
contains the expected on-call receiver and a `component=orchestration` matcher
on the same route object. For live checks, it also queries Prometheus
`/api/v1/alertmanagers` and fails if `ALERTMANAGER_URL` is not present in the
active target set. The report does not print discovered Alertmanager target URLs.

When validating a Prometheus instance, do not assume an unrelated Alertmanager
container on another network is usable evidence. The Prometheus
`alerting.alertmanagers` target must point at a reachable Alertmanager with an
approved receiver and live notification integration.

Do not commit the webhook URL file. Store it in a secret location controlled by
the operator, then mount it read-only through `ALERTMANAGER_SECRET_DIR`. After
applying the live config, verify that the Prometheus container can discover the
target:

```bash
curl -fsS http://127.0.0.1:3902/api/v1/alertmanagers \
  | jq '.data.activeAlertmanagers | length'
```

The value must be greater than zero, and the active target must match the
`ALERTMANAGER_URL` used by the route checker.

The route-contract file must be generated or reviewed by SRE from the live
Alertmanager route, without webhook URLs or notification tokens:

```json
{
  "routes": [
    {
      "receiver": "platform-oncall",
      "matchers": ["component=\"orchestration\""]
    }
  ],
  "receivers": [
    {
      "name": "platform-oncall",
      "integration_count": 1,
      "integration_types": ["webhook_configs"]
    }
  ]
}
```

Generate the sanitized contract from the live loaded Alertmanager config inside
the SRE-controlled environment. Do not commit, paste, or attach the raw config:

```bash
scripts/release/orchestration_alert_route_contract.mjs \
  --config-file /secure/live-alertmanager.yml \
  --output /secure/evidence/orchestration/alert-route-contract.json
```

```bash
TARGET_NAME=staging \
PROMETHEUS_URL=https://staging-prometheus.example.com \
PROMETHEUS_BEARER_TOKEN="$PROMETHEUS_BEARER_TOKEN" \
ALERTMANAGER_URL=https://staging-alertmanager.example.com \
ALERT_ROUTE_JSON_FILE=/secure/evidence/orchestration/alert-route-contract.json \
ALERT_ROUTE_EXPECTED_RECEIVER=platform-oncall \
ALERT_ROUTE_OUTPUT="docs/evidence/orchestration/alert-route-$(date -u +%Y%m%dT%H%M%SZ).md" \
scripts/release/orchestration_alert_route_check.sh
```

Do not pass a raw Alertmanager config containing webhook URLs or tokens. The
script intentionally checks only `/-/ready` on live Alertmanager and never fetches
`/api/v2/status`, because the status API can include loaded receiver config. A
passing report supports alert-rule and route-contract review only. It verifies a
sanitized receiver integration count, but it still does not prove that an on-call
notification was received, and it does not replace the 24h soak, dashboard trend
review, canary, rollback, or owner signoff evidence.

## Rollout Flags

All flags default to `true` for the durable production path. Flip through
`docker/.env` or the equivalent environment-management layer for your
deployment.

| Flag                                                | Effect                                                             |
| --------------------------------------------------- | ------------------------------------------------------------------ |
| `ORCHESTRATION_RESULT_CONSUMER_ENABLED`             | drains durable sidecar results into DB                             |
| `ORCHESTRATION_ASSIGNMENT_OUTBOX_PUBLISHER_ENABLED` | publishes DB assignment outbox rows to JetStream                   |
| `ORCHESTRATION_PARTICIPANT_LIVENESS_ENABLED`        | consumes heartbeats, expires leases, drains available participants |
| `ORCHESTRATION_CONTROL_PLANE_METRICS_ENABLED`       | samples DB consistency gauges for alerts                           |
| `ORCHESTRATION_WS_PROJECTOR_ENABLED`                | emits committed task/participant updates to the board websocket    |

Rollback order during an incident:

1. Disable `ORCHESTRATION_WS_PROJECTOR_ENABLED` if only realtime projection is bad. The UI polling fallback still converges.
2. Disable `ORCHESTRATION_ASSIGNMENT_OUTBOX_PUBLISHER_ENABLED` to stop sending new work while preserving outbox rows.
3. Disable `ORCHESTRATION_RESULT_CONSUMER_ENABLED` only if result application is corrupting state. Results remain in JetStream for replay.
4. Keep additive schema in place. Do not drop `orchestration_outbox`, `orchestration_inbox`, or lease columns during rollback.

After any flag change:

```bash
docker compose --env-file docker/.env -f docker/compose.yml --profile external up -d --force-recreate agentforge-server
curl -fsS http://127.0.0.1:4003/health
```

## Alert Playbooks

### assignment-outbox-backlog

Alerts:

- `OrchestrationAssignmentOutboxBacklog`
- `OrchestrationAssignmentOutboxStalled`
- `OrchestrationOutboxPublishErrors`

Verify:

```sql
SELECT COUNT(*) AS backlog,
       EXTRACT(EPOCH FROM (NOW() - MIN(created_at))) AS oldest_age_seconds
FROM orchestration_outbox
WHERE published_at IS NULL AND event_type = 'assignment';
```

Check NATS stream state:

```bash
curl -fsS 'http://127.0.0.1:8222/jsz?accounts=true&streams=true&consumers=true&config=true' \
  | jq '.account_details[].stream_detail[] | select(.name=="ORCHESTRATION_ASSIGNMENTS")'
```

Resolve:

- If NATS is unhealthy, restore `nats` first; do not delete outbox rows.
- If DB is healthy and NATS is healthy, restart `deploy/agentforge` to restart the publisher.
- If backlog keeps growing after restart, disable `ORCHESTRATION_ASSIGNMENT_OUTBOX_PUBLISHER_ENABLED=false` and page backend/SRE.

### expired-working-leases

Alert: `OrchestrationExpiredWorkingLeases`

Verify:

```sql
SELECT id, organization_id, assigned_agent_id, last_assignment_id, attempt, lease_expires_at
FROM orchestration_tasks
WHERE status = 'working'
  AND lease_expires_at IS NOT NULL
  AND lease_expires_at < NOW()
ORDER BY lease_expires_at ASC
LIMIT 50;
```

Resolve:

- Confirm `ORCHESTRATION_PARTICIPANT_LIVENESS_ENABLED=true`.
- Restart `deploy/agentforge` if the liveness worker stopped.
- If leases still do not fail closed, keep assignment publisher disabled and page backend; do not manually reassign in-flight work.

### stale-participants

Alert: `OrchestrationStaleParticipants`

Verify:

```sql
SELECT organization_id, agent_id, name, status, last_heartbeat_at
FROM participants
WHERE status <> 'offline'
ORDER BY last_heartbeat_at ASC
LIMIT 50;
```

Resolve:

- Check agent pod/container logs for sidecar heartbeat failures.
- Check NATS auth-callout logs if agents cannot connect.
- If agents are intentionally drained, mark the rollout note and wait for stale-offline recovery to converge.

### participant-task-divergence

Alerts:

- `OrchestrationBusyParticipantWithoutWork`
- `OrchestrationWorkingTaskWithoutBusyParticipant`

Verify:

```sql
SELECT p.organization_id, p.agent_id, p.name, p.status
FROM participants p
WHERE p.status = 'busy'
  AND NOT EXISTS (
      SELECT 1 FROM orchestration_tasks t
      WHERE t.organization_id = p.organization_id
        AND t.assigned_agent_id = p.agent_id
        AND t.status = 'working'
  );

SELECT t.id, t.organization_id, t.assigned_agent_id, t.status, t.last_assignment_id
FROM orchestration_tasks t
WHERE t.status = 'working'
  AND t.assigned_agent_id IS NOT NULL
  AND NOT EXISTS (
      SELECT 1 FROM participants p
      WHERE p.organization_id = t.organization_id
        AND p.agent_id = t.assigned_agent_id
        AND p.status = 'busy'
  );
```

Resolve:

- Treat as state-machine divergence. Stop new assignment publishing first.
- Capture task IDs, participant IDs, and deployment SHA.
- Do not repair rows manually unless an incident commander approves exact SQL.

### result-apply-lag

Alert: `OrchestrationResultApplyLag`

Verify:

```bash
curl -fsS 'http://127.0.0.1:8222/jsz?accounts=true&streams=true&consumers=true&config=true' \
  | jq '.account_details[].stream_detail[] | select(.name=="ORCHESTRATION_RESULTS")'
```

Resolve:

- If consumer pending is high, restart the Rust API service in the Compose stack.
- If Postgres latency is high, scale DB or reduce API load before increasing worker replicas.
- If the same delivery repeats, confirm `orchestration_inbox.delivery_id` dedupe rows are present.

### unauthorized-results

Alert: `OrchestrationResultUnauthorizedSpike`

Verify:

```bash
docker compose -f docker/compose.yml --profile external logs --since=15m agentforge-server \
  | rg 'orchestration result rejected|agent_unknown|signature_mismatch|timestamp_outside_window'
```

Resolve:

- `agent_unknown` after contract tests may be harmless; verify subjects are test UUIDs.
- `signature_mismatch` or cross-agent mismatch is security-relevant. Disable affected agents and rotate HMAC/NATS credentials.
- Confirm NATS auth callout denies cross-agent `orchestration.result.<other-agent>` publish.

## Staging Soak

Before production canary, record 24h of:

- `agentforge_orchestration_outbox_backlog == 0`
- `agentforge_orchestration_expired_working_leases == 0`
- `agentforge_orchestration_busy_participants_without_work == 0`
- `agentforge_orchestration_working_tasks_without_busy_participant == 0`
- result apply P95 below 10s
- no unauthorized-result spike outside known tests

Manual scenarios to execute:

1. `queued -> working -> completed`
2. sidecar restart after assignment intake
3. backend restart before result apply
4. NATS interruption and recovery
5. websocket disabled with polling fallback convergence
6. flag rollback and re-enable without data repair

### 24h Soak Runner

Use the soak runner to collect repeatable point-in-time snapshots through the
24h staging window and produce a summary file plus the full snapshot bundle for
the release gate:

```bash
TARGET_NAME=staging \
API_BASE_URL=https://staging.example.com \
ORCHESTRATOR_BASE_URL=https://staging-orchestrator.example.com \
NATS_MONITOR_URL=https://staging-nats.example.com \
METRICS_BEARER_TOKEN="$METRICS_BEARER_TOKEN" \
DATABASE_URL="$DATABASE_URL" \
SOAK_DURATION_SECONDS=86400 \
SOAK_INTERVAL_SECONDS=3600 \
SOAK_MIN_SAMPLES=24 \
SOAK_REQUIRE_OPTIONAL=true \
SOAK_OUTPUT_DIR="/secure/evidence/orchestration/staging-$(date -u +%Y%m%dT%H%M%SZ)" \
scripts/release/orchestration_soak_runner.sh
```

Run a short command smoke before starting the real 24h window, using the same
target endpoint and credential environment as the real run:

```bash
TARGET_NAME=staging \
API_BASE_URL=https://staging.example.com \
ORCHESTRATOR_BASE_URL=https://staging-orchestrator.example.com \
NATS_MONITOR_URL=https://staging-nats.example.com \
METRICS_BEARER_TOKEN="$METRICS_BEARER_TOKEN" \
DATABASE_URL="$DATABASE_URL" \
SOAK_DURATION_SECONDS=0 \
SOAK_MIN_SAMPLES=1 \
SOAK_REQUIRE_OPTIONAL=true \
SOAK_ALLOW_SHORT=true \
SOAK_OUTPUT_DIR=/tmp/agentforge-orchestration-soak-smoke \
scripts/release/orchestration_soak_runner.sh
```

The runner writes `summary.md`, `samples.tsv`, `runner.log`, and one Markdown
snapshot per sample. It intentionally does not print DSNs, bearer tokens, JWTs,
or raw logs. Treat `runner.log` as local diagnostics only; review and redact it
before attaching it to any release evidence package. A passing runner summary
still does **not** replace the full snapshot bundle, dashboard trend review,
alert-routing confirmation, manual scenario records, canary timeline, rollback
drill record, or owner signoffs.

While the 24h soak is still running, generate a progress-only report from the
current `samples.tsv` without touching the running tmux/session process:

```bash
SOAK_PROGRESS_OUTPUT="/secure/evidence/orchestration/soak-progress-$(date -u +%Y%m%dT%H%M%SZ).md" \
scripts/release/orchestration_soak_progress.sh "$SOAK_OUTPUT_DIR"
```

The progress report is useful for handoff updates, but an `INCOMPLETE` report is
not release evidence for the final 24h soak gate.

Do not treat issue tracker status transitions as gate closure. If a support
change moves a release-blocking item to Done while the checker still fails,
reopen the item and attach the current checker/progress evidence.

### Release-Gate Snapshot

Use the snapshot collector at soak start, at regular intervals during the soak,
before each canary expansion, and after rollback drills:

```bash
TARGET_NAME=staging \
API_BASE_URL=https://staging.example.com \
ORCHESTRATOR_BASE_URL=https://staging-orchestrator.example.com \
NATS_MONITOR_URL=https://staging-nats.example.com \
METRICS_BEARER_TOKEN="$METRICS_BEARER_TOKEN" \
DATABASE_URL="$DATABASE_URL" \
SNAPSHOT_OUTPUT="docs/evidence/orchestration/staging-$(date -u +%Y%m%dT%H%M%SZ).md" \
scripts/release/orchestration_gate_snapshot.sh
```

For local `prod-ext`, the default ports match the compose stack:

```bash
DATABASE_URL="$(sed -n 's/^DATABASE_URL=//p' docker/.env | head -n 1)" \
PSQL_DOCKER_NETWORK="$(sed -n 's/^EXTERNAL_NETWORK=//p' docker/.env | head -n 1)" \
DOCKER_CONTAINER=agentforge-server \
SNAPSHOT_OUTPUT=/tmp/orchestration-gate-snapshot.md \
scripts/release/orchestration_gate_snapshot.sh
```

The script intentionally does not print DSNs, bearer tokens, JWTs, or raw logs.
It collects point-in-time health, DB convergence, JetStream, and Prometheus
signals. It does **not** replace the 24h trend review, canary timeline,
rollback drill record, dashboard links, alert-routing confirmation, browser
fallback check, or owner signoffs.

## Local Compose Check

Use this path when reproducing against local `prod-ext`:

```bash
make prod-ext
docker compose -f docker/compose.yml -f docker/compose.external.yml --profile external ps
curl -fsS http://127.0.0.1:4003/health
curl -fsS 'http://127.0.0.1:8222/jsz?accounts=true&streams=true&consumers=true&config=true' \
  | jq '.account_details[].stream_detail[] | select(.name | test("ORCHESTRATION"))'
```
