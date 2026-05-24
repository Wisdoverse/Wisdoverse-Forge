import { describe, test, expect, afterEach, beforeEach, vi } from 'vitest'
import { render, screen, cleanup, fireEvent, waitFor, within } from '@testing-library/react'
import { AgentListView } from '@app/features/agents/AgentListView'
import { useAgentsStore } from '@app/shared/model/agents.store'
import { useBoardStore } from '@app/shared/model/board.store'
import { useNavigationStore } from '@app/entities/navigation'
import type { AgentInfo } from '@app/shared/model/agents.store'

function makeAgent(overrides: Partial<AgentInfo>): AgentInfo {
  return {
    id: 'agent-default',
    name: 'Default Agent',
    provider: 'OpenAI',
    model: 'gpt-5',
    status: 'idle',
    tasksCompleted: 0,
    tasksInProgress: 0,
    successRate: 0,
    ...overrides,
  }
}

afterEach(() => {
  cleanup()
  useAgentsStore.getState().reset()
  useBoardStore.getState().reset()
  useNavigationStore.getState().reset()
})

beforeEach(() => {
  useAgentsStore.getState().reset()
  useBoardStore.getState().reset()
  useNavigationStore.getState().reset()
  useAgentsStore.setState({ loadAgents: vi.fn(async () => undefined) })
})

