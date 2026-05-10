import fs from 'node:fs'
import path from 'node:path'
import { describe, expect, it } from 'vitest'

const projectRoot = path.resolve(import.meta.dirname, '../../..')
const deployScript = fs.readFileSync(path.join(projectRoot, 'scripts/deploy.sh'), 'utf8')
const validatorScript = fs.readFileSync(
  path.join(projectRoot, 'scripts/validate-deploy-nats-env.sh'),
  'utf8'
)

describe('deploy NATS environment guard', () => {
  it('runs before staging and production docker operations', () => {
    // The validator is invoked from deploy.sh once docker/.env has been loaded
    // and before the per-environment branching that drives docker pulls.
    const invokeIdx = deployScript.indexOf('validate-deploy-nats-env.sh')
    const branchIdx = deployScript.indexOf('case "$ENV" in')
    expect(invokeIdx).toBeGreaterThan(-1)
    expect(branchIdx).toBeGreaterThan(-1)
    expect(invokeIdx).toBeLessThan(branchIdx)
  })

  it('rejects stale NATS_URL values that bypass the backend account user', () => {
    expect(validatorScript).toContain('NATS_URL overrides the compose backend default')
    expect(validatorScript).toContain('*://backend:*@*')
    expect(validatorScript).toContain('NATS_BACKEND_PASSWORD')
    expect(validatorScript).toContain('NATS_AUTH_SERVICE_PASSWORD')
    expect(validatorScript).toContain('NATS_SYS_PASSWORD')
  })
})
