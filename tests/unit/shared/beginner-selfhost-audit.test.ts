import fs from 'node:fs'
import path from 'node:path'
import { describe, expect, it } from 'vitest'

const projectRoot = path.resolve(import.meta.dirname, '../../..')
const auditScript = fs.readFileSync(
  path.join(projectRoot, 'scripts/audit-beginner-selfhost.sh'),
  'utf8'
)

describe('beginner self-host audit', () => {
  it('can verify Provider+Prompt using an existing verified provider', () => {
    expect(auditScript).toContain('BEGINNER_USE_EXISTING_PROVIDER')
    expect(auditScript).toContain('no enabled provider with persisted passed test status was found')
    expect(auditScript).toContain('"/llm-providers/$provider_id/test"')
    expect(auditScript).toContain('/agents/$agent_id/prompt')
  })

  it('still requires an explicit key for cloud providers when not reusing an existing provider', () => {
    expect(auditScript).toContain('not required for ollama')
    expect(auditScript).toContain("tr '[:upper:]' '[:lower:]'")
    expect(auditScript).toContain('[ "$provider" != "ollama" ]')
    expect(auditScript).toContain('BEGINNER_API_KEY is required for --provider')
    expect(auditScript).toContain('BEGINNER_PROVIDER is required for --provider')
    expect(auditScript).toContain('BEGINNER_MODEL is required for --provider')
  })
})
