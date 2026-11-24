# Self-host operator runbook

One-page operational reference for a Wisdoverse Forge deployment. Each knob
links to its full guide; this page is the _checklist_.

## Config knobs (set in `docker/.env`)

| Purpose                        | Variables                                                                                                                              | Guide                                           |
| ------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| Analytics cost estimates       | `LLM_PRICING` (`{"model":{"input","output"}}` USD/1M)                                                                                  | [configuration](../guides/configuration.md)     |
| Required review gates          | `REVIEW_REQUIRED_GATES` (comma keys; unknown keys fail boot)                                                                           | [configuration](../guides/configuration.md)     |
| Scheduled compliance exports   | `COMPLIANCE_EXPORT_INTERVAL_HOURS` + `COMPLIANCE_EXPORT_DIR` (paired)                                                                  | [configuration](../guides/configuration.md)     |
| Telemetry retention            | `ANALYTICS_RETENTION_DAYS` (events/analytics_events)                                                                                   | [configuration](../guides/configuration.md)     |
| Run retention                  | `RUN_RETENTION_DAYS` (finished runs of terminal tasks)                                                                                 | [configuration](../guides/configuration.md)     |
| SSO                            | `AUTH_SSO__*` (`ENABLED`, OIDC, `ROLE_CLAIM`, `ADMIN_GROUPS`, `ORG_GROUP_MAP`, `TEAM_GROUP_MAP`, `DEPROVISION`)                        | [configuration](../guides/configuration.md)     |
| Provision/deprovision webhooks | `AUTH_SSO__DEPROVISION_TOKEN` (+ `POST /api/v1/auth/sso/provision`, `POST /api/v1/auth/deprovision`)                                   | [configuration](../guides/configuration.md)     |
| SCIM 2.0 Users                 | same token; `GET                                                                                                                       | POST /api/v1/auth/sso/scim/Users`, `GET         | DELETE /Users/{id}` (paged list, create, deactivate) | [configuration](../guides/configuration.md) |
| OTLP trace export              | `OTEL_EXPORTER_OTLP_ENDPOINT` (+ `OTEL_EXPORTER_OTLP_PROTOCOL`, `OTEL_TRACES_SAMPLER`, `OTEL_TRACES_SAMPLER_ARG`, `OTEL_SERVICE_NAME`) | [configuration](../guides/configuration.md)     |
| Offline bundle signing         | `BUNDLE_SIGNING_KEY` (Ed25519 PEM) + `agentforge tuf` (root pinning, rotation)                                                         | [offline install](../guides/offline-install.md) |
| Container CLI model            | `CODEX_DEFAULT_MODEL`, `CONTAINER_*_API_KEY`                                                                                           | [configuration](../guides/configuration.md)     |

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
7. **Bundle signing** — offline bundles carry `metadata/` (TUF chain) plus
   `SHA256SUMS.sig`; the host pin (`/etc/agentforge/tuf/root.json`) exists and
   advances automatically only after a rotated root and full bundle verify.

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
- **`tuf verify` rejects the root** — the bundle root was signed by a key
  that is not your pin (forged/unknown bundle) or downgraded; restore the
  bundle from the connected host. Rotation must be done with `tuf rotate`.
- **No traces in the collector** — the server logs
  `agentforge-telemetry: OTLP exporter init failed` on boot when the endpoint
  is unreachable; export is disabled (not fatal). Check the endpoint and
  `OTEL_EXPORTER_OTLP_PROTOCOL`, then restart.
- **Bundle verification fails** — re-transfer and verify the host pin or legacy
  signing key; an unsigned checksum file is not authoritative (see
  [offline install](../guides/offline-install.md)).

## Upgrade step: orchestrator database password is now required

Applies to: self-contained installs (`make prod`, `make prod-pull`,
`make prod-backup`, `make prod-storage`, `make prod-casdoor`, and any
`docker compose ... -f docker/compose.prod.yml` command). Not affected:
`make prod-ext` (it uses `ORCHESTRATOR_DATABASE_URL`) and `make dev`.

Symptom after pulling: Compose stops before starting anything with
`required variable ORCHESTRATOR_DB_PASSWORD is missing a value`, or
`make bootstrap-selfhost` stops with `ORCHESTRATOR_DB_PASSWORD is missing`.

Shortest safe path (keeps your data and the current password):

1. Add the password the orchestrator database was created with to
   `docker/.env`. If you never set one, it is the old built-in default:

   ```bash
   echo 'ORCHESTRATOR_DB_PASSWORD=orchestrator' >> docker/.env
   ```

2. Start the stack the way you normally do, for example `make prod-pull`.

Success looks like this: `make selfhost-check` passes, and
`curl -s http://localhost:4010/health/ready` returns HTTP 200. Do not use a new
random value in step 1. The database keeps the password it was created with, so
a different value locks the orchestrator out (`password authentication failed
for user "orchestrator"`).

Then rotate away from the well-known default:

1. Generate a value: `openssl rand -hex 24`. Use hex, because the password is
   placed inside a connection URL.
2. Change the database password. `\password` prompts for it, so it stays out of
   shell history:

   ```bash
   docker compose --env-file docker/.env -f docker/compose.yml -f docker/compose.prod.yml --profile prod \
     exec orchestrator-db psql -U orchestrator -d orchestrator -c '\password orchestrator'
   ```

3. Replace the `ORCHESTRATOR_DB_PASSWORD=` line in `docker/.env` with the new
   value.
4. Recreate the services that use it, for example `make prod-pull` (add the
   `backup` profile if you run it).

Success looks like this: `curl -s http://localhost:4010/health/ready` returns
HTTP 200 again. If the orchestrator logs show `password authentication failed`,
steps 2 and 3 used different values. Repeat step 2 with the value from
`docker/.env`.

## Upgrade step: internal ports now listen only on this machine

Applies to: every Compose profile. Before this change, the bundled database,
Redis, NATS (client and monitoring ports), Temporal, the Temporal UI, Casdoor,
Adminer, Redis Commander and the image registry listened on every address of
the host. Now they listen on `127.0.0.1` unless you set a host address. The
Caddy reverse proxy (`80`, `443`) is unchanged.

Symptom after upgrading: a Host CLI agent on another machine stays offline, or
a tool you opened from another machine, such as the Temporal UI, no longer
loads.

Shortest safe path: keep the new default and use an SSH tunnel for tools, for
example `ssh -L 8233:127.0.0.1:8233 <user>@<server>`. If other machines must
reach a port, add only the lines you need to `docker/.env`:

```bash
# Host CLI agents on other machines (NATS client port 4222)
NATS_BIND_ADDRESS=0.0.0.0
# Browsers on other machines sign in through Casdoor (port 8000)
CASDOOR_BIND_ADDRESS=0.0.0.0
# Database, Redis, NATS monitoring, Temporal, its UI and the tools.
# This also opens the API (4003) and orchestrator (4010) ports.
BIND_ADDRESS=0.0.0.0
```

Then start the stack the way you normally do, for example `make prod-pull`;
Compose recreates the containers whose ports changed. `0.0.0.0` means every
IPv4 address; Docker listened on IPv6 too before. Prefer the address of one
interface, and allow only the machines that need the port in the host firewall.
The NATS monitoring port, Temporal and its UI have no login. Host CLI details:
[Deployment Guide](../guides/deployment.md#host-cli-agents-on-other-machines).

Success looks like this: on the host, `ss -ltn` lists these ports on
`127.0.0.1`, or on the address you set.

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
