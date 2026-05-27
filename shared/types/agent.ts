/**
 * Agent & Orchestration Types
 *
 * Agent management, group orchestration, text tiles, and related types.
 */

import type { CliTool } from './tools.js'

// ============================================================================
// Agent Management (Orchestration)
// ============================================================================

/** Status of a managed agent */
export type AgentStatus = 'idle' | 'working' | 'waiting' | 'attention' | 'offline'

/**
 * Canonical runtime kind discriminator for a managed agent.
 *
 * - 'container' — runs inside a managed workspace container.
 * - 'cli'       — enrolled on an operator machine (host CLI / sidecar).
 * - 'api'       — provider+prompt agent with no container.
 *
 * Optional for one release cycle to support rolling deploys where old server
 * responses do not yet send this field.
 */
export type AgentRuntimeKind = 'container' | 'cli' | 'api'

/** A managed agent */
export interface ManagedAgent {
  /** Our internal ID (UUID) */
  id: string
  /** User-friendly name ("Frontend", "Tests") */
  name: string
  /** Platform runtime identifier for the agent (format: 'af-{id-prefix}') */
  runtimeId?: string | null
  /** Actual Docker/K8s container hash (may be undefined if container not yet started) */
  containerId?: string
  /** Current status */
  status: AgentStatus
  /** CLI session ID (from events, may differ from our ID) */
  cliSessionId?: string
  /** Creation timestamp */
  createdAt: number
  /** Last activity timestamp */
  lastActivity: number
  /** Working directory */
  cwd?: string
  /** Current tool being used (if working) */
  currentTool?: string
  /** Token count for this agent */
  tokens?: {
    current: number
    cumulative: number
  }
  /** Git status for this agent's working directory */
  gitStatus?: GitStatus
  /** Zone position in hex grid (for layout persistence) */
  zonePosition?: {
    q: number
    r: number
  }
  /** Agent group ID (undefined = standalone) */
  groupId?: string
  /** Role in the group */
  groupRole?: GroupRole
  /** CLI tool used for this agent */
  cliTool?: CliTool
  /**
   * Runtime kind discriminator. Optional for one release cycle; required next
   * release. Frontend derives from cliTool + runtimeId when absent.
   */
  runtimeKind?: AgentRuntimeKind
  /** Provider key for provider+prompt agents. CLI agents: null. */
  provider?: string | null
  /** Model name for provider+prompt agents. CLI agents: null. */
  model?: string | null
  /** Owner's user ID */
  userId?: string
  /** Organization ID (for org-scoped filtering) */
  orgId?: string
  /** Workspace execution/access boundary */
  workspaceId?: string
  /** Workspace display name */
  workspaceName?: string
  /** Primary project ID for task routing and UI context */
  projectId?: string
  /** Primary project display name */
  projectName?: string
  /** Owner's username (populated by active_all scope; NOT present in agent_update broadcasts) */
  ownerUsername?: string
  /** Owner's email (populated by active_all scope; NOT present in agent_update broadcasts) */
  ownerEmail?: string
  /** Provider+prompt agent system prompt. Null for CLI-container agents. */
  systemPrompt?: string | null
}

/** Git repository status */
export interface GitStatus {
  /** Current branch name */
  branch: string
  /** Commits ahead of upstream */
  ahead: number
  /** Commits behind upstream */
  behind: number
  /** Staged file counts */
  staged: {
    added: number
    modified: number
    deleted: number
  }
  /** Unstaged file counts */
  unstaged: {
    added: number
    modified: number
    deleted: number
  }
  /** Untracked file count */
  untracked: number
  /** Total changed files (staged + unstaged + untracked) */
  totalFiles: number
  /** Lines added (staged + unstaged) */
  linesAdded: number
  /** Lines removed (staged + unstaged) */
  linesRemoved: number
  /** Last commit timestamp (unix seconds) */
  lastCommitTime: number | null
  /** Last commit message (first line) */
  lastCommitMessage: string | null
  /** Whether directory is a git repo */
  isRepo: boolean
  /** Last time we checked (unix ms) */
  lastChecked: number
}

