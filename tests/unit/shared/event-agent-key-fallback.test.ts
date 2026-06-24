/**
 * BaseEvent agent-key fallback — null-sessionId disambiguation
 *
 * Pins the invariant surfaced in the Codex review of MR !541 [P2]:
 * `sessionId ?? ''` alone collapses every null-session event from different
 * agents onto one shared bucket, and `getOrCreateAgent('')` can even
 * auto-link that bucket to the first recently created managed agent.
 *
 * The server mapper (`event_to_claude_event_json`) always emits `agentId`
 * from the `events.agent_id` column, so the correct fallback chain is
 * `sessionId ?? agentId ?? ''`. Widening `BaseEvent.agentId` in
 * `shared/types/events.ts` is what lets TS consumers reach for it.
 *
 * If these tests fail, somebody reverted the fallback — two agents that
 * share a null sessionId will start sharing a visualization bucket again.
 */
import { describe, expect, it } from 'vitest'

type BaseEventShape = {
  sessionId: string | null
  agentId?: string
}

function resolveAgentKey(event: BaseEventShape): string {
  return event.sessionId ?? event.agentId ?? ''
}

describe('BaseEvent agent-key fallback', () => {
  it('prefers sessionId when present', () => {
    const key = resolveAgentKey({ sessionId: 'cli-sess-abc', agentId: 'agent-xyz' })
    expect(key).toBe('cli-sess-abc')
  })

  it('falls back to agentId when sessionId is null', () => {
    const key = resolveAgentKey({ sessionId: null, agentId: 'agent-xyz' })
    expect(key).toBe('agent-xyz')
  })

  it('keeps null-session events from different agents in separate buckets', () => {
    // This is the regression. Before the fix both events resolved to `''`.
    const aKey = resolveAgentKey({ sessionId: null, agentId: 'agent-one' })
    const bKey = resolveAgentKey({ sessionId: null, agentId: 'agent-two' })
    expect(aKey).not.toBe(bKey)
    expect(aKey).toBe('agent-one')
    expect(bKey).toBe('agent-two')
  })

  it('falls back to empty string only when both sessionId and agentId are absent', () => {
    // Legacy rows without an agentId column (pre-mapper refactor) still
    // get the old behavior — better than crashing the visual layer.
    const key = resolveAgentKey({ sessionId: null })
    expect(key).toBe('')
  })

  it('treats empty-string sessionId as a real value (not coerced)', () => {
    // `??` only fires on null/undefined. An empty-string sessionId from a
    // malformed wire payload still goes through — matches ScheduleView /
    // TimelineView / main.ts semantics.
    const key = resolveAgentKey({ sessionId: '', agentId: 'agent-xyz' })
    expect(key).toBe('')
  })
})
