import { describe, test, expect, afterEach, vi } from 'vitest'
import { render, screen, cleanup } from '@testing-library/react'
import { AgentDetailView } from '@app/widgets/agent-detail/AgentDetailView'

afterEach(cleanup)

// AgentTasksTab triggers an API call on mount; we don't want unit tests to
// depend on the fetch shim, so stub it out at module level.
vi.mock('@app/shared/api/orchestration', () => ({
  orchestrationApi: { getTasksByAgent: vi.fn().mockResolvedValue([]) },
}))

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

  test('explains workspace access and primary project context', () => {
    render(<AgentDetailView agent={{ ...containerAgent, cwd: '/workspace' }} onBack={() => {}} />)
    expect(screen.getByText('Container CWD')).toBeDefined()
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
    expect(screen.getByText('Container CWD')).toBeDefined()
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
