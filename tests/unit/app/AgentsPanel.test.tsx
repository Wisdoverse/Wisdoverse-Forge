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
    lastActivity: Number.POSITIVE_INFINITY,
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
  test('explains agent list loading for first-time admins', () => {
    useAdminStore.setState({
      agents: [],
      agentsLoading: true,
      agentsError: null,
    })

    render(<AgentsPanel />)

    const loading = screen.getByRole('status', { name: /checking managed agents/i })
    expect(loading).toHaveTextContent('Checking managed agents')
    expect(loading).toHaveTextContent(
      'Forge is checking which agents are available across this team space.'
    )
    expect(loading).toHaveTextContent(
      'If this takes more than a moment, open Admin again or ask an owner to check agent access.'
    )
    expect(loading).toHaveTextContent(
      'Success looks like agent rows or a no-agents setup step.'
    )
    expect(loading).not.toHaveTextContent('Loading agents')
  })

  test('renders the work location filter with every plain-language option', async () => {
    render(<AgentsPanel />)

    const select = (await screen.findByTestId('admin-agents-runtime-filter')) as HTMLSelectElement
    const optionLabels = within(select)
      .getAllByRole('option')
      .map((o) => o.textContent)

    expect(optionLabels).toEqual([
      'All work locations',
      'Project files',
      'This computer',
      'Chat-only AI service',
    ])
    expect(
      screen.getByText('Check agents across every team space and filter them by work location.')
    ).toBeDefined()
    expect(screen.getByText(/shared project files/i)).toBeDefined()
    expect(screen.queryByText(/Forge project area/i)).toBeNull()
    expect(screen.queryByText(/every organization/i)).toBeNull()
    expect(loadAgentsMock).toHaveBeenCalled()
  })

  test('shows a runtime-kind badge for each agent row', async () => {
    render(<AgentsPanel />)

    expect(await screen.findAllByTestId('agent-kind-badge-container')).toHaveLength(2)
    expect(screen.getAllByTestId('agent-kind-badge-cli')).toHaveLength(1)
    expect(screen.getAllByTestId('agent-kind-badge-api')).toHaveLength(2)

    expect(screen.getAllByText('Project files').length).toBeGreaterThanOrEqual(2)
    expect(screen.getAllByText('This computer').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Chat-only')).toHaveLength(2)
    expect(screen.queryByText(/Text-only model/i)).toBeNull()
    expect(screen.getByText('Ready')).toBeDefined()
    expect(screen.queryByText('idle')).toBeNull()
    expect(screen.getByText('Check agent status')).toBeDefined()
    expect(screen.getByText('Check agents again to confirm status')).toBeDefined()
    expect(screen.queryByText('Status not reported')).toBeNull()
    expect(screen.queryByText('paused')).toBeNull()
    expect(screen.queryByText('Unknown')).toBeNull()
    expect(screen.queryByText('Work tool: codex')).toBeNull()
    expect(screen.getAllByText('Work tool: Codex')).toHaveLength(2)
    expect(screen.getByText('Work tool: Claude')).toBeDefined()

    expect(screen.getAllByTestId('admin-agent-row')).toHaveLength(5)
  })

  test('explains missing admin agent fields instead of showing placeholder symbols', async () => {
    render(<AgentsPanel />)

    expect(await screen.findByText('Check agents again to load owner')).toBeDefined()
    expect(screen.getAllByText('Check agents again to load project')).toHaveLength(2)
    expect(screen.queryByText('Owner not reported yet')).toBeNull()
    expect(screen.queryByText('Project not reported yet')).toBeNull()
    expect(screen.getByText('Activity appears after work starts')).toBeDefined()
    expect(screen.getByText('Check activity time')).toBeDefined()
    expect(screen.queryByText('No activity yet')).toBeNull()
    expect(screen.queryByText('Activity time needs review')).toBeNull()
    expect(screen.queryByText('Invalid Date')).toBeNull()
    expect(screen.queryByText('—')).toBeNull()
  })

  test('selecting a work location triggers the filtered fetch', async () => {
    render(<AgentsPanel />)

    const select = (await screen.findByTestId('admin-agents-runtime-filter')) as HTMLSelectElement
    fireEvent.change(select, { target: { value: 'cli' } })

    expect(setAgentRuntimeKindFilterMock).toHaveBeenCalledWith('cli')
  })

  test('renders recovery guidance when agents fail to load', async () => {
    useAdminStore.setState({ agentsError: 'HTTP 503' })

    render(<AgentsPanel />)

    const error = await screen.findByTestId('admin-agents-error')
    expect(error).toHaveAttribute('aria-live', 'polite')
    expect(within(error).getByText('Open Admin again, then choose agents.')).toBeDefined()
    expect(within(error).queryByText('HTTP 503')).toBeNull()
    expect(
      within(error).getByText(
        'Open Admin again, then choose this section. If it still fails, ask an owner or admin to check your Admin access and this Admin page.'
      )
    ).toBeDefined()
    expect(within(error).queryByText(/admin service/i)).toBeNull()
    expect(within(error).queryByText(/Admin setup/i)).toBeNull()
  })

  test('guides admins when no agents exist yet', async () => {
    useAdminStore.setState({ agents: [], agentsTotal: 0 })

    render(<AgentsPanel />)

    const guide = await screen.findByTestId('admin-agents-guide')
    expect(
      within(guide).getByText(
        'Create the first agent from Agents, then return here to check it across team spaces.'
      )
    ).toBeDefined()
    expect(
      within(guide).queryByText('No agents have been created across any team space yet.')
    ).toBeNull()

    const emptyState = await screen.findByTestId('admin-agents-empty')
    expect(within(emptyState).getByText('Create or connect an agent first')).toBeDefined()
    expect(within(emptyState).getByText(/create the first agent from agents/i)).toBeDefined()
    expect(within(emptyState).getByText(/confirm it becomes ready or working/i)).toBeDefined()
    expect(within(emptyState).getByText(/return to admin and choose agents/i)).toBeDefined()
    expect(within(emptyState).getByText(/check it across team spaces/i)).toBeDefined()
    expect(
      within(emptyState).getByText('Next step: open Agents and choose New agent.')
    ).toBeDefined()
    expect(
      within(emptyState).getByText(
        'Success looks like one agent listed here with Ready or Working now.'
      )
    ).toBeDefined()
    expect(within(emptyState).queryByText(/review it across team spaces/i)).toBeNull()
    expect(within(emptyState).queryByText(/refresh admin and check again/i)).toBeNull()
    expect(within(emptyState).queryByText('No agents to show')).toBeNull()
    expect(within(emptyState).queryByText(/organizations/i)).toBeNull()
  })

  test('guides admins to clear a work location filter before assuming an agent is missing', async () => {
    useAdminStore.setState({ agents: [], agentsTotal: 0, agentRuntimeKindFilter: 'cli' })

    render(<AgentsPanel />)

    const emptyState = await screen.findByTestId('admin-agents-empty')
    expect(within(emptyState).getByText('The current filter is hiding agents')).toBeDefined()
    expect(within(emptyState).getByText(/choose "all work locations"/i)).toBeDefined()
    expect(within(emptyState).getByText(/before assuming the agent is missing/i)).toBeDefined()
    expect(
      within(emptyState).getByText('Next step: change Work location to All work locations.')
    ).toBeDefined()
    expect(
      within(emptyState).getByText(
        'Success looks like the agent appearing here with its work location and status.'
      )
    ).toBeDefined()
    expect(within(emptyState).queryByText('No agents match this filter')).toBeNull()
  })
})
