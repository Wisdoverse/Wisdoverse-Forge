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
  name: 'Claude-1',
  provider: 'Anthropic',
  model: 'claude-4-opus',
  status: 'idle' as const,
  tasksCompleted: 12,
  tasksInProgress: 1,
  successRate: 0.98,
  cliTool: 'claude' as const,
  containerId: 'c-abc',
  workspaceId: 'w1',
  workspaceName: 'Engineering',
  projectId: 'p1',
  projectName: 'Platform',
}

const providerAgent = {
  ...containerAgent,
  id: 'a2',
  name: 'Sonnet-via-API',
  cliTool: undefined,
  containerId: undefined,
}

const codexContainerAgent = {
  ...containerAgent,
  id: 'a3',
  name: 'Codex-container',
  provider: 'OpenAI',
  model: 'gpt-5.5',
  cliTool: 'codex',
}

const hostCliAgent = {
  ...codexContainerAgent,
  id: 'a5',
  name: 'Host Codex',
  containerId: undefined,
  runtimeId: 'host-aabbccdd',
  runtimeKind: 'host-cli' as const,
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
    expect(screen.getByText('Claude-1')).toBeDefined()
  })

  test('container-CLI agent shows the Terminal tab and labels chat as History', () => {
    render(<AgentDetailView agent={containerAgent} onBack={() => {}} />)
    expect(screen.getByRole('button', { name: 'Overview' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Tasks' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'History' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Terminal' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Plugins' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Config' })).toBeDefined()
  })

  test('Codex container agent still shows the Terminal tab', () => {
    render(<AgentDetailView agent={codexContainerAgent} onBack={() => {}} />)
    expect(screen.getByRole('button', { name: 'Terminal' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'History' })).toBeDefined()
  })

  test('CLI runtime agent keeps the Terminal tab while container is pending', () => {
    render(
      <AgentDetailView
        agent={{ ...codexContainerAgent, id: 'a4', name: 'Codex-pending', containerId: undefined }}
        onBack={() => {}}
      />
    )
    expect(screen.getByText('Terminal')).toBeDefined()
    expect(screen.getByText('Pending')).toBeDefined()
  })

  test('provider+prompt agent hides Terminal and labels chat as Chat', () => {
    render(<AgentDetailView agent={providerAgent} onBack={() => {}} />)
    expect(screen.getByRole('button', { name: 'Overview' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Tasks' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Chat' })).toBeDefined()
    expect(screen.queryByRole('button', { name: 'Terminal' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'History' })).toBeNull()
  })

  test('Host CLI agent is managed without container terminal actions', () => {
    render(<AgentDetailView agent={hostCliAgent} onBack={() => {}} />)
    expect(screen.getByText('Host CLI')).toBeDefined()
    expect(screen.getByRole('button', { name: 'History' })).toBeDefined()
    expect(screen.queryByRole('button', { name: 'Terminal' })).toBeNull()
    expect(screen.getByText('host-aabbccdd')).toBeDefined()
    expect(screen.getByText(/run on the enrolled machine/i)).toBeDefined()
    expect(screen.queryByRole('button', { name: /start agent/i })).toBeNull()
  })

  test('provider+prompt agent ignores stale container ids for Terminal visibility', () => {
    render(
      <AgentDetailView
        agent={{ ...providerAgent, containerId: 'stale-provider-container' }}
        onBack={() => {}}
      />
    )
    expect(screen.getByRole('button', { name: 'Chat' })).toBeDefined()
    expect(screen.queryByRole('button', { name: 'Terminal' })).toBeNull()
  })

  test('shows overview stats by default', () => {
    render(<AgentDetailView agent={containerAgent} onBack={() => {}} />)
    expect(screen.getByText('12')).toBeDefined()
    expect(screen.getByText('98%')).toBeDefined()
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
    expect(screen.getByText('Assign a First Safe Task')).toBeDefined()
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
          ...codexContainerAgent,
          id: 'pending-offline',
          status: 'offline',
          containerId: undefined,
        }}
        onBack={() => {}}
      />
    )

    expect(screen.getByText('Start the Container Runtime')).toBeDefined()
    fireEvent.click(screen.getByRole('button', { name: /open terminal/i }))
    expect(screen.getByText('No container is running')).toBeDefined()
  })

  test('guides offline Host CLI agents back to the local sidecar', () => {
    render(<AgentDetailView agent={{ ...hostCliAgent, status: 'offline' }} onBack={() => {}} />)

    expect(screen.getByText('Reconnect the Local Sidecar')).toBeDefined()
    expect(screen.getByText(/Start the sidecar on the enrolled machine/i)).toBeDefined()
    expect(screen.queryByRole('button', { name: /open terminal/i })).toBeNull()
  })

  test('explains workspace access and primary project context', () => {
    render(<AgentDetailView agent={{ ...containerAgent, cwd: '/workspace' }} onBack={() => {}} />)
    expect(screen.getByText('Working Directory')).toBeDefined()
    expect(screen.getByText('Workspace Access')).toBeDefined()
    expect(screen.getByText('Engineering')).toBeDefined()
    expect(screen.getByText('Primary Project')).toBeDefined()
    expect(screen.getByText('Platform')).toBeDefined()
    expect(screen.getByText(/may include multiple projects/i)).toBeDefined()
    expect(
      screen.getByText(/provider \+ prompt agents do not access files directly/i)
    ).toBeDefined()
    expect(screen.getByText(/separate workspace for strict filesystem isolation/i)).toBeDefined()
  })

  test('explains provider+prompt agents do not mount workspace files', () => {
    render(<AgentDetailView agent={providerAgent} onBack={() => {}} />)
    expect(screen.getByText('Working Directory')).toBeDefined()
    expect(screen.getAllByText('Not applicable').length).toBeGreaterThan(0)
    expect(screen.getByText(/do not mount \/workspace or read files directly/i)).toBeDefined()
    expect(
      screen.getByText(/use a container cli agent when filesystem tools are required/i)
    ).toBeDefined()
  })

  test('shows back button', () => {
    render(<AgentDetailView agent={containerAgent} onBack={() => {}} />)
    expect(screen.getByTestId('agent-back')).toBeDefined()
  })
})
