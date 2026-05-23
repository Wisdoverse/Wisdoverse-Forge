# Threat Model

This document records the working threat model for Wisdoverse Forge in a
single-tenant self-hosted deployment. It uses STRIDE for each major trust
boundary and lists the controls the codebase enforces today. Update this file
in the same PR that introduces a new surface, transport, or trust boundary.

## Trust Zones

```text
+----------------------+      JWT + WS Origin       +----------------------+
| Browser (operator)   |  -----------------------> | Rust API :4003       |
+----------------------+                            +----------------------+
                                                            |
                                                            | internal MCP, NATS, SQLx
                                                            v
+----------------------+       JWT + scope         +----------------------+
| Rust orchestrator    |  -----------------------> | PostgreSQL / Redis  |
| :4010                |                            | NATS / Temporal     |
+----------------------+                            +----------------------+
                                                            |
                                                            | per-agent creds
                                                            v
                                                  +----------------------+
                                                  | Agent containers     |
                                                  | (sidecar + CLI)      |
                                                  +----------------------+
                                                            |
                                                            | NATS event relay
                                                            v
                                                  +----------------------+
                                                  | External LLM         |
                                                  | providers            |
                                                  +----------------------+
```

The trust boundaries this document covers:

1. Browser ↔ Rust API
2. Rust API ↔ Rust orchestrator
3. Rust API ↔ PostgreSQL / Redis / NATS / Temporal
4. Rust API ↔ Agent container (sidecar / hook / Container CLI)
5. Service ↔ External LLM provider

## Cross-Cutting Assumptions

- The host that runs the Compose stack is trusted. An attacker with shell
  access to that host wins. Disk encryption and OS-level hardening are the
  operator's responsibility and out of scope.
- TLS for browser traffic is terminated by Caddy in self-host mode and by the
  operator-supplied ingress in external-service mode. The API binds to
  loopback or to the Docker bridge; never to a public interface directly.
- Container CLIs themselves are third-party software. Wisdoverse Forge does
  not trust the CLI's process or output. See
  [docs/security/third-party-cli-images.md](third-party-cli-images.md).
- Operators are warned through `docs/security/dependency-policy.md` that
  dependency updates must complete the security policy before merge.

## Boundary 1 — Browser ↔ Rust API

### Assets

- Operator session JWTs (cookie `af_rt` for refresh, `Authorization` header
  for access).
- Operator's organizational data (tasks, evidence, attachments, agent
  outputs).
- Operator's own provider API keys (encrypted at rest).

### Threats (STRIDE)

| Class                  | Threat                                                                 | Control                                                                                                                                |
| ---------------------- | ---------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Spoofing               | Forged JWT or cross-tenant identity claim.                             | JWT signed by `agentforge_auth::JwtManager`; the auth middleware re-verifies every request and refuses requests without a valid scope. |
| Tampering              | Modified refresh token via cookie tampering.                           | `af_rt` is `HttpOnly`, `SameSite=Strict`, `Secure` in production. JWT signature check rejects modifications.                           |
| Repudiation            | Operator denies issuing a destructive action.                          | Audit events recorded in `events` table with actor `UserId`, scope, and request timestamp.                                             |
| Information Disclosure | Sensitive fields leaked through API responses.                         | Domain projections opt out of serialization for secret fields (`#[serde(skip_serializing)]` on password hashes, API keys, tokens).     |
| Denial of Service      | Resource exhaustion via large request bodies or per-IP request floods. | Axum default body-size limits; tower-http rate limiting can be added per route. The Compose stack ships with worker concurrency caps.  |
| Elevation of Privilege | Cross-tenant data read by changing path/query identifiers.             | `TenantScope` is constructed only by auth middleware; every repository method that accepts a scope filters on it. See ADR 0004.        |

### WebSocket

- Auth uses the JWT from `?token=…`. The handshake is rejected if absent or
  invalid.
- The `Origin` header is validated against the configured CORS allow-list.
  Arbitrary origins are refused.

### Cross-Site

- CORS allow-list is explicit per environment.
- Refresh cookie path scoped to `/api/v1/auth` to minimize the attack surface.
- XSS is prevented by React escaping by default; raw HTML rendering uses
  curated sanitizers.

## Boundary 2 — Rust API ↔ Rust Orchestrator

### Assets

- Workflow definitions, task assignments, and run state.

### Threats

| Class                  | Threat                                                        | Control                                                                                                                                                        |
| ---------------------- | ------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Spoofing               | A rogue process pretends to be the orchestrator.              | Both services authenticate with shared JWT signing keys derived from `LLM_ENCRYPTION_KEY` (production) / dev config. The orchestrator presents a scoped token. |
| Tampering              | Manipulated MCP payloads.                                     | Internal MCP requests carry the auth scope through the middleware; payload schemas are typed.                                                                  |
| Repudiation            | Workflow signal source unverifiable.                          | Temporal records each signal with timestamps; the orchestrator persists `task_run` rows alongside.                                                             |
| Information Disclosure | Workflow inputs leak between tenants.                         | Workflow IDs include the tenant scope; the orchestrator filters by `OrgId` before exposing run state.                                                          |
| Denial of Service      | Workflow flood blocks Temporal queue.                         | Worker concurrency is bounded; the orchestrator rejects new workflows when the queue is saturated.                                                             |
| Elevation of Privilege | Orchestrator-initiated calls bypass tenant checks on the API. | The orchestrator calls go through the same auth middleware. Service tokens carry an org scope and cannot widen it.                                             |

