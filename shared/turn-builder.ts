/**
 * TurnBuilder — Shared Turn projection from ClaudeEvents
 *
 * Used by both server (Turn API) and client (real-time updates).
 * Converts a flat event stream into structured Turns with steps.
 *
 * Design: defensive default:break (unknown events skip, never crash),
 * explicit timestamp sort before processing.
 */

import type { ClaudeEvent } from './types/events.js'

// ============================================================================
// Types
// ============================================================================

export type TurnStatus = 'pending' | 'thinking' | 'tool_use' | 'complete' | 'error' | 'interrupted'
export type TurnStepStatus = 'pending' | 'complete' | 'error' | 'timeout'

export interface TurnStep {
  id: string // tool_use_id
  toolName: string
  input: string // truncated max 200 chars
  output?: string // truncated max 500 chars
  hasFullContent: true // always true when events exist
  success?: boolean
  durationMs?: number
  startedAt: number
  status: TurnStepStatus
  isSubagent: boolean
  metadata?: StepMetadata
}

export interface StepMetadata {
  filePath?: string
  command?: string
  language?: string
}

export interface Turn {
  id: string
  sessionId: string
  sequence: number
  type: 'user' | 'assistant' | 'system' | 'worker'
  status: TurnStatus
  prompt?: string
  images?: string[]
  thinking?: string
  response?: string
  steps: TurnStep[]
  cliTool?: string
  tokenUsage?: { input: number; output: number }
  startedAt: number
  completedAt?: number
  durationMs?: number
  rawEventCount: number
  isFavorite?: boolean
}

// ============================================================================
// API Response Types
// ============================================================================

export interface TurnPageResponse {
  turns: Turn[]
  cursor: string | null // null = no more pages
  hasMore: boolean
  totalTurnCount: number // approximate, for UI progress
  lastEvent?: { timestamp: string; id: string } // watermark for CQRS handoff
}

export type StepContentResult =
  | { status: 'ok'; fullInput: string; fullOutput: string }
  | { status: 'not_found' }
  | { status: 'error'; message: string }

export interface BatchContentResponse {
  results: Record<string, StepContentResult>
}

// ============================================================================
// Client Cache Types
// ============================================================================

export type ContentCacheEntry =
  | { type: 'loaded'; fullInput: string; fullOutput: string }
  | { type: 'not_found'; cachedAt: number } // tombstone, expire after 5min
  | { type: 'error'; cachedAt: number } // error tombstone, expire after 30s

// ============================================================================
// Constants
// ============================================================================

const MAX_INPUT_PREVIEW = 200
const MAX_OUTPUT_PREVIEW = 500
const SUBAGENT_TOOLS = new Set(['Task', 'dispatch_agent'])
const KNOWN_EVENT_TYPES = new Set([
  'user_prompt_submit',
  'pre_tool_use',
  'post_tool_use',
  'stop',
  'subagent_stop',
  'session_start',
  'session_end',
  'notification',
  'pre_compact',
  'terminal_output',
  'text_stream',
])

// ============================================================================
// Build Result
// ============================================================================

export interface BuildTurnsResult {
  turns: Turn[]
  unknownEventTypeCount: number
  deduplicatedEventCount: number
  /** Maps event.id → turn.id for persist-time turn_id assignment */
  eventTurnMap: Map<string, string>
}

// ============================================================================
// TurnBuilder
// ============================================================================

export function buildTurns(events: ClaudeEvent[], now: number = Date.now()): BuildTurnsResult {
  // Explicit sort by timestamp ASC (R7): handles WAL replay interleave,
  // consumer NAK+retry reorder, and Redis PubSub vs NATS timing
  const sorted = [...events].sort((a, b) => a.timestamp - b.timestamp)

  // Deduplicate by event ID (defense-in-depth against DB-level duplicate events)
  const seen = new Set<string>()
  const deduped = sorted.filter((e) => {
    if (seen.has(e.id)) return false
    seen.add(e.id)
    return true
  })

  const ctx: BuildContext = { turns: [], currentTurn: null, sequence: 0, unknownEventTypeCount: 0 }
  const eventTurnMap = new Map<string, string>()

  for (const event of deduped) {
    if (!KNOWN_EVENT_TYPES.has(event.type)) {
      ctx.unknownEventTypeCount++
      continue
    }
    processEvent(ctx, event)
    if (ctx.currentTurn) {
      eventTurnMap.set(event.id, ctx.currentTurn.id)
    }
    // For user_prompt_submit, the event creates a user turn (id=event.id) then
    // sets currentTurn to the assistant turn. Override the mapping to point at
    // the user turn, since this event belongs to the user turn.
    if (event.type === 'user_prompt_submit') {
      eventTurnMap.set(event.id, event.id)
    }
  }

  // Mark any still-open assistant turn as pending/thinking
  if (ctx.currentTurn?.type === 'assistant') {
    if (ctx.currentTurn.status !== 'complete' && ctx.currentTurn.status !== 'error') {
      const lastActivity = getLastActivityTime(ctx.currentTurn, now)
      if (now - lastActivity > 120_000) {
        ctx.currentTurn.status = 'interrupted'
        finalizeTurn(ctx.currentTurn, lastActivity)
      }
    }
  }

  const deduplicatedEventCount = sorted.length - deduped.length
  return {
    turns: ctx.turns,
    unknownEventTypeCount: ctx.unknownEventTypeCount,
    deduplicatedEventCount,
    eventTurnMap,
  }
}

