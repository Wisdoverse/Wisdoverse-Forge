import { describe, it, expect, vi, beforeEach } from 'vitest'
import { navigationActionErrorMessage, useNavigationStore } from '@app/entities/navigation'
import { useBoardStore } from '@app/shared/model/board.store'

vi.mock('@app/entities/organization', () => ({
  organizationApi: { getOrgs: vi.fn(), updateOrg: vi.fn() },
}))

vi.mock('@app/entities/team', () => ({
  teamApi: { getTeams: vi.fn(), updateTeam: vi.fn(), deleteTeam: vi.fn() },
}))

vi.mock('@app/entities/project', () => ({
  projectApi: { getProjects: vi.fn(), updateProject: vi.fn(), deleteProject: vi.fn() },
}))

vi.mock('@app/entities/agent-group', () => ({
  agentGroupApi: { getGroups: vi.fn(), createGroup: vi.fn() },
}))

import { organizationApi } from '@app/entities/organization'
import { projectApi } from '@app/entities/project'
import { agentGroupApi } from '@app/entities/agent-group'
import { teamApi } from '@app/entities/team'

function apiError(status: number, payload: Record<string, unknown> | string): Error {
  const body = typeof payload === 'string' ? payload : JSON.stringify(payload)
  return new Error(`API ${status}: ${body}`)
}

beforeEach(() => {
  useNavigationStore.getState().reset()
  useBoardStore.getState().reset()
  vi.clearAllMocks()
})

