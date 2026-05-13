import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import '@app/i18n'
import { GettingStartedView } from '@app/pages/getting-started'
import { useNavigationStore } from '@app/entities/navigation'
import { useAgentsStore } from '@app/shared/model/agents.store'
import { useSettingsStore } from '@app/shared/model/settings.store'

const navigateMock = vi.hoisted(() => vi.fn())

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => navigateMock,
}))

const loadOrgsMock = vi.fn().mockResolvedValue(undefined)
const loadProvidersMock = vi.fn().mockResolvedValue(undefined)
const loadAgentsMock = vi.fn().mockResolvedValue(undefined)
const originalLoadOrgs = useNavigationStore.getState().loadOrgs
const originalLoadProviders = useSettingsStore.getState().loadProviders
const originalLoadAgents = useAgentsStore.getState().loadAgents

beforeEach(() => {
  navigateMock.mockReset()
  loadOrgsMock.mockClear()
  loadProvidersMock.mockClear()
  loadAgentsMock.mockClear()
  useNavigationStore.getState().reset()
  useAgentsStore.getState().reset()
  useSettingsStore.setState({
    providers: [],
    providersLoading: false,
    providersError: null,
    loadProviders: loadProvidersMock,
  })
  useNavigationStore.setState({ loadOrgs: loadOrgsMock })
  useAgentsStore.setState({ loadAgents: loadAgentsMock })
})

afterEach(() => {
  cleanup()
  useNavigationStore.getState().reset()
  useAgentsStore.getState().reset()
  useNavigationStore.setState({ loadOrgs: originalLoadOrgs })
  useSettingsStore.setState({ providers: [], loadProviders: originalLoadProviders })
  useAgentsStore.setState({ loadAgents: originalLoadAgents })
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

    expect(await screen.findByTestId('page-start')).toBeDefined()
    expect(screen.getByText('Launch Team')).toBeDefined()
    expect(screen.getAllByText('Launch Project').length).toBeGreaterThan(0)
    expect(screen.getByText('OpenAI')).toBeDefined()
    expect(screen.getByText('Starter Agent')).toBeDefined()
    expect(screen.getByText('100%')).toBeDefined()
    expect(loadOrgsMock).toHaveBeenCalled()
    expect(loadProvidersMock).toHaveBeenCalled()
    expect(loadAgentsMock).toHaveBeenCalled()
  })

  test('routes an incomplete provider step to provider settings', async () => {
    render(<GettingStartedView />)

    fireEvent.click(await screen.findByRole('button', { name: /add provider/i }))

    expect(navigateMock).toHaveBeenCalledWith({ to: '/settings/providers' })
  })
})
