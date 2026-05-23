import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import '@app/i18n'
import { GettingStartedView } from '@app/pages/getting-started'
import { useNavigationStore } from '@app/entities/navigation'
import { useAgentsStore } from '@app/shared/model/agents.store'
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
const loadAgentsMock = vi.fn().mockResolvedValue(undefined)
const loadSkillsMock = vi.fn().mockResolvedValue(undefined)
const originalLoadOrgs = useNavigationStore.getState().loadOrgs
const originalLoadProviders = useSettingsStore.getState().loadProviders
const originalLoadRuntimeSettings = useSettingsStore.getState().loadRuntimeSettings
const originalLoadAgents = useAgentsStore.getState().loadAgents
const originalLoadSkills = useSkillsStore.getState().loadSkills

beforeEach(() => {
  navigateMock.mockReset()
  getTasksMock.mockReset().mockResolvedValue([])
  loadOrgsMock.mockClear()
  loadProvidersMock.mockClear()
  loadRuntimeSettingsMock.mockClear()
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
    loadProviders: loadProvidersMock,
    loadRuntimeSettings: loadRuntimeSettingsMock,
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
    loadProviders: originalLoadProviders,
    loadRuntimeSettings: originalLoadRuntimeSettings,
  })
  useAgentsStore.setState({ loadAgents: originalLoadAgents })
  useSkillsStore.setState({ loadSkills: originalLoadSkills })
})

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
          provider: 'openai',
          displayName: 'OpenAI',
          model: 'gpt-5.4',
          isEnabled: true,
          isDefault: true,
          lastTestStatus: 'passed',
        } as any,
      ],
      runtimeSettings: {
        defaultRuntime: 'container',
        availableRuntimes: ['container', 'api'],
        defaultCliTool: 'codex',
        availableCliTools: ['codex', 'claude'],
        cliToolDetails: [
          {
            cliTool: 'codex',
            image: 'agentforge-agent:codex',
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
          provider: 'openai',
          model: 'gpt-5.4',
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
    expect(screen.getByText(/container runtime with codex/i)).toBeDefined()
    expect(screen.getByText('OpenAI')).toBeDefined()
    expect(screen.getByText('Starter Agent')).toBeDefined()
    expect(await screen.findByText('100%')).toBeDefined()
    expect(loadOrgsMock).toHaveBeenCalled()
    expect(loadProvidersMock).toHaveBeenCalled()
    expect(loadRuntimeSettingsMock).toHaveBeenCalled()
    expect(loadAgentsMock).toHaveBeenCalled()
    expect(loadSkillsMock).toHaveBeenCalled()
    expect(getTasksMock).toHaveBeenCalledWith('group-1')
  })

  test('routes an incomplete provider step to provider settings', async () => {
    render(<GettingStartedView />)

    fireEvent.click(await screen.findByRole('button', { name: /add provider/i }))

    expect(navigateMock).toHaveBeenCalledWith({ to: '/settings/providers' })
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
        defaultCliTool: 'codex',
        availableCliTools: ['codex'],
        cliToolDetails: [
          {
            cliTool: 'codex',
            image: 'agentforge-agent:codex',
            version: '1.0.0',
            imagePresent: true,
            versionSource: 'docker-label',
          },
        ],
      },
      providers: [
        {
          id: 'provider-1',
          provider: 'openai',
          displayName: 'OpenAI',
          model: 'gpt-5.4',
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
          provider: 'openai',
          model: 'gpt-5.4',
          status: 'idle',
          tasksCompleted: 0,
          tasksInProgress: 0,
          successRate: 0,
        },
      ],
    })

    render(<GettingStartedView />)

    expect(
      await screen.findByText('Run Test on a provider before creating an agent.')
    ).toBeDefined()
    expect(screen.queryByText('100%')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: /test provider/i }))
    expect(navigateMock).toHaveBeenCalledWith({ to: '/settings/providers' })
  })

  test('routes runtime setup directly to runtime settings', async () => {
    render(<GettingStartedView />)

    fireEvent.click(await screen.findByRole('button', { name: /check runtime/i }))

    expect(navigateMock).toHaveBeenCalledWith({ to: '/settings/runtime' })
  })
})
