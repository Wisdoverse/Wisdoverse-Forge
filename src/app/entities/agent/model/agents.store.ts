import { create } from 'zustand'
import type { CliTool, ManagedAgent } from '@shared/types'
import { getAgentApi } from '@app/shared/api/legacy'
import { extractApiError, type LocalAgentEnrollmentResponse } from '../api/AgentAPI'
import type { AgentInfo, AgentRuntimeKind, AgentStatus } from './types'
import { isHostCliAgent } from './runtime-kind'

export type { AgentInfo, AgentRuntimeKind, AgentStatus }
export { isHostCliAgent }
export type AgentCreateInitialKind = 'cli' | 'local-cli' | 'provider'

interface AgentsState {
  agents: AgentInfo[]
  selectedAgentId: string | null
  loading: boolean
  createModalOpen: boolean
  createModalInitialKind: AgentCreateInitialKind | null
  error: string | null

  setAgents: (agents: AgentInfo[]) => void
  selectAgent: (id: string | null) => void
  updateAgentStatus: (id: string, status: AgentStatus) => void
  setLoading: (loading: boolean) => void
  setCreateModalOpen: (open: boolean, initialKind?: AgentCreateInitialKind | null) => void
  setError: (error: string | null) => void
  reset: () => void

  // CRUD actions
  loadAgents: () => Promise<void>
  /**
   * Create an agent. Two kinds are supported:
   *   - `{ kind: 'cli', cliTool }` — container-backed CLI agent (claude/codex/…).
   *   - `{ kind: 'provider', provider, model }` — provider+prompt agent with no container.
   */
  createAgent: (
    options: {
      name: string
      cwd?: string
      workspaceId?: string
      projectId?: string
      groupId?: string
    } & (
      | { kind: 'cli'; cliTool: CliTool }
      | { kind: 'provider'; provider: string; model: string; systemPrompt?: string }
    )
  ) => Promise<boolean>
  enrollLocalAgent: (options: {
    name: string
    cliTool: CliTool
    model?: string
    cwd?: string
    workspaceId?: string
    projectId?: string
  }) => Promise<LocalAgentEnrollmentResponse | null>
  deleteAgent: (id: string) => Promise<boolean>
  updateAgentSystemPrompt: (id: string, systemPrompt: string | null) => Promise<boolean>
  sendPrompt: (id: string, prompt: string) => Promise<boolean>
  startAgent: (id: string) => Promise<boolean>
  restartAgent: (id: string) => Promise<boolean>
}

type AgentErrorAction =
  | 'load'
  | 'create'
  | 'enrollLocal'
  | 'updateInstructions'
  | 'delete'
  | 'sendPrompt'
  | 'start'
  | 'restart'

function agentActionPhrase(action: AgentErrorAction): string {
  switch (action) {
    case 'load':
      return 'load agents'
    case 'create':
      return 'create the agent'
    case 'enrollLocal':
      return 'connect the agent from this computer'
    case 'updateInstructions':
      return 'save agent instructions'
    case 'delete':
      return 'delete the agent'
    case 'sendPrompt':
      return 'send the instruction'
    case 'start':
      return 'start the agent'
    case 'restart':
      return 'restart the agent'
  }
}

function rawAgentErrorMessage(error: unknown): string | null {
  if (typeof error === 'string' && error.trim()) return error.trim()
  if (error instanceof Error && error.message.trim()) return error.message.trim()
  if (error && typeof error === 'object') {
    const details = (error as { details?: { reason?: unknown } }).details
    if (typeof details?.reason === 'string' && details.reason.trim()) return details.reason.trim()

    const value = extractApiError(error as Parameters<typeof extractApiError>[0], '')
    if (value.trim()) return value.trim()
  }
  return null
}

function detailFromPayload(payload: unknown): string | null {
  if (!payload || typeof payload !== 'object') return null
  const record = payload as Record<string, unknown>
  const nestedError = record.error
  if (nestedError && typeof nestedError === 'object' && !Array.isArray(nestedError)) {
    const message = (nestedError as { message?: unknown }).message
    if (typeof message === 'string' && message.trim()) return message.trim()
  }
  const details = record.details
  if (details && typeof details === 'object' && !Array.isArray(details)) {
    const reason = (details as { reason?: unknown }).reason
    if (typeof reason === 'string' && reason.trim()) return reason.trim()
  }
  for (const key of ['message', 'error', 'detail']) {
    const value = record[key]
    if (typeof value === 'string' && value.trim()) return value.trim()
  }
  return null
}

