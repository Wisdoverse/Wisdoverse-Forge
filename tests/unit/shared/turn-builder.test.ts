import { describe, it, expect, beforeEach } from 'vitest'
import { buildTurns } from '@shared/turn-builder'
import type { ClaudeEvent } from '@shared/types/events'

// ============================================================================
// Helpers
// ============================================================================

const BASE = {
  sessionId: 'sess-1',
  cwd: '/workspace',
} as const

let idSeq = 0
function makeEvent<T extends ClaudeEvent['type']>(
  type: T,
  ts: number,
  extra: Record<string, unknown> = {}
): ClaudeEvent {
  return {
    id: `evt-${++idSeq}`,
    timestamp: ts,
    type,
    ...BASE,
    ...extra,
  } as unknown as ClaudeEvent
}

function userPrompt(ts: number, prompt = 'Hello'): ClaudeEvent {
  return makeEvent('user_prompt_submit', ts, { prompt })
}

function preTool(ts: number, tool = 'Bash', toolUseId?: string): ClaudeEvent {
  return makeEvent('pre_tool_use', ts, {
    tool,
    toolUseId: toolUseId || `tu-${idSeq}`,
    toolInput: { command: 'ls' },
  })
}

function postTool(ts: number, toolUseId: string, success = true): ClaudeEvent {
  return makeEvent('post_tool_use', ts, {
    tool: 'Bash',
    toolUseId,
    toolInput: { command: 'ls' },
    toolResponse: { output: 'file.ts' },
    success,
    duration: 100,
  })
}

function stop(ts: number, response = 'Done'): ClaudeEvent {
  return makeEvent('stop', ts, { stopHookActive: false, response })
}

function sessionStart(ts: number, source = 'startup' as const): ClaudeEvent {
  return makeEvent('session_start', ts, { source })
}

function sessionEnd(ts: number, reason = 'clear' as const): ClaudeEvent {
  return makeEvent('session_end', ts, { reason })
}

// ============================================================================
// Tests
// ============================================================================