describe('navigation.store', () => {
  it('turns expired sessions into a sign-in step', () => {
    expect(
      navigationActionErrorMessage(
        'organizations',
        'load',
        apiError(401, { error: 'token expired' })
      )
    ).toBe('Sign in again, then load organizations. Code: 401. Details: token expired')
  })

  it('turns permission failures into workspace access guidance', () => {
    expect(
      navigationActionErrorMessage('teamProjects', 'load', apiError(403, { message: 'forbidden' }))
    ).toBe(
      'You do not have permission to load teams and projects. Ask an admin to update your workspace access. Code: 403. Details: forbidden'
    )
  })

  it('turns raw network failures into connection guidance', () => {
    expect(
      navigationActionErrorMessage('workLanes', 'load', new TypeError('Failed to fetch'))
    ).toBe(
      'Navigation could not load work lanes because the browser could not reach the server. Check your connection and refresh the page.'
    )
  })

  it('loadOrgs fetches and stores orgs, auto-selects first', async () => {
    vi.mocked(organizationApi.getOrgs).mockResolvedValue([
      { id: 'org1', name: 'Org 1', slug: 'org-1', plan: 'pro', role: 'owner' },
    ])
    vi.mocked(teamApi.getTeams).mockResolvedValue([])

    await useNavigationStore.getState().loadOrgs()

    const state = useNavigationStore.getState()
    expect(state.orgs).toHaveLength(1)
    expect(state.selectedOrgId).toBe('org1')
  })

  it('selectOrg sets org and loads teams/projects', async () => {
    vi.mocked(teamApi.getTeams).mockResolvedValue([
      {
        id: 't1',
        orgId: 'org2',
        name: 'Team A',
        slug: 'team-a',
        visibility: 'open',
        description: '',
      },
    ])
    vi.mocked(projectApi.getProjects).mockResolvedValue([
      { id: 'p1', teamId: 't1', name: 'Proj', slug: 'proj', color: '#007AFF', description: '' },
    ])

    await useNavigationStore.getState().selectOrg('org2')

    const state = useNavigationStore.getState()
    expect(state.selectedOrgId).toBe('org2')
    expect(state.teams).toHaveLength(1)
    expect(state.projects['t1']).toHaveLength(1)
    expect(state.selectedProjectId).toBeNull()
  })

  it('selectProject sets project and loads groups into board store', async () => {
    vi.mocked(agentGroupApi.getGroups).mockResolvedValue([
      { id: 'g1', name: 'Default', projectId: 'p1' },
    ])

    await useNavigationStore.getState().selectProject('p1')

    expect(useNavigationStore.getState().selectedProjectId).toBe('p1')
    expect(useBoardStore.getState().selectedGroupId).toBe('g1')
  })

  it('selectProject clears group when no groups exist', async () => {
    useBoardStore.getState().setSelectedGroupId('g-old')
    vi.mocked(agentGroupApi.getGroups).mockResolvedValue([])

    await useNavigationStore.getState().selectProject('p-empty')

    expect(useBoardStore.getState().selectedGroupId).toBeNull()
  })

  it('selectProject clears stale groups before loading the next project', async () => {
    useNavigationStore.setState({
      selectedProjectId: 'p-old',
      agentGroups: [{ id: 'g-old', name: 'Old', projectId: 'p-old' }],
    })
    useBoardStore.getState().setSelectedGroupId('g-old')
    vi.mocked(agentGroupApi.getGroups).mockRejectedValue(new Error('network down'))

    await useNavigationStore.getState().selectProject('p-new')

    expect(useNavigationStore.getState().selectedProjectId).toBe('p-new')
    expect(useNavigationStore.getState().agentGroups).toEqual([])
    expect(useBoardStore.getState().selectedGroupId).toBeNull()
  })

  it('createAgentGroup appends the group and selects it for board routing', async () => {
    useNavigationStore.setState({ selectedProjectId: 'p1', agentGroups: [] })
    vi.mocked(agentGroupApi.createGroup).mockResolvedValue({
      id: 'g-new',
      name: 'Design Review',
      projectId: 'p1',
    })

    const group = await useNavigationStore.getState().createAgentGroup('p1', {
      name: 'Design Review',
      description: 'Agents in this group can receive tasks from the board.',
    })

    expect(agentGroupApi.createGroup).toHaveBeenCalledWith({
      projectId: 'p1',
      name: 'Design Review',
      description: 'Agents in this group can receive tasks from the board.',
    })
    expect(group.id).toBe('g-new')
    expect(useNavigationStore.getState().agentGroups).toEqual([group])
    expect(useBoardStore.getState().selectedGroupId).toBe('g-new')
  })

  it('toggleSidebar toggles expanded state', () => {
    expect(useNavigationStore.getState().sidebarExpanded).toBe(true)
    useNavigationStore.getState().toggleSidebar()
    expect(useNavigationStore.getState().sidebarExpanded).toBe(false)
    useNavigationStore.getState().toggleSidebar()
    expect(useNavigationStore.getState().sidebarExpanded).toBe(true)
  })

  it('toggleTeam adds/removes from expandedTeams', () => {
    useNavigationStore.getState().toggleTeam('t1')
    expect(useNavigationStore.getState().expandedTeams).toContain('t1')
    useNavigationStore.getState().toggleTeam('t1')
    expect(useNavigationStore.getState().expandedTeams).not.toContain('t1')
  })

  it('updateTeam persists through team API and patches local teams', async () => {
    useNavigationStore.setState({
      teams: [
        {
          id: 't1',
          orgId: 'org1',
          name: 'Team A',
          slug: 'team-a',
          visibility: 'open',
          description: '',
        },
      ],
    })
    vi.mocked(teamApi.updateTeam).mockResolvedValue({
      id: 't1',
      orgId: 'org1',
      name: 'Team Renamed',
      slug: 'team-a',
      visibility: 'open',
      description: '',
    })

    await useNavigationStore.getState().updateTeam('t1', { name: 'Team Renamed' })

    expect(teamApi.updateTeam).toHaveBeenCalledWith('org1', 't1', { name: 'Team Renamed' })
    expect(useNavigationStore.getState().teams[0].name).toBe('Team Renamed')
  })

  it('deleteTeam removes team, projects, expansion and selected project state', async () => {
    useNavigationStore.setState({
      teams: [
        {
          id: 't1',
          orgId: 'org1',
          name: 'Team A',
          slug: 'team-a',
          visibility: 'open',
          description: '',
        },
      ],
      projects: {
        t1: [
          {
            id: 'p1',
            teamId: 't1',
            name: 'Project',
            slug: 'project',
            color: '#007AFF',
            description: '',
          },
        ],
      },
      expandedTeams: ['t1'],
      selectedProjectId: 'p1',
    })
    useBoardStore.getState().setSelectedGroupId('g1')
    vi.mocked(teamApi.deleteTeam).mockResolvedValue(undefined)

    await useNavigationStore.getState().deleteTeam('t1')

    expect(teamApi.deleteTeam).toHaveBeenCalledWith('org1', 't1')
    expect(useNavigationStore.getState().teams).toHaveLength(0)
    expect(useNavigationStore.getState().projects.t1).toBeUndefined()
    expect(useNavigationStore.getState().expandedTeams).not.toContain('t1')
    expect(useNavigationStore.getState().selectedProjectId).toBeNull()
    expect(useBoardStore.getState().selectedGroupId).toBeNull()
  })

  it('selectOrg clears selectedProjectId and resets board', async () => {
    useNavigationStore.setState({ selectedProjectId: 'p-old' })
    useBoardStore.getState().setSelectedGroupId('g-old')

    vi.mocked(teamApi.getTeams).mockResolvedValue([])

    await useNavigationStore.getState().selectOrg('org-new')

    expect(useNavigationStore.getState().selectedProjectId).toBeNull()
    expect(useBoardStore.getState().selectedGroupId).toBeNull()
  })

  it('stores beginner guidance when organizations cannot load', async () => {
    vi.mocked(organizationApi.getOrgs).mockRejectedValue(
      apiError(503, { error: { message: 'database unavailable' } })
    )

    await useNavigationStore.getState().loadOrgs()

    expect(useNavigationStore.getState().error).toBe(
      'The workspace navigation service had a server problem. Try again after the backend is healthy. Code: 503. Details: database unavailable'
    )
    expect(useNavigationStore.getState().loading).toBe(false)
  })

  it('stores beginner guidance when team and project loading is denied', async () => {
    vi.mocked(teamApi.getTeams).mockRejectedValue(apiError(403, { error: 'missing team access' }))

    await useNavigationStore.getState().selectOrg('org-denied')

    expect(useNavigationStore.getState().error).toBe(
      'You do not have permission to load teams and projects. Ask an admin to update your workspace access. Code: 403. Details: missing team access'
    )
  })

  it('stores connection guidance when work lanes cannot load', async () => {
    vi.mocked(agentGroupApi.getGroups).mockRejectedValue(new TypeError('Failed to fetch'))

    await useNavigationStore.getState().selectProject('p-offline')

    expect(useNavigationStore.getState().error).toBe(
      'Navigation could not load work lanes because the browser could not reach the server. Check your connection and refresh the page.'
    )
  })

  it('stores field guidance when work lane creation is invalid', async () => {
    vi.mocked(agentGroupApi.createGroup).mockRejectedValue(
      apiError(422, { error: 'name is required' })
    )

    await expect(
      useNavigationStore.getState().createAgentGroup('p1', {
        name: '',
        description: 'Agents in this group can receive tasks from the board.',
      })
    ).rejects.toThrow('API 422')

    expect(useNavigationStore.getState().error).toBe(
      'Check the required fields for the work lane, then try again. Code: 422. Details: name is required'
    )
  })
})