function agentErrorDetail(error: unknown): string | null {
  const raw = rawAgentErrorMessage(error)
  if (!raw) return null

  const apiMatch = raw.match(/\b(?:API|HTTP)\s+\d{3}:?\s*(.*)$/i)
  const body = apiMatch?.[1]?.trim()
  if (body) {
    try {
      const detail = detailFromPayload(JSON.parse(body) as unknown)
      if (detail) return detail
    } catch {
      return body
    }
  }

  return raw
}

function agentErrorStatus(error: unknown): number | null {
  if (error && typeof error === 'object') {
    const value = error as { status?: unknown; statusCode?: unknown; code?: unknown }
    for (const candidate of [value.status, value.statusCode, value.code]) {
      const status = numericStatus(candidate)
      if (status) return status
    }
  }

  const raw = rawAgentErrorMessage(error)
  const match = raw?.match(/\b(?:API|HTTP|Server error \()? ?(\d{3})\b/)
  return match ? Number(match[1]) : null
}

function numericStatus(value: unknown): number | null {
  if (typeof value === 'number' && Number.isInteger(value)) return value
  if (typeof value === 'string') {
    const trimmed = value.trim()
    if (/^\d{3}$/.test(trimmed)) return Number(trimmed)
  }
  return null
}

function isRawAgentFailure(detail: string | null): boolean {
  if (!detail) return true
  return (
    /^API \d{3}/i.test(detail) ||
    /^HTTP \d{3}/i.test(detail) ||
    /^Server error \(\d{3}\)$/i.test(detail) ||
    /^Network error$/i.test(detail) ||
    /^Failed to fetch$/i.test(detail)
  )
}

function isAgentServiceUnavailable(detail: string | null): boolean {
  const normalized = detail?.toLowerCase() ?? ''
  return (
    normalized.includes('database') ||
    normalized.includes('unavailable') ||
    normalized.includes('timeout') ||
    normalized.includes('temporarily') ||
    normalized.includes('internal server')
  )
}

function agentConnectionMessage(actionPhrase: string, action: AgentErrorAction): string {
  if (action === 'enrollLocal') {
    return 'Forge could not prepare the setup command for this computer. Check your connection, then choose Create Agent again.'
  }
  if (action === 'load') {
    return 'Check your connection, then refresh Agents to load agents.'
  }
  const operation = 'updating Agents'
  return `Forge could not ${actionPhrase}. It could not connect while ${operation}. Check your connection, then refresh Agents.`
}

export function agentActionErrorMessage(action: AgentErrorAction, error?: unknown): string {
  const actionPhrase = agentActionPhrase(action)
  const status = agentErrorStatus(error)
  const detail = agentErrorDetail(error)

  if (!status) {
    if (action === 'enrollLocal' && isAgentServiceUnavailable(detail)) {
      return agentServerMessage(action)
    }
    if (!isRawAgentFailure(detail)) {
      return agentValidationMessage(action, detail)
    }
    return agentConnectionMessage(actionPhrase, action)
  }

  if (status === 401) {
    return `Sign in again, then open Agents and try to ${actionPhrase} again.`
  }
  if (status === 403) {
    return `You do not have permission to ${actionPhrase}. Ask an owner or admin to update your team space access.`
  }
  if (status === 404) {
    return 'This agent could not be found. Refresh the Agents page, choose the current agent, then try again.'
  }
  if (status === 409) {
    return agentConflictMessage(action, detail)
  }
  if (status === 422) {
    return agentValidationMessage(action, detail)
  }
  if (status === 429) {
    return `The Agents page is busy. Wait a moment, then try to ${actionPhrase} again.`
  }
  if (status >= 500) {
    return agentServerMessage(action)
  }

  return `Forge could not ${actionPhrase}. Refresh the Agents page, then try again.`
}

function agentValidationMessage(action: AgentErrorAction, detail: string | null): string {
  const normalized = detail?.toLowerCase() ?? ''

  if (action === 'load') {
    return 'Refresh Agents to load agents.'
  }

  if (action === 'create') {
    if (normalized.includes('name')) {
      return 'Name this agent, choose where it should work, then try creating it again.'
    }
    if (normalized.includes('provider') || normalized.includes('model')) {
      return 'Choose a tested AI service and model, then try creating this agent again.'
    }
    if (normalized.includes('workspace') || normalized.includes('project')) {
      return 'Choose a team space and project you can access, then try creating this agent again.'
    }
  }

  if (action === 'enrollLocal') {
    return 'Check the agent name, work tool, and project access, then choose Create Agent again.'
  }
  if (action === 'sendPrompt') {
    return 'Write one clear instruction and make sure the agent is not already working, then send again.'
  }
  if (action === 'updateInstructions') {
    return 'Check the instruction text, refresh this agent, then save the instructions again.'
  }
  if (normalized.includes('runtime') || normalized.includes('docker')) {
    return agentRuntimeRecoveryMessage(detail)
  }

  return `Check the agent details, refresh the Agents page, then try to ${agentActionPhrase(action)} again.`
}

function agentConflictMessage(action: AgentErrorAction, detail: string | null): string {
  const normalized = detail?.toLowerCase() ?? ''
  if (normalized.includes('working') || normalized.includes('busy')) {
    return 'This agent is already working. Wait for the current work to finish, refresh the Agents page, then try again.'
  }
  if (action === 'delete') {
    return 'This agent changed while you were deleting it. Refresh the Agents page, review the current status, then try again.'
  }
  return 'This agent changed while you were working. Refresh the Agents page, review its current status, then try again.'
}

function agentServerMessage(action: AgentErrorAction): string {
  if (action === 'enrollLocal') {
    return 'Forge could not prepare the setup command for this computer right now. Wait a moment, then choose Create Agent again. If it still fails, ask an owner or admin to check Where agents run.'
  }
  if (action === 'load') {
    return 'Refresh Agents to load agents. If it still fails, ask an owner or admin to check Where agents run.'
  }
  if (action === 'start' || action === 'restart' || action === 'create') {
    return 'Forge could not prepare where this agent runs right now. Wait a moment, then try again. If it still fails, ask an owner or admin to check Where agents run.'
  }
  return `Refresh Agents, then try to ${agentActionPhrase(action)} again. If it still fails, ask an owner or admin to check Where agents run.`
}

function agentRuntimeRecoveryMessage(detail: string | null): string {
  const normalized = detail?.toLowerCase() ?? ''
  if (normalized.includes('docker')) {
    return 'The place where this agent runs is not ready. Ask an owner or admin to check Where agents run, then start this agent from the agent card.'
  }
  return 'The place where this agent runs is not ready. Ask an owner or admin to check Where agents run, then start this agent from the agent card.'
}

function agentCreatedStartFailureMessage(error?: unknown): string {
  const detail = agentErrorDetail(error)
  const normalized = detail?.toLowerCase() ?? ''
  if (normalized.includes('docker')) {
    return 'Agent was created, but the place where it runs is not ready yet. It will stay in the list. Ask an owner or admin to check Where agents run, then start this agent from the card.'
  }
  if (
    normalized.includes('runtime') ||
    normalized.includes('container') ||
    normalized.includes('image')
  ) {
    return 'Agent was created, but it could not start yet. It will stay in the list. Ask an owner or admin to check Where agents run, then start this agent from the card.'
  }
  return 'Agent was created, but it could not start yet. It will stay in the list. Refresh the Agents page, then start this agent from the card after the place where it runs is ready.'
}

function mapManagedAgentStatus(status: string): AgentStatus {
  switch (status) {
    case 'working':
    case 'busy':
      return 'working'
    case 'offline':
    case 'error':
      return 'offline'
    default:
      return 'idle'
  }
}

function nonBlankLabel(value?: string | null): string | null {
  const trimmed = value?.trim()
  return trimmed ? trimmed : null
}

function aiServiceLabel(provider?: string | null): string | null {
  switch (provider?.trim().toLowerCase()) {
    case 'anthropic':
      return 'Anthropic'
    case 'openai':
      return 'OpenAI'
    case 'google':
    case 'gemini':
      return 'Google'
    case 'ollama':
      return 'Ollama'
    case 'openrouter':
      return 'OpenRouter'
    case 'together':
      return 'Together AI'
    case 'openai_compatible':
    case 'openai-compatible':
      return 'OpenAI-compatible service'
    case undefined:
    case '':
      return null
    default:
      return 'Check AI service'
  }
}

function cliToolLabel(cliTool?: CliTool | string | null): string | null {
  switch (cliTool?.trim().toLowerCase()) {
    case 'claude':
      return 'Claude'
    case 'gemini':
      return 'Gemini'
    case 'codex':
      return 'Codex'
    case 'opencode':
      return 'OpenCode'
    case undefined:
    case '':
      return null
    default:
      return 'Check work tool'
  }
}

function cliToolToProvider(cliTool?: CliTool | string | null): string | null {
  switch (cliTool?.trim().toLowerCase()) {
    case 'claude':
      return 'Anthropic'
    case 'gemini':
      return 'Google'
    case 'codex':
      return 'OpenAI'
    case 'opencode':
      return 'OpenAI'
    case undefined:
    case '':
      return null
    default:
      return 'Check work tool'
  }
}

export function managedToAgentInfo(agent: ManagedAgent): AgentInfo {
  // Rolling-deploy fallback: use server-sent runtimeKind when available; otherwise
  // derive from cliTool + runtimeId for backward compat with old server responses.
  const derivedRuntimeKind: AgentRuntimeKind =
    agent.runtimeKind ??
    (agent.cliTool ? (agent.runtimeId?.startsWith('host-') ? 'cli' : 'container') : 'api')

  return {
    id: agent.id,
    name: agent.name,
    // Provider+prompt agents: backend carries the real provider/model keys.
    // CLI-tool agents: backend leaves them null, fall back to cliTool-derived labels.
    provider:
      aiServiceLabel(nonBlankLabel(agent.provider)) ??
      cliToolToProvider(agent.cliTool) ??
      'Refresh AI service',
    model: nonBlankLabel(agent.model) ?? cliToolLabel(agent.cliTool) ?? 'Refresh AI model',
    status: mapManagedAgentStatus(agent.status),
    tasksCompleted: 0,
    tasksInProgress: 0,
    successRate: 0,
    cliTool: agent.cliTool,
    runtimeId: agent.runtimeId ?? undefined,
    runtimeKind: derivedRuntimeKind,
    cwd: agent.cwd,
    containerId: agent.containerId,
    workspaceId: agent.workspaceId,
    workspaceName: agent.workspaceName,
    projectId: agent.projectId,
    projectName: agent.projectName,
    systemPrompt: agent.systemPrompt,
  }
}

function responseContainerId(response: {
  container_id?: string
  containerId?: string
}): string | undefined {
  return response.containerId ?? response.container_id
}

const initialState = {
  agents: [] as AgentInfo[],
  selectedAgentId: null as string | null,
  loading: false,
  createModalOpen: false,
  createModalInitialKind: null as AgentCreateInitialKind | null,
  error: null as string | null,
}

export const useAgentsStore = create<AgentsState>((set, get) => ({
  ...initialState,
  setAgents: (agents) => set({ agents }),
  selectAgent: (selectedAgentId) => set({ selectedAgentId }),
  updateAgentStatus: (id, status) =>
    set((state) => ({
      agents: state.agents.map((a) => (a.id === id ? { ...a, status } : a)),
    })),
  setLoading: (loading) => set({ loading }),
  setCreateModalOpen: (createModalOpen, createModalInitialKind = null) =>
    set({
      createModalOpen,
      createModalInitialKind: createModalOpen ? createModalInitialKind : null,
    }),
  setError: (error) => set({ error }),
  reset: () => set(initialState),

  loadAgents: async () => {
    set({ loading: true, error: null })
    try {
      const api = getAgentApi()
      const result = await api.getAgents()
      if (result.ok) {
        set({ agents: result.agents.map(managedToAgentInfo), loading: false })
      } else {
        set({ loading: false, error: agentActionErrorMessage('load', result) })
      }
    } catch (err) {
      console.error('loadAgents failed:', err)
      set({ loading: false, error: agentActionErrorMessage('load', err) })
    }
  },

  createAgent: async (options) => {
    set({ loading: true, error: null })
    try {
      const api = getAgentApi()
      const result = await api.createAgent({
        name: options.name,
        cwd: options.cwd,
        workspaceId: options.workspaceId,
        projectId: options.projectId,
        cliTool: options.kind === 'cli' ? options.cliTool : undefined,
        groupId: options.groupId,
        provider: options.kind === 'provider' ? options.provider : undefined,
        model: options.kind === 'provider' ? options.model : undefined,
        systemPrompt: options.kind === 'provider' ? options.systemPrompt : undefined,
      })
      if (result.ok && result.agent) {
        let newAgent = managedToAgentInfo(result.agent)
        if (options.kind === 'cli') {
          const startResult = await api.startAgent(result.agent.id)
          if (startResult.ok) {
            newAgent = {
              ...newAgent,
              containerId: responseContainerId(startResult) ?? newAgent.containerId,
              status: 'idle',
            }
          } else {
            // Keep the modal open: it is the only surface that renders this
            // store's error, so closing it here would throw the message away
            // and leave the user staring at an unexplained offline agent.
            set((state) => ({
              agents: [...state.agents, newAgent],
              loading: false,
              createModalOpen: true,
              error: agentCreatedStartFailureMessage(startResult),
            }))
            return true
          }
        }
        set((state) => ({
          agents: [...state.agents, newAgent],
          loading: false,
          createModalOpen: false,
          createModalInitialKind: null,
        }))
        return true
      } else {
        set({ loading: false, error: agentActionErrorMessage('create', result) })
        return false
      }
    } catch (err) {
      console.error('createAgent failed:', err)
      set({ loading: false, error: agentActionErrorMessage('create', err) })
      return false
    }
  },

  enrollLocalAgent: async (options) => {
    set({ loading: true, error: null })
    try {
      const api = getAgentApi()
      const result = await api.enrollLocalAgent({
        name: options.name,
        cliTool: options.cliTool,
        model: options.model,
        cwd: options.cwd,
        workspaceId: options.workspaceId,
        projectId: options.projectId,
      })
      if (result.ok && result.agent && result.enrollment) {
        const newAgent = managedToAgentInfo(result.agent)
        set((state) => ({
          agents: [...state.agents, newAgent],
          loading: false,
        }))
        return result
      }
      set({ loading: false, error: agentActionErrorMessage('enrollLocal', result) })
      return null
    } catch (err) {
      console.error('enrollLocalAgent failed:', err)
      set({
        loading: false,
        error: agentActionErrorMessage('enrollLocal', err),
      })
      return null
    }
  },

  updateAgentSystemPrompt: async (id, systemPrompt) => {
    set({ error: null })
    try {
      const api = getAgentApi()
      const result = await api.updateAgent(id, { systemPrompt })
      if (!result.ok) {
        set({ error: agentActionErrorMessage('updateInstructions', result) })
        return false
      }
      // Reload so the cached agent reflects the new value.
      await get().loadAgents()
      return true
    } catch (err) {
      console.error('updateAgentSystemPrompt failed:', err)
      set({ error: agentActionErrorMessage('updateInstructions', err) })
      return false
    }
  },

  deleteAgent: async (id) => {
    set({ error: null })
    try {
      const api = getAgentApi()
      const result = await api.deleteAgent(id)
      if (result.ok) {
        set((state) => ({
          agents: state.agents.filter((a) => a.id !== id),
          selectedAgentId: state.selectedAgentId === id ? null : state.selectedAgentId,
        }))
        return true
      } else {
        set({ error: agentActionErrorMessage('delete', result) })
        return false
      }
    } catch (err) {
      console.error('deleteAgent failed:', err)
      set({ error: agentActionErrorMessage('delete', err) })
      return false
    }
  },

  sendPrompt: async (id, prompt) => {
    set({ error: null })
    try {
      const api = getAgentApi()
      const result = await api.sendPrompt(id, prompt)
      if (result.ok) {
        // Update status to working
        get().updateAgentStatus(id, 'working')
        return true
      } else {
        set({ error: agentActionErrorMessage('sendPrompt', result) })
        return false
      }
    } catch (err) {
      console.error('sendPrompt failed:', err)
      set({ error: agentActionErrorMessage('sendPrompt', err) })
      return false
    }
  },

  startAgent: async (id) => {
    set({ error: null })
    try {
      const api = getAgentApi()
      const result = await api.startAgent(id)
      if (result.ok) {
        const containerId = responseContainerId(result)
        set((state) => ({
          agents: state.agents.map((agent) =>
            agent.id === id
              ? {
                  ...agent,
                  containerId: containerId ?? agent.containerId,
                  status: 'idle',
                }
              : agent
          ),
        }))
        return true
      }
      set({ error: agentActionErrorMessage('start', result) })
      return false
    } catch (err) {
      console.error('startAgent failed:', err)
      set({ error: agentActionErrorMessage('start', err) })
      return false
    }
  },

  restartAgent: async (id) => {
    set({ error: null })
    try {
      const api = getAgentApi()
      const result = await api.restartAgent(id)
      if (result.ok) {
        get().updateAgentStatus(id, 'idle')
        return true
      } else {
        const message = agentActionErrorMessage('restart', result)
        await get().loadAgents()
        set({ error: message })
        return false
      }
    } catch (err) {
      console.error('restartAgent failed:', err)
      set({ error: agentActionErrorMessage('restart', err) })
      return false
    }
  },
}))
