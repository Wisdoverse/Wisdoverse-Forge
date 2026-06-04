import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { AgentsPanel } from '@app/features/admin/AgentsPanel'
import type { AdminAgent } from '@app/shared/model/admin.store'
import { useAdminStore } from '@app/shared/model/admin.store'

const loadAgentsMock = vi.fn().mockResolvedValue(undefined)
const setAgentRuntimeKindFilterMock = vi.fn().mockResolvedValue(undefined)

const originalLoadAgents = useAdminStore.getState().loadAgents
const originalSetFilter = useAdminStore.getState().setAgentRuntimeKindFilter

const agents: AdminAgent[] = [
  {
    id: 'agent-1',
    name: 'Container Worker',
    status: 'idle',
    runtimeKind: 'container',
    cliTool: 'codex',
    ownerUsername: 'alice',
    ownerEmail: 'alice@example.com',
    projectName: 'Forge',
    lastActivity: 1_700_000_000_000,
  },
  {
    id: 'agent-2',
    name: 'Laptop Agent',
    status: 'offline',
    runtimeKind: 'cli',
    cliTool: 'claude',
    ownerUsername: null,
    ownerEmail: 'bob@example.com',
    projectName: null,
    lastActivity: 1_700_000_100_000,
  },
  {
    id: 'agent-3',
    name: 'Prompt Bot',
    status: 'working',
    runtimeKind: 'api',
    cliTool: null,
    ownerUsername: 'carol',
    ownerEmail: 'carol@example.com',
    projectName: 'Research',
    lastActivity: 1_700_000_200_000,
  },
]

beforeEach(() => {
  loadAgentsMock.mockClear()
  setAgentRuntimeKindFilterMock.mockClear()
  useAdminStore.setState({
    agents,
    agentsTotal: agents.length,
    agentsLoading: false,
    agentsError: null,
    agentRuntimeKindFilter: 'all',
    loadAgents: loadAgentsMock,
    setAgentRuntimeKindFilter: setAgentRuntimeKindFilterMock,
  })
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  useAdminStore.setState({
    agents: [],
    agentsTotal: 0,
    agentsLoading: false,
    agentsError: null,
    agentRuntimeKindFilter: 'all',
    loadAgents: originalLoadAgents,
    setAgentRuntimeKindFilter: originalSetFilter,
  })
})

describe('AgentsPanel', () => {
  test('renders the runtime filter with every canonical option', async () => {
    render(<AgentsPanel />)

    const select = (await screen.findByTestId('admin-agents-runtime-filter')) as HTMLSelectElement
    const optionLabels = within(select)
      .getAllByRole('option')
      .map((o) => o.textContent)

    expect(optionLabels).toEqual([
      'All runtimes',
      'Container (Docker)',
      'Host CLI (local process)',
      'API (direct LLM calls)',
    ])
    expect(loadAgentsMock).toHaveBeenCalled()
  })

  test('shows a runtime-kind badge for each agent row', async () => {
    render(<AgentsPanel />)

    expect(await screen.findByTestId('agent-kind-badge-container')).toBeDefined()
    expect(screen.getByTestId('agent-kind-badge-cli')).toBeDefined()
    expect(screen.getByTestId('agent-kind-badge-api')).toBeDefined()

    expect(screen.getByText('Container')).toBeDefined()
    expect(screen.getByText('Host CLI')).toBeDefined()
    expect(screen.getByText('API')).toBeDefined()

    expect(screen.getAllByTestId('admin-agent-row')).toHaveLength(3)
  })

  test('selecting a runtime kind triggers the filtered fetch', async () => {
    render(<AgentsPanel />)

    const select = (await screen.findByTestId('admin-agents-runtime-filter')) as HTMLSelectElement
    fireEvent.change(select, { target: { value: 'cli' } })

    expect(setAgentRuntimeKindFilterMock).toHaveBeenCalledWith('cli')
  })

  test('renders recovery guidance when agents fail to load', async () => {
    useAdminStore.setState({ agentsError: 'HTTP 503' })

    render(<AgentsPanel />)

    const error = await screen.findByTestId('admin-agents-error')
    expect(within(error).getByText('HTTP 503')).toBeDefined()
    expect(
      within(error).getByText(
        'Refresh after the API is healthy, or confirm this account has admin access.'
      )
    ).toBeDefined()
  })

  test('guides admins when no agents exist yet', async () => {
    useAdminStore.setState({ agents: [], agentsTotal: 0 })

    render(<AgentsPanel />)

    const emptyState = await screen.findByTestId('admin-agents-empty')
    expect(within(emptyState).getByText(/create the first agent from agents/i)).toBeDefined()
    expect(within(emptyState).getByText(/confirm it becomes idle or working/i)).toBeDefined()
    expect(within(emptyState).getByText(/refresh after the api is healthy/i)).toBeDefined()
  })

  test('guides admins to clear a runtime filter before assuming an agent is missing', async () => {
    useAdminStore.setState({ agents: [], agentsTotal: 0, agentRuntimeKindFilter: 'cli' })

    render(<AgentsPanel />)

    const emptyState = await screen.findByTestId('admin-agents-empty')
    expect(within(emptyState).getByText(/choose "all runtimes"/i)).toBeDefined()
    expect(within(emptyState).getByText(/before assuming the agent is missing/i)).toBeDefined()
  })
})