describe('buildTurns', () => {
  beforeEach(() => {
    idSeq = 0
  })

  it('returns empty array for empty events', () => {
    const result = buildTurns([])
    expect(result.turns).toEqual([])
    expect(result.unknownEventTypeCount).toBe(0)
  })

  it('creates user + assistant turn pair from prompt + stop', () => {
    const events = [userPrompt(1000), stop(2000, 'Hello back')]

    const { turns } = buildTurns(events)

    expect(turns).toHaveLength(2)
    expect(turns[0].type).toBe('user')
    expect(turns[0].status).toBe('complete')
    expect(turns[0].prompt).toBe('Hello')
    expect(turns[0].sequence).toBe(1)

    expect(turns[1].type).toBe('assistant')
    expect(turns[1].status).toBe('complete')
    expect(turns[1].response).toBe('Hello back')
    expect(turns[1].sequence).toBe(2)
  })

  it('handles tool use within assistant turn', () => {
    const tuId = 'tu-bash-1'
    const events = [
      userPrompt(1000),
      preTool(1100, 'Bash', tuId),
      postTool(1200, tuId, true),
      stop(1300, 'Done'),
    ]

    const { turns } = buildTurns(events)

    expect(turns).toHaveLength(2)
    const assistant = turns[1]
    expect(assistant.status).toBe('complete')
    expect(assistant.steps).toHaveLength(1)
    expect(assistant.steps[0].toolName).toBe('Bash')
    expect(assistant.steps[0].status).toBe('complete')
    expect(assistant.steps[0].success).toBe(true)
    expect(assistant.steps[0].durationMs).toBe(100)
  })

  it('handles multiple tool calls in one turn', () => {
    const events = [
      userPrompt(1000),
      preTool(1100, 'Read', 'tu-1'),
      postTool(1200, 'tu-1'),
      preTool(1300, 'Edit', 'tu-2'),
      postTool(1400, 'tu-2'),
      preTool(1500, 'Bash', 'tu-3'),
      postTool(1600, 'tu-3'),
      stop(1700),
    ]

    const { turns } = buildTurns(events)
    const assistant = turns[1]
    expect(assistant.steps).toHaveLength(3)
    expect(assistant.steps.map((s) => s.toolName)).toEqual(['Read', 'Edit', 'Bash'])
  })

  it('handles failed tool call', () => {
    const events = [
      userPrompt(1000),
      preTool(1100, 'Bash', 'tu-fail'),
      postTool(1200, 'tu-fail', false),
      stop(1300),
    ]

    const { turns } = buildTurns(events)
    const step = turns[1].steps[0]
    expect(step.status).toBe('error')
    expect(step.success).toBe(false)
  })

  it('handles undefined success as complete (Codex events)', () => {
    const events = [
      userPrompt(1000),
      preTool(1100, 'Bash', 'tu-undef'),
      makeEvent('post_tool_use', 1200, {
        tool: 'Bash',
        toolUseId: 'tu-undef',
        toolInput: { command: 'echo hello' },
        toolResponse: { output: 'hello' },
        duration: 100,
      }),
    ]
    const { turns } = buildTurns(events)
    const step = turns[1].steps[0]
    expect(step.status).toBe('complete')
  })

  it('handles null success as complete', () => {
    const events = [
      userPrompt(1000),
      preTool(1100, 'Bash', 'tu-null'),
      makeEvent('post_tool_use', 1200, {
        tool: 'Bash',
        toolUseId: 'tu-null',
        toolInput: { command: 'ls' },
        toolResponse: { output: 'file.ts' },
        success: null,
        duration: 50,
      }),
    ]
    const { turns } = buildTurns(events)
    const step = turns[1].steps[0]
    expect(step.status).toBe('complete')
  })

  it('sorts events by timestamp before processing', () => {
    // Events in wrong order
    const events = [
      stop(2000, 'Done'),
      preTool(1100, 'Bash', 'tu-1'),
      userPrompt(1000),
      postTool(1200, 'tu-1'),
    ]

    const { turns } = buildTurns(events)

    expect(turns).toHaveLength(2)
    expect(turns[0].type).toBe('user')
    expect(turns[1].type).toBe('assistant')
    expect(turns[1].status).toBe('complete')
    expect(turns[1].steps).toHaveLength(1)
  })

  it('creates system turn for session_start', () => {
    const events = [sessionStart(1000, 'startup')]
    const { turns } = buildTurns(events)

    expect(turns).toHaveLength(1)
    expect(turns[0].type).toBe('system')
    expect(turns[0].response).toBe('Session startup')
  })

  it('creates system turn for session_end and interrupts open turn', () => {
    const events = [userPrompt(1000), preTool(1100, 'Bash', 'tu-1'), sessionEnd(1500)]

    const { turns } = buildTurns(events)

    // user + assistant (interrupted) + system (session_end)
    expect(turns).toHaveLength(3)
    expect(turns[1].type).toBe('assistant')
    expect(turns[1].status).toBe('interrupted')
    expect(turns[2].type).toBe('system')
  })

  it('handles multiple conversation rounds', () => {
    const events = [
      userPrompt(1000),
      stop(1100, 'Answer 1'),
      userPrompt(2000),
      stop(2100, 'Answer 2'),
    ]

    const { turns } = buildTurns(events)

    expect(turns).toHaveLength(4) // user1, assistant1, user2, assistant2
    expect(turns[0].prompt).toBe('Hello')
    expect(turns[1].response).toBe('Answer 1')
    expect(turns[2].prompt).toBe('Hello')
    expect(turns[3].response).toBe('Answer 2')
  })

  it('skips unknown event types with counter increment', () => {
    const events = [
      userPrompt(1000),
      { ...makeEvent('user_prompt_submit', 1500), type: 'some_future_event' } as unknown as ClaudeEvent,
      stop(2000),
    ]

    const { turns, unknownEventTypeCount } = buildTurns(events)

    expect(turns).toHaveLength(2) // user + assistant, unknown skipped
    expect(unknownEventTypeCount).toBe(1)
  })

  it('marks pending steps as timeout on turn finalization', () => {
    const events = [
      userPrompt(1000),
      preTool(1100, 'Bash', 'tu-pending'),
      // no post_tool_use
      stop(2000),
    ]

    const { turns } = buildTurns(events)
    const step = turns[1].steps[0]
    expect(step.status).toBe('timeout')
  })

  it('marks stale open turns as interrupted', () => {
    const now = 1000 + 200_000 // 200s after start (>2min)
    const events = [userPrompt(1000)]

    const { turns } = buildTurns(events, now)

    expect(turns[1].status).toBe('interrupted')
  })

  it('leaves active open turns as thinking', () => {
    const now = 1000 + 10_000 // 10s after start (<2min)
    const events = [userPrompt(1000)]

    const { turns } = buildTurns(events, now)

    expect(turns[1].status).toBe('thinking')
  })

  it('creates implicit assistant turn for orphan pre_tool_use', () => {
    const events = [preTool(1000, 'Bash', 'tu-orphan'), postTool(1100, 'tu-orphan'), stop(1200)]

    const { turns } = buildTurns(events)

    expect(turns).toHaveLength(1) // just the implicit assistant
    expect(turns[0].type).toBe('assistant')
    expect(turns[0].steps).toHaveLength(1)
  })

  it('identifies subagent tools', () => {
    const events = [userPrompt(1000), preTool(1100, 'Task', 'tu-task'), stop(2000)]

    const { turns } = buildTurns(events)
    expect(turns[1].steps[0].isSubagent).toBe(true)
  })

  it('extracts metadata from Read tool', () => {
    const events = [
      userPrompt(1000),
      makeEvent('pre_tool_use', 1100, {
        tool: 'Read',
        toolUseId: 'tu-read',
        toolInput: { file_path: '/src/index.ts' },
      }),
      stop(2000),
    ]

    const { turns } = buildTurns(events)
    expect(turns[1].steps[0].metadata?.filePath).toBe('/src/index.ts')
  })

  it('truncates long input text', () => {
    const longInput = 'x'.repeat(300)
    const events = [
      userPrompt(1000),
      makeEvent('pre_tool_use', 1100, {
        tool: 'Bash',
        toolUseId: 'tu-long',
        toolInput: { command: longInput },
      }),
      stop(2000),
    ]

    const { turns } = buildTurns(events)
    expect(turns[1].steps[0].input.length).toBeLessThanOrEqual(203) // 200 + '...'
  })

  it('handles session_start between conversation rounds', () => {
    const events = [
      sessionStart(500, 'startup'),
      userPrompt(1000),
      stop(1100, 'Answer'),
      sessionStart(2000, 'resume'),
      userPrompt(2500),
      stop(2600, 'Answer 2'),
    ]

    const { turns } = buildTurns(events)

    expect(turns).toHaveLength(6) // system, user, assistant, system, user, assistant
    expect(turns[0].type).toBe('system')
    expect(turns[1].type).toBe('user')
    expect(turns[2].type).toBe('assistant')
    expect(turns[3].type).toBe('system')
    expect(turns[3].response).toBe('Session resume')
  })

  it('calculates duration on completed turns', () => {
    const events = [userPrompt(1000), stop(3000)]

    const { turns } = buildTurns(events)
    const assistant = turns[1]
    expect(assistant.durationMs).toBe(2000)
    expect(assistant.completedAt).toBe(3000)
  })

  it('preserves assistantText as thinking', () => {
    const events = [
      userPrompt(1000),
      makeEvent('pre_tool_use', 1100, {
        tool: 'Bash',
        toolUseId: 'tu-think',
        toolInput: {},
        assistantText: 'Let me think about this...',
      }),
      stop(2000),
    ]

    const { turns } = buildTurns(events)
    expect(turns[1].thinking).toBe('Let me think about this...')
  })

  it('counts notification events in rawEventCount', () => {
    const events = [
      userPrompt(1000),
      makeEvent('notification', 1100, { message: 'Permission needed', notificationType: 'permission_prompt' }),
      stop(2000),
    ]

    const { turns } = buildTurns(events)
    // assistant turn: 1 (creation) + 1 (notification) = rawEventCount includes notification
    expect(turns[1].rawEventCount).toBeGreaterThanOrEqual(2)
  })

  it('counts text_stream events in rawEventCount without spawning a turn', () => {
    const events = [
      userPrompt(1000),
      makeEvent('text_stream', 1100, { text: 'partial token', delta: true }),
      makeEvent('text_stream', 1110, { text: 'more text', delta: true }),
      stop(2000),
    ]

    const { turns } = buildTurns(events)
    // text_stream must NOT create new turns — it's an in-progress signal
    expect(turns).toHaveLength(2)
    expect(turns[1].rawEventCount).toBeGreaterThanOrEqual(3)
  })

  it('handles pre_compact event without crashing', () => {
    const events = [
      userPrompt(1000),
      makeEvent('pre_compact', 1100, { trigger: 'auto' }),
      stop(2000),
    ]

    const { turns } = buildTurns(events)
    expect(turns).toHaveLength(2)
  })

  it('handles terminal_output event without crashing', () => {
    const events = [
      userPrompt(1000),
      makeEvent('terminal_output', 1100, { lines: ['output line'], context: 'general' }),
      stop(2000),
    ]

    const { turns } = buildTurns(events)
    expect(turns).toHaveLength(2)
  })

  it('preserves cliTool from events', () => {
    const events = [
      { ...userPrompt(1000), cliTool: 'claude' } as ClaudeEvent,
    ]

    const { turns } = buildTurns(events)
    expect(turns[0].cliTool).toBe('claude')
  })

  it('does not leak unknownEventTypeCount across calls', () => {
    const events1 = [
      { ...makeEvent('user_prompt_submit', 1000), type: 'unknown_1' } as unknown as ClaudeEvent,
    ]
    const events2 = [
      { ...makeEvent('user_prompt_submit', 1000), type: 'unknown_2' } as unknown as ClaudeEvent,
    ]

    const r1 = buildTurns(events1)
    const r2 = buildTurns(events2)

    expect(r1.unknownEventTypeCount).toBe(1)
    expect(r2.unknownEventTypeCount).toBe(1) // independent, not accumulated
  })
})
