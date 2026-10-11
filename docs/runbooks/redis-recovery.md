# Redis Connection Recovery

This runbook restores API access to the configured Redis state store. It covers Container CLI authorization state and the API readiness probe. It does not qualify every Redis-backed feature.

## Before You Start

- Use an authorized operator account.
- Use the existing `dev@example.com` account for an authorized login.
- Confirm that the existing `agentforge-server` image is available locally.
- Confirm that `docker/.env` names the intended external Redis instance.
- Confirm that you own the API service and the Redis instance for this operation.
- For a state-retention drill, use isolated Redis with AOF and its same data volume after restart.
- Do not stop a shared Redis service.
- Do not change another Compose stack.
- Keep Redis URLs, bearer tokens, authorization URLs, PKCE state, and callback data private.

Read the [external-service deployment guide](../guides/deployment.md#external-service-production).
Read the [configuration guide](../guides/configuration.md) for `REQUIRE_EXTERNAL_STATE`.

## Enable Redis Readiness

Set `REQUIRE_EXTERNAL_STATE=true` in the existing `docker/.env` file.
Keep the existing `REDIS_URL` value private.
This setting runs Redis write and consume operations during readiness.

From the repository root, recreate only the API service:

```bash
docker compose --env-file docker/.env \
  -f docker/compose.yml \
  -f docker/compose.external.yml \
  --profile external up -d --no-build --no-deps --force-recreate agentforge-server
```

This command uses the canonical base and external Compose files.
It uses the existing image and does not recreate dependent services.
Use `make prod-ext` only when you need to start or reconcile the full stack.

## Confirm Redis Operations

Use an authenticated API session for the target organization.
Send requests to `http://127.0.0.1:4003/api/v1` when the API uses its default loopback port.
Use your configured loopback port when it differs.

Readiness must return HTTP 200:

```bash
curl --fail --silent --show-error http://127.0.0.1:4003/api/health
```

`/health` reports process liveness only. Use `/api/health` to confirm Redis readiness.

Send an authenticated `POST /cli-auth-proxy/openai/authorize` request.
The response must contain `ok: true` and a URL.
Do not open the authorization URL.
Do not include the authorization URL in public logs.
The request stores PKCE state in Redis for 300 seconds.

Use an approved Redis client to run `EXISTS` and `TTL` for the returned state key.
The key prefix is `cli-auth-proxy:state:`.
The expected results are `1` and a remaining TTL from 1 through 300 seconds.
Do not run `GET`.
Do not copy the state value into a report.

To confirm single-use behavior, submit that state to the wrong provider's
`POST /cli-auth-proxy/claude/complete-manual` endpoint.
Use a dummy callback code and the returned state in `input` as `code#state`.
The API must return HTTP 400 for the provider mismatch.
Repeat the same request to the original provider's
`POST /cli-auth-proxy/openai/complete-manual` endpoint.
The API must return HTTP 400 because the first request consumed the state.
Do not use a real callback code.
Do not complete a provider login.

Clear temporary shell variables when the requests finish.
Remove any temporary response files that contain an authorization URL.
Keep only status codes, TTL ranges, and pass or fail results in the run record.

## Recover After Redis Returns

Restore Redis through its approved service procedure.
Do not assume that a healthy Redis process restored the API connection.
Readiness and new authorization requests can remain unavailable after Redis returns.

Query `http://127.0.0.1:4003/api/health` again.
Send a new authenticated authorize request only when readiness returns HTTP 200.
Confirm the new state has a 300-second TTL and the API returns `ok: true`.

If readiness or the authorize request still fails, restart only the API service:

```bash
docker compose --env-file docker/.env \
  -f docker/compose.yml \
  -f docker/compose.external.yml \
  --profile external restart agentforge-server
```

An API restart interrupts active requests.
Repeat readiness and authorize requests after the API restarts.
An unexpired state can remain usable when Redis retains its data volume.
Use isolated Redis with AOF enabled for the state-retention proof.
Record only metadata and a controlled mismatch request as state-retention evidence.
Do not finish the OAuth flow during this recovery procedure.

## Expected Results And Limits

The local qualification on 2026-10-11 passed `make prod-ext` with the required state flag. An authenticated authorize request stored state with a 300-second TTL. The wrong-provider request returned HTTP 400 and consumed the state. The replay request returned HTTP 400.

Redis restoration alone did not restore API operations or readiness. Restarting the API restored readiness and new state writes. The unexpired control state remained available after API recovery with Redis AOF and the same data volume.

The Temporal gate completed in 0.142 seconds. An anonymous Temporal workflow creation request returned HTTP 401. A Temporal workflow status request for another organization returned HTTP 404. These Temporal results do not establish CLI OAuth tenant isolation. We did not run model execution or provider login.

This result does not qualify production Redis or power-loss recovery. It does not prove automatic reconnection, complete OAuth, or provider login. It does not qualify presence state or multi-replica behavior. The [runtime validation guide](runtime-validation.md) tracks other runtime evidence.

## Source References

- [`RedisClient`](../../rust/crates/infra/src/redis_client.rs) runs the Redis `SET` and `GETDEL` readiness probe.
- [`cli_auth_proxy`](../../rust/crates/api/src/services/cli_auth_proxy/mod.rs) stores state with a 300-second TTL and consumes it with `GETDEL`.
- [`CLI auth proxy routes`](../../rust/crates/api/src/routes/cli_auth_proxy.rs) require authenticated requests.
- [`API readiness`](../../rust/crates/api/src/health.rs) calls the probe when the service requires external state.
- [`API startup`](../../rust/bins/server/src/main.rs) runs the probe when the service requires external state.
