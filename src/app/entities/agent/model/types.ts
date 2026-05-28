import type { CliTool } from '@shared/types'

/**
 * Canonical runtime kind discriminator.
 *
 * - 'container' — runs inside a managed workspace container.
 * - 'cli'       — enrolled on an operator machine (host CLI / sidecar).
 * - 'api'       — provider+prompt agent with no container.
 *
 * Optional for one release cycle to support rolling deploys where old server
 * responses do not yet send this field. The store derives it from cliTool +
 * runtimeId when absent.
 */
export type AgentRuntimeKind = 'container' | 'cli' | 'api'

export type AgentStatus = 'working' | 'idle' | 'offline'

export type { CliTool }

export interface AgentInfo {
  id: string
  name: string
  provider: string
  model: string
  status: AgentStatus
  tasksCompleted: number
  tasksInProgress: number
  successRate: number
  currentTask?: string
  cliTool?: CliTool
  runtimeId?: string
  /** Optional during one-cycle rolling deploy; required next release. */
  runtimeKind?: AgentRuntimeKind
  cwd?: string
  containerId?: string
  workspaceId?: string
  workspaceName?: string
  projectId?: string
  projectName?: string
  /** Provider+prompt agents only. `null` when unset. Not present for CLI-tool agents. */
  systemPrompt?: string | null
}
