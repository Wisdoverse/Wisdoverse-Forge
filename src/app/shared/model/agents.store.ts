import { create } from 'zustand'
import type { CliTool, ManagedAgent } from '@shared/types'
import { getAgentApi } from '@app/shared/api/legacy'
import type { LocalAgentEnrollmentResponse } from '@app/shared/api/legacy/AgentAPI'

export type AgentStatus = 'working' | 'idle' | 'offline'
export type AgentRuntimeKind = 'container-cli' | 'host-cli' | 'provider'

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

export function isHostCliAgent(agent: Pick<AgentInfo, 'runtimeKind' | 'runtimeId'>): boolean {
  return agent.runtimeKind === 'host-cli' || agent.runtimeId?.startsWith('host-') === true
}

interface AgentsState {
  agents: AgentInfo[]
  selectedAgentId: string | null
  loading: boolean
  createModalOpen: boolean
  error: string | null

  setAgents: (agents: AgentInfo[]) => void
  selectAgent: (id: string | null) => void
  updateAgentStatus: (id: string, status: AgentStatus) => void
  setLoading: (loading: boolean) => void
  setCreateModalOpen: (open: boolean) => void
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

function cliToolToProvider(cliTool?: CliTool): string {
  switch (cliTool) {
    case 'claude':
      return 'Anthropic'
    case 'gemini':
      return 'Google'
    case 'codex':
      return 'OpenAI'
    case 'opencode':
      return 'OpenAI'
    default:
      return 'Refresh AI service'
  }
}

function managedToAgentInfo(agent: ManagedAgent): AgentInfo {
  const runtimeKind: AgentRuntimeKind = agent.cliTool
    ? agent.runtimeId?.startsWith('host-')
      ? 'host-cli'
      : 'container-cli'
    : 'provider'

  return {
    id: agent.id,
    name: agent.name,
    // Provider+prompt agents: backend carries the real provider/model keys.
    // CLI-tool agents: backend leaves them null, fall back to cliTool-derived labels.
    provider: agent.provider ?? cliToolToProvider(agent.cliTool),
    model: agent.model ?? agent.cliTool ?? 'Refresh AI model',
    status: mapManagedAgentStatus(agent.status),
    tasksCompleted: 0,
    tasksInProgress: 0,
    successRate: 0,
    cliTool: agent.cliTool,
    runtimeId: agent.runtimeId ?? undefined,
    runtimeKind,
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
  error: null as string | null,
}

const AGENTS_LOAD_ERROR = 'Refresh Agents to load agents.'
const AGENT_CREATE_ERROR = 'Agent was not created. Check the agent details, then try again.'
const AGENT_CREATED_START_ERROR =
  'Agent was created, but its workspace was not started. Ask an owner or admin to check Where agents run, then start this agent from the card.'
const THIS_COMPUTER_SETUP_ERROR =
  'This computer setup text could not be prepared. Check the agent name and work tool, then choose Create Agent again.'
const AGENT_INSTRUCTIONS_ERROR =
  'Agent instructions were not saved. Review the instructions, then try again.'
const AGENT_DELETE_ERROR = 'Agent was not removed. Refresh Agents, then try again.'
const AGENT_PROMPT_ERROR = 'Instruction was not sent. Refresh this agent, then try again.'
const AGENT_START_ERROR = 'Agent was not started. Refresh this agent, then try again.'
const AGENT_RESTART_ERROR = 'Agent was not restarted. Refresh this agent, then try again.'

export const useAgentsStore = create<AgentsState>((set, get) => ({
  ...initialState,
  setAgents: (agents) => set({ agents }),
  selectAgent: (selectedAgentId) => set({ selectedAgentId }),
  updateAgentStatus: (id, status) =>
    set((state) => ({
      agents: state.agents.map((a) => (a.id === id ? { ...a, status } : a)),
    })),
  setLoading: (loading) => set({ loading }),
  setCreateModalOpen: (createModalOpen) => set({ createModalOpen }),
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
        set({ loading: false, error: AGENTS_LOAD_ERROR })
      }
    } catch (err) {
      console.error('loadAgents failed:', err)
      set({ loading: false, error: AGENTS_LOAD_ERROR })
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
            set((state) => ({
              agents: [...state.agents, newAgent],
              loading: false,
              createModalOpen: false,
              error: AGENT_CREATED_START_ERROR,
            }))
            return true
          }
        }
        set((state) => ({
          agents: [...state.agents, newAgent],
          loading: false,
          createModalOpen: false,
        }))
        return true
      } else {
        set({ loading: false, error: AGENT_CREATE_ERROR })
        return false
      }
    } catch (err) {
      console.error('createAgent failed:', err)
      set({ loading: false, error: AGENT_CREATE_ERROR })
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
      set({
        loading: false,
        error: THIS_COMPUTER_SETUP_ERROR,
      })
      return null
    } catch (err) {
      console.error('enrollLocalAgent failed:', err)
      set({
        loading: false,
        error: THIS_COMPUTER_SETUP_ERROR,
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
        set({ error: AGENT_INSTRUCTIONS_ERROR })
        return false
      }
      // Reload so the cached agent reflects the new value.
      await get().loadAgents()
      return true
    } catch (err) {
      console.error('updateAgentSystemPrompt failed:', err)
      set({ error: AGENT_INSTRUCTIONS_ERROR })
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
        set({ error: AGENT_DELETE_ERROR })
        return false
      }
    } catch (err) {
      console.error('deleteAgent failed:', err)
      set({ error: AGENT_DELETE_ERROR })
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
        set({ error: AGENT_PROMPT_ERROR })
        return false
      }
    } catch (err) {
      console.error('sendPrompt failed:', err)
      set({ error: AGENT_PROMPT_ERROR })
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
      set({ error: AGENT_START_ERROR })
      return false
    } catch (err) {
      console.error('startAgent failed:', err)
      set({ error: AGENT_START_ERROR })
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
        await get().loadAgents()
        set({ error: AGENT_RESTART_ERROR })
        return false
      }
    } catch (err) {
      console.error('restartAgent failed:', err)
      set({ error: AGENT_RESTART_ERROR })
      return false
    }
  },
}))
