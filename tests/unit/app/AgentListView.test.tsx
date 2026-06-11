import { describe, test, expect, afterEach, beforeEach, vi } from 'vitest'
import { render, screen, cleanup, fireEvent, waitFor, within } from '@testing-library/react'
import { AgentListView } from '@app/features/agents/AgentListView'
import { useAgentsStore } from '@app/entities/agent'
import { useBoardStore } from '@app/shared/model/board.store'
import { useNavigationStore } from '@app/entities/navigation'
import type { AgentInfo } from '@app/entities/agent'

function makeAgent(overrides: Partial<AgentInfo>): AgentInfo {
  return {
    id: 'agent-default',
    name: 'Default Agent',
    provider: 'Model Service',
    model: 'general-model',
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
    const emptyState = screen.getByTestId('agent-empty-state')
    expect(within(emptyState).getByText(/create your first agent/i)).toBeDefined()
    expect(within(emptyState).getByText(/start with chat-only/i)).toBeDefined()
    expect(within(emptyState).getByText(/managed workspace or this computer/i)).toBeDefined()
    expect(within(emptyState).getByText(/success looks like one ready agent/i)).toBeDefined()
    expect(within(emptyState).queryByText(/text only/i)).toBeNull()
  })

  test('waits for a selected project before showing a command for this computer', () => {
    render(<AgentListView />)

    const enrollment = screen.getByTestId('host-cli-enrollment-panel')
    expect(within(enrollment).getByText(/starting project/i)).toBeDefined()
    expect(within(enrollment).getByText('Select a project first')).toBeDefined()
    expect(within(enrollment).getByLabelText(/work tool on this computer/i)).toHaveValue('codex')
    expect(within(enrollment).getByText(/Choose the tool you already use here/i)).toBeDefined()
    expect(within(enrollment).getByTestId('host-cli-command-waiting')).toHaveTextContent(
      /this panel will show the command to copy/i
    )
    expect(enrollment.textContent).not.toContain('<project-id>')
    expect(enrollment.textContent).not.toContain('agentforge agents enroll-local')
    expect(within(enrollment).getByRole('button', { name: /select project first/i })).toBeDisabled()
  })

  test('shows beginner command steps for adding this computer to the selected project', () => {
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
    } as never)

    render(<AgentListView />)

    const enrollment = screen.getByTestId('host-cli-enrollment-panel')
    expect(within(enrollment).getByText('Add This Computer to Forge')).toBeDefined()
    expect(
      within(enrollment).getByText(/files or tool sign-in already on this computer/i)
    ).toBeDefined()
    expect(within(enrollment).getByText('Project selected')).toBeDefined()
    expect(enrollment.textContent).not.toContain('Project: p1')
    expect(enrollment.textContent).toContain('agentforge agents enroll-local')
    expect(enrollment.textContent).toContain('--tool codex')
    expect(enrollment.textContent).toContain('--name "Codex on this computer"')
    expect(enrollment.textContent).toContain('--project p1')
    expect(enrollment.textContent).not.toContain('<tool-name>')
    expect(enrollment.textContent).toContain('Choose the work tool above')
    expect(enrollment.textContent).toContain('Open Terminal or PowerShell in the project folder')
    expect(enrollment.textContent).toContain('keep that window open while work runs')
    expect(within(enrollment).getByRole('button', { name: /copy command to run/i })).toBeDefined()

    fireEvent.change(within(enrollment).getByLabelText(/work tool on this computer/i), {
      target: { value: 'opencode' },
    })
    expect(enrollment.textContent).toContain('--tool opencode')
    expect(enrollment.textContent).toContain('--name "OpenCode on this computer"')

    fireEvent.click(within(enrollment).getByRole('button', { name: /windows/i }))
    expect(enrollment.textContent).toContain('--shell-format powershell')
    expect(enrollment.textContent).toContain('--cwd "$($PWD.Path)"')
  })

  test('renders agent cards', () => {
    useAgentsStore.getState().setAgents([
      makeAgent({
        id: 'a1',
        name: 'Review Agent',
        provider: 'Review Model',
        model: 'review-model',
        status: 'working',
        tasksCompleted: 12,
        tasksInProgress: 1,
        successRate: 0.98,
      }),
      makeAgent({
        id: 'a2',
        name: 'Draft Agent',
        provider: 'Draft Model',
        model: 'draft-model',
        status: 'offline',
        tasksCompleted: 5,
        tasksInProgress: 0,
        successRate: 0.9,
      }),
    ])
    render(<AgentListView />)
    expect(screen.getByText('Review Agent')).toBeDefined()
    expect(screen.getByText('Draft Agent')).toBeDefined()
  })

  test('shows agent status indicators', () => {
    useAgentsStore.getState().setAgents([
      makeAgent({
        id: 'a1',
        name: 'Review Agent',
        provider: 'Review Model',
        model: 'review-model',
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
        provider: 'Workspace Model',
        model: 'workspace-runner',
        cliTool: 'workspace-tool' as never,
        status: 'working',
        projectName: 'Platform',
        tasksInProgress: 2,
      }),
      makeAgent({
        id: 'provider-agent',
        name: 'Review Analyst',
        provider: 'Text Model',
        model: 'text-review-model',
        status: 'idle',
        projectName: 'Review',
      }),
      makeAgent({
        id: 'offline-agent',
        name: 'Legacy Worker',
        provider: 'Legacy Model',
        model: 'legacy-runner',
        cliTool: 'workspace-tool' as never,
        status: 'offline',
      }),
      makeAgent({
        id: 'host-agent',
        name: 'Local Agent',
        provider: 'Local Model',
        model: 'local-runner',
        cliTool: 'workspace-tool' as never,
        runtimeId: 'host-abc12345',
        runtimeKind: 'cli',
        status: 'idle',
      }),
    ])

    render(<AgentListView />)

    fireEvent.change(screen.getByTestId('agent-search'), { target: { value: 'review' } })
    expect(screen.getByText('Review Analyst')).toBeDefined()
    expect(screen.queryByText('Build Runner')).toBeNull()

    fireEvent.change(screen.getByTestId('agent-search'), { target: { value: '' } })
    const runtimeFilters = screen.getByRole('group', { name: /work type filter/i })
    fireEvent.click(within(runtimeFilters).getByRole('button', { name: /chat-only agent\s*1/i }))
    expect(screen.getByText('Review Analyst')).toBeDefined()
    expect(screen.queryByText('Build Runner')).toBeNull()

    fireEvent.click(within(runtimeFilters).getByRole('button', { name: /this computer\s*1/i }))
    expect(screen.getByText('Local Agent')).toBeDefined()
    expect(screen.queryByText('Build Runner')).toBeNull()

    fireEvent.click(within(runtimeFilters).getByRole('button', { name: /all agents\s*4/i }))
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
        provider: 'Workspace Model',
        model: 'workspace-runner',
        cliTool: 'workspace-tool' as never,
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

  test('creates a task queue from the selected project context', async () => {
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

    expect(screen.getByText('Task Queues')).toBeDefined()
    expect(screen.getByText(/task queues are simple places agents check for tasks/i)).toBeDefined()
    fireEvent.change(screen.getByLabelText(/task queue name/i), {
      target: { value: 'Frontend Delivery' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^create task queue$/i }))

    await waitFor(() =>
      expect(createAgentGroup).toHaveBeenCalledWith(
        'p1',
        expect.objectContaining({
          name: 'Frontend Delivery',
          description: 'This task queue lets agents receive board tasks.',
        })
      )
    )
    expect(useBoardStore.getState().selectedGroupId).toBe('g-new')
    expect(screen.getByRole('button', { name: /frontend delivery/i })).toHaveAttribute(
      'aria-pressed',
      'true'
    )
  })

  test('applies a task queue template before creating routing', async () => {
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

    const templates = screen.getByRole('group', { name: /task queue templates/i })
    fireEvent.click(within(templates).getByRole('button', { name: /review/i }))

    expect(screen.getByLabelText(/task queue name/i)).toHaveValue('Review Queue')
    expect((screen.getByLabelText(/task queue description/i) as HTMLInputElement).value).toContain(
      'release risk'
    )

    fireEvent.click(screen.getByRole('button', { name: /^create task queue$/i }))

    await waitFor(() =>
      expect(createAgentGroup).toHaveBeenCalledWith(
        'p1',
        expect.objectContaining({
          name: 'Review Queue',
          description: expect.stringContaining('release risk'),
        })
      )
    )
    expect(useBoardStore.getState().selectedGroupId).toBe('g-review')
  })
})
