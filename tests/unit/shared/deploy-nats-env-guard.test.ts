import fs from 'node:fs'
import path from 'node:path'
import { describe, expect, it } from 'vitest'

const projectRoot = path.resolve(import.meta.dirname, '../../..')
const deployScript = fs.readFileSync(path.join(projectRoot, 'scripts/deploy.sh'), 'utf8')

describe('deploy NATS environment guard', () => {
  it('runs before staging and production docker operations', () => {
    expect(deployScript).toContain('validate_deploy_nats_env')
    expect(deployScript.indexOf('validate_deploy_nats_env')).toBeLessThan(
      deployScript.indexOf('case "$ENV" in')
    )
  })

  it('rejects stale NATS_URL values that bypass the backend account user', () => {
    expect(deployScript).toContain('NATS_URL overrides the compose backend default')
    expect(deployScript).toContain('*://backend:*@*')
    expect(deployScript).toContain('NATS_BACKEND_PASSWORD')
    expect(deployScript).toContain('NATS_AUTH_SERVICE_PASSWORD')
    expect(deployScript).toContain('NATS_SYS_PASSWORD')
  })
})