/** Workspace project metadata returned by GET /workspace/projects */
export interface WorkspaceProject {
  /** Directory name */
  name: string
  /** Display path (~/projects/...), NOT a system-resolvable path */
  path: string
  /** Whether .git directory exists */
  hasGit: boolean
  /** Current branch from .git/HEAD (only present when hasGit is true) */
  gitBranch?: string
  /** Last modified time (unix epoch milliseconds) */
  lastModified: number
  /** Number of direct children (files + directories) */
  childCount: number
}

/** Known project directory for autocomplete */
export interface KnownProject {
  /** Absolute path to the directory */
  path: string
  /** Display name (defaults to directory basename) */
  name: string
  /** Last time this project was used (unix ms) */
  lastUsed: number
  /** Number of times this project has been opened */
  useCount: number
}

/** Request to create a new agent */
export interface CreateAgentRequest {
  name?: string
  cwd?: string
  /** Claude command flags */
  flags?: {
    continue?: boolean // -c (continue last conversation)
    skipPermissions?: boolean // --dangerously-skip-permissions
    chrome?: boolean // --chrome
  }
  /** CLI tool to use (default: 'claude') */
  cliTool?: CliTool
  /** Project scope for navigation, permissions, and filtering */
  projectId?: string
  /** Workspace execution/access boundary. If omitted, inferred from projectId or tenant default. */
  workspaceId?: string
}

/** Request to update an agent */
export interface UpdateAgentRequest {
  name?: string
  zonePosition?: {
    q: number
    r: number
  }
}

/** Image attachment for prompt requests */
export interface ImageAttachment {
  /** Original filename */
  name: string
  /** Base64 encoded image data */
  data: string
  /** MIME type (image/png, image/jpeg, etc.) */
  type: string
}

/** Request to send a prompt to an agent */
export interface AgentPromptRequest {
  prompt: string
  send?: boolean
  /** Optional image attachments */
  images?: ImageAttachment[]
}

/** Response for agent operations */
export interface AgentResponse {
  ok: boolean
  agent?: ManagedAgent
  error?: string
}

/** Response for listing agents */
export interface AgentListResponse {
  ok: boolean
  agents: ManagedAgent[]
}

// ============================================================================
// Text Tiles (Grid Labels)
// ============================================================================

/** A text label tile on the hex grid */
export interface TextTile {
  /** Unique ID (UUID) */
  id: string
  /** The label text */
  text: string
  /** Hex grid position */
  position: {
    q: number
    r: number
  }
  /** Optional color (hex string, default white) */
  color?: string
  /** Creation timestamp */
  createdAt: number
}

/** Request to create a text tile */
export interface CreateTextTileRequest {
  text: string
  position: {
    q: number
    r: number
  }
  color?: string
}

/** Request to update a text tile */
export interface UpdateTextTileRequest {
  text?: string
  position?: {
    q: number
    r: number
  }
  color?: string
}

// ============================================================================
// Agent Groups (Multi-Agent Orchestration)
// ============================================================================

/** Configuration for an agent group */
export interface GroupConfig {
  /** Auto-report Worker results on stop (default: true) */
  autoReport: boolean
  /** Custom Manager initialization prompt */
  managerPrompt?: string
}

/** A worker in a group */
export interface GroupWorker {
  /** Agent ID of the worker */
  agentId: string
  /** Worker name within the group */
  name: string
  /** Worker status (inherited from agent) */
  status?: 'idle' | 'running' | 'offline'
}

/** A group of agents with Manager-Workers pattern */
export interface AgentGroup {
  /** Unique ID (e.g., "group-a1b2c3") */
  id: string
  /** User-defined name */
  name: string
  /** Optional description */
  description?: string
  /** Manager agent ID (optional for simpler groups) */
  managerId?: string
  /** Worker agent IDs (max 9) - legacy format */
  workerIds: string[]
  /** Workers with names - new format for test compatibility */
  workers?: GroupWorker[]
  /** Creation timestamp */
  createdAt: number
  /** Group configuration */
  config: GroupConfig
  /** Team scope (group belongs to this team) */
  teamId?: string
  /** Project scope (group belongs to this project; requires teamId) */
  projectId?: string
}

