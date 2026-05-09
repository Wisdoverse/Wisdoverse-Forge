import { spawnSync } from 'node:child_process'
import path from 'node:path'
import { describe, expect, it } from 'vitest'

const projectRoot = path.resolve(import.meta.dirname, '../../..')
const checkScript = path.join(projectRoot, 'scripts/check-production-env.sh')

function runCheck(overrides: Record<string, string | undefined> = {}) {
  const env: NodeJS.ProcessEnv = {
    PATH: process.env.PATH ?? '',
    JWT_SECRET: 'x'.repeat(44),
    API_KEY_SALT: 'deploy-api-key-salt',
    LLM_ENCRYPTION_KEY: 'a'.repeat(64),
    DATABASE_URL: ['postgres://agentforge:', 'secret', '@db:5432/agentforge'].join(''),
    REDIS_URL: ['redis://:', 'secret', '@redis:6379'].join(''),
    APP_URL: 'https://coding.example.test',
    TRUST_PROXY: 'true',
    ALLOWED_ORIGINS: 'https://coding.example.test',
    AGENTFORGE_WORKSPACE_ROOT: '/data/agentforge/workspaces',
    CONTAINER_ALLOWED_MOUNT_PREFIXES: '/data/agentforge/workspaces,/tmp/agentforge/oauth-mounts',
    NATS_BACKEND_PASSWORD: 'backend-secret',
    NATS_AUTH_SERVICE_PASSWORD: 'auth-service-secret',
    NATS_SYS_PASSWORD: 'sys-secret',
    NATS_CALLOUT_ISSUER_SEED: 'issuer-seed',
    NATS_CALLOUT_ACCOUNT_SIGNING_KEY_SEED: 'account-signing-seed',
    NATS_CALLOUT_XKEY_SEED: 'xkey-seed',
    NATS_CALLOUT_ISSUER_PUBLIC: 'issuer-public',
    NATS_CALLOUT_XKEY_PUBLIC: 'xkey-public',
  }

  for (const [key, value] of Object.entries(overrides)) {
    if (value === undefined) {
      delete env[key]
    } else {
      env[key] = value
    }
  }

  return spawnSync('sh', [checkScript], {
    env,
    encoding: 'utf8',
  })
}

describe('production NATS environment validation', () => {
  it('accepts compose-derived backend NATS_URL when NATS_URL is omitted', () => {
    const result = runCheck()

    expect(result.status).toBe(0)
    expect(result.stdout).toContain('Production environment validation passed')
  })

  it('accepts an explicit backend-user NATS_URL override', () => {
    const result = runCheck({
      NATS_URL: ['nats://backend:', 'backend-secret', '@nats:4222'].join(''),
    })

    expect(result.status).toBe(0)
  })

  it('rejects legacy token-only NATS_URL overrides', () => {
    const result = runCheck({
      NATS_URL: 'nats://legacy-token@nats:4222',
    })

    expect(result.status).toBe(1)
    expect(result.stderr).toContain('NATS_URL must use backend user credentials')
  })
})
