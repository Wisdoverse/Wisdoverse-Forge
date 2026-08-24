# Self-host operator runbook

One-page operational reference for a Wisdoverse Forge deployment. Each knob
links to its full guide; this page is the *checklist*.

## Config knobs (set in `docker/.env`)

| Purpose | Variables | Guide |
| --- | --- | --- |
| Analytics cost estimates | `LLM_PRICING` (`{"model":{"input","output"}}` USD/1M) | [configuration](../guides/configuration.md) |
| Required review gates | `REVIEW_REQUIRED_GATES` (comma keys; unknown keys fail boot) | [configuration](../guides/configuration.md) |
| Scheduled compliance exports | `COMPLIANCE_EXPORT_INTERVAL_HOURS` + `COMPLIANCE_EXPORT_DIR` (paired) | [configuration](../guides/configuration.md) |
| Telemetry retention | `ANALYTICS_RETENTION_DAYS` (events/analytics_events) | [configuration](../guides/configuration.md) |
| Run retention | `RUN_RETENTION_DAYS` (finished runs of terminal tasks) | [configuration](../guides/configuration.md) |
| SSO | `AUTH_SSO__*` (`ENABLED`, OIDC, `ROLE_CLAIM`, `ADMIN_GROUPS`, `ORG_GROUP_MAP`, `TEAM_GROUP_MAP`, `DEPROVISION`) | [configuration](../guides/configuration.md) |
| Provision/deprovision webhooks | `AUTH_SSO__DEPROVISION_TOKEN` (+ `POST /api/v1/auth/sso/provision`, `POST /api/v1/auth/deprovision`) | [configuration](../guides/configuration.md) |
| SCIM 2.0 Users | same token; `GET|POST /api/v1/auth/sso/scim/Users`, `GET|DELETE /Users/{id}` (paged list, create, deactivate) | [configuration](../guides/configuration.md) |
| OTLP trace export | `OTEL_EXPORTER_OTLP_ENDPOINT` (+ `OTEL_EXPORTER_OTLP_PROTOCOL`, `OTEL_TRACES_SAMPLER`, `OTEL_TRACES_SAMPLER_ARG`, `OTEL_SERVICE_NAME`) | [configuration](../guides/configuration.md) |
| Container CLI model | `CODEX_DEFAULT_MODEL`, `CONTAINER_*_API_KEY` | [configuration](../guides/configuration.md) |

## Weekly checklist

1. **Compliance exports** — confirm `COMPLIANCE_EXPORT_DIR/<org-slug>/` gained a
   fresh `agentforge-compliance-<timestamp>.csv`; check `ls -t | head -3`.
2. **Retention working** — `ANALYTICS_RETENTION_DAYS` / `RUN_RETENTION_DAYS`
   sweeps run on boot + every 6 h; server log shows `retention purge removed`
   lines when rows expire.
3. **Health** — `curl https://your-host/api/health` shows `database: true`;
   `GET /metrics` (admin-gated) has `http_requests_total` increasing.
4. **Recurring tasks** — at least one schedule created and one run produced
   (`orchestration_tasks` newest rows); the runner ticks every 60 s.
5. **LLM pricing** — when `LLM_PRICING` is set, Analytics usage rows show
   `≈ $…` estimates (missing models show nothing by design).
6. **OTLP export (when configured)** — after any request, the collector logs
   trace batches with `service.name = agentforge-server`; knob changes need a
   restart.

## Incident pointers

- **Boot fails with `LLM_PRICING must be valid JSON` / unknown review gate** —
  a typo in `.env`; the message names the offending key.
- **SSO sign-in loops** — verify `AUTH_SSO__SPA_BASE_URL` matches the public
  URL and the redirect URI registered with the IdP; state is Redis or
  single-replica memory.
- **Provision/deprovision or SCIM returns 404** — the webhook token is unset; set
  `AUTH_SSO__DEPROVISION_TOKEN`.
- **`compliance export sweep failed` in logs** — the configured directory is
  not writable by the API process; fix permissions.
- **`recurring task sweep failed`** — usually a task row FK problem; the log's
  `recurring_task_id` identifies the schedule to pause first.
- **No traces in the collector** — the server logs
  `agentforge-telemetry: OTLP exporter init failed` on boot when the endpoint
  is unreachable; export is disabled (not fatal). Check the endpoint and
  `OTEL_EXPORTER_OTLP_PROTOCOL`, then restart.
- **Bundle verification fails** — re-transfer; `SHA256SUMS` is authoritative
  (see [offline install](../guides/offline-install.md)).

## Rotation & upgrade

- Rotate the bundle signing key: generate a new Ed25519 pair, re-bundle on the
  connected host, distribute the new `.pub`; keep the old `.pub` available
  until all hosts re-loaded.
- Upgrade path: `make prod-ext` (or `make dev`) after pulling, then run the
  two health probes above. Migrations run automatically at server boot;
  `scripts/` helpers and `docker/` compose are version-pinned via `VERSION`.

## Observability SLIs

See [observability-slo.md](observability-slo.md) for availability, latency,
and orchestrator success-rate targets and their alert thresholds.
