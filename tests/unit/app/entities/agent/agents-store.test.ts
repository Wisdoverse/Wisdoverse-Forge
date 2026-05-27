import { describe, expect, it } from 'vitest'
// `managedToAgentInfo` is a non-exported helper; either re-export it from the
// store module for testing or inline the same derivation logic via a tiny adapter.
// First try the import:
import { managedToAgentInfo } from '@app/entities/agent/model/agents.store'

const base = {
  id: 'a1',
  name: 'a',
  status: 'idle' as const,
  tasksCompleted: 0,
  tasksInProgress: 0,
  successRate: 0,
}

describe('agents.store managedToAgentInfo backward-compat', () => {
  it('uses server-provided runtimeKind="cli" when present', () => {
    const info = managedToAgentInfo({ ...base, cliTool: 'codex', runtimeKind: 'cli', runtimeId: 'host-x' } as any)
    expect(info.runtimeKind).toBe('cli')
  })

  it('uses server-provided runtimeKind="container" when present', () => {
    const info = managedToAgentInfo({ ...base, cliTool: 'codex', runtimeKind: 'container' } as any)
    expect(info.runtimeKind).toBe('container')
  })

  it('uses server-provided runtimeKind="api" when present', () => {
    const info = managedToAgentInfo({ ...base, runtimeKind: 'api' } as any)
    expect(info.runtimeKind).toBe('api')
  })

  it('fallback when server omits runtimeKind: host- prefix → cli', () => {
    const info = managedToAgentInfo({ ...base, cliTool: 'codex', runtimeId: 'host-abc' } as any)
    expect(info.runtimeKind).toBe('cli')
  })

  it('fallback when server omits runtimeKind: cliTool without host- prefix → container', () => {
    const info = managedToAgentInfo({ ...base, cliTool: 'codex' } as any)
    expect(info.runtimeKind).toBe('container')
  })

  it('fallback when server omits runtimeKind: no cliTool → api', () => {
    const info = managedToAgentInfo({ ...base } as any)
    expect(info.runtimeKind).toBe('api')
  })
})