// ============================================================================
// Event Processing (extracted to reduce buildTurns complexity)
// ============================================================================

interface BuildContext {
  turns: Turn[]
  currentTurn: Turn | null
  sequence: number
  unknownEventTypeCount: number
}

type PromptEvent = Extract<ClaudeEvent, { type: 'user_prompt_submit' }>
type PreToolEvent = Extract<ClaudeEvent, { type: 'pre_tool_use' }>
type PostToolEvent = Extract<ClaudeEvent, { type: 'post_tool_use' }>
type StopEventType = Extract<ClaudeEvent, { type: 'stop' }>
type SessionStartEventType = Extract<ClaudeEvent, { type: 'session_start' }>
type SessionEndEventType = Extract<ClaudeEvent, { type: 'session_end' }>

function processEvent(ctx: BuildContext, event: ClaudeEvent): void {
  switch (event.type) {
    case 'user_prompt_submit':
      handlePromptSubmit(ctx, event)
      break
    case 'pre_tool_use':
      handlePreToolUse(ctx, event)
      break
    case 'post_tool_use':
      handlePostToolUse(ctx, event)
      break
    case 'stop':
      handleStop(ctx, event)
      break
    case 'session_start':
      handleSessionStart(ctx, event)
      break
    case 'session_end':
      handleSessionEnd(ctx, event)
      break
    case 'subagent_stop':
    case 'notification':
    case 'pre_compact':
    case 'terminal_output':
    case 'text_stream':
      if (ctx.currentTurn?.type === 'assistant') ctx.currentTurn.rawEventCount++
      break
    default:
      break
  }
}

function handlePromptSubmit(ctx: BuildContext, event: PromptEvent): void {
  if (
    ctx.currentTurn &&
    ctx.currentTurn.status !== 'complete' &&
    ctx.currentTurn.status !== 'error'
  ) {
    finalizeTurn(ctx.currentTurn, event.timestamp)
  }

  const userTurn = createTurn({
    id: event.id,
    sessionId: event.sessionId ?? '',
    sequence: ++ctx.sequence,
    type: 'user',
    status: 'complete',
    prompt: event.prompt,
    startedAt: event.timestamp,
    completedAt: event.timestamp,
    cliTool: event.cliTool,
  })
  ctx.turns.push(userTurn)

  ctx.currentTurn = createTurn({
    id: `${event.id}-assistant`,
    sessionId: event.sessionId ?? '',
    sequence: ++ctx.sequence,
    type: 'assistant',
    status: 'thinking',
    startedAt: event.timestamp,
    cliTool: event.cliTool,
  })
  ctx.turns.push(ctx.currentTurn)
}

function handlePreToolUse(ctx: BuildContext, event: PreToolEvent): void {
  if (ctx.currentTurn?.type !== 'assistant') {
    ctx.currentTurn = createTurn({
      id: `${event.id}-assistant`,
      sessionId: event.sessionId ?? '',
      sequence: ++ctx.sequence,
      type: 'assistant',
      status: 'tool_use',
      startedAt: event.timestamp,
      cliTool: event.cliTool,
    })
    ctx.turns.push(ctx.currentTurn)
  }

  ctx.currentTurn.status = 'tool_use'
  if (event.assistantText && !ctx.currentTurn.thinking) {
    ctx.currentTurn.thinking = event.assistantText
  }

  ctx.currentTurn.steps.push({
    id: event.toolUseId,
    toolName: event.tool,
    input: truncateText(stringifyInput(event.toolInput), MAX_INPUT_PREVIEW),
    hasFullContent: true,
    startedAt: event.timestamp,
    status: 'pending',
    isSubagent: SUBAGENT_TOOLS.has(event.tool),
    metadata: extractStepMetadata(event.tool, event.toolInput),
  })
  ctx.currentTurn.rawEventCount++
}

