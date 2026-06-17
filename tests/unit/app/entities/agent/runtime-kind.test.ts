import { describe, expect, it } from 'vitest'
import {
  isApiAgent,
  isContainerAgent,
  isHostCliAgent,
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
    expect(runtimeKindLabel('api')).toBe('Chat-only AI service')
    expect(runtimeKindShortLabel('api')).toBe('Chat-only')
  })

  it('uses result-facing labels for agents that can edit project files', () => {
    expect(runtimeKindLabel('container')).toBe('Project files')
    expect(runtimeKindShortLabel('container')).toBe('Project files')
  })

  it('uses beginner-facing labels when runtime kind is missing', () => {
    expect(runtimeKindLabel(undefined)).toBe('Refresh work location')
    expect(runtimeKindShortLabel(undefined)).toBe('Refresh location')
  })

  it('does not expose unknown runtime kind slugs', () => {
    expect(runtimeKindLabel('future_runtime' as never)).toBe('Check work location')
    expect(runtimeKindShortLabel('future_runtime' as never)).toBe('Review location')
    expect(runtimeKindLabel('future_runtime' as never)).not.toContain('future_runtime')
    expect(runtimeKindShortLabel('future_runtime' as never)).not.toContain('future_runtime')
  })
})
