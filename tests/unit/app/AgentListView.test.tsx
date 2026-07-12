import { describe, test, expect, afterEach, beforeEach, vi } from 'vitest'
import { render, screen, cleanup, fireEvent, waitFor, within } from '@testing-library/react'
import { AgentListView } from '@app/features/agents/AgentListView'
import { useAgentsStore } from '@app/entities/agent'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
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

function openMoreAgentSetup() {
  fireEvent.click(screen.getByRole('button', { name: /more agent setup/i }))
}

describe('AgentListView', () => {
  test('explains the first agent loading state for beginners', () => {
    useAgentsStore.setState({ loading: true, agents: [] })

    render(<AgentListView />)

    const loading = screen.getByRole('status', { name: /checking agents/i })
    expect(loading).toHaveTextContent('Checking agents')
    expect(loading).toHaveTextContent(
      'Forge is checking which agents can receive work in this team space.'
    )
    expect(loading).toHaveTextContent(
      'If this takes more than a moment, open Agents again or ask an owner or admin to check agent access.'
    )
    expect(loading).toHaveTextContent(
      'Success looks like an agent card with a status such as Ready or Not connected.'
    )
    expect(loading).not.toHaveTextContent('Loading agents')
  })

  test('shows empty state when no agents', () => {
    render(<AgentListView />)
    expect(screen.getByText('Agents')).toBeDefined()
    expect(screen.queryByText('Agent Fleet')).toBeNull()
    expect(screen.getByText('Add first agent')).toBeDefined()
    expect(screen.queryByText('No agents')).toBeNull()
    expect(screen.getByText(/add your first agent/i)).toBeDefined()
    expect(screen.getByText(/If this agent should take Tasks or change code/i)).toBeDefined()
    expect(screen.getByText(/choose Project files/i)).toBeDefined()
    expect(screen.getByText(/questions and result checks/i)).toBeDefined()
    expect(
      screen.getByText(/does not take Tasks, change code, or use computer apps/i)
    ).toBeDefined()
    expect(screen.queryByText(/run commands/i)).toBeNull()
    expect(screen.getByText(/files or apps on your computer/i)).toBeDefined()
    expect(screen.queryByText(/files and commands on your machine/i)).toBeNull()
    expect(screen.getByText(/Next: choose New agent/i)).toBeDefined()
    expect(screen.queryByText(/If unsure, pick Simple chat agent first/i)).toBeNull()
    expect(screen.queryByText(/Start with Simple chat/i)).toBeNull()
    expect(screen.queryByText(/Start with a chat-only AI service/i)).toBeNull()
    const guide = screen.getByTestId('agent-choice-guide')
    expect(within(guide).getByText('Choose by what the agent needs to use')).toBeDefined()
    expect(screen.queryByText('Pick by where the work should happen')).toBeNull()
    expect(within(guide).getByText('Simple chat agent')).toBeDefined()
    expect(within(guide).getByText('This computer')).toBeDefined()
    expect(within(guide).getByText('Project files')).toBeDefined()
    expect(screen.queryByTestId('agent-fleet-controls')).toBeNull()
    expect(screen.queryByText(/connected model for text-only work/i)).toBeNull()
    expect(screen.getAllByRole('button', { name: /^new agent$/i }).length).toBeGreaterThan(0)
    expect(screen.queryByRole('button', { name: /^create agent$/i })).toBeNull()
  })

  test('keeps task queue and local computer setup collapsed by default', () => {
    render(<AgentListView />)

    const layout = screen.getByTestId('agent-list-layout')
    expect(layout.className).not.toContain('xl:grid-cols')
    const header = screen.getByTestId('agent-list-header')
    expect(within(header).getByRole('button', { name: /more agent setup/i })).toHaveAttribute(
      'aria-expanded',
      'false'
    )
    expect(screen.queryByText('Task Queues')).toBeNull()
    expect(screen.queryByTestId('host-cli-enrollment-panel')).toBeNull()

    openMoreAgentSetup()

    expect(within(header).getByRole('button', { name: /hide more agent setup/i })).toHaveAttribute(
      'aria-expanded',
      'true'
    )
    expect(screen.getByTestId('more-agent-setup')).toHaveClass('space-y-4')
    expect(screen.getByTestId('more-agent-setup').className).not.toContain('grid-cols')
    expect(screen.getByText('Task Queues')).toBeDefined()
    expect(screen.getByTestId('host-cli-enrollment-panel')).toBeDefined()
  })

  test('shows a recovery action when agents cannot load', () => {
    const loadAgents = vi.fn(async () => undefined)
    useAgentsStore.setState({
      agents: [],
      loading: false,
      error: 'Check your connection, then open Agents again to load agents.',
      loadAgents,
    })

    render(<AgentListView />)

    const alert = screen.getByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent('Check agents again')
    expect(alert).toHaveTextContent('Check your connection, then open Agents again to load agents.')
    expect(screen.queryByText(/add your first agent/i)).toBeNull()

    fireEvent.click(within(alert).getByRole('button', { name: /check agents again/i }))

    expect(loadAgents).toHaveBeenCalledTimes(2)
  })

  test('waits for a selected project before showing a command for this computer', () => {
    const onOpenProjectsSetup = vi.fn()
    render(<AgentListView onOpenProjectsSetup={onOpenProjectsSetup} />)
    openMoreAgentSetup()

    const enrollment = screen.getByTestId('host-cli-enrollment-panel')
    expect(within(enrollment).getByText(/connect this computer/i)).toBeDefined()
    expect(enrollment.textContent).toContain('files or apps on your computer')
    expect(enrollment.textContent).not.toContain('files or commands on your computer')
    expect(enrollment.textContent).toContain('manages it with your other agents')
    expect(enrollment.textContent).toContain('This computer')
    expect(enrollment.textContent).toContain('If the button does not work')
    expect(enrollment.textContent).not.toContain(
      'Use this backup if the guided setup does not open'
    )
    expect(enrollment.textContent).not.toContain('your team asks you to run a command')
    expect(enrollment.textContent).not.toContain('choose Add this computer as an agent above')
    expect(enrollment.textContent).not.toContain('Computer type')
    expect(within(enrollment).queryByTestId('host-cli-project-label')).toBeNull()
    expect(within(enrollment).queryByTestId('host-cli-command-waiting')).toBeNull()
    expect(within(enrollment).queryByRole('button', { name: /choose project first/i })).toBeNull()

    fireEvent.click(within(enrollment).getByText('If the button does not work'))

    expect(enrollment.textContent).toContain('Use this backup if the guided setup does not open')
    expect(enrollment.textContent).toContain('choose Add this computer as an agent above')
    expect(enrollment.textContent).toContain('Computer type')
    expect(
      within(enrollment).getByRole('group', { name: /choose this computer type/i })
    ).toBeDefined()
    expect(within(enrollment).getByText(/project:/i)).toBeDefined()
    expect(within(enrollment).getByTestId('host-cli-project-label')).toHaveTextContent(
      'Project: Open project settings first.'
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
      /open project settings to create a project/i
    )
    expect(within(enrollment).getByTestId('host-cli-command-waiting')).toHaveTextContent(
      /setup text appears here/i
    )
    expect(enrollment.textContent).not.toContain('Choose a project from the sidebar')
    expect(enrollment.textContent).not.toContain('agentforge agents enroll-local')
    expect(within(enrollment).getByRole('button', { name: /choose project first/i })).toBeDisabled()
    fireEvent.click(within(enrollment).getByRole('button', { name: /open project settings/i }))
    expect(onOpenProjectsSetup).toHaveBeenCalledTimes(1)

    fireEvent.click(
      within(enrollment).getByRole('button', { name: /add this computer as an agent/i })
    )
    expect(screen.getByRole('dialog', { name: /new agent/i })).toBeDefined()
    expect(screen.getByRole('radio', { name: /this computer/i })).toBeChecked()
    expect(screen.getByRole('textbox', { name: /folder on this computer/i })).toBeDefined()
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
    openMoreAgentSetup()

    const enrollment = screen.getByTestId('host-cli-enrollment-panel')
    expect(within(enrollment).getByText(/agent needs files or apps/i)).toBeDefined()
    expect(enrollment.textContent).not.toContain('agent needs files or commands')
    expect(enrollment.textContent).toContain('Forge shows it here')
    expect(enrollment.textContent).toContain('If the button does not work')
    expect(enrollment.textContent).not.toContain(
      'Use this backup if the guided setup does not open'
    )
    expect(enrollment.textContent).not.toContain('Computer type')
    expect(within(enrollment).queryByTestId('host-cli-project-label')).toBeNull()
    expect(enrollment.textContent).not.toContain('p1')
    expect(enrollment.textContent).not.toContain('agentforge agents enroll-local')
    expect(within(enrollment).queryByText('Backup setup text')).toBeNull()
    expect(within(enrollment).queryByRole('button', { name: /copy setup text/i })).toBeNull()

    fireEvent.click(within(enrollment).getByText('If the button does not work'))

    expect(within(enrollment).getByTestId('host-cli-project-label')).toHaveTextContent(
      'Project: Platform'
    )
    expect(enrollment.textContent).toContain('Setup app for macOS or Linux')
    expect(enrollment.textContent).toContain('Setup app for Windows')
    expect(enrollment.textContent).not.toContain('Terminal app')
    expect(enrollment.textContent).not.toContain('PowerShell app')
    expect(within(enrollment).getByTestId('host-cli-project-label')).not.toHaveTextContent('p1')
    expect(enrollment.textContent).toContain('p1')
    expect(enrollment.textContent).toContain('agentforge agents enroll-local')
    expect(within(enrollment).getByText('Backup setup text')).toBeDefined()
    expect(enrollment.textContent).toContain(
      'Copy this only if Add this computer as an agent does not open.'
    )
    expect(enrollment.textContent).toContain('--name "This Computer Codex"')
    expect(enrollment.textContent).toContain('--tool codex')
    expect(enrollment.textContent).toContain('--project p1')
    expect(enrollment.textContent).toContain(
      'Open the setup app shown above for this computer type.'
    )
    expect(enrollment.textContent).not.toContain(
      'Open Terminal on macOS/Linux or PowerShell on Windows'
    )
    expect(enrollment.textContent).toContain('Copy the setup text and paste it into that window')
    expect(enrollment.textContent).toContain(
      'Do not edit the setup text. Forge already filled in the selected project.'
    )
    expect(enrollment.textContent).not.toContain(
      'Copy this setup command and paste it into that Terminal or PowerShell window'
    )
    expect(enrollment.textContent).not.toContain('paste it there')
    expect(enrollment.textContent).not.toContain('Keep the suggested setup values')
    expect(enrollment.textContent).not.toContain('Leave the work tool as Codex unless')
    expect(within(enrollment).getByTestId('host-cli-success-hint')).toHaveTextContent(
      /come back to Forge/i
    )
    expect(within(enrollment).getByTestId('host-cli-success-hint')).toHaveTextContent(
      /agent appears in this list as Ready/i
    )
    expect(within(enrollment).getByTestId('host-cli-success-hint')).toHaveTextContent(
      /send one small task/i
    )
    expect(within(enrollment).getByTestId('host-cli-success-hint')).toHaveTextContent(
      /Keep that app open/i
    )
    expect(enrollment.textContent).not.toMatch(/command app/i)
    expect(within(enrollment).getByTestId('host-cli-success-hint')).not.toHaveTextContent(
      /This Computer Codex/i
    )
    expect(within(enrollment).getByTestId('host-cli-success-hint')).not.toHaveTextContent(
      /Keep Terminal or PowerShell open/i
    )
    expect(within(enrollment).getByTestId('host-cli-success-hint')).not.toHaveTextContent(
      /command window/i
    )
    expect(enrollment.textContent).not.toContain('Run this manual command')
    expect(enrollment.textContent).not.toContain('Change codex only if')
    expect(within(enrollment).getByRole('button', { name: /copy setup text/i })).toBeDefined()

    fireEvent.click(within(enrollment).getByRole('button', { name: /windows/i }))
    expect(enrollment.textContent).toContain('--shell-format powershell')
    expect(enrollment.textContent).toContain('--cwd "$($PWD.Path)"')
    expect(enrollment.textContent).toContain('--name "This Computer Codex"')
    expect(enrollment.textContent).not.toContain('Host Codex')
  })

  test('shows manual-copy guidance when the setup command cannot be copied', async () => {
    const scrollSpy = vi
      .spyOn(Element.prototype, 'scrollIntoView')
      .mockImplementation(() => undefined)
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
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: vi.fn().mockRejectedValue(new Error('denied')) },
    })

    render(<AgentListView />)
    openMoreAgentSetup()

    const enrollment = screen.getByTestId('host-cli-enrollment-panel')
    fireEvent.click(within(enrollment).getByText('If the button does not work'))
    fireEvent.click(within(enrollment).getByRole('button', { name: /copy setup text/i }))

    const alert = await within(enrollment).findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent(
      'Copy did not work. Select the setup text in the box, then copy it yourself.'
    )
    expect(alert).not.toHaveTextContent(/clipboard access/i)
    await waitFor(() => expect(scrollSpy.mock.calls.length).toBeGreaterThan(0))
    const callsAfterFirstFailure = scrollSpy.mock.calls.length

    fireEvent.click(within(enrollment).getByRole('button', { name: /copy setup text/i }))

    await waitFor(() => expect(scrollSpy.mock.calls.length).toBeGreaterThan(callsAfterFirstFailure))

    fireEvent.click(within(enrollment).getByRole('button', { name: /windows/i }))

    expect(within(enrollment).queryByRole('alert')).toBeNull()
    scrollSpy.mockRestore()
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

  test('keeps the agent choice guide collapsed when agents already exist', () => {
    useAgentsStore.getState().setAgents([
      makeAgent({
        id: 'a1',
        name: 'Review Agent',
        provider: 'Review Model',
        model: 'review-model',
      }),
    ])

    render(<AgentListView />)

    expect(screen.queryByTestId('agent-choice-guide')).toBeNull()
    expect(screen.getByRole('button', { name: /which agent should i use/i })).toHaveAttribute(
      'aria-expanded',
      'false'
    )
    expect(screen.getByText('Review Agent')).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /which agent should i use/i }))

    const guide = screen.getByTestId('agent-choice-guide')
    expect(screen.getByRole('button', { name: /hide agent choice help/i })).toHaveAttribute(
      'aria-expanded',
      'true'
    )
    expect(within(guide).getByText('Choose by what the agent needs to use')).toBeDefined()
    expect(screen.queryByText('Pick by where the work should happen')).toBeNull()
    expect(within(guide).getByText(/simplest agent/i)).toBeDefined()
    expect(within(guide).getByText('Simple chat agent')).toBeDefined()
    expect(within(guide).getByText(/questions, writing, and checking results/i)).toBeDefined()
    expect(
      within(guide).getByText(/cannot take Tasks, change code, or use computer apps/i)
    ).toBeDefined()
    expect(within(guide).queryByText(/run commands/i)).toBeNull()
    expect(within(guide).queryByText(/planning, writing, and review/i)).toBeNull()
    expect(within(guide).getByText('This computer')).toBeDefined()
    expect(within(guide).getByText(/folder, accounts, or apps on your own computer/i)).toBeDefined()
    expect(within(guide).queryByText(/folder, accounts, or tools on your own machine/i)).toBeNull()
    expect(within(guide).getByText('Project files')).toBeDefined()
    expect(within(guide).getByText(/shared project files/i)).toBeDefined()
  })

  test('hides fleet filters until the list is large enough to need them', () => {
    useAgentsStore.getState().setAgents([
      makeAgent({
        id: 'a1',
        name: 'Review Agent',
        provider: 'Review Model',
        model: 'review-model',
      }),
      makeAgent({
        id: 'a2',
        name: 'Draft Agent',
        provider: 'Draft Model',
        model: 'draft-model',
      }),
    ])

    render(<AgentListView />)

    expect(screen.getByText('Review Agent')).toBeDefined()
    expect(screen.getByText('Draft Agent')).toBeDefined()
    expect(screen.queryByTestId('agent-fleet-controls')).toBeNull()
    expect(screen.queryByTestId('agent-search')).toBeNull()
    expect(screen.queryByRole('group', { name: /status filter/i })).toBeNull()
    expect(screen.queryByRole('group', { name: /work location filter/i })).toBeNull()
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
    expect(screen.getByRole('status')).toHaveTextContent('4/4 agents')
    expect(screen.getByTestId('agent-search')).toHaveAttribute(
      'aria-describedby',
      'agent-search-help'
    )
    expect(
      screen.getByText(/use show all agents to see every agent and work location again/i)
    ).toBeDefined()

    fireEvent.change(screen.getByTestId('agent-search'), { target: { value: 'review' } })
    expect(screen.getByText('Review Analyst')).toBeDefined()
    expect(screen.queryByText('Build Runner')).toBeNull()

    fireEvent.change(screen.getByTestId('agent-search'), {
      target: { value: 'text-review-model' },
    })
    expect(screen.getByTestId('agent-filter-empty')).toBeDefined()
    expect(screen.queryByText('Review Analyst')).toBeNull()

    fireEvent.change(screen.getByTestId('agent-search'), { target: { value: 'review analyst' } })
    expect(screen.getByText('Review Analyst')).toBeDefined()

    fireEvent.change(screen.getByTestId('agent-search'), { target: { value: '' } })
    const workLocationFilters = screen.getByRole('group', { name: /work location filter/i })
    expect(within(workLocationFilters).queryByRole('button', { name: /text only\s*1/i })).toBeNull()
    fireEvent.click(
      within(workLocationFilters).getByRole('button', { name: /simple chat agent\s*1/i })
    )
    expect(screen.getByText('Review Analyst')).toBeDefined()
    expect(screen.queryByText('Build Runner')).toBeNull()

    fireEvent.click(within(workLocationFilters).getByRole('button', { name: /this computer\s*1/i }))
    expect(screen.getByText('Local Agent')).toBeDefined()
    expect(screen.queryByText('Build Runner')).toBeNull()

    fireEvent.click(within(workLocationFilters).getByRole('button', { name: /all agents\s*4/i }))
    const statusFilters = screen.getByRole('group', { name: /status filter/i })
    fireEvent.click(within(statusFilters).getByRole('button', { name: /not connected\s*1/i }))
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
      makeAgent({
        id: 'review-agent',
        name: 'Review Runner',
        provider: 'Review Model',
        model: 'review-runner',
        status: 'idle',
      }),
      makeAgent({
        id: 'draft-agent',
        name: 'Draft Runner',
        provider: 'Draft Model',
        model: 'draft-runner',
        status: 'idle',
      }),
    ])

    render(<AgentListView />)

    fireEvent.change(screen.getByTestId('agent-search'), { target: { value: 'missing' } })
    const emptyState = screen.getByTestId('agent-filter-empty')
    expect(emptyState).toHaveAttribute('role', 'status')
    expect(emptyState).toHaveAttribute('aria-live', 'polite')
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
      makeAgent({
        id: 'review-agent',
        name: 'Review Runner',
        provider: 'Review Model',
        model: 'review-runner',
        cliTool: 'workspace-tool' as never,
        status: 'working',
      }),
      makeAgent({
        id: 'draft-agent',
        name: 'Draft Runner',
        provider: 'Draft Model',
        model: 'draft-runner',
        cliTool: 'workspace-tool' as never,
        status: 'working',
      }),
    ])

    render(<AgentListView />)

    const statusFilters = screen.getByRole('group', { name: /status filter/i })
    fireEvent.click(within(statusFilters).getByRole('button', { name: /not connected\s*0/i }))
    const emptyState = screen.getByTestId('agent-filter-empty')
    expect(emptyState).toHaveAttribute('role', 'status')
    expect(emptyState).toHaveAttribute('aria-live', 'polite')
    expect(within(emptyState).getByText('This status filter hides every agent')).toBeDefined()
    expect(
      within(emptyState).getByText(/another status, such as Working now, Ready, or Not connected/i)
    ).toBeDefined()
    expect(within(emptyState).getByText(/before deciding nobody is available/i)).toBeDefined()
    expect(emptyState.textContent).not.toContain('No Agents Match This View')
    expect(emptyState.textContent).not.toContain('idle')
    expect(emptyState.textContent).not.toContain('offline')

    fireEvent.click(within(emptyState).getByRole('button', { name: /show all agents/i }))
    expect(screen.getByText('Build Runner')).toBeDefined()
  })

  test('shows New agent buttons', () => {
    render(<AgentListView />)
    // Both the toolbar and the empty-state CTA render "New agent"
    expect(screen.getAllByText(/^New agent$/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/^Create Agent$/i)).toBeNull()
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
    openMoreAgentSetup()

    expect(screen.getByText('Task Queues')).toBeDefined()
    expect(
      screen.getByText(/shared task queues tell agents where to start new work/i)
    ).toBeDefined()
    expect(screen.queryByText(/agents check for tasks/i)).toBeNull()
    expect(screen.queryByText(/waiting places/i)).toBeNull()
    fireEvent.change(screen.getByLabelText(/task queue name/i), {
      target: { value: 'Frontend Delivery' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^create task queue$/i }))

    await waitFor(() =>
      expect(createAgentGroup).toHaveBeenCalledWith(
        'p1',
        expect.objectContaining({
          name: 'Frontend Delivery',
          description: 'Project tasks wait here until an available agent starts them.',
        })
      )
    )
    expect(useBoardStore.getState().selectedGroupId).toBe('g-new')
    expect(screen.getByRole('button', { name: /frontend delivery/i })).toHaveAttribute(
      'aria-pressed',
      'true'
    )
  })

  test('applies a task queue template before creating where tasks wait', async () => {
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
    openMoreAgentSetup()

    const templates = screen.getByRole('group', { name: /task queue templates/i })
    fireEvent.click(within(templates).getByRole('button', { name: /check results/i }))

    expect(screen.getByLabelText(/task queue name/i)).toHaveValue('Result Check Tasks')
    expect((screen.getByLabelText(/task queue description/i) as HTMLInputElement).value).toContain(
      'unsafe to use'
    )
    expect(
      (screen.getByLabelText(/task queue description/i) as HTMLInputElement).value
    ).not.toContain('Review completed work')
    expect(
      (screen.getByLabelText(/task queue description/i) as HTMLInputElement).value
    ).not.toContain('block release')

    fireEvent.click(screen.getByRole('button', { name: /^create task queue$/i }))

    await waitFor(() =>
      expect(createAgentGroup).toHaveBeenCalledWith(
        'p1',
        expect.objectContaining({
          name: 'Result Check Tasks',
          description: expect.stringContaining('unsafe to use'),
        })
      )
    )
    expect(useBoardStore.getState().selectedGroupId).toBe('g-review')
  })
})