## Boundary 3 — Rust API ↔ Data Layer

### Assets

- All persistent state. Encrypted credential blobs. PII (operator email,
  display name).

### Threats

| Class                  | Threat                                                  | Control                                                                                                                                                                           |
| ---------------------- | ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Spoofing               | Forged DB credentials.                                  | Connection strings configured per environment; the host that runs the API holds the only valid Postgres credentials.                                                              |
| Tampering              | Malicious SQL via parameter injection.                  | All queries use parameter binding through SQLx; no string interpolation. `clippy::unwrap_used` is denied to catch sloppy paths.                                                   |
| Repudiation            | Schema modifications without a migration.               | Migrations are checksum-tracked in `_sqlx_migrations`. ADR 0006 prohibits editing run migrations.                                                                                 |
| Information Disclosure | LLM provider secrets leaked from disk or logs.          | Encrypted at rest via `LLM_ENCRYPTION_KEY`. Decryption happens only inside the credential service; encrypted blobs and decrypted contents are excluded from logs.                 |
| Denial of Service      | Long-running query starves connection pool.             | SQLx pool is bounded; query timeouts configured per environment.                                                                                                                  |
| Elevation of Privilege | Cross-tenant query because of a missing `WHERE` clause. | The `TenantScope` pattern (ADR 0004) makes tenant filtering a type-system signal. Pre-auth methods (login, context-switch authorization) are explicitly documented as exceptions. |

## Boundary 4 — Rust API ↔ Agent Container

### Assets

- Per-agent NATS credentials. The contents of `/workspace` (operator code).
- Hook event stream from the container CLI back to the API.

### Threats

| Class                  | Threat                                                                 | Control                                                                                                                                                                                                                  |
| ---------------------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Spoofing               | One agent impersonates another to read its event stream.               | NATS auth callout binds each connection to a per-agent identity; pub/sub permissions are scoped to that agent's subjects. See `docs/runbooks/nats-auth.md`.                                                              |
| Tampering              | Container escapes its workspace mount and edits other tenants' data.   | Container creation runs through `agentforge_platform::security`. Privileged mode is rejected. Host PID, docker socket mounts, and missing resource limits are refused. The workspace mount is org/workspace-scoped only. |
| Repudiation            | A hook posts events claiming a session ID it never owned.              | Hook events are signed with the agent's NATS credentials; the sidecar validates session ownership before forwarding.                                                                                                     |
| Information Disclosure | Hook events leak across orgs.                                          | NATS subjects are scoped; the API trusts the sidecar's `agent_id` and refuses subjects outside that agent's allow-list.                                                                                                  |
| Denial of Service      | Runaway CLI consumes host CPU/memory.                                  | Resource limits are required at container creation; missing limits are a security-policy violation.                                                                                                                      |
| Elevation of Privilege | A container reaches the host through docker socket or host filesystem. | `platform::security` validation refuses docker socket mounts, privileged mode, host PID, and `/var/run` bind mounts. The buildx plugin (`rust/bins/buildx-plugin`) routes `docker buildx build` through a proxy.         |

## Boundary 5 — Service ↔ External LLM Provider

### Assets

- Provider API keys, request contents (which may include operator data),
  response contents.

### Threats

| Class                  | Threat                                                                              | Control                                                                                                                                                                           |
| ---------------------- | ----------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Spoofing               | Outbound request hijacked by a malicious proxy.                                     | TLS to provider endpoints with system certificate roots; no plaintext fallback.                                                                                                   |
| Tampering              | Prompt injection from operator-supplied data.                                       | This is a known accepted risk in LLM systems. Approval workflows for context candidates (ADR 0005 + `domain::context_governance`) keep a human in the loop for sensitive context. |
| Repudiation            | Provider denies receiving a request.                                                | `usage_analytics` records each provider call with timestamp, model, and token counts.                                                                                             |
| Information Disclosure | Operator data sent to a provider against intent.                                    | Per-context sensitivity classification (`domain::context_governance::Sensitivity`) gates what may leave the deployment. Operators configure which providers are permitted.        |
| Denial of Service      | Provider rate limit blocks the workbench.                                           | Per-provider rate limiting and retries with backoff in `agentforge-llm`. Operators can configure fallback providers.                                                              |
| Elevation of Privilege | Compromised provider returns hostile output that escalates inside Wisdoverse Forge. | Model output is treated as untrusted text everywhere; tool invocations and shell commands always require explicit operator approval through the orchestration approval surface.   |

## Known Accepted Risks

- **Prompt injection.** No general-purpose mitigation exists today; the
  governance surface keeps a human reviewer in the loop for sensitive
  contexts.
- **Self-hosted operator account compromise.** If an operator account is
  taken over, the attacker inherits that operator's scope. Recovery requires
  the operator to use the password-reset flow (signed token, single-use) and
  the audit trail in `events`.

## Out of Scope

- Datacenter and host-level physical security.
- Operating system patching and kernel hardening.
- Network-layer DDoS mitigation upstream of Caddy/operator ingress.

## Related Documents

- [docs/security/dependency-policy.md](dependency-policy.md)
- [docs/security/context-data-policy.md](context-data-policy.md)
- [docs/security/third-party-cli-images.md](third-party-cli-images.md)
- [docs/runbooks/nats-auth.md](../runbooks/nats-auth.md)
- [docs/runbooks/credential-sync.md](../runbooks/credential-sync.md)
- [SECURITY.md](../../SECURITY.md)
