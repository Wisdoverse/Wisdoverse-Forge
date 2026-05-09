# CLI Auth Proxy Runbook

## Purpose

The CLI auth proxy stores per-user OAuth credentials for containerized CLI agents such as Codex. A background worker refreshes stale credentials and revokes rows after repeated `invalid_grant` responses so a broken refresh token does not keep retrying forever.

## Configuration

| Variable | Default | Notes |
| --- | ---: | --- |
| `CLI_AUTH_PROXY_REVOKE_THRESHOLD` | `2` | Consecutive `invalid_grant` refresh failures before a credential row is revoked. Must be `>= 1`. |
| `APP_URL` | unset | Enables server callback mode when paired with custom OpenAI OAuth app settings. |
| `CLI_AUTH_PROXY_OPENAI_CLIENT_ID` | built-in public Codex client | Set to use an operator-owned OAuth app. |
| `CLI_AUTH_PROXY_OPENAI_CLIENT_SECRET` | unset | Optional confidential-client secret. |
| `CLI_AUTH_PROXY_OPENAI_AUTH_ENDPOINT` | OpenAI default | Override only for a compatible identity provider. |
| `CLI_AUTH_PROXY_OPENAI_TOKEN_ENDPOINT` | OpenAI default | Override only for a compatible identity provider. |

Use `CLI_AUTH_PROXY_REVOKE_THRESHOLD=1` only for fast test environments. Production should normally keep `2`; raise it to `3` or higher only when provider-side refresh behavior is known to produce false `invalid_grant` responses.

## Operational Checks

1. Check status without exposing tokens:

   ```bash
   curl -fsS -H "Authorization: Bearer $TOKEN" \
     "$APP_URL/api/v1/cli-auth-proxy/status"
   ```

2. Watch refresh-worker outcomes:

   ```bash
   docker compose logs -f agentforge | rg "CLI auth proxy refresh|invalid_grant|invalid_client"
   ```

3. Alert operators on `invalid_client`. This means the OAuth app credentials or IdP registration are wrong and user credential rows must not be revoked automatically.

4. Treat `revokedAt` plus `revokeReason=invalid_grant` as a user re-authentication action. The UI should prompt the user to reconnect the CLI provider.

## Rollback

Unset `CLI_AUTH_PROXY_REVOKE_THRESHOLD` to restore the default value of `2`. Values below `1` fail startup intentionally.
