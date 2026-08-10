import fs from 'node:fs'
import path from 'node:path'
import { describe, expect, it } from 'vitest'

const projectRoot = path.resolve(import.meta.dirname, '../../..')
const read = (file: string) => fs.readFileSync(path.join(projectRoot, file), 'utf8')

describe('first administrator setup token', () => {
  it('is generated, required, passed only to the server, and explained to the operator', () => {
    const bootstrap = read('scripts/bootstrap-selfhost.sh')
    const audit = read('scripts/audit-beginner-selfhost.sh')
    const compose = read('docker/compose.yml')
    const envExample = read('docker/.env.example')
    const deploymentGuide = read('docs/guides/deployment.md')
    const e2eSetup = read('tests/e2e/global-setup.ts')

    expect(bootstrap).toContain('ensure_generated_env_value BOOTSTRAP_ADMIN_TOKEN')
    expect(bootstrap).toContain('BOOTSTRAP_ADMIN_TOKEN \\\n')
    expect(bootstrap).toContain('Read the first-account setup token')
    expect(audit).toContain('setupToken')
    expect(audit).toContain('BOOTSTRAP_ADMIN_TOKEN')
    expect(compose).toContain('BOOTSTRAP_ADMIN_TOKEN=${BOOTSTRAP_ADMIN_TOKEN:-}')
    expect(envExample).toContain('BOOTSTRAP_ADMIN_TOKEN=')
    expect(deploymentGuide).toContain('Deployment setup token')
    expect(deploymentGuide).toContain('BOOTSTRAP_ADMIN_TOKEN')
    expect(e2eSetup).toContain('setupToken: process.env.BOOTSTRAP_ADMIN_TOKEN')
  })
})
