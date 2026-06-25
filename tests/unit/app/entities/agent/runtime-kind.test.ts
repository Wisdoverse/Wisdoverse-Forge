import { describe, expect, it } from 'vitest'
import {
  isApiAgent,
  isContainerAgent,
  isHostCliAgent,
  isTaskImageCapable,
  runtimeKindLabel,
  runtimeKindShortLabel,
} from '@app/entities/agent'

describe('runtime-kind specifications', () => {
  it('isHostCliAgent matches only runtimeKind="cli"', () => {
    expect(isHostCliAgent({ runtimeKind: 'cli' })).toBe(true)
    expect(isHostCliAgent({ runtimeKind: 'container' })).toBe(false)
    expect(isHostCliAgent({ runtimeKind: 'api' })).toBe(false)
  })

  it('no prefix fallback: runtimeKind="container" returns false regardless of runtimeId', () => {
    expect(isHostCliAgent({ runtimeKind: 'container' as const })).toBe(false)
  })

  it('undefined runtimeKind during rolling-deploy returns false for all three', () => {
    const u = { runtimeKind: undefined }
    expect(isHostCliAgent(u)).toBe(false)
    expect(isContainerAgent(u)).toBe(false)
    expect(isApiAgent(u)).toBe(false)
  })

  it('isContainerAgent exhaustive', () => {
    expect(isContainerAgent({ runtimeKind: 'container' })).toBe(true)
    expect(isContainerAgent({ runtimeKind: 'cli' })).toBe(false)
    expect(isContainerAgent({ runtimeKind: 'api' })).toBe(false)
  })

  it('isApiAgent exhaustive', () => {
    expect(isApiAgent({ runtimeKind: 'api' })).toBe(true)
    expect(isApiAgent({ runtimeKind: 'cli' })).toBe(false)
    expect(isApiAgent({ runtimeKind: 'container' })).toBe(false)
  })

  it('uses beginner-facing labels for chat-only agents', () => {
    expect(runtimeKindLabel('api')).toBe('Simple chat agent')
    expect(runtimeKindShortLabel('api')).toBe('Questions only')
  })

  it('uses result-facing labels for agents that can edit project files', () => {
    expect(runtimeKindLabel('container')).toBe('Project files')
    expect(runtimeKindShortLabel('container')).toBe('Project files')
  })

  it('uses beginner-facing labels when runtime kind is missing', () => {
    expect(runtimeKindLabel(undefined)).toBe('Check where it works')
    expect(runtimeKindShortLabel(undefined)).toBe('Check location')
  })

  it('isTaskImageCapable requires a container runtime AND a vision-capable CLI', () => {
    const cap = (runtimeKind: 'container' | 'cli' | 'api' | undefined, capabilities?: string[]) =>
      isTaskImageCapable({ runtimeKind, capabilities })
    // Container + vision CLI → allowed (case-insensitive).
    expect(cap('container', ['claude'])).toBe(true)
    expect(cap('container', ['codex'])).toBe(true)
    expect(cap('container', ['gemini'])).toBe(true)
    expect(cap('container', ['Claude'])).toBe(true)
    // Host CLI reports the same tool but is off-host — excluded.
    expect(cap('cli', ['claude'])).toBe(false)
    // opencode has no vision; API/chat agents report no CLI tool.
    expect(cap('container', ['opencode'])).toBe(false)
    expect(cap('api', [])).toBe(false)
    // Missing runtime kind (older server) degrades safely to hidden.
    expect(cap(undefined, ['claude'])).toBe(false)
    expect(isTaskImageCapable(undefined)).toBe(false)
  })

  it('does not expose unknown runtime kind slugs', () => {
    expect(runtimeKindLabel('future_runtime' as never)).toBe('Check work location')
    expect(runtimeKindShortLabel('future_runtime' as never)).toBe('Check location')
    expect(runtimeKindLabel('future_runtime' as never)).not.toContain('future_runtime')
    expect(runtimeKindShortLabel('future_runtime' as never)).not.toContain('future_runtime')
  })
})
