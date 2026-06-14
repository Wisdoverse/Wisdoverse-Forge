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
    const info = managedToAgentInfo({
      ...base,
      cliTool: 'codex',
      runtimeKind: 'cli',
      runtimeId: 'host-x',
    } as any)
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

  it('uses beginner-facing service and model labels for known work tools', () => {
    const info = managedToAgentInfo({
      ...base,
      cliTool: 'codex',
      provider: null,
      model: null,
    } as any)

    expect(info.provider).toBe('OpenAI')
    expect(info.model).toBe('Codex')
  })

  it('uses display names for known AI service keys', () => {
    expect(
      managedToAgentInfo({
        ...base,
        provider: 'anthropic',
        model: 'claude-sonnet-4-6',
      } as any).provider
    ).toBe('Anthropic')
    expect(
      managedToAgentInfo({
        ...base,
        provider: 'openai_compatible',
        model: 'custom-model',
      } as any).provider
    ).toBe('OpenAI-compatible service')
  })

  it('uses beginner-facing labels when AI service and model are missing', () => {
    const info = managedToAgentInfo({
      ...base,
      provider: null,
      model: null,
      cliTool: null,
    } as any)

    expect(info.provider).toBe('Refresh AI service')
    expect(info.model).toBe('Model not reported')
    expect(info.provider).not.toBe('Unknown')
    expect(info.model).not.toBe('unknown')
  })

  it('does not expose unknown work tool slugs in service or model labels', () => {
    const info = managedToAgentInfo({
      ...base,
      cliTool: 'future_tool',
      provider: null,
      model: null,
    } as any)

    expect(info.provider).toBe('Work tool needs review')
    expect(info.model).toBe('Work tool needs review')
    expect(info.provider).not.toContain('future_tool')
    expect(info.model).not.toContain('future_tool')
  })

  it('does not expose unknown AI service slugs', () => {
    const info = managedToAgentInfo({
      ...base,
      provider: 'future_provider',
      model: 'future-model',
      cliTool: null,
    } as any)

    expect(info.provider).toBe('AI service needs review')
    expect(info.provider).not.toContain('future_provider')
    expect(info.model).toBe('future-model')
  })
})
