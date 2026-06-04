import { describe, test, expect, afterEach, beforeEach, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { AgentDetailView } from '@app/widgets/agent-detail/AgentDetailView'
import type { TaskSummary } from '@app/shared/api/orchestration'

afterEach(cleanup)

const getTasksByAgentMock = vi.hoisted(() => vi.fn())

// AgentTasksTab triggers an API call on mount; we don't want unit tests to
// depend on the fetch shim, so stub it out at module level.
vi.mock('@app/shared/api/orchestration', () => ({
  orchestrationApi: { getTasksByAgent: getTasksByAgentMock },
}))

beforeEach(() => {
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

  test('workspace tool agent shows the Console tab and labels chat as History', () => {
    render(<AgentDetailView agent={containerAgent} onBack={() => {}} />)
    expect(screen.getByRole('button', { name: 'Overview' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Tasks' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'History' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Console' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Plugins' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Instructions' })).toBeDefined()
  })

  test('alternate workspace tool agent still shows the Console tab', () => {
    render(<AgentDetailView agent={workspaceToolAgent} onBack={() => {}} />)
    expect(screen.getByRole('button', { name: 'Console' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'History' })).toBeDefined()
  })

  test('workspace tool agent keeps the Console tab while the environment is pending', () => {
    render(
      <AgentDetailView
        agent={{ ...workspaceToolAgent, id: 'a4', name: 'Pending Agent', containerId: undefined }}
        onBack={() => {}}
      />
    )
    expect(screen.getByText('Waiting to start')).toBeDefined()
  })

  test('prompt agent hides Console and labels chat as Chat', () => {
    render(<AgentDetailView agent={providerAgent} onBack={() => {}} />)
    expect(screen.getByRole('button', { name: 'Overview' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Tasks' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Chat' })).toBeDefined()
    expect(screen.queryByRole('button', { name: 'Console' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'History' })).toBeNull()
  })

  test('Host CLI agent is managed without container terminal actions', () => {
    render(<AgentDetailView agent={hostCliAgent} onBack={() => {}} />)
    expect(screen.getAllByText(/Local CLI/).length).toBeGreaterThan(0)
    expect(screen.getByRole('button', { name: 'History' })).toBeDefined()
    expect(screen.queryByRole('button', { name: 'Console' })).toBeNull()
    expect(screen.getByText('host-aabbccdd')).toBeDefined()
    expect(screen.getAllByText(/run on the enrolled computer/i).length).toBeGreaterThan(0)
    expect(screen.queryByRole('button', { name: /start agent/i })).toBeNull()
  })

  test('prompt agent ignores stale environment ids for Console visibility', () => {
    render(
      <AgentDetailView
        agent={{ ...providerAgent, containerId: 'stale-provider-container' }}
        onBack={() => {}}
      />
    )
    expect(screen.getByRole('button', { name: 'Chat' })).toBeDefined()
    expect(screen.queryByRole('button', { name: 'Console' })).toBeNull()
  })

  test('shows overview stats by default', () => {
    render(<AgentDetailView agent={containerAgent} onBack={() => {}} />)
    expect(screen.getByText('12')).toBeDefined()
    expect(screen.getByText('98%')).toBeDefined()
    expect(screen.getByText('Model service')).toBeDefined()
  })

  test('foregrounds assignment fit on the agent profile', () => {
    render(
      <AgentDetailView
        agent={{ ...containerAgent, currentTask: 'Implement onboarding flow' }}
        onBack={() => {}}
      />
    )
    expect(screen.getByTestId('agent-assignment-fit')).toBeDefined()
    expect(screen.getByText('Can be assigned now')).toBeDefined()
    expect(screen.getByText('Implement onboarding flow')).toBeDefined()
    expect(screen.getByText(/attach and review skills/i)).toBeDefined()
  })

  test('guides an idle agent toward a first safe task', () => {
    render(<AgentDetailView agent={containerAgent} onBack={() => {}} />)

    expect(screen.getByTestId('agent-next-step')).toBeDefined()
    expect(screen.getByText('Ready')).toBeDefined()
    expect(screen.getByText('Send a small first task')).toBeDefined()
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
    expect(screen.getByText('Do This Next')).toBeDefined()
  })

  test('guides pending container agents to the Terminal tab', () => {
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

    expect(screen.getByText('Start the managed workspace')).toBeDefined()
    fireEvent.click(screen.getByRole('button', { name: /open console/i }))
    expect(screen.getByText('Start the managed workspace to open the console')).toBeDefined()
    expect(screen.getByText(/start the workspace when you need terminal access/i)).toBeDefined()
    expect(
      screen.getByText(/success looks like the agent status changing to idle or working/i)
    ).toBeDefined()
    expect(
      screen.getByText(/ask an admin to check the container runtime and agent image/i)
    ).toBeDefined()
    expect(screen.getByRole('button', { name: /start agent workspace/i })).toBeDefined()
  })

  test('guides offline Host CLI agents back to the local connection', () => {
    render(<AgentDetailView agent={{ ...hostCliAgent, status: 'offline' }} onBack={() => {}} />)

    expect(screen.getByText('Reconnect the local computer')).toBeDefined()
    expect(screen.getByText(/start the sidecar again/i)).toBeDefined()
    expect(screen.queryByRole('button', { name: /open terminal/i })).toBeNull()
  })

  test('explains workspace access and primary project context', () => {
    render(<AgentDetailView agent={{ ...containerAgent, cwd: '/workspace' }} onBack={() => {}} />)
    expect(screen.getByText('Working folder')).toBeDefined()
    expect(screen.getAllByText('How it runs').length).toBeGreaterThan(0)
    expect(screen.getByText('Workspace it can use')).toBeDefined()
    expect(screen.getByText('Engineering')).toBeDefined()
    expect(screen.getByText('Default project for tasks')).toBeDefined()
    expect(screen.getByText('Platform')).toBeDefined()
    expect(screen.getByText(/can include several projects/i)).toBeDefined()
    expect(screen.getByText(/default place for new tasks/i)).toBeDefined()
    expect(screen.getByText(/files must be kept apart/i)).toBeDefined()
  })

  test('explains provider-backed agents do not open workspace files', () => {
    render(<AgentDetailView agent={providerAgent} onBack={() => {}} />)
    expect(screen.getByText('Working folder')).toBeDefined()
    expect(screen.getAllByText('Not needed for this agent').length).toBeGreaterThan(0)
    expect(screen.getByText(/do not open workspace files by themselves/i)).toBeDefined()
    expect(
      screen.getByText(/choose a local or container cli agent when the task must inspect or edit/i)
    ).toBeDefined()
  })

  test('shows back button', () => {
    render(<AgentDetailView agent={containerAgent} onBack={() => {}} />)
    expect(screen.getByTestId('agent-back')).toBeDefined()
  })
})