describe('AgentListView', () => {
  test('shows empty state when no agents', () => {
    render(<AgentListView />)
    expect(screen.getByText(/no agents/i)).toBeDefined()
  })

  test('renders agent cards', () => {
    useAgentsStore.getState().setAgents([
      makeAgent({
        id: 'a1',
        name: 'Claude-1',
        provider: 'Anthropic',
        model: 'claude-4-opus',
        status: 'working',
        tasksCompleted: 12,
        tasksInProgress: 1,
        successRate: 0.98,
      }),
      makeAgent({
        id: 'a2',
        name: 'Gemini-1',
        provider: 'Google',
        model: 'gemini-2.5-pro',
        status: 'offline',
        tasksCompleted: 5,
        tasksInProgress: 0,
        successRate: 0.9,
      }),
    ])
    render(<AgentListView />)
    expect(screen.getByText('Claude-1')).toBeDefined()
    expect(screen.getByText('Gemini-1')).toBeDefined()
  })

  test('shows agent status indicators', () => {
    useAgentsStore.getState().setAgents([
      makeAgent({
        id: 'a1',
        name: 'Claude-1',
        provider: 'Anthropic',
        model: 'claude-4-opus',
        status: 'idle',
        tasksCompleted: 12,
        tasksInProgress: 1,
        successRate: 0.98,
      }),
    ])
    render(<AgentListView />)
    expect(screen.getByTestId('agent-status-a1')).toBeDefined()
  })

  test('filters agent fleet by search, runtime, and status', () => {
    useAgentsStore.getState().setAgents([
      makeAgent({
        id: 'cli-agent',
        name: 'Build Runner',
        provider: 'OpenAI',
        model: 'codex',
        cliTool: 'codex',
        status: 'working',
        projectName: 'Platform',
        tasksInProgress: 2,
      }),
      makeAgent({
        id: 'provider-agent',
        name: 'Review Analyst',
        provider: 'Anthropic',
        model: 'claude-4-opus',
        status: 'idle',
        projectName: 'Review',
      }),
      makeAgent({
        id: 'offline-agent',
        name: 'Legacy Worker',
        provider: 'Google',
        model: 'gemini-2.5-pro',
        cliTool: 'gemini',
        status: 'offline',
      }),
    ])

    render(<AgentListView />)

    fireEvent.change(screen.getByTestId('agent-search'), { target: { value: 'review' } })
    expect(screen.getByText('Review Analyst')).toBeDefined()
    expect(screen.queryByText('Build Runner')).toBeNull()

    fireEvent.change(screen.getByTestId('agent-search'), { target: { value: '' } })
    const runtimeFilters = screen.getByRole('group', { name: /runtime filter/i })
    fireEvent.click(within(runtimeFilters).getByRole('button', { name: /provider\s*1/i }))
    expect(screen.getByText('Review Analyst')).toBeDefined()
    expect(screen.queryByText('Build Runner')).toBeNull()

    fireEvent.click(within(runtimeFilters).getByRole('button', { name: /all runtimes\s*3/i }))
    const statusFilters = screen.getByRole('group', { name: /status filter/i })
    fireEvent.click(within(statusFilters).getByRole('button', { name: /offline\s*1/i }))
    expect(screen.getByText('Legacy Worker')).toBeDefined()
    expect(screen.queryByText('Review Analyst')).toBeNull()
  })

  test('shows filter empty state and clears filters', () => {
    useAgentsStore.getState().setAgents([
      makeAgent({
        id: 'cli-agent',
        name: 'Build Runner',
        provider: 'OpenAI',
        model: 'codex',
        cliTool: 'codex',
        status: 'working',
      }),
    ])

    render(<AgentListView />)

    fireEvent.change(screen.getByTestId('agent-search'), { target: { value: 'missing' } })
    expect(screen.getByTestId('agent-filter-empty')).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /clear filters/i }))
    expect(screen.getByText('Build Runner')).toBeDefined()
  })

  test('shows + New Agent button', () => {
    render(<AgentListView />)
    // Both the toolbar and the empty-state CTA render "New Agent"
    expect(screen.getAllByText(/new agent/i).length).toBeGreaterThan(0)
  })

  test('creates a task group from the selected project context', async () => {
    const createAgentGroup = vi.fn(
      async (projectId: string, input: { name: string; description?: string }) => {
        const group = { id: 'g-new', name: input.name, projectId }
        useNavigationStore.setState({ agentGroups: [group] })
        useBoardStore.getState().setSelectedGroupId(group.id)
        return group
      }
    )
    useNavigationStore.setState({
      selectedProjectId: 'p1',
      projects: {
        t1: [
          {
            id: 'p1',
            teamId: 't1',
            name: 'Platform',
            slug: 'platform',
            color: '#007AFF',
            description: '',
          },
        ],
      },
      agentGroups: [],
      createAgentGroup,
    } as never)

    render(<AgentListView />)

    expect(screen.getByText('Task Routing')).toBeDefined()
    fireEvent.change(screen.getByLabelText(/task group name/i), {
      target: { value: 'Frontend Delivery' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^create$/i }))

    await waitFor(() =>
      expect(createAgentGroup).toHaveBeenCalledWith(
        'p1',
        expect.objectContaining({
          name: 'Frontend Delivery',
          description: 'Agents in this group can receive tasks from the board.',
        })
      )
    )
    expect(useBoardStore.getState().selectedGroupId).toBe('g-new')
    expect(screen.getByRole('button', { name: /frontend delivery/i })).toHaveAttribute(
      'aria-pressed',
      'true'
    )
  })

  test('applies a task group template before creating routing', async () => {
    const createAgentGroup = vi.fn(
      async (projectId: string, input: { name: string; description?: string }) => {
        const group = { id: 'g-review', name: input.name, projectId }
        useNavigationStore.setState({ agentGroups: [group] })
        useBoardStore.getState().setSelectedGroupId(group.id)
        return group
      }
    )
    useNavigationStore.setState({
      selectedProjectId: 'p1',
      projects: {
        t1: [
          {
            id: 'p1',
            teamId: 't1',
            name: 'Platform',
            slug: 'platform',
            color: '#007AFF',
            description: '',
          },
        ],
      },
      agentGroups: [],
      createAgentGroup,
    } as never)

    render(<AgentListView />)

    const templates = screen.getByRole('group', { name: /task group templates/i })
    fireEvent.click(within(templates).getByRole('button', { name: /review/i }))

    expect(screen.getByLabelText(/task group name/i)).toHaveValue('Review Group')
    expect((screen.getByLabelText(/task group description/i) as HTMLInputElement).value).toContain(
      'release risk'
    )

    fireEvent.click(screen.getByRole('button', { name: /^create$/i }))

    await waitFor(() =>
      expect(createAgentGroup).toHaveBeenCalledWith(
        'p1',
        expect.objectContaining({
          name: 'Review Group',
          description: expect.stringContaining('release risk'),
        })
      )
    )
    expect(useBoardStore.getState().selectedGroupId).toBe('g-review')
  })
})
