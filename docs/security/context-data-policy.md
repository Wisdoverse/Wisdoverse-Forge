# Context Data Policy

This policy covers governed memory, skills, context candidates, context
injection evidence, usage analytics, and `governance.context.*` audit rows.

## Classification

Governed context data is classified at write time by
`ContextGovernanceService::classify_sensitivity`.

| Class             | Meaning                                               | Default Handling                                  |
| ----------------- | ----------------------------------------------------- | ------------------------------------------------- |
| `public`          | Safe to share inside the authorized scope             | May be injected into runs for that scope          |
| `internal`        | Normal work context                                   | Scope checks required before read or injection    |
| `confidential`    | Sensitive business or customer context                | Scope checks plus explicit approval path          |
| `secret_detected` | Token, password, key, or secret-like payload detected | Block write unless the stored content is redacted |

Approval-time classification is an inference, not a permanent guarantee.
Every mutation path that changes memory or skill content must re-run the
classifier.

## Data That Must Not Leave The System

The following fields must not be serialized into API responses, audit payloads,
NATS messages, WebSocket events, envelope JSON, logs, traces, or exports:

- Decrypted memory content outside the authorized context-injection path.
- Full skill content when it contains secrets or secret-like settings.
- Provider API keys, OAuth tokens, webhook URLs, and external secret values.
- Run-scoped content dereference tokens.
- `HMAC_SECRET`, NATS credentials, sidecar credentials, and container
  environment secrets.
- `LLM_ENCRYPTION_KEY`, `CONTEXT_AUDIT_HMAC_KEY`, or derived key material.
- Password hashes, encrypted credential blobs, nonces, Stripe identifiers, and
  equivalent secret-bearing fields.

Use redacted previews, stable content references, or HMAC subject hashes instead
of raw values.

## Scope And Subject Identity

All memory and skill reads must be constrained by authenticated tenant scope.
`agents.workspace_id` is the execution boundary for container-backed agents;
`agents.project_id` is only UI/task context.

Governance audit projection follows this rule:

- Same-scope readers may see `rawItemId`.
- Cross-scope readers receive `auditSubjectHash` and `rawItemId: null`.
- Organization owners/admins can inspect org-wide rows but cannot cross
  organization boundaries.

`auditSubjectHash` is deterministic for a key version and is computed from the
subject ID, scope kind, and scope ID. Treat `CONTEXT_AUDIT_HMAC_KEY` as a
production secret.

## Audit

Governance audit events use the `governance.context.*` namespace and are stored
in `audit_log.action`. The `events` table is not the durability source for this
audit domain.

Required audit properties:

- Every governance mutation, approval, rejection, injection, feedback action,
  export, and sensitivity hit must emit an audit row.
- Audit write failure in a governance mutation is a compliance bug. The
  mutation should roll back or fail closed.
- Audit payloads may include item kind, scope, actor, classifier result,
  reason, and redacted previews. They must not include decrypted content or
  secrets.
- Export events must themselves be audited as
  `governance.context.audit.exported`.

## Export

The supported export path is `POST /api/v1/governance/audit/export`.

Export rules:

- `redactSecrets=true` is the default and should remain enabled for compliance
  exports.
- Export results are tenant-scoped and follow the same raw-ID masking as the
  list route.
- Mixed-scope exports must be treated as sensitive even after redaction because
  event timing, actor IDs, and hashes may still be identifying.
- Do not export raw database rows from `audit_log` for external review unless a
  security owner has approved the exact field list.

## Retention

Governed context audit rows are retained for 365 days by default. Legal hold may
extend retention. Ordinary user offboarding must not delete recent audit rows.

Current enforcement is manual. Operators should run retention deletes only as a
tracked ops task and only for rows outside the policy window:

```sql
DELETE FROM audit_log
WHERE action LIKE 'governance.context.%'
  AND created_at < now() - interval '365 days';
```

Memory and skill content retention is separate from audit retention. Revoking or
deleting an item must not erase the audit trail that proves the action happened.

## Offboarding And Deletion

When a user leaves:

1. Revoke active user-scoped memory items and skills owned by the user.
2. Reassign or revoke team/project items where the user is the owner or sole
   maintainer.
3. Delete or rotate any external secrets referenced by the user's skills.
4. Emit or verify governance audit rows for the revoke/delete actions.
5. Keep audit rows until the retention window expires unless legal erasure is
   explicitly approved.

If legal erasure is approved before the audit retention window expires, remove
or pseudonymize user-identifying fields in a transaction and record the erasure
approval outside the erased row set.

## Async Re-Scan

The target operating policy is weekly re-scan of active governed memory and
skill content with the current classifier patterns.

On a hit:

- Set or keep sensitivity as `secret_detected`.
- Revoke the item from future injection.
- Store only redacted preview data in audit details.
- Emit `governance.context.async_rescan_hit`.
- Notify operators if a whole organization cannot be scanned in the weekly
  window.

Current Unit 5.2 code exposes the audit projection and documents this policy.
The always-on weekly worker is still a follow-up implementation item and should
not be assumed to be active until runtime metrics and scheduler evidence exist.

## Logging And Tracing

Do not log raw context content, raw secret fields, encrypted payloads, or
decrypted provider responses. SQL logs in production must not expose bind
values for governance content or credential-bearing writes.

Use IDs only when the reader is authorized for the same scope. For broader
operator views, use `auditSubjectHash`, redacted previews, and aggregate counts.
