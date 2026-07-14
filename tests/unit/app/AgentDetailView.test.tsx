import { describe, test, expect, afterEach, beforeEach, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { useAgentsStore } from '@app/entities/agent'
import {
  agentDetailHeaderSubtitle,
  AgentDetailView,
} from '@app/widgets/agent-detail/AgentDetailView'
import type { TaskSummary } from '@app/shared/api/orchestration'

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

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
    expect(agentDetailHeaderSubtitle(containerAgent)).toBe('OpenCode with project files')

    render(<AgentDetailView agent={containerAgent} onBack={() => {}} />)

    expect(screen.getAllByText('OpenCode with project files').length).toBeGreaterThan(0)
    expect(screen.queryByText('tool-large')).toBeNull()
    expect(screen.queryByText('OpenCode in a managed workspace')).toBeNull()
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

  test('lets users return to Agents when tools cannot load from the detail view', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValueOnce({
        ok: false,
        status: 403,
      })
    )
    const onBack = vi.fn()

    render(<AgentDetailView agent={containerAgent} onBack={onBack} />)

    fireEvent.click(screen.getByRole('button', { name: 'Tools' }))
    const alert = await screen.findByRole('alert')
    fireEvent.click(within(alert).getByRole('button', { name: /back to agents/i }))

    expect(onBack).toHaveBeenCalledTimes(1)
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

    expect(screen.getAllByText('Check if ready').length).toBeGreaterThan(0)
    expect(screen.queryByText('Check agent status')).toBeNull()
    expect(screen.queryByText(/warming_up/i)).toBeNull()
    expect(screen.queryByText(/warming up/i)).toBeNull()
  })

  test('prompt agent hides task and live-work tabs and points users to Chat', () => {
    render(<AgentDetailView agent={providerAgent} onBack={() => {}} />)
    expect(screen.getByRole('button', { name: 'Overview' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Chat' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Chat instructions' })).toBeDefined()
    expect(screen.queryByRole('button', { name: 'Tasks' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Tools' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Instructions' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Live work' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Command window' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'History' })).toBeNull()
    expect(screen.getByText('Send a message in Chat')).toBeDefined()
    expect(
      screen.getByText(/Use Chat for direct questions, writing, and result checks/i)
    ).toBeDefined()
    expect(screen.getByTestId('agent-next-step')).toHaveTextContent(
      'It cannot take Tasks or change project files.'
    )
    expect(screen.getByRole('button', { name: /open chat/i })).toBeDefined()
    expect(screen.getAllByText('Ready for direct chat').length).toBeGreaterThan(0)
    expect(screen.queryByText('Ready for work')).toBeNull()
    expect(screen.getByText('Send a message in Chat to create the first reply.')).toBeDefined()
    expect(screen.getByText('Messages answered')).toBeDefined()
    expect(screen.getByText('Replies in progress')).toBeDefined()
    expect(screen.getByText('Answer success')).toBeDefined()
    expect(screen.getByText('Can answer')).toBeDefined()
    expect(screen.getByText('What it can use')).toBeDefined()
    expect(screen.getByText('Connected AI service')).toBeDefined()
    expect(screen.getByText('Where to start')).toBeDefined()
    expect(screen.getByText('File access')).toBeDefined()
    expect(screen.getByText('Current chat')).toBeDefined()
    expect(
      screen.getAllByText(
        'No project files. Use an agent with Project files or This computer for Tasks and code changes.'
      ).length
    ).toBeGreaterThan(0)
    expect(screen.getByText('Save useful chat notes after a reply.')).toBeDefined()
    expect(screen.queryByText('Ready for a task')).toBeNull()
    expect(screen.queryByText('Send a task to create the first update.')).toBeNull()
    expect(screen.queryByText('Current work')).toBeNull()
    expect(screen.queryByText('Finish a task, then save useful steps.')).toBeNull()
    expect(screen.queryByText('Tasks done')).toBeNull()
    expect(screen.queryByText('In progress')).toBeNull()
    expect(screen.queryByText('Finished cleanly')).toBeNull()
    expect(screen.queryByText('Can take work')).toBeNull()
    expect(screen.queryByText('Project files it can use')).toBeNull()
    expect(screen.queryByText('Project for new tasks')).toBeNull()
    expect(screen.queryByText('Folder agents open')).toBeNull()
    expect(getTasksByAgentMock).not.toHaveBeenCalled()
  })

  test('agent joined from this computer is managed without live work actions', () => {
    render(<AgentDetailView agent={hostCliAgent} onBack={() => {}} />)
    expect(screen.getAllByText(/This computer/).length).toBeGreaterThan(0)
    expect(screen.getByRole('button', { name: 'History' })).toBeDefined()
    expect(screen.queryByRole('button', { name: 'Live work' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Command window' })).toBeNull()
    expect(screen.getByText('How it connects')).toBeDefined()
    expect(screen.getByText('Connected from this computer')).toBeDefined()
    expect(screen.queryByText('host-aabbccdd')).toBeNull()
    expect(screen.getByText('Selected work folder: /home/operator/project')).toBeDefined()
    expect(screen.queryByText('/home/operator/project')).toBeNull()
    expect(screen.getByText(/this computer does the work/i)).toBeDefined()
    expect(screen.getByText(/folder where you pasted the setup text/i)).toBeDefined()
    expect(screen.queryByText(/setup command/i)).toBeNull()
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
    expect(screen.getByText('Tasks done')).toBeDefined()
    expect(screen.getByText('In progress')).toBeDefined()
    expect(screen.getByText('Finished cleanly')).toBeDefined()
    expect(screen.getByText('Work type')).toBeDefined()
    expect(screen.getAllByText('Project files').length).toBeGreaterThan(0)
    expect(screen.queryByText('Tasks Done')).toBeNull()
    expect(screen.queryByText('In Progress')).toBeNull()
    expect(screen.queryByText('Success Rate')).toBeNull()
  })

  test('foregrounds assignment fit on the agent overview', async () => {
    getTasksByAgentMock.mockResolvedValueOnce([
      makeTask({
        id: 'reuse-task',
        params: { task: 'Reuse saved guidance', message: '' },
        contextCounts: { total: 2, appliedMemories: 0, appliedSkills: 2 },
      }),
    ])

    render(
      <AgentDetailView
        agent={{ ...containerAgent, currentTask: 'Implement onboarding flow' }}
        onBack={() => {}}
      />
    )
    expect(screen.getByTestId('agent-assignment-fit')).toBeDefined()
    expect(screen.getByText('Ready for work')).toBeDefined()
    expect(screen.getByText('Implement onboarding flow')).toBeDefined()
    expect(screen.getByText('Saved guidance')).toBeDefined()
    expect(await screen.findByText('2 saved guidance items used in recent work')).toBeDefined()
    expect(screen.queryByText('Saved instructions')).toBeNull()
    expect(screen.queryByText(/saved instructions? used in recent work/i)).toBeNull()
    expect(screen.queryByText(/task\s+context/i)).toBeNull()
  })

  test('guides an idle agent toward a first safe task', () => {
    render(<AgentDetailView agent={containerAgent} onBack={() => {}} />)

    expect(screen.getByTestId('agent-next-step')).toBeDefined()
    expect(screen.getAllByText('Ready').length).toBeGreaterThan(0)
    expect(screen.getByText('Ready for a task')).toBeDefined()
    expect(screen.getByText('Send a task to create the first update.')).toBeDefined()
    expect(screen.getByText('Send a small first task')).toBeDefined()
    const nextStep = screen.getByTestId('agent-next-step')
    expect(
      within(nextStep).getByText(
        'Use Tasks to send a small, low-risk task. Choose this agent directly, or let another agent take it.'
      )
    ).toBeDefined()
    expect(within(nextStep).queryByText(/task queue/i)).toBeNull()
    expect(within(nextStep).queryByText(/place where new tasks wait/i)).toBeNull()
    expect(screen.queryByText('No active task')).toBeNull()
    expect(screen.queryByText('No recent task updates')).toBeNull()
    expect(screen.queryByText(/unassigned/i)).toBeNull()
    expect(screen.getByRole('button', { name: /open tasks/i })).toBeDefined()
    expect(screen.queryByLabelText(/send a quick message/i)).toBeNull()
    expect(screen.getByRole('button', { name: /need a quick message instead/i })).toBeDefined()
  })

  test('explains managed file folders without showing the internal folder path', () => {
    render(
      <AgentDetailView
        agent={{ ...containerAgent, cwd: '/workspace/projects/platform' }}
        onBack={() => {}}
      />
    )

    expect(screen.getByText('Shared project files')).toBeDefined()
    expect(document.body.textContent).not.toContain('/workspace/projects/platform')
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

    expect(await screen.findByText('Check what this agent is doing')).toBeDefined()
    expect(screen.getAllByText(/Fix onboarding copy/).length).toBeGreaterThan(0)
    expect(
      screen.getByText(
        'Build Agent is already handling "Fix onboarding copy". Go to Tasks to follow progress or handle anything that needs your help.'
      )
    ).toBeDefined()
    expect(
      screen.getByText(/What success looks like: You can see the active task state/)
    ).toBeDefined()
    expect(screen.queryByText(new RegExp(['unblock', 'it'].join('\\s+'), 'i'))).toBeNull()
    expect(screen.queryByText(/owner input/i)).toBeNull()
    expect(screen.getByText('Do this next')).toBeDefined()
    expect(screen.queryByText('Review Current Work')).toBeNull()
    expect(screen.queryByText('Do This Next')).toBeNull()
  })

  test('guides agents without loaded task history into the Tasks tab', async () => {
    render(<AgentDetailView agent={{ ...containerAgent, status: 'working' }} onBack={() => {}} />)

    expect(await screen.findByText('Go to Tasks to check recent activity')).toBeDefined()
    expect(screen.queryByText('Go to Tasks to review recent activity')).toBeNull()
    expect(
      screen.getByText(
        "Go to Tasks to load this agent's task history and decide what task to send next."
      )
    ).toBeDefined()
    expect(
      screen.queryByText(
        "Go to Tasks to load this agent's work history and decide what to send next."
      )
    ).toBeNull()
    expect(screen.getByText(/check result files/)).toBeDefined()
    expect(screen.queryByText(/review evidence/i)).toBeNull()
    expect(screen.queryByText('No task activity has been loaded yet.')).toBeNull()
  })

  test('shows a recovery step when recent task history cannot load', async () => {
    getTasksByAgentMock.mockRejectedValueOnce(new TypeError('Failed to fetch'))

    render(<AgentDetailView agent={{ ...containerAgent, status: 'working' }} onBack={() => {}} />)

    expect(await screen.findByText('Choose this agent again or open Tasks')).toBeDefined()
    expect(
      screen.getByText(
        "This page could not load the agent's recent task history. Go back to Agents and choose this agent again, or open Tasks to confirm the latest task state before sending another task."
      )
    ).toBeDefined()
    expect(
      screen.getByText(
        "Go back to Agents and choose this agent again, or open Tasks to check this agent's latest task state."
      )
    ).toBeDefined()
    expect(screen.queryByText(/latest work/i)).toBeNull()
    expect(screen.queryByText(/sending more work/i)).toBeNull()
    expect(screen.getByText(/latest task state before deciding/)).toBeDefined()
    expect(screen.getByRole('button', { name: /open tasks/i })).toBeDefined()
    expect(screen.queryByText(/failed to fetch/i)).toBeNull()
    expect(screen.queryByText('Send a task to create the first update.')).toBeNull()
  })

  test('does not suggest new work when a ready agent history cannot load', async () => {
    getTasksByAgentMock.mockRejectedValueOnce(new TypeError('Failed to fetch'))

    render(<AgentDetailView agent={containerAgent} onBack={() => {}} />)

    expect(await screen.findByText('Choose this agent again or open Tasks')).toBeDefined()
    expect(
      screen.getByText(
        "This page could not load the agent's recent task history. Go back to Agents and choose this agent again, or open Tasks to confirm the latest task state before sending another task."
      )
    ).toBeDefined()
    expect(screen.queryByText(/sending more work/i)).toBeNull()
    expect(screen.queryByText('Send a small first task')).toBeNull()
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

    expect(screen.getAllByText('Start project files').length).toBeGreaterThan(1)
    expect(screen.getByText('Open Live work and start project files')).toBeDefined()
    expect(screen.queryByText('Unavailable until restarted or reconnected')).toBeNull()
    expect(
      screen.getByText(
        'Open Live work, choose Start project files, and wait until this agent shows Ready before sending Tasks or code changes.'
      )
    ).toBeDefined()
    fireEvent.click(screen.getByRole('button', { name: /open live work/i }))
    expect(screen.getByText('Start project files to open Live work')).toBeDefined()
    expect(
      screen.getByText(/start project files before this agent changes project files/i)
    ).toBeDefined()
    expect(
      screen.getByText(/success looks like the agent status changing to ready or working/i)
    ).toBeDefined()
    expect(
      screen.getByText(/ask an owner or admin to check this agent's connection and access/i)
    ).toBeDefined()
    expect(screen.queryByText(/check this agent setup/i)).toBeNull()
    expect(screen.queryByText(/open terminal/i)).toBeNull()
    expect(screen.queryByText(/terminal access/i)).toBeNull()
    expect(screen.queryByText(/live terminal/i)).toBeNull()
    expect(screen.queryByText(/command window/i)).toBeNull()
    expect(screen.queryByText('Loading live work...')).toBeNull()
    expect(screen.getByRole('button', { name: /start project files/i })).toBeDefined()
    expect(screen.queryByText(/file work/i)).toBeNull()
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
    expect(alert).toHaveTextContent('Check the agent status')
    expect(alert).toHaveTextContent('choose Start project files again')
    expect(alert).toHaveTextContent(
      "ask an owner or admin to check this agent's connection and access in Agents"
    )
    expect(alert).not.toHaveTextContent('check this agent setup')
    expect(alert).not.toHaveTextContent('Start did not finish')
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
    const startButton = screen.getByRole('button', { name: /start project files/i })
    fireEvent.click(startButton)

    await waitFor(() => expect(startButton).not.toBeDisabled())

    const alert = screen.getByRole('alert')
    expect(alert).toHaveTextContent('Check the agent status')
    expect(alert).toHaveTextContent('choose Start project files again')
    expect(alert).toHaveTextContent(
      "ask an owner or admin to check this agent's connection and access in Agents"
    )
    expect(alert).not.toHaveTextContent('check this agent setup')
    expect(alert).not.toHaveTextContent('Start did not finish')
    expect(alert.textContent).not.toContain('socket hang up')
  })

  test('names the project-files action while a pending workspace is starting', async () => {
    let finishStart: (started: boolean) => void = () => undefined
    useAgentsStore.setState({
      error: null,
      startAgent: vi.fn(
        () =>
          new Promise<boolean>((resolve) => {
            finishStart = resolve
          })
      ),
    } as never)

    render(
      <AgentDetailView
        agent={{
          ...workspaceToolAgent,
          id: 'pending-starting-label',
          status: 'offline',
          containerId: undefined,
        }}
        onBack={() => {}}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: /open live work/i }))
    fireEvent.click(screen.getByRole('button', { name: /start project files/i }))

    expect(screen.getByRole('button', { name: /opening project files/i })).toBeDisabled()
    expect(screen.queryByRole('button', { name: /^Starting\.\.\.$/i })).toBeNull()

    await act(async () => {
      finishStart(true)
    })
  })

  test('guides offline agents joined from this computer back to the local connection', () => {
    const onBack = vi.fn()
    render(<AgentDetailView agent={{ ...hostCliAgent, status: 'offline' }} onBack={onBack} />)

    const nextStep = screen.getByTestId('agent-next-step')
    expect(screen.getAllByText('Reconnect this computer from Agents').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Use Connect this computer in Agents').length).toBeGreaterThan(0)
    expect(screen.getByText('Open Agents and connect this computer again')).toBeDefined()
    expect(
      within(nextStep).getByText(/go back to Agents, choose Connect this computer/i)
    ).toBeDefined()
    expect(within(nextStep).getByText(/copy the new setup text/i)).toBeDefined()
    expect(within(nextStep).getByText(/paste it in the setup app shown there/i)).toBeDefined()
    expect(within(nextStep).queryByText(/Terminal or PowerShell/i)).toBeNull()
    fireEvent.click(within(nextStep).getByRole('button', { name: /back to agents/i }))
    expect(onBack).toHaveBeenCalledTimes(1)
    expect(within(nextStep).queryByText(/command app/i)).toBeNull()
    expect(screen.queryByText(/paste setup text on this computer again/i)).toBeNull()
    expect(screen.queryByText(/paste setup text again on this computer/i)).toBeNull()
    expect(screen.queryByText(/paste the setup text again/i)).toBeNull()
    expect(screen.queryByText(/setup command/i)).toBeNull()
    expect(screen.queryByText('Connected from this computer')).toBeNull()
    expect(screen.queryByText('Reconnect from Agents')).toBeNull()
    expect(screen.queryByText('Reconnect from Agents first')).toBeNull()
    expect(screen.queryByText('Unavailable until restarted or reconnected')).toBeNull()
    expect(screen.queryByRole('button', { name: /open terminal/i })).toBeNull()
  })

  test('guides offline chat-only agents to AI service settings', () => {
    render(<AgentDetailView agent={{ ...providerAgent, status: 'offline' }} onBack={() => {}} />)

    const nextStep = screen.getByTestId('agent-next-step')
    expect(screen.getByText('Check the AI service before sending a message')).toBeDefined()
    expect(screen.getByText('Open AI services in Settings and choose Check connection')).toBeDefined()
    expect(
      screen.getByText(
        'Open AI services in Settings to confirm this simple chat agent can answer. It cannot take Tasks, change code, or use computer apps.'
      )
    ).toBeDefined()
    expect(within(nextStep).getByText(/choose Check connection for this service/i)).toBeDefined()
    expect(
      within(nextStep).getByText(
        /return to Agents and choose this agent again before sending a message/i
      )
    ).toBeDefined()
    expect(screen.getByText(/returns to Ready and can answer in chat/i)).toBeDefined()
    expect(screen.getByRole('link', { name: /open AI services/i })).toHaveAttribute(
      'href',
      '/settings/providers'
    )
    expect(screen.queryByText(/Open AI service settings/i)).toBeNull()
    expect(screen.queryByText('Check the AI service before sending work')).toBeNull()
    expect(within(nextStep).queryByText(/chat work/i)).toBeNull()
    expect(screen.queryByText('Unavailable until restarted or reconnected')).toBeNull()
    expect(screen.queryByText(/click Check/i)).toBeNull()
  })

  test('explains workspace access and primary project context', () => {
    render(<AgentDetailView agent={{ ...containerAgent, cwd: '/workspace' }} onBack={() => {}} />)
    expect(screen.getByText('Folder agents open')).toBeDefined()
    expect(screen.getByText('Default project folder')).toBeDefined()
    expect(screen.getByText('How it connects')).toBeDefined()
    expect(screen.getByText('Ready with project files')).toBeDefined()
    expect(screen.queryByText('/workspace')).toBeNull()
    expect(screen.queryByText('c-abc')).toBeNull()
    expect(screen.getAllByText('Where it works').length).toBeGreaterThan(0)
    expect(screen.getByText('Can take work')).toBeDefined()
    expect(screen.getAllByText('OpenCode with project files').length).toBeGreaterThan(0)
    expect(screen.queryByText('Status')).toBeNull()
    expect(screen.queryByText('Connection')).toBeNull()
    expect(screen.queryByText('opencode managed workspace')).toBeNull()
    expect(screen.queryByText('Workspace project folder')).toBeNull()
    expect(screen.queryByText('Ready in managed workspace')).toBeNull()
    expect(screen.getByText('Project files it can use')).toBeDefined()
    expect(screen.queryByText('Project area it can use')).toBeNull()
    expect(screen.getByText('Engineering')).toBeDefined()
    expect(screen.getByText('Project for new tasks')).toBeDefined()
    expect(screen.getByText('Platform')).toBeDefined()
    expect(screen.getByText(/may include several projects/i)).toBeDefined()
    expect(screen.getByText(/where new tasks begin/i)).toBeDefined()
    expect(screen.getByText(/separate set of project files/i)).toBeDefined()
    expect(screen.getByText(/files must be kept apart/i)).toBeDefined()
    expect(screen.queryByText(/shared project area/i)).toBeNull()
    expect(screen.queryByText('Starting folder')).toBeNull()
    expect(screen.queryByText('Starting project for tasks')).toBeNull()
  })

  test('explains chat-only agents do not open workspace files', () => {
    render(<AgentDetailView agent={providerAgent} onBack={() => {}} />)
    expect(screen.getByText('File access')).toBeDefined()
    expect(
      screen.getAllByText(
        'No project files. Use an agent with Project files or This computer for Tasks and code changes.'
      ).length
    ).toBeGreaterThan(0)
    expect(screen.queryByText('No file access needed')).toBeNull()
    expect(screen.getByText('How it connects')).toBeDefined()
    expect(screen.getByText('AI service is ready for chat')).toBeDefined()
    expect(screen.getAllByText('Simple chat agent').length).toBeGreaterThan(0)
    expect(screen.getByText(/answers in chat through an AI service/i)).toBeDefined()
    expect(
      screen.getByText(/can answer questions, write, and check text or results/i)
    ).toBeDefined()
    expect(screen.queryByText(/can plan, write, and review text/i)).toBeNull()
    expect(
      screen.getByText(/cannot take Tasks, change code, use computer apps, or open project files/i)
    ).toBeDefined()
    expect(
      screen.getByText(
        /for Tasks and code changes, use an agent with Project files or This computer/i
      )
    ).toBeDefined()
    const accessNote = screen.getByText(/cannot take Tasks, change code, or use computer apps/i)
    expect(accessNote).toBeDefined()
    expect(accessNote).toHaveClass('break-words')
    expect(accessNote).not.toHaveClass('truncate')
    expect(screen.queryByText(/run commands/i)).toBeNull()
    expect(screen.queryByText('Not needed for this agent')).toBeNull()
    expect(screen.queryByText('Not needed')).toBeNull()
    expect(screen.queryByText(/model provider/i)).toBeNull()
    expect(screen.queryByText(/text-only model/i)).toBeNull()
    expect(screen.queryByText(/model service/i)).toBeNull()
  })

  test('keeps review-needed AI service labels readable in agent details', () => {
    render(
      <AgentDetailView
        agent={{ ...providerAgent, provider: 'Check AI service' }}
        onBack={() => {}}
      />
    )

    expect(screen.getAllByText('Check AI service').length).toBeGreaterThan(0)
    expect(screen.queryByText(/Check AI service AI service/i)).toBeNull()
  })

  test('shows back button', () => {
    render(<AgentDetailView agent={containerAgent} onBack={() => {}} />)
    expect(screen.getByTestId('agent-back')).toBeDefined()
  })
})
