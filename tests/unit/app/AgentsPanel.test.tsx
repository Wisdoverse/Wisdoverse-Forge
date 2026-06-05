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
  {
    id: 'agent-4',
    name: 'Review Needed Agent',
    status: 'paused',
    runtimeKind: 'container',
    cliTool: 'codex',
    ownerUsername: null,
    ownerEmail: 'dana@example.com',
    projectName: 'Ops',
    lastActivity: 1_700_000_300_000,
  },
  {
    id: 'agent-5',
    name: 'Missing Status Agent',
    status: ' ',
    runtimeKind: 'api',
    cliTool: null,
    ownerUsername: null,
    ownerEmail: null,
    projectName: null,
    lastActivity: 0,
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
  test('renders the work type filter with every plain-language option', async () => {
    render(<AgentsPanel />)

    const select = (await screen.findByTestId('admin-agents-runtime-filter')) as HTMLSelectElement
    const optionLabels = within(select)
      .getAllByRole('option')
      .map((o) => o.textContent)

    expect(optionLabels).toEqual([
      'All work types',
      'Managed workspace',
      'This computer',
      'Chat-only AI service',
    ])
    expect(loadAgentsMock).toHaveBeenCalled()
  })

  test('shows a runtime-kind badge for each agent row', async () => {
    render(<AgentsPanel />)

    expect(await screen.findAllByTestId('agent-kind-badge-container')).toHaveLength(2)
    expect(screen.getAllByTestId('agent-kind-badge-cli')).toHaveLength(1)
    expect(screen.getAllByTestId('agent-kind-badge-api')).toHaveLength(2)

    expect(screen.getAllByText('Managed')).toHaveLength(2)
    expect(screen.getAllByText('This computer').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Chat-only')).toHaveLength(2)
    expect(screen.queryByText(/Text-only model/i)).toBeNull()
    expect(screen.getByText('Ready')).toBeDefined()
    expect(screen.queryByText('idle')).toBeNull()
    expect(screen.getByText('Needs review')).toBeDefined()
    expect(screen.getByText('Status not reported')).toBeDefined()
    expect(screen.queryByText('paused')).toBeNull()
    expect(screen.queryByText('Unknown')).toBeNull()

    expect(screen.getAllByTestId('admin-agent-row')).toHaveLength(5)
  })

  test('selecting a work type triggers the filtered fetch', async () => {
    render(<AgentsPanel />)

    const select = (await screen.findByTestId('admin-agents-runtime-filter')) as HTMLSelectElement
    fireEvent.change(select, { target: { value: 'cli' } })

    expect(setAgentRuntimeKindFilterMock).toHaveBeenCalledWith('cli')
  })

  test('renders recovery guidance when agents fail to load', async () => {
    useAdminStore.setState({ agentsError: 'HTTP 503' })

    render(<AgentsPanel />)

    const error = await screen.findByTestId('admin-agents-error')
    expect(within(error).getByText('The admin agents could not load.')).toBeDefined()
    expect(within(error).queryByText('HTTP 503')).toBeNull()
    expect(
      within(error).getByText(
        'Refresh Admin, then try again. If it still fails, ask an owner or admin to check Admin setup and your role.'
      )
    ).toBeDefined()
    expect(within(error).queryByText(/admin service/i)).toBeNull()
  })

  test('guides admins when no agents exist yet', async () => {
    useAdminStore.setState({ agents: [], agentsTotal: 0 })

    render(<AgentsPanel />)

    const emptyState = await screen.findByTestId('admin-agents-empty')
    expect(within(emptyState).getByText(/create the first agent from agents/i)).toBeDefined()
    expect(within(emptyState).getByText(/confirm it becomes ready or working/i)).toBeDefined()
    expect(within(emptyState).getByText(/refresh admin and check again/i)).toBeDefined()
  })

  test('guides admins to clear a work type filter before assuming an agent is missing', async () => {
    useAdminStore.setState({ agents: [], agentsTotal: 0, agentRuntimeKindFilter: 'cli' })

    render(<AgentsPanel />)

    const emptyState = await screen.findByTestId('admin-agents-empty')
    expect(within(emptyState).getByText(/choose "all work types"/i)).toBeDefined()
    expect(within(emptyState).getByText(/before assuming the agent is missing/i)).toBeDefined()
  })
})
