/**
 * Event Types
 *
 * Core event types, tool events, lifecycle events, user events,
 * and the ClaudeEvent union type.
 */

// ============================================================================
// Core Event Types
// ============================================================================

export type HookEventType =
  | 'pre_tool_use'
  | 'permission_request'
  | 'post_tool_use'
  | 'stop'
  | 'subagent_stop'
  | 'session_start'
  | 'session_end'
  | 'user_prompt_submit'
  | 'notification'
  | 'pre_compact'
  | 'terminal_output'
  | 'text_stream'

export type ToolName =
  | 'Read'
  | 'Write'
  | 'Edit'
  | 'Bash'
  | 'Grep'
  | 'Glob'
  | 'WebFetch'
  | 'WebSearch'
  | 'Task'
  | 'TodoWrite'
  | 'AskUserQuestion'
  | 'NotebookEdit'
  | string // MCP tools and future tools

// ============================================================================
// Base Event
// ============================================================================

export interface BaseEvent {
  /** Unique event ID */
  id: string
  /** Unix timestamp in milliseconds */
  timestamp: number
  /** Event type */
  type: HookEventType
  /** Claude Code session ID (null when event predates CLI session attach) */
  sessionId: string | null
  /** Current working directory */
  cwd: string
  /** Platform runtime identifier for session auto-linking */
  runtimeId?: string
  /** Organization ID for tenant isolation (set by server) */
  orgId?: string
  /** Managed-agent ID (server mapper emits this from `events.agent_id`).
   *  Use as a stable per-agent fallback key when `sessionId` is null —
   *  collapsing null-session events onto the empty string merges multiple
   *  agents into one bucket and can misattribute history via auto-link. */
  agentId?: string
  /** CLI tool that produced this event (injected by WebSocket gateway) */
  cliTool?: 'claude' | 'opencode' | 'codex' | 'gemini'
  /** Turn ID assigned during event persistence (CQRS) */
  turnId?: string
}

// ============================================================================
// Tool Events
// ============================================================================

export interface PreToolUseEvent extends BaseEvent {
  type: 'pre_tool_use'
  tool: ToolName
  toolInput: Record<string, unknown>
  toolUseId: string
  /** Assistant text that came before this tool call */
  assistantText?: string
}

export interface PostToolUseEvent extends BaseEvent {
  type: 'post_tool_use'
  tool: ToolName
  toolInput: Record<string, unknown>
  toolResponse: Record<string, unknown>
  toolUseId: string
  success: boolean
  /** Duration in milliseconds (calculated from matching pre_tool_use) */
  duration?: number
}

export interface PermissionRequestEvent extends BaseEvent {
  type: 'permission_request'
  tool: ToolName
  toolInput: Record<string, unknown>
  description?: string
}

// ============================================================================
// Lifecycle Events
// ============================================================================

export interface StopEvent extends BaseEvent {
  type: 'stop'
  stopHookActive: boolean
  /** Claude's text response (extracted from transcript) */
  response?: string
}

export interface SubagentStopEvent extends BaseEvent {
  type: 'subagent_stop'
  stopHookActive: boolean
  response?: string
}

export interface AgentStartEvent extends BaseEvent {
  type: 'session_start'
  source: 'startup' | 'resume' | 'clear' | 'compact'
}

export interface AgentEndEvent extends BaseEvent {
  type: 'session_end'
  reason: 'clear' | 'logout' | 'prompt_input_exit' | 'other'
}

// ============================================================================
// User Interaction Events
// ============================================================================

export interface UserPromptSubmitEvent extends BaseEvent {
  type: 'user_prompt_submit'
  prompt: string
}

export interface NotificationEvent extends BaseEvent {
  type: 'notification'
  message: string
  notificationType?:
    | 'permission_prompt'
    | 'idle_prompt'
    | 'auth_success'
    | 'elicitation_dialog'
    | string
}

// ============================================================================
// Other Events
// ============================================================================

export interface PreCompactEvent extends BaseEvent {
  type: 'pre_compact'
  trigger?: 'manual' | 'auto'
  customInstructions?: string
}

export interface TerminalOutputEvent extends BaseEvent {
  type: 'terminal_output'
  /** Terminal output lines (ANSI-stripped) */
  lines: string[]
  /** Context: builtin_command when triggered by /login etc., general otherwise */
  context: 'builtin_command' | 'general'
}

/**
 * Streaming text from the LLM (assistant tokens).
 *
 * Distinct from `notification` so the activity feed and timeline can suppress
 * token spam while ChatView renders incremental text. Issue #34.
 */
export interface TextStreamEvent extends BaseEvent {
  type: 'text_stream'
  /** Text chunk — either an incremental delta or a complete chunk */
  text: string
  /** True for incremental token delta, false/undefined for a complete chunk */
  delta?: boolean
  /** Turn correlation, when known */
  turnId?: string
}

// ============================================================================
// Union Type
// ============================================================================

export type ClaudeEvent =
  | PreToolUseEvent
  | PermissionRequestEvent
  | PostToolUseEvent
  | StopEvent
  | SubagentStopEvent
  | AgentStartEvent
  | AgentEndEvent
  | UserPromptSubmitEvent
  | NotificationEvent
  | PreCompactEvent
  | TerminalOutputEvent
  | TextStreamEvent
