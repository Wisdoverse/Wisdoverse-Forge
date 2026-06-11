import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import '@app/i18n'
import { GettingStartedView } from '@app/pages/getting-started'
import { useNavigationStore } from '@app/entities/navigation'
import { useAgentsStore } from '@app/entities/agent'
import { useSettingsStore } from '@app/shared/model/settings.store'
import { useSkillsStore } from '@app/shared/model/skills.store'

const navigateMock = vi.hoisted(() => vi.fn())
const getTasksMock = vi.hoisted(() => vi.fn())

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => navigateMock,
}))

vi.mock('@app/shared/api/orchestration', () => ({
  taskResultArtifacts: (result: unknown) => (Array.isArray(result) ? result : []),
  orchestrationApi: {
    getTasks: (...args: unknown[]) => getTasksMock(...args),
  },
}))

const loadOrgsMock = vi.fn().mockResolvedValue(undefined)
const loadProvidersMock = vi.fn().mockResolvedValue(undefined)
const loadRuntimeSettingsMock = vi.fn().mockResolvedValue(undefined)
const loadPreferencesMock = vi.fn().mockResolvedValue(undefined)
const setGettingStartedDismissedMock = vi.fn()
const loadAgentsMock = vi.fn().mockResolvedValue(undefined)
const loadSkillsMock = vi.fn().mockResolvedValue(undefined)
const originalLoadOrgs = useNavigationStore.getState().loadOrgs
const originalLoadProviders = useSettingsStore.getState().loadProviders
const originalLoadRuntimeSettings = useSettingsStore.getState().loadRuntimeSettings
const originalLoadPreferences = useSettingsStore.getState().loadPreferences
const originalSetGettingStartedDismissed = useSettingsStore.getState().setGettingStartedDismissed
const originalLoadAgents = useAgentsStore.getState().loadAgents
const originalLoadSkills = useSkillsStore.getState().loadSkills

beforeEach(() => {
  navigateMock.mockReset()
  getTasksMock.mockReset().mockResolvedValue([])
  loadOrgsMock.mockClear()
  loadProvidersMock.mockClear()
  loadRuntimeSettingsMock.mockClear()
  loadPreferencesMock.mockClear()
  setGettingStartedDismissedMock.mockReset().mockResolvedValue(true)
  loadAgentsMock.mockClear()
  loadSkillsMock.mockClear()
  useNavigationStore.getState().reset()
  useAgentsStore.getState().reset()
  useSkillsStore.getState().reset()
  useSettingsStore.setState({
    providers: [],
    providersLoading: false,
    providersError: null,
    runtimeSettings: null,
    runtimeLoading: false,
    runtimeError: null,
    preferences: {},
    preferencesLoaded: true,
    preferencesLoading: false,
    loadProviders: loadProvidersMock,
    loadRuntimeSettings: loadRuntimeSettingsMock,
    loadPreferences: loadPreferencesMock,
    setGettingStartedDismissed: setGettingStartedDismissedMock,
  })
  useNavigationStore.setState({ loadOrgs: loadOrgsMock })
  useAgentsStore.setState({ loadAgents: loadAgentsMock })
  useSkillsStore.setState({ loadSkills: loadSkillsMock })
})

afterEach(() => {
  cleanup()
  useNavigationStore.getState().reset()
  useAgentsStore.getState().reset()
  useSkillsStore.getState().reset()
  useNavigationStore.setState({ loadOrgs: originalLoadOrgs })
  useSettingsStore.setState({
    providers: [],
    runtimeSettings: null,
    preferences: null,
    preferencesLoaded: false,
    preferencesLoading: false,
    loadProviders: originalLoadProviders,
    loadRuntimeSettings: originalLoadRuntimeSettings,
    loadPreferences: originalLoadPreferences,
    setGettingStartedDismissed: originalSetGettingStartedDismissed,
  })
  useAgentsStore.setState({ loadAgents: originalLoadAgents })
  useSkillsStore.setState({ loadSkills: originalLoadSkills })
})

