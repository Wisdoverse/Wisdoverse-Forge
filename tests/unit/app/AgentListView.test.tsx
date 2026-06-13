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
    expect(screen.getByText('Agents')).toBeDefined()
    expect(screen.queryByText('Agent Fleet')).toBeNull()
    expect(screen.getByText(/create your first agent/i)).toBeDefined()
    expect(screen.getByText(/chat-only AI service for planning and review/i)).toBeDefined()
    expect(screen.getByText(/files and commands on your machine/i)).toBeDefined()
    expect(screen.queryByText(/connected model for text-only work/i)).toBeNull()
    expect(screen.getAllByRole('button', { name: /new agent/i }).length).toBeGreaterThan(0)
  })

  test('waits for a selected project before showing a command for this computer', () => {
    render(<AgentListView />)

    const enrollment = screen.getByTestId('host-cli-enrollment-panel')
    expect(within(enrollment).getByText(/connect this computer/i)).toBeDefined()
    expect(enrollment.textContent).toContain('files or commands on your computer')
    expect(enrollment.textContent).toContain('manages it with your other agents')
    expect(enrollment.textContent).toContain('This computer')
    expect(enrollment.textContent).toContain('If the button does not work')
    expect(enrollment.textContent).toContain(
      'Use this backup if your browser cannot open the setup window'
    )
    expect(enrollment.textContent).toContain('your team asks you to run a command')
    expect(enrollment.textContent).toContain('choose New agent on this computer above')
    expect(enrollment.textContent).toContain('Computer type')
    expect(
      within(enrollment).getByRole('group', { name: /choose this computer type/i })
    ).toBeDefined()
    expect(within(enrollment).getByText(/project:/i)).toBeDefined()
    expect(within(enrollment).getByTestId('host-cli-project-label')).toHaveTextContent(
      'Project: Choose a project from the sidebar first.'
    )
    expect(enrollment.textContent).not.toContain('<project-id>')
    expect(enrollment.textContent).not.toMatch(
      new RegExp(['select', 'a project first'].join(' '), 'i')
    )
    expect(enrollment.textContent).not.toContain('Advanced:')
    expect(enrollment.textContent).not.toContain('Manual setup for this computer')
    expect(enrollment.textContent).not.toContain('Forge CLI')
    expect(enrollment.textContent).not.toContain('Platform CLI')
    expect(enrollment.textContent).not.toContain('Host CLI platform')
    expect(enrollment.textContent).not.toContain('Connect a Local Agent')
    expect(enrollment.textContent).not.toContain('Already installed the setup tool')
    expect(within(enrollment).getByTestId('host-cli-command-waiting')).toHaveTextContent(
      /project tells Forge where this computer can receive tasks/i
    )
    expect(within(enrollment).getByTestId('host-cli-command-waiting')).toHaveTextContent(
      /setup command appears here/i
    )
    expect(enrollment.textContent).not.toContain('agentforge agents enroll-local')
    expect(within(enrollment).getByRole('button', { name: /choose project first/i })).toBeDisabled()
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
    expect(within(enrollment).getByText(/agent needs files or commands/i)).toBeDefined()
    expect(enrollment.textContent).toContain('Forge shows it here')
    expect(within(enrollment).getByTestId('host-cli-project-label')).toHaveTextContent(
      'Project: Platform'
    )
    expect(within(enrollment).getByTestId('host-cli-project-label')).not.toHaveTextContent('p1')
    expect(enrollment.textContent).toContain('p1')
    expect(enrollment.textContent).toContain('agentforge agents enroll-local')
    expect(enrollment.textContent).toContain('--name "This Computer Codex"')
    expect(enrollment.textContent).toContain('--tool codex')
    expect(enrollment.textContent).toContain('--project p1')
    expect(enrollment.textContent).toContain('Open Terminal on macOS/Linux or PowerShell')
    expect(enrollment.textContent).toContain('Copy this setup command and paste it there')
    expect(enrollment.textContent).toContain('Leave the work tool as Codex unless')
    expect(within(enrollment).getByTestId('host-cli-success-hint')).toHaveTextContent(
      /new agent named This Computer Codex appears in this list/i
    )
    expect(within(enrollment).getByTestId('host-cli-success-hint')).toHaveTextContent(
      /keep the command window open/i
    )
    expect(enrollment.textContent).not.toContain('Run this manual command')
    expect(enrollment.textContent).not.toContain('Change codex only if')
    expect(within(enrollment).getByRole('button', { name: /copy setup command/i })).toBeDefined()

    fireEvent.click(within(enrollment).getByRole('button', { name: /windows/i }))
    expect(enrollment.textContent).toContain('--shell-format powershell')
    expect(enrollment.textContent).toContain('--cwd "$($PWD.Path)"')
    expect(enrollment.textContent).toContain('--name "This Computer Codex"')
    expect(enrollment.textContent).not.toContain('Host Codex')
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
    expect(screen.getByPlaceholderText(/AI services, projects/i)).toBeDefined()
    expect(screen.queryByPlaceholderText(/models, projects/i)).toBeNull()

    fireEvent.change(screen.getByTestId('agent-search'), { target: { value: 'review' } })
    expect(screen.getByText('Review Analyst')).toBeDefined()
    expect(screen.queryByText('Build Runner')).toBeNull()

    fireEvent.change(screen.getByTestId('agent-search'), { target: { value: '' } })
    const workLocationFilters = screen.getByRole('group', { name: /work location filter/i })
    expect(within(workLocationFilters).queryByRole('button', { name: /text only\s*1/i })).toBeNull()
    fireEvent.click(
      within(workLocationFilters).getByRole('button', { name: /chat-only AI service\s*1/i })
    )
    expect(screen.getByText('Review Analyst')).toBeDefined()
    expect(screen.queryByText('Build Runner')).toBeNull()

    fireEvent.click(within(workLocationFilters).getByRole('button', { name: /this computer\s*1/i }))
    expect(screen.getByText('Local Agent')).toBeDefined()
    expect(screen.queryByText('Build Runner')).toBeNull()

    fireEvent.click(within(workLocationFilters).getByRole('button', { name: /all agents\s*4/i }))
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
    const emptyState = screen.getByTestId('agent-filter-empty')
    expect(within(emptyState).getByText('Search is hiding every agent')).toBeDefined()
    expect(within(emptyState).getByText(/none match the words you typed/i)).toBeDefined()
    expect(within(emptyState).getByText(/before creating another one/i)).toBeDefined()
    expect(emptyState.textContent).not.toContain('No Agents Match This View')
    expect(emptyState.textContent).not.toContain('review every agent')

    fireEvent.click(within(emptyState).getByRole('button', { name: /show all agents/i }))
    expect(screen.getByTestId('agent-search')).toHaveValue('')
    expect(screen.getByText('Build Runner')).toBeDefined()
  })

  test('explains when a status filter hides every agent', () => {
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

    const statusFilters = screen.getByRole('group', { name: /status filter/i })
    fireEvent.click(within(statusFilters).getByRole('button', { name: /offline\s*0/i }))
    const emptyState = screen.getByTestId('agent-filter-empty')
    expect(within(emptyState).getByText('This status filter hides every agent')).toBeDefined()
    expect(within(emptyState).getByText(/another status/i)).toBeDefined()
    expect(within(emptyState).getByText(/before deciding nobody is available/i)).toBeDefined()
    expect(emptyState.textContent).not.toContain('No Agents Match This View')

    fireEvent.click(within(emptyState).getByRole('button', { name: /show all agents/i }))
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
          description:
            'This task queue gives project tasks a place to wait for an available agent.',
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
      'block release'
    )

    fireEvent.click(screen.getByRole('button', { name: /^create task queue$/i }))

    await waitFor(() =>
      expect(createAgentGroup).toHaveBeenCalledWith(
        'p1',
        expect.objectContaining({
          name: 'Review Queue',
          description: expect.stringContaining('block release'),
        })
      )
    )
    expect(useBoardStore.getState().selectedGroupId).toBe('g-review')
  })
})
