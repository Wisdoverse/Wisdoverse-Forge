import { describe, it, expect } from 'vitest'
import fs from 'node:fs'
import path from 'node:path'

describe('agentforge-relay-hook event ID generation', () => {
  const source = fs.readFileSync(
    path.resolve(__dirname, '../../../hooks/agentforge-relay-hook.cjs'),
    'utf8'
  )

  it('should use crypto.randomUUID() instead of Math.random()', () => {
    expect(source).toContain('randomUUID')
    expect(source).not.toMatch(/Math\.random\s*\(\s*\)/)
  })
})