/**
 * Seed every store so all eight checklist steps read complete. Mirrors the
 * fixture used by the "shows the first-run checklist" test.
 */
function seedCompletedSetup() {
  useNavigationStore.setState({
    teams: [
      {
        id: 'team-1',
        orgId: 'org-1',
        name: 'Launch Team',
        slug: 'launch-team',
        visibility: 'open',
        description: '',
      },
    ],
    projects: {
      'team-1': [
        {
          id: 'project-1',
          teamId: 'team-1',
          name: 'Launch Project',
          slug: 'launch-project',
          color: '#007AFF',
          description: '',
        },
      ],
    },
    selectedProjectId: 'project-1',
    agentGroups: [{ id: 'group-1', projectId: 'project-1', name: 'Default' }],
  })
  useSettingsStore.setState({
    providers: [
      {
        id: 'provider-1',
        provider: 'model-service',
        displayName: 'Model Service',
        model: 'general-model',
        isEnabled: true,
        isDefault: true,
        lastTestStatus: 'passed',
      } as any,
    ],
    runtimeSettings: {
      defaultRuntime: 'container',
      availableRuntimes: ['container', 'api'],
      defaultCliTool: 'workspace-tool',
      availableCliTools: ['workspace-tool', 'review-tool'],
      cliToolDetails: [
        {
          cliTool: 'workspace-tool',
          image: 'agentforge-agent:workspace-tool',
          version: '1.0.0',
          imagePresent: true,
          versionSource: 'docker-label',
        },
      ],
    },
  })
  useAgentsStore.setState({
    agents: [
      {
        id: 'agent-1',
        name: 'Starter Agent',
        provider: 'model-service',
        model: 'general-model',
        status: 'idle',
        tasksCompleted: 0,
        tasksInProgress: 0,
        successRate: 0,
      },
    ],
  })
  getTasksMock.mockResolvedValueOnce([
    {
      id: 'task-1',
      groupId: 'group-1',
      state: 'completed',
      method: 'tasks/send',
      params: { task: 'Ship first flow', message: 'Review output' },
      assignedTo: 'agent-1',
      assignedAgentName: 'Starter Agent',
      priority: 'normal',
      progress: 100,
      result: [{ name: 'summary.md', mimeType: 'text/markdown', data: 'Done' }],
      contextCounts: { appliedMemories: 0, appliedSkills: 1, total: 1 },
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      completedAt: new Date().toISOString(),
    },
  ])
}