/** Role of an agent within a group */
export type GroupRole = 'manager' | 'worker'

/** Request to create an agent group */
export interface CreateGroupRequest {
  name: string
  description?: string
  managerId?: string
  workerIds?: string[]
  config?: Partial<GroupConfig>
  /** Team scope (required for team-scoped groups) */
  teamId: string
  /** Project scope (optional; requires teamId) */
  projectId?: string
}

/** Request to update an agent group */
export interface UpdateGroupRequest {
  name?: string
  description?: string
  managerId?: string
  workerIds?: string[]
  config?: Partial<GroupConfig>
}

/** Request to add a worker to a group */
export interface AddWorkerRequest {
  agentId: string
  name: string
}

/** Request to dispatch a message to workers */
export interface DispatchRequest {
  message: string
  task: string
  workers?: string[]
  metadata?: Record<string, unknown>
}

/** Response for dispatch operations */
export interface DispatchResponse {
  ok: boolean
  dispatched: number
  total: number
}

/** Response for group operations */
export interface GroupResponse {
  ok: boolean
  group?: AgentGroup
  error?: string
}

/** Response for listing groups */
export interface GroupListResponse {
  ok: boolean
  groups: AgentGroup[]
}

/** Worker report message payload */
export interface WorkerReportPayload {
  groupId: string
  workerId: string
  workerName: string
  content: string
  /** true = triggered by stop event, false = manual <report> tag */
  isAutoReport: boolean
}

// ============================================================================
// Orchestration Task & Participant Summaries (for WS + API)
// ============================================================================

/** Task state machine states */
export type OrchTaskState =
  | 'backlog'
  | 'queued'
  | 'working'
  | 'blocked'
  | 'completed'
  | 'failed'
  | 'canceled'

/** Task priority levels */
export type OrchTaskPriority = 'low' | 'normal' | 'high' | 'urgent'

export interface TaskContextCounts {
  appliedMemories: number
  appliedSkills: number
  total: number
}

/** Lightweight task representation for WS broadcasts and list views */
export interface TaskSummary {
  id: string
  groupId: string
  state: OrchTaskState
  method: string
  assignedTo?: string
  assignedAgentName?: string
  progress: number
  priority: OrchTaskPriority
  params: { task: string; message: string }
  error?: string
  createdAt: string
  updatedAt: string
  completedAt?: string
  contextCounts?: TaskContextCounts
}

/** Lightweight participant representation for WS broadcasts */
export interface ParticipantSummary {
  id: string
  agentId: string
  name: string
  status: 'online' | 'busy' | 'offline'
  groupId?: string
}

/** Task stats response from /groups/:groupId/tasks/stats */
export interface OrchTaskStats {
  byState: Record<OrchTaskState, number>
  queueStats?: {
    waiting: number
    active: number
    completed: number
    failed: number
    delayed: number
  }
}

// ============================================================================
// Agent Messages (Provider+Prompt chat history)
// ============================================================================

/** One row from `GET /agents/:id/messages`. Matches the Rust `AgentMessage` entity. */
export interface AgentMessageRow {
  id: string
  agentId: string
  role: 'user' | 'assistant'
  content: string
  tokensIn: number | null
  tokensOut: number | null
  model: string | null
  finishReason: string | null
  createdAt: string
}

// ===== DEPRECATED ALIASES (remove after all layers migrated) =====
/** @deprecated Use ManagedAgent */
export type ManagedSession = ManagedAgent
/** @deprecated Use AgentStatus */
export type SessionStatus = AgentStatus
/** @deprecated Use CreateAgentRequest */
export type CreateSessionRequest = CreateAgentRequest
/** @deprecated Use UpdateAgentRequest */
export type UpdateSessionRequest = UpdateAgentRequest
/** @deprecated Use AgentPromptRequest */
export type SessionPromptRequest = AgentPromptRequest
/** @deprecated Use AgentResponse */
export type SessionResponse = AgentResponse
/** @deprecated Use AgentListResponse */
export type SessionListResponse = AgentListResponse
/** @deprecated Use AgentGroup */
export type SessionGroup = AgentGroup
/** @deprecated Use ParticipantSummary */
export type AgentSummary = ParticipantSummary
