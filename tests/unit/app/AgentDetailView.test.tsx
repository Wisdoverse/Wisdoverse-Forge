import { describe, test, expect, afterEach, beforeEach, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { useAgentsStore } from '@app/entities/agent'
import {
  agentDetailHeaderSubtitle,
  AgentDetailView,
} from '@app/widgets/agent-detail/AgentDetailView'
import type { TaskSummary } from '@app/shared/api/orchestration'

afterEach(cleanup)

const getTasksByAgentMock = vi.hoisted(() => vi.fn())

// AgentTasksTab triggers an API call on mount; we don't want unit tests to
// depend on the fetch shim, so stub it out at module level.
vi.mock('@app/shared/api/orchestration', () => ({
  orchestrationApi: { getTasksByAgent: getTasksByAgentMock },
}))

beforeEach(() => {
  useAgentsStore.getState().reset()
  getTasksByAgentMock.mockResolvedValue([])
})

const containerAgent = {
  id: 'a1',
  name: 'Build Agent',
  provider: 'Workspace Tools',
  model: 'tool-large',
  status: 'idle' as const,
  tasksCompleted: 12,
  tasksInProgress: 1,
  successRate: 0.98,
  cliTool: 'opencode' as const,
  containerId: 'c-abc',
  workspaceId: 'w1',
  workspaceName: 'Engineering',
  projectId: 'p1',
  projectName: 'Platform',
}

const providerAgent = {
  ...containerAgent,
  id: 'a2',
  name: 'Prompt Agent',
  provider: 'Prompt Service',
  model: 'general-large',
  cliTool: undefined,
  containerId: undefined,
}

const workspaceToolAgent = {
  ...containerAgent,
  id: 'a3',
  name: 'Workspace Tool Agent',
  cliTool: 'opencode',
}

const hostCliAgent = {
  ...workspaceToolAgent,
  id: 'a5',
  name: 'Local Tool Agent',
  containerId: undefined,
  runtimeId: 'host-aabbccdd',
  runtimeKind: 'cli' as const,
  cwd: '/home/operator/project',
}

function makeTask(overrides: Partial<TaskSummary>): TaskSummary {
  return {
    id: 'task-default',
    groupId: 'group-1',
    state: 'working',
    method: 'tasks/send',
    params: { task: 'Default task', message: '' },
    priority: 'normal',
    progress: 20,
    createdAt: '2026-05-24T08:00:00.000Z',
    updatedAt: '2026-05-24T08:30:00.000Z',
    ...overrides,
  }
}

describe('AgentDetailView', () => {
  test('renders agent name', () => {
    render(<AgentDetailView agent={containerAgent} onBack={() => {}} />)
    expect(screen.getByText('Build Agent')).toBeDefined()
  })

  test('agent header summarizes work location without raw model names', () => {
    expect(agentDetailHeaderSubtitle(containerAgent)).toBe('OpenCode in a managed workspace')

    render(<AgentDetailView agent={containerAgent} onBack={() => {}} />)

    expect(screen.getAllByText('OpenCode in a managed workspace').length).toBeGreaterThan(0)
    expect(screen.queryByText('tool-large')).toBeNull()
  })

  test('workspace tool agent shows the live work tab and labels chat as History', () => {
    render(<AgentDetailView agent={containerAgent} onBack={() => {}} />)
    expect(screen.getByRole('button', { name: 'Overview' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Tasks' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'History' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Live work' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Tools' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Instructions' })).toBeDefined()
    expect(screen.queryByRole('button', { name: ['Plug', 'ins'].join('') })).toBeNull()
  })

  test('alternate workspace tool agent still shows the live work tab', () => {
    render(<AgentDetailView agent={workspaceToolAgent} onBack={() => {}} />)
    expect(screen.getByRole('button', { name: 'Live work' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'History' })).toBeDefined()
  })

  test('workspace tool agent keeps the live work tab while the environment is pending', () => {
    render(
      <AgentDetailView
        agent={{ ...workspaceToolAgent, id: 'a4', name: 'Pending Agent', containerId: undefined }}
        onBack={() => {}}
      />
    )
    expect(screen.getByText('Waiting to start')).toBeDefined()
  })

  test('shows readable status labels in agent details', () => {
    render(<AgentDetailView agent={containerAgent} onBack={() => {}} />)

    const statusValues = screen.getAllByText('Ready')
    expect(statusValues.length).toBeGreaterThan(0)
    expect(screen.queryByText('idle')).toBeNull()
  })

  test('labels unknown agent statuses without exposing raw backend values', () => {
    render(
      <AgentDetailView
        agent={{ ...containerAgent, status: 'warming_up' as never }}
        onBack={() => {}}
      />
    )

    expect(screen.getAllByText('Status needs review').length).toBeGreaterThan(0)
    expect(screen.queryByText(/warming_up/i)).toBeNull()
    expect(screen.queryByText(/warming up/i)).toBeNull()
  })

  test('prompt agent hides live work and labels chat as Chat', () => {
    render(<AgentDetailView agent={providerAgent} onBack={() => {}} />)
    expect(screen.getByRole('button', { name: 'Overview' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Tasks' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Chat' })).toBeDefined()
    expect(screen.queryByRole('button', { name: 'Live work' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Command window' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'History' })).toBeNull()
  })

  test('agent joined from this computer is managed without live work actions', () => {
    render(<AgentDetailView agent={hostCliAgent} onBack={() => {}} />)
    expect(screen.getAllByText(/This computer/).length).toBeGreaterThan(0)
    expect(screen.getByRole('button', { name: 'History' })).toBeDefined()
    expect(screen.queryByRole('button', { name: 'Live work' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Command window' })).toBeNull()
    expect(screen.getByText('Connection')).toBeDefined()
    expect(screen.getByText('Connected from this computer')).toBeDefined()
    expect(screen.queryByText('host-aabbccdd')).toBeNull()
    expect(screen.getByText(/this computer does the work/i)).toBeDefined()
    expect(screen.getByText(/folder where you ran the setup command/i)).toBeDefined()
    expect(screen.queryByText(/connection command/i)).toBeNull()
    expect(screen.queryByRole('button', { name: /start agent/i })).toBeNull()
  })

  test('prompt agent ignores stale environment ids for live work visibility', () => {
    render(
      <AgentDetailView
        agent={{ ...providerAgent, containerId: 'stale-provider-container' }}
        onBack={() => {}}
      />
    )
    expect(screen.getByRole('button', { name: 'Chat' })).toBeDefined()
    expect(screen.queryByRole('button', { name: 'Live work' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Command window' })).toBeNull()
  })

  test('shows overview stats by default', () => {
    render(<AgentDetailView agent={containerAgent} onBack={() => {}} />)
    expect(screen.getByText('12')).toBeDefined()
    expect(screen.getByText('98%')).toBeDefined()
    expect(screen.getByText('Work setup')).toBeDefined()
    expect(screen.getAllByText('Managed workspace').length).toBeGreaterThan(0)
  })

  test('foregrounds assignment fit on the agent overview', () => {
    render(
      <AgentDetailView
        agent={{ ...containerAgent, currentTask: 'Implement onboarding flow' }}
        onBack={() => {}}
      />
    )
    expect(screen.getByTestId('agent-assignment-fit')).toBeDefined()
    expect(screen.getByText('Can be assigned now')).toBeDefined()
    expect(screen.getByText('Implement onboarding flow')).toBeDefined()
    expect(screen.getByText('Finish a task, then save useful steps.')).toBeDefined()
    expect(screen.queryByText(/task\s+context/i)).toBeNull()
  })

  test('guides an idle agent toward a first safe task', () => {
    render(<AgentDetailView agent={containerAgent} onBack={() => {}} />)

    expect(screen.getByTestId('agent-next-step')).toBeDefined()
    expect(screen.getAllByText('Ready').length).toBeGreaterThan(0)
    expect(screen.getByText('Ready for a task')).toBeDefined()
    expect(screen.getByText('Send a task to create the first update.')).toBeDefined()
    expect(screen.getByText('Send a small first task')).toBeDefined()
    expect(screen.getByText(/Choose this agent, or choose a task queue/i)).toBeDefined()
    expect(screen.queryByText('No active task')).toBeNull()
    expect(screen.queryByText('No recent task updates')).toBeNull()
    expect(screen.queryByText(/unassigned/i)).toBeNull()
    expect(screen.getByRole('button', { name: /open tasks/i })).toBeDefined()
  })

  test('guides active work into the Tasks tab', async () => {
    getTasksByAgentMock.mockResolvedValueOnce([
      makeTask({
        id: 'active-task',
        state: 'working',
        params: { task: 'Fix onboarding copy', message: '' },
      }),
    ])

    render(<AgentDetailView agent={{ ...containerAgent, status: 'working' }} onBack={() => {}} />)

    expect(await screen.findByText('Review Current Work')).toBeDefined()
    expect(screen.getAllByText(/Fix onboarding copy/).length).toBeGreaterThan(0)
    expect(
      screen.getByText(
        'Build Agent is already handling "Fix onboarding copy". Open Tasks to follow progress or handle anything that needs your help.'
      )
    ).toBeDefined()
    expect(screen.getByText(/Success: You can see the active task state/)).toBeDefined()
    expect(screen.queryByText(new RegExp(['unblock', 'it'].join('\\s+'), 'i'))).toBeNull()
    expect(screen.queryByText(/owner input/i)).toBeNull()
    expect(screen.getByText('Do This Next')).toBeDefined()
  })

  test('guides pending managed workspace agents to the live work tab', () => {
    render(
      <AgentDetailView
        agent={{
          ...workspaceToolAgent,
          id: 'pending-offline',
          status: 'offline',
          containerId: undefined,
        }}
        onBack={() => {}}
      />
    )

    expect(screen.getByText('Start this workspace')).toBeDefined()
    expect(
      screen.getByText(
        'Open Live work, choose Start workspace, and wait until this agent shows Ready before sending file work.'
      )
    ).toBeDefined()
    fireEvent.click(screen.getByRole('button', { name: /open live work/i }))
    expect(screen.getByText('Start this workspace to open Live work')).toBeDefined()
    expect(screen.getByText(/start the workspace before this agent works on files/i)).toBeDefined()
    expect(
      screen.getByText(/success looks like the agent status changing to ready or working/i)
    ).toBeDefined()
    expect(screen.getByText(/ask an owner or admin to check this agent setup/i)).toBeDefined()
    expect(screen.queryByText(/open terminal/i)).toBeNull()
    expect(screen.queryByText(/terminal access/i)).toBeNull()
    expect(screen.queryByText(/live terminal/i)).toBeNull()
    expect(screen.queryByText(/command window/i)).toBeNull()
    expect(screen.getByRole('button', { name: /start workspace/i })).toBeDefined()
  })

  test('shows start failure guidance without raw setup details', () => {
    useAgentsStore.setState({
      error: 'Docker socket refused',
      startAgent: vi.fn(async () => false),
    } as never)

    render(
      <AgentDetailView
        agent={{
          ...workspaceToolAgent,
          id: 'pending-error',
          status: 'offline',
          containerId: undefined,
        }}
        onBack={() => {}}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: /open live work/i }))

    const alert = screen.getByRole('alert')
    expect(alert).toHaveTextContent('Start did not finish')
    expect(alert).toHaveTextContent('ask an owner or admin to check this agent setup')
    expect(alert.textContent).not.toContain('Details:')
    expect(alert.textContent).not.toContain('Docker socket refused')
  })

  test('recovers the start button when starting the workspace fails unexpectedly', async () => {
    useAgentsStore.setState({
      error: null,
      startAgent: vi.fn(async () => {
        throw new Error('socket hang up')
      }),
    } as never)

    render(
      <AgentDetailView
        agent={{
          ...workspaceToolAgent,
          id: 'pending-start-reject',
          status: 'offline',
          containerId: undefined,
        }}
        onBack={() => {}}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: /open live work/i }))
    const startButton = screen.getByRole('button', { name: /start workspace/i })
    fireEvent.click(startButton)

    await waitFor(() => expect(startButton).not.toBeDisabled())

    const alert = screen.getByRole('alert')
    expect(alert).toHaveTextContent('Start did not finish')
    expect(alert).toHaveTextContent('try once more')
    expect(alert).toHaveTextContent('ask an owner or admin to check this agent setup')
    expect(alert.textContent).not.toContain('socket hang up')
  })

  test('guides offline agents joined from this computer back to the local connection', () => {
    render(<AgentDetailView agent={{ ...hostCliAgent, status: 'offline' }} onBack={() => {}} />)

    expect(screen.getByText('Run the setup command on this computer again')).toBeDefined()
    expect(screen.getByText(/open Terminal or PowerShell in the project folder/i)).toBeDefined()
    expect(screen.getByText(/keep that window open/i)).toBeDefined()
    expect(screen.getByText('Run setup again on this computer')).toBeDefined()
    expect(screen.queryByText('Connected from this computer')).toBeNull()
    expect(screen.queryByRole('button', { name: /open terminal/i })).toBeNull()
  })

  test('explains workspace access and primary project context', () => {
    render(<AgentDetailView agent={{ ...containerAgent, cwd: '/workspace' }} onBack={() => {}} />)
    expect(screen.getByText('Starting folder')).toBeDefined()
    expect(screen.getByText('Workspace project folder')).toBeDefined()
    expect(screen.getByText('Connection')).toBeDefined()
    expect(screen.getByText('Ready in managed workspace')).toBeDefined()
    expect(screen.queryByText('/workspace')).toBeNull()
    expect(screen.queryByText('c-abc')).toBeNull()
    expect(screen.getAllByText('Where it works').length).toBeGreaterThan(0)
    expect(screen.getAllByText('OpenCode in a managed workspace').length).toBeGreaterThan(0)
    expect(screen.queryByText('opencode managed workspace')).toBeNull()
    expect(screen.getByText('Workspace it can use')).toBeDefined()
    expect(screen.getByText('Engineering')).toBeDefined()
    expect(screen.getByText('Starting project for tasks')).toBeDefined()
    expect(screen.getByText('Platform')).toBeDefined()
    expect(screen.getByText(/can include several projects/i)).toBeDefined()
    expect(screen.getByText(/starting project for new tasks/i)).toBeDefined()
    expect(screen.getByText(/files must be kept apart/i)).toBeDefined()
  })

  test('explains chat-only agents do not open workspace files', () => {
    render(<AgentDetailView agent={providerAgent} onBack={() => {}} />)
    expect(screen.getByText('Starting folder')).toBeDefined()
    expect(screen.getAllByText('No file access needed').length).toBeGreaterThan(0)
    expect(screen.getByText('Connection')).toBeDefined()
    expect(screen.getByText('Uses AI service connection')).toBeDefined()
    expect(screen.getAllByText('Chat-only AI service').length).toBeGreaterThan(0)
    expect(screen.getByText(/can plan, write, and review in chat/i)).toBeDefined()
    expect(screen.getByText(/cannot open project files on its own/i)).toBeDefined()
    expect(
      screen.getByText(/choose an agent on this computer or a managed workspace agent/i)
    ).toBeDefined()
    expect(screen.getByText(/settings shows whether the AI service is ready/i)).toBeDefined()
    expect(screen.queryByText('Not needed for this agent')).toBeNull()
    expect(screen.queryByText('Not needed')).toBeNull()
    expect(screen.queryByText(/model provider/i)).toBeNull()
    expect(screen.queryByText(/text-only model/i)).toBeNull()
    expect(screen.queryByText(/model service/i)).toBeNull()
  })

  test('keeps review-needed AI service labels readable in agent details', () => {
    render(
      <AgentDetailView
        agent={{ ...providerAgent, provider: 'AI service needs review' }}
        onBack={() => {}}
      />
    )

    expect(screen.getAllByText('AI service needs review').length).toBeGreaterThan(0)
    expect(screen.queryByText(/AI service needs review AI service/i)).toBeNull()
  })

  test('shows back button', () => {
    render(<AgentDetailView agent={containerAgent} onBack={() => {}} />)
    expect(screen.getByTestId('agent-back')).toBeDefined()
  })
})