function handlePostToolUse(ctx: BuildContext, event: PostToolEvent): void {
  if (ctx.currentTurn?.type !== 'assistant') return

  const step = ctx.currentTurn.steps.find((s) => s.id === event.toolUseId)
  if (step) {
    step.output = truncateText(stringifyOutput(event.toolResponse), MAX_OUTPUT_PREVIEW)
    step.success = event.success
    step.durationMs = event.duration
    step.status = event.success === false ? 'error' : 'complete'
  }
  ctx.currentTurn.rawEventCount++
}

function handleStop(ctx: BuildContext, event: StopEventType): void {
  if (ctx.currentTurn?.type === 'assistant') {
    if (event.response) ctx.currentTurn.response = event.response
    finalizeTurn(ctx.currentTurn, event.timestamp)
  }
}

function handleSessionStart(ctx: BuildContext, event: SessionStartEventType): void {
  if (
    ctx.currentTurn &&
    ctx.currentTurn.status !== 'complete' &&
    ctx.currentTurn.status !== 'error'
  ) {
    finalizeTurn(ctx.currentTurn, event.timestamp)
  }

  ctx.turns.push(
    createTurn({
      id: event.id,
      sessionId: event.sessionId ?? '',
      sequence: ++ctx.sequence,
      type: 'system',
      status: 'complete',
      response: `Session ${event.source}`,
      startedAt: event.timestamp,
      completedAt: event.timestamp,
    })
  )
  ctx.currentTurn = null
}

function handleSessionEnd(ctx: BuildContext, event: SessionEndEventType): void {
  if (
    ctx.currentTurn &&
    ctx.currentTurn.status !== 'complete' &&
    ctx.currentTurn.status !== 'error'
  ) {
    ctx.currentTurn.status = 'interrupted'
    finalizeTurn(ctx.currentTurn, event.timestamp)
  }

  ctx.turns.push(
    createTurn({
      id: event.id,
      sessionId: event.sessionId ?? '',
      sequence: ++ctx.sequence,
      type: 'system',
      status: 'complete',
      response: `Session ended: ${event.reason}`,
      startedAt: event.timestamp,
      completedAt: event.timestamp,
    })
  )
  ctx.currentTurn = null
}

// ============================================================================
// Helpers
// ============================================================================

function createTurn(
  partial: Partial<Turn> & {
    id: string
    sessionId: string
    sequence: number
    type: Turn['type']
    status: TurnStatus
    startedAt: number
  }
): Turn {
  return {
    steps: [],
    rawEventCount: 1,
    ...partial,
  }
}

function finalizeTurn(turn: Turn, timestamp: number): void {
  if (turn.status === 'thinking' || turn.status === 'tool_use') {
    turn.status = 'complete'
  }
  // Mark any pending steps as timeout
  for (const step of turn.steps) {
    if (step.status === 'pending') {
      step.status = 'timeout'
    }
  }
  turn.completedAt = timestamp
  turn.durationMs = timestamp - turn.startedAt
}

function getLastActivityTime(turn: Turn, fallback: number): number {
  if (turn.steps.length > 0) {
    const lastStep = turn.steps[turn.steps.length - 1]
    if (lastStep.durationMs) {
      return lastStep.startedAt + lastStep.durationMs
    }
    return lastStep.startedAt
  }
  return turn.startedAt || fallback
}

function truncateText(text: string, maxLen: number): string {
  if (text.length <= maxLen) return text
  return `${text.slice(0, maxLen)}...`
}

function stringifyInput(input: unknown): string {
  if (typeof input === 'string') return input
  if (input == null) return ''
  try {
    return JSON.stringify(input)
  } catch {
    return String(input)
  }
}

function stringifyOutput(output: unknown): string {
  if (typeof output === 'string') return output
  if (output == null) return ''
  try {
    return JSON.stringify(output)
  } catch {
    return String(output)
  }
}

function extractStepMetadata(
  tool: string,
  input: Record<string, unknown>
): StepMetadata | undefined {
  switch (tool) {
    case 'Read':
    case 'Write':
    case 'Edit':
    case 'Glob':
      return input.file_path ? { filePath: String(input.file_path) } : undefined
    case 'Bash':
      return input.command ? { command: String(input.command) } : undefined
    case 'Grep':
      return input.pattern ? { command: `grep: ${String(input.pattern)}` } : undefined
    default:
      return undefined
  }
}
