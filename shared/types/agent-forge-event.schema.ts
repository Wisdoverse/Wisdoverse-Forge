// shared/types/agent-forge-event.schema.ts
//
// Unified event contract v1 — all CLI adapters must produce events
// conforming to this schema before publishing to NATS.

export interface AgentForgeEventBase {
  schemaVersion: 1
  id: string
  type: AgentForgeEventType
  sessionId: string
  orgId: string
  timestamp: number
  cwd: string
  runtimeId: string
  cliTool: CliToolType
  sourceType: EventSourceType
}

export type AgentForgeEventType =
  | 'user_prompt_submit'
  | 'pre_tool_use'
  | 'permission_request'
  | 'post_tool_use'
  | 'stop'
  | 'subagent_stop'
  | 'session_start'
  | 'session_end'
  | 'notification'
  | 'pre_compact'
  | 'terminal_output'

export type CliToolType = 'claude' | 'gemini' | 'codex' | 'opencode'

export type EventSourceType = 'native-hook' | 'rollout-watcher' | 'notify-adapter' | 'exec-json'

export interface UserPromptSubmitEvent extends AgentForgeEventBase {
  type: 'user_prompt_submit'
  prompt: string
}

export interface PreToolUseEvent extends AgentForgeEventBase {
  type: 'pre_tool_use'
  tool: string
  toolUseId: string
  toolInput: Record<string, unknown>
  assistantText?: string
}

export interface PostToolUseEvent extends AgentForgeEventBase {
  type: 'post_tool_use'
  tool: string
  toolUseId: string
  toolInput?: Record<string, unknown>
  toolResponse: unknown
  success: boolean | null
  durationMs?: number
}

export interface PermissionRequestEvent extends AgentForgeEventBase {
  type: 'permission_request'
  tool: string
  toolInput: Record<string, unknown>
  description?: string
}

export interface StopEvent extends AgentForgeEventBase {
  type: 'stop'
  response: string
  stopReason?: 'end_turn' | 'max_tokens' | 'stop_sequence' | 'tool_use'
}

export interface SubagentStopEvent extends AgentForgeEventBase {
  type: 'subagent_stop'
  response: string
}

export interface SessionStartEvent extends AgentForgeEventBase {
  type: 'session_start'
}

export interface SessionEndEvent extends AgentForgeEventBase {
  type: 'session_end'
}

export interface NotificationEvent extends AgentForgeEventBase {
  type: 'notification'
  message: string
}

export interface PreCompactEvent extends AgentForgeEventBase {
  type: 'pre_compact'
}

export interface TerminalOutputEvent extends AgentForgeEventBase {
  type: 'terminal_output'
}

export type AgentForgeEvent =
  | UserPromptSubmitEvent
  | PreToolUseEvent
  | PermissionRequestEvent
  | PostToolUseEvent
  | StopEvent
  | SubagentStopEvent
  | SessionStartEvent
  | SessionEndEvent
  | NotificationEvent
  | PreCompactEvent
  | TerminalOutputEvent
