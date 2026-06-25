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

  it('isTaskImageCapable requires container runtime, a vision CLI, AND the image_input token', () => {
    const cap = (runtimeKind: 'container' | 'cli' | 'api' | undefined, capabilities?: string[]) =>
      isTaskImageCapable({ runtimeKind, capabilities })
    // Container + vision CLI + sidecar image_input token → allowed (case-insensitive).
    expect(cap('container', ['claude', 'image_input'])).toBe(true)
    expect(cap('container', ['codex', 'image_input'])).toBe(true)
    expect(cap('container', ['gemini', 'image_input'])).toBe(true)
    expect(cap('container', ['Claude', 'IMAGE_INPUT'])).toBe(true)
    // Vision CLI but NO image_input token (old/stale sidecar) → excluded, matching
    // the server dispatch gate so the user isn't offered an upload that fails.
    expect(cap('container', ['claude'])).toBe(false)
    // Host CLI reports the same tools but is off-host — excluded.
    expect(cap('cli', ['claude', 'image_input'])).toBe(false)
    // opencode has no vision; API/chat agents report no CLI tool.
    expect(cap('container', ['opencode', 'image_input'])).toBe(false)
    expect(cap('api', ['image_input'])).toBe(false)
    // Missing runtime kind (older server) degrades safely to hidden.
    expect(cap(undefined, ['claude', 'image_input'])).toBe(false)
    expect(isTaskImageCapable(undefined)).toBe(false)
  })

  it('does not expose unknown runtime kind slugs', () => {
    expect(runtimeKindLabel('future_runtime' as never)).toBe('Check work location')
    expect(runtimeKindShortLabel('future_runtime' as never)).toBe('Check location')
    expect(runtimeKindLabel('future_runtime' as never)).not.toContain('future_runtime')
    expect(runtimeKindShortLabel('future_runtime' as never)).not.toContain('future_runtime')
  })
})
