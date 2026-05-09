# Context Governance Audit Runbook

Use this when governed context audit rows are missing, audit exports fail,
operators need to verify scope masking, or retention/offboarding work touches
memory and skill governance data.

## Symptom

- `/context/audit` is empty after context approvals, skill changes, or feedback.
- `GET /api/v1/governance/audit` returns `403`, `422`, or empty results for a
  user who should see audit rows.
- Audit exports include unexpected raw item IDs, unredacted secrets, or invalid
  tamper status.
- Compliance asks for the current retention, export, or offboarding behavior.

## Verify

1. Confirm the API and database are healthy:

   ```bash
   curl -fsS http://127.0.0.1:4003/health
   ```

2. Confirm migration 59 and the governance indexes exist:

   ```sql
   SELECT version, success
   FROM _sqlx_migrations
   WHERE version = 59;

   SELECT indexname, indexdef
   FROM pg_indexes
   WHERE tablename = 'audit_log'
     AND indexname LIKE 'idx_audit_log_governance_context%';
   ```

3. Confirm producers are writing to `audit_log.action`, not `events.event_type`:

   ```sql
   SELECT created_at, action, user_id, resource_type, resource_id, details
   FROM audit_log
   WHERE action LIKE 'governance.context.%'
   ORDER BY created_at DESC
   LIMIT 20;
   ```

4. Confirm the route projects the same rows:

   ```bash
   curl -fsS \
     -H "Authorization: Bearer $TOKEN" \
     "http://127.0.0.1:4003/api/v1/governance/audit?eventPrefix=governance.context.&limit=20"
   ```

5. Confirm cross-scope masking:
   - Same-scope readers may receive `rawItemId`.
   - Readers outside the item scope must receive `auditSubjectHash` and
     `rawItemId: null`.
   - Admins can inspect org-wide rows, but tenant organization boundaries still
     apply.

## What Is Recorded

Current governance services write durable rows to `audit_log` with
`action LIKE 'governance.context.%'`. The projection supports:

- Candidate lifecycle events such as created, approved, rejected, and scope
  expansion rejection.
- Memory and skill governance mutations.
- Context feedback events.
- Audit export events emitted by `POST /api/v1/governance/audit/export`.

The projection derives subject fields from `details.item_id`,
`details.memory_id`, `details.skill_id`, or `resource_id`, then joins
`memory_items` and `skills` to determine item scope.

## What Is Not Recorded

Audit payloads must not contain decrypted memory content, full skill content,
provider secrets, webhook URLs, run-scoped content tokens, NATS credentials,
HMAC secrets, or raw container environment values. If any producer needs these
values for debugging, store a redacted preview or stable external reference
instead.

The `events` table remains the agent/run activity stream. It is not the
source of truth for context governance audit durability.

## Mitigate

### Audit Rows Missing

1. Reproduce the governance mutation and inspect `audit_log` in the same
   organization.
2. If rows are missing, inspect the Rust service path that performed the
   mutation. Context and skill governance writes should call
   `ContextGovernanceService::emit_audit` inside the mutation transaction.
3. Treat a mutation that commits without the matching audit row as a compliance
   bug. Disable the affected feature flag if needed before broad use.

### Projection Empty But Rows Exist

1. Check auth role and tenant axes. The route uses the authenticated
   `TenantScope`; stale or narrow workspace/team/project axes can hide rows.
2. Remove filters and retry with only `eventPrefix=governance.context.`.
3. For non-admin users, verify project/team/workspace membership for the item
   scope. Actor-authored rows can be returned without raw item ID visibility.

### Raw IDs Leak

1. Stop using the affected projection response.
2. Verify `rawItemId` is `null` for a user outside the item's scope.
3. Check `rust/crates/api/src/repositories/governance_audit.rs` for scope-axis
   joins and `rust/crates/api/src/routes/governance_audit.rs` for HMAC
   projection.
4. Rotate `CONTEXT_AUDIT_HMAC_KEY` only if the hash key was exposed.

### Export Fails

1. Check `CONTEXT_AUDIT_HMAC_KEY`. Production should set an explicit 32-byte
   hex key:

   ```bash
   openssl rand -hex 32
   ```

2. Confirm the API has the variable in its runtime environment.
3. Retry with `redactSecrets=true`. Redacted export is the default and is the
   only supported compliance export path for mixed-scope readers.

## Retention

Policy: retain governed context audit rows for 365 days unless legal hold
requires longer retention.

Current enforcement is manual. Until an automated retention worker lands, run
retention only as an explicit ops task, after confirming backups and legal hold:

```sql
DELETE FROM audit_log
WHERE action LIKE 'governance.context.%'
  AND created_at < now() - interval '365 days';
```

Do not delete recent audit rows to satisfy ordinary user offboarding. Revoke or
delete the user's context items instead, then retain the audit trail for the
policy window.

## Offboarding

1. Identify user-owned memory and skill records.
2. Revoke active user-scoped items and team/project items owned by the user.
3. Emit or verify a `governance.context.*` audit row for every revoke/delete.
4. Export the user's governance audit trail with redaction enabled if required
   by compliance.
5. Preserve audit rows until the 365-day retention window expires, unless a
   legal erasure process explicitly approves earlier deletion.

## HMAC Key Rotation

`auditSubjectHash` is generated at read time from the subject ID, scope kind,
scope ID, and `CONTEXT_AUDIT_HMAC_KEY`. Use a dedicated key in production even
though the current route can fall back to `LLM_ENCRYPTION_KEY`.

Rotation procedure:

1. Generate a new key with `openssl rand -hex 32`.
2. Store the old and new key versions in the secret manager with rotation time,
   operator, and reason.
3. Deploy the new key to the Rust API.
4. Verify `/api/v1/governance/audit` still returns entries and no raw IDs leak
   across scope.
5. Keep the old key for historical export comparison until the last export
   generated with that key ages out.

After rotation, subject hashes change because they are computed dynamically.
Rows with a historical `details.hmac_signature` may show `invalid` until
multi-key verification is implemented. Rows without signatures show
`not_configured`.

## Async Re-Scan Policy

The security policy requires weekly classifier re-scan of active governed
memory and skill content with the current classifier patterns. The current
Unit 5.2 implementation documents the policy and exposes audit projection for
hits; it does not yet add an always-on background worker.

Operational behavior when the worker or manual job is enabled:

- Re-scan only active governed memory and skill content.
- On `secret_detected`, revoke the item, store only a redacted preview in audit
  details, and emit `governance.context.async_rescan_hit`.
- Keep the original item ID visible only to same-scope readers; cross-scope
  readers get the HMAC subject hash.
- Alert if re-scan cannot complete for an organization within the weekly
  window.

## Rollback

The projection and indexes are additive. If the route causes production risk:

1. Remove or hide the `/context/audit` frontend entry point.
2. Block `GET /api/v1/governance/audit` and export at the gateway if needed.
3. Leave `audit_log` rows and migration 59 in place.
4. Keep governance producers writing audit rows; do not roll back audit writes
   unless the underlying governance mutation is also disabled.
