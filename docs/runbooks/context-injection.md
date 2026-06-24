# Context Injection Runbook

Use this when governed context preview, run-start injection, sidecar envelope
loading, skill mounting, or context rollback needs operator action.

## Scope

Applies to the governed context runtime path:

- task context preview
- run-start context envelope creation
- sidecar envelope fetch
- Container CLI adapter translation
- skill mount and context materialization
- usage analytics and governance audit signals related to injected context

## Symptom

- Task create skips context preview when it should show one.
- Runs start without expected governed context.
- Sidecar logs show envelope fetch or adapter translation failures.
- Skills are missing from the agent container.
- Usage analytics or governance audit rows are missing after a context-backed
  run.
- Operators need to perform a feature-flag rollback.

## Verify

1. Confirm the API, orchestrator, NATS, Prometheus, and Grafana are healthy:

   ```bash
   curl -fsS http://127.0.0.1:4003/health
   curl -fsS http://127.0.0.1:4010/health
   curl -fsS http://127.0.0.1:8222/healthz
   curl -fsS http://127.0.0.1:3902/-/healthy
   curl -fsS http://127.0.0.1:33000/api/health
   ```

2. Confirm the rollout flags for the target organization or deployment:

   ```sql
   SELECT key, enabled, organization_id, metadata
   FROM feature_flags
   WHERE key IN (
     'context.governance.enabled',
     'context.preview.enabled',
     'context.injection.enabled',
     'context.analytics.enabled'
   )
   ORDER BY key;
   ```

3. Confirm run-start injections were persisted:

   ```sql
   SELECT task_id, run_id, agent_id, envelope_version, item_count, created_at
   FROM run_context_injections
   ORDER BY created_at DESC
   LIMIT 20;
   ```

4. Confirm sidecar envelope variables are present without printing token values:

   ```bash
   docker compose -f docker/compose.yml -f docker/compose.external.yml \
     --profile external logs --since=15m agentforge-server \
     | rg 'context|envelope|injection|adapter'
   ```

5. Confirm the governance audit path recorded context events:

   ```sql
   SELECT created_at, action, user_id, resource_type, resource_id
   FROM audit_log
   WHERE action LIKE 'governance.context.%'
   ORDER BY created_at DESC
   LIMIT 20;
   ```

Do not log `AGENTFORGE_CONTEXT_ENVELOPE_TOKEN`, provider keys, NATS credentials,
HMAC secrets, decrypted memory content, or raw mounted skill content.

## Mitigate

### Preview Disabled Unexpectedly

1. Check `context.preview.enabled`.
2. Verify the frontend is using the current Rust API endpoint.
3. If only preview is affected, keep `context.governance.enabled` on and disable
   `context.preview.enabled` until the preview route is fixed. Task creation
   should continue without preview.

### Injection Missing

1. Check `context.injection.enabled`.
2. Inspect `run_context_injections` for the task and run.
3. Inspect sidecar logs for envelope fetch and adapter translation errors.
4. If the sidecar path is failing, disable `context.injection.enabled`. Runs
   should start without mounted context while governance and audit stay online.

### Skill Mount Failing

1. Confirm the skill is active and within the agent workspace or tenant scope.
2. Confirm the sidecar did not reject the mount path.
3. Disable `context.injection.enabled` if mount behavior could expose the wrong
   scope or block all runs.
4. Keep audit rows and schema in place for incident review.

### Analytics Missing

1. Check `context.analytics.enabled`.
2. Confirm the usage analytics refresh job is running only when the flag is on.
3. Query governance audit rows directly to distinguish missing analytics from
   missing source events.
4. Disable `context.analytics.enabled` if aggregation is causing load or stale
   projections. Raw audit writes should continue when governance is enabled.

## Cutover

Use this order for internal tenant rollout:

1. Confirm `context.governance.enabled=false`,
   `context.preview.enabled=false`, `context.injection.enabled=false`, and
   `context.analytics.enabled=false` before starting.
2. Enable `context.governance.enabled` for the internal tenant.
3. Verify approval queue writes and governance audit rows.
4. Enable `context.preview.enabled`.
5. Create a task and confirm preview renders the expected item set.
6. Enable `context.injection.enabled`.
7. Start a run and confirm `run_context_injections` has the task and run.
8. Confirm sidecar adapter report exists and no secrets are printed in logs.
9. Enable `context.analytics.enabled`.
10. Confirm usage analytics and governance audit views converge.
11. Attach dashboard snapshots and release-gate evidence before widening the
    rollout percentage.

## Rollback

Rollback order during an incident:

1. Disable `context.analytics.enabled` if only dashboards or aggregation jobs are
   failing.
2. Disable `context.injection.enabled` if sidecar, envelope, adapter, or skill
   mount behavior is risky. Runs should continue without context.
3. Disable `context.preview.enabled` if task create preview is stale or blocking
   users. Task creation should continue without preview.
4. Disable `context.governance.enabled` only if governance writes are corrupt or
   exposing data. Preserve existing audit rows for investigation.

After any flag change:

```bash
make prod-ext
curl -fsS http://127.0.0.1:4003/health
curl -fsS http://127.0.0.1:4010/health
```

Schema additions are additive. Do not drop context tables, audit rows, or
governance indexes during rollback unless a separate data-recovery plan has
explicit owner signoff.

## Root-Cause Investigation

- Correlate `run_context_injections.created_at` with task creation and run start
  time.
- Compare sidecar adapter reports with API envelope versions.
- Check governance audit rows for rejected scope expansion or secret-detected
  events.
- Review Prometheus panels for resolver latency, envelope fetch latency, adapter
  translation failures, approval queue throughput, feedback ingestion lag, and
  audit event apply lag.
- Confirm release-gate evidence has been updated before any second rollout
  attempt.