describe('GettingStartedView', () => {
  test('shows the first-run checklist from current workspace state', async () => {
    useNavigationStore.setState({
      teams: [
        {
          id: 'team-1',
          orgId: 'org-1',
          name: 'Launch Team',
          slug: 'launch-team',
          visibility: 'open',
          description: '',
        },
      ],
      projects: {
        'team-1': [
          {
            id: 'project-1',
            teamId: 'team-1',
            name: 'Launch Project',
            slug: 'launch-project',
            color: '#007AFF',
            description: '',
          },
        ],
      },
      selectedProjectId: 'project-1',
      agentGroups: [{ id: 'group-1', projectId: 'project-1', name: 'Default' }],
    })
    useSettingsStore.setState({
      providers: [
        {
          id: 'provider-1',
          provider: 'model-service',
          displayName: 'Model Service',
          model: 'general-model',
          isEnabled: true,
          isDefault: true,
          lastTestStatus: 'passed',
        } as any,
      ],
      runtimeSettings: {
        defaultRuntime: 'container',
        availableRuntimes: ['container', 'api'],
        defaultCliTool: 'workspace-tool',
        availableCliTools: ['workspace-tool', 'review-tool'],
        cliToolDetails: [
          {
            cliTool: 'workspace-tool',
            image: 'agentforge-agent:workspace-tool',
            version: '1.0.0',
            imagePresent: true,
            versionSource: 'docker-label',
          },
        ],
      },
    })
    useAgentsStore.setState({
      agents: [
        {
          id: 'agent-1',
          name: 'Starter Agent',
          provider: 'model-service',
          model: 'general-model',
          status: 'idle',
          tasksCompleted: 0,
          tasksInProgress: 0,
          successRate: 0,
        },
      ],
    })
    getTasksMock.mockResolvedValueOnce([
      {
        id: 'task-1',
        groupId: 'group-1',
        state: 'completed',
        method: 'tasks/send',
        params: { task: 'Ship first flow', message: 'Review output' },
        assignedTo: 'agent-1',
        assignedAgentName: 'Starter Agent',
        priority: 'normal',
        progress: 100,
        result: [{ name: 'summary.md', mimeType: 'text/markdown', data: 'Done' }],
        contextCounts: { appliedMemories: 0, appliedSkills: 1, total: 1 },
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        completedAt: new Date().toISOString(),
      },
    ])

    render(<GettingStartedView />)

    expect(await screen.findByTestId('page-start')).toBeDefined()
    expect(screen.getAllByText('Launch Project').length).toBeGreaterThan(0)
    expect(screen.getByText(/managed workspace is ready for agent work/i)).toBeDefined()
    expect(screen.getByText('Model Service')).toBeDefined()
    expect(screen.getByText('Starter Agent')).toBeDefined()
    expect(await screen.findByText('100%')).toBeDefined()
    expect(screen.getByText('Ready to run work')).toBeDefined()
    expect(screen.getByText(/The basic path is complete/i)).toBeDefined()
    expect(screen.getAllByText('Reuse what worked').length).toBeGreaterThan(0)
    expect(screen.getByText('Saved instructions are available for future tasks.')).toBeDefined()
    expect(
      screen.getAllByRole('button', { name: /show saved instructions/i }).length
    ).toBeGreaterThan(0)
    expect(screen.queryByText('Reusable learning')).toBeNull()
    expect(screen.queryByText(/applied skill context/i)).toBeNull()
    expect(screen.queryByText(/skill candidates/i)).toBeNull()
    expect(loadOrgsMock).toHaveBeenCalled()
    expect(loadProvidersMock).toHaveBeenCalled()
    expect(loadRuntimeSettingsMock).toHaveBeenCalled()
    expect(loadAgentsMock).toHaveBeenCalled()
    expect(loadSkillsMock).toHaveBeenCalled()
    expect(getTasksMock).toHaveBeenCalledWith('group-1')
  })

  test('routes an incomplete provider step to provider settings', async () => {
    render(<GettingStartedView />)

    expect(await screen.findByText('Do this next')).toBeDefined()
    expect(screen.getAllByText(/A project gives tasks a clear home/i).length).toBeGreaterThan(0)
    fireEvent.click(await screen.findByRole('button', { name: /add AI service/i }))

    expect(navigateMock).toHaveBeenCalledWith({ to: '/settings/providers' })
  })

  test('explains task routing as a beginner-friendly queue', async () => {
    useNavigationStore.setState({
      teams: [
        {
          id: 'team-1',
          orgId: 'org-1',
          name: 'Launch Team',
          slug: 'launch-team',
          visibility: 'open',
          description: '',
        },
      ],
      projects: {
        'team-1': [
          {
            id: 'project-1',
            teamId: 'team-1',
            name: 'Launch Project',
            slug: 'launch-project',
            color: '#007AFF',
            description: '',
          },
        ],
      },
      selectedProjectId: 'project-1',
      agentGroups: [],
    })
    useSettingsStore.setState({
      providers: [
        {
          id: 'provider-1',
          provider: 'model-service',
          displayName: 'Model Service',
          model: 'general-model',
          isEnabled: true,
          isDefault: true,
          lastTestStatus: 'passed',
        } as any,
      ],
      runtimeSettings: {
        defaultRuntime: 'container',
        availableRuntimes: ['container'],
        defaultCliTool: 'workspace-tool',
        availableCliTools: ['workspace-tool'],
        cliToolDetails: [
          {
            cliTool: 'workspace-tool',
            image: 'agentforge-agent:workspace-tool',
            version: '1.0.0',
            imagePresent: true,
            versionSource: 'docker-label',
          },
        ],
      },
    })
    useAgentsStore.setState({
      agents: [
        {
          id: 'agent-1',
          name: 'Starter Agent',
          provider: 'model-service',
          model: 'general-model',
          status: 'idle',
          tasksCompleted: 0,
          tasksInProgress: 0,
          successRate: 0,
        },
      ],
    })

    render(<GettingStartedView />)

    expect(await screen.findByText('Do this next')).toBeDefined()
    expect(screen.getAllByText('Task queue').length).toBeGreaterThan(0)
    expect(screen.getByText('Create a task queue for this project.')).toBeDefined()
    expect(
      screen.getAllByText(
        'A task queue is the place new work waits until an agent is ready to pick it up.'
      ).length
    ).toBeGreaterThanOrEqual(2)
    expect(screen.getByText('Create a task queue before the first task.')).toBeDefined()
  })

  test('does not expose raw AI service keys in the first-run checklist', async () => {
    useSettingsStore.setState({
      providers: [
        {
          id: 'provider-future',
          provider: 'future_provider',
          displayName: '',
          model: 'future-model',
          isEnabled: true,
          isDefault: true,
          lastTestStatus: 'passed',
        } as any,
      ],
    })

    render(<GettingStartedView />)

    expect(await screen.findByText('AI service needs review')).toBeDefined()
    expect(screen.queryByText(/future_provider/i)).toBeNull()
    expect(screen.queryByText(/future provider/i)).toBeNull()
  })

  test('accepts a local agent as work access', async () => {
    useSettingsStore.setState({
      runtimeSettings: {
        defaultRuntime: 'container',
        availableRuntimes: ['container'],
        defaultCliTool: 'workspace-tool',
        availableCliTools: ['workspace-tool'],
        cliToolDetails: [
          {
            cliTool: 'workspace-tool',
            image: 'agentforge-agent:workspace-tool',
            version: '1.0.0',
            imagePresent: true,
            versionSource: 'docker-label',
          },
        ],
      },
      providers: [],
    })
    useAgentsStore.setState({
      agents: [
        {
          id: 'host-agent',
          name: 'Local Agent',
          provider: 'local-model',
          model: 'local-runner',
          cliTool: 'workspace-tool',
          runtimeId: 'host-abc12345',
          runtimeKind: 'cli',
          status: 'idle',
          tasksCompleted: 0,
          tasksInProgress: 0,
          successRate: 0,
        },
      ],
    })

    render(<GettingStartedView />)

    expect(
      await screen.findByText('Local Agent is ready to run work from this computer.')
    ).toBeDefined()
    fireEvent.click(screen.getByRole('button', { name: /review agents/i }))
    expect(navigateMock).toHaveBeenCalledWith({ to: '/agents' })
  })

  test('does not complete provider step until connection test has passed', async () => {
    useNavigationStore.setState({
      teams: [
        {
          id: 'team-1',
          orgId: 'org-1',
          name: 'Launch Team',
          slug: 'launch-team',
          visibility: 'open',
          description: '',
        },
      ],
      projects: {
        'team-1': [
          {
            id: 'project-1',
            teamId: 'team-1',
            name: 'Launch Project',
            slug: 'launch-project',
            color: '#007AFF',
            description: '',
          },
        ],
      },
      selectedProjectId: 'project-1',
      agentGroups: [{ id: 'group-1', projectId: 'project-1', name: 'Default' }],
    })
    useSettingsStore.setState({
      runtimeSettings: {
        defaultRuntime: 'container',
        availableRuntimes: ['container'],
        defaultCliTool: 'workspace-tool',
        availableCliTools: ['workspace-tool'],
        cliToolDetails: [
          {
            cliTool: 'workspace-tool',
            image: 'agentforge-agent:workspace-tool',
            version: '1.0.0',
            imagePresent: true,
            versionSource: 'docker-label',
          },
        ],
      },
      providers: [
        {
          id: 'provider-1',
          provider: 'model-service',
          displayName: 'Model Service',
          model: 'general-model',
          isEnabled: true,
          isDefault: true,
        } as any,
      ],
    })
    useAgentsStore.setState({
      agents: [
        {
          id: 'agent-1',
          name: 'Starter Agent',
          provider: 'model-service',
          model: 'general-model',
          status: 'idle',
          tasksCompleted: 0,
          tasksInProgress: 0,
          successRate: 0,
        },
      ],
    })

    render(<GettingStartedView />)

    expect(await screen.findByText('Check the AI service before giving agents work.')).toBeDefined()
    expect(screen.getByText('Do this next')).toBeDefined()
    expect(screen.getAllByText(/Agents need one ready option/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/checked model service/i)).toBeNull()
    expect(screen.queryByText(/assigning work/i)).toBeNull()
    expect(screen.queryByText('100%')).toBeNull()
    const [testProviderButton] = screen.getAllByRole('button', { name: /check AI service/i })
    expect(testProviderButton).toBeDefined()
    fireEvent.click(testProviderButton!)
    expect(navigateMock).toHaveBeenCalledWith({ to: '/settings/providers' })
  })

  test('routes agent setup directly to work settings', async () => {
    render(<GettingStartedView />)

    fireEvent.click(await screen.findByRole('button', { name: /choose work location/i }))

    expect(navigateMock).toHaveBeenCalledWith({ to: '/settings/runtime' })
  })

  test('loads stored preferences alongside the other first-run data', async () => {
    render(<GettingStartedView />)

    expect(await screen.findByTestId('page-start')).toBeDefined()
    expect(loadPreferencesMock).toHaveBeenCalled()
  })

  test('skip action persists the dismissal and moves to the task board', async () => {
    render(<GettingStartedView />)

    fireEvent.click(await screen.findByTestId('getting-started-skip'))

    expect(setGettingStartedDismissedMock).toHaveBeenCalledWith(true)
    expect(navigateMock).toHaveBeenCalledWith({ to: '/tasks' })
  })

  test('auto-dismisses exactly once when every step is complete', async () => {
    seedCompletedSetup()

    const view = render(<GettingStartedView />)
    expect(await screen.findByText('100%')).toBeDefined()

    await waitFor(() => expect(setGettingStartedDismissedMock).toHaveBeenCalledTimes(1))
    expect(setGettingStartedDismissedMock).toHaveBeenCalledWith(true)

    // The same mounted page re-rendering with unchanged completion state must
    // not fire the persistence again (the mock does not update the store, so
    // only the ref guard prevents a second call here).
    view.rerender(<GettingStartedView />)
    expect(await screen.findByText('100%')).toBeDefined()
    expect(setGettingStartedDismissedMock).toHaveBeenCalledTimes(1)
  })

  test('does not auto-dismiss while preferences are still loading', async () => {
    seedCompletedSetup()
    useSettingsStore.setState({ preferences: null, preferencesLoaded: false })

    render(<GettingStartedView />)

    expect(await screen.findByText('100%')).toBeDefined()
    expect(screen.getByText('Ready to run work')).toBeDefined()
    expect(setGettingStartedDismissedMock).not.toHaveBeenCalled()
  })

  test('does not re-persist a dismissal that is already stored', async () => {
    seedCompletedSetup()
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: true },
      preferencesLoaded: true,
    })

    render(<GettingStartedView />)

    expect(await screen.findByText('100%')).toBeDefined()
    expect(setGettingStartedDismissedMock).not.toHaveBeenCalled()
  })
})
