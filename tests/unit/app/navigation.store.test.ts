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

function expectBeginnerError(actual: string | null, expected: string): void {
  expect(actual).toBe(expected)
  expect(actual).not.toContain('Code:')
  expect(actual).not.toContain('Details:')
}

beforeEach(() => {
  useNavigationStore.getState().reset()
  useBoardStore.getState().reset()
  vi.clearAllMocks()
})

describe('navigation.store', () => {
  it('turns expired sessions into a sign-in step', () => {
    expectBeginnerError(
      navigationActionErrorMessage(
        'organizations',
        'load',
        apiError(401, { error: 'token expired' })
      ),
      'Sign in again, then open the left menu and try to load team spaces again.'
    )
  })

  it('turns permission failures into team space access guidance', () => {
    expectBeginnerError(
      navigationActionErrorMessage('teamProjects', 'load', apiError(403, { message: 'forbidden' })),
      'You do not have permission to load teams and projects. Ask an owner or admin to update your team space access.'
    )
  })

  it('turns structured permission failures into team space access guidance', () => {
    const message = navigationActionErrorMessage('teamProjects', 'load', {
      serverError: 'owner policy denied for team list',
      status: '403',
    })

    expectBeginnerError(
      message,
      'You do not have permission to load teams and projects. Ask an owner or admin to update your team space access.'
    )
    expect(message).not.toContain('owner policy denied')
    expect(message).not.toContain('workspace access')
  })

  it('turns team and project validation failures into team-space guidance', () => {
    const message = navigationActionErrorMessage(
      'teamProjects',
      'load',
      apiError(422, { error: 'organization is required' })
    )

    expectBeginnerError(
      message,
      'Choose a team space you can access, refresh the left menu, then load its teams and projects again.'
    )
    expect(message).not.toContain('organization')
    expect(message).not.toContain('sidebar')
  })

  it('turns raw network failures into connection guidance', () => {
    const message = navigationActionErrorMessage(
      'workLanes',
      'load',
      new TypeError('Failed to fetch')
    )

    expectBeginnerError(
      message,
      'Check your connection, then refresh the left menu to load task queues.'
    )
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('service')
    expect(message).not.toContain('sidebar')
  })

  it('uses structured validation details for task queue names', () => {
    const message = navigationActionErrorMessage('workLane', 'create', {
      code: '422',
      details: { reason: 'name is required' },
    })

    expectBeginnerError(message, 'Name this task queue, choose its project, then create it again.')
    expect(message).not.toContain('name is required')
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

  it('selectProject resolves true on success and false when group loading fails', async () => {
    vi.mocked(agentGroupApi.getGroups).mockResolvedValueOnce([])
    await expect(useNavigationStore.getState().selectProject('p1')).resolves.toBe(true)

    vi.mocked(agentGroupApi.getGroups).mockRejectedValueOnce(new Error('network down'))
    await expect(useNavigationStore.getState().selectProject('p1')).resolves.toBe(false)
    expect(useNavigationStore.getState().error).toBe(
      'Refresh the selected project, then load task queues again.'
    )
    expect(useNavigationStore.getState().error).not.toContain('network down')
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

    expectBeginnerError(
      useNavigationStore.getState().error,
      'Refresh the left menu to load teams and projects. If it still fails, ask an owner or admin to check team space setup.'
    )
    expect(useNavigationStore.getState().error).not.toContain('temporarily unavailable')
    expect(useNavigationStore.getState().error).not.toContain('organization')
    expect(useNavigationStore.getState().error).not.toContain('workspace navigation')
    expect(useNavigationStore.getState().loading).toBe(false)
  })

  it('stores beginner guidance when team and project loading is denied', async () => {
    vi.mocked(teamApi.getTeams).mockRejectedValue(apiError(403, { error: 'missing team access' }))

    await useNavigationStore.getState().selectOrg('org-denied')

    expectBeginnerError(
      useNavigationStore.getState().error,
      'You do not have permission to load teams and projects. Ask an owner or admin to update your team space access.'
    )
  })

  it('stores connection guidance when task queues cannot load', async () => {
    vi.mocked(agentGroupApi.getGroups).mockRejectedValue(new TypeError('Failed to fetch'))

    await useNavigationStore.getState().selectProject('p-offline')

    expectBeginnerError(
      useNavigationStore.getState().error,
      'Check your connection, then refresh the left menu to load task queues.'
    )
    expect(useNavigationStore.getState().error).not.toContain('Failed to fetch')
  })

  it('stores field guidance when task queue creation is invalid', async () => {
    vi.mocked(agentGroupApi.createGroup).mockRejectedValue(
      apiError(422, { error: 'name is required' })
    )

    await expect(
      useNavigationStore.getState().createAgentGroup('p1', {
        name: '',
        description: 'Agents in this group can receive tasks from the board.',
      })
    ).rejects.toThrow('API 422')

    expectBeginnerError(
      useNavigationStore.getState().error,
      'Name this task queue, choose its project, then create it again.'
    )
  })

  it('stores field guidance when task queue creation returns structured details', async () => {
    vi.mocked(agentGroupApi.createGroup).mockRejectedValue({
      statusCode: '422',
      details: { reason: 'project is required' },
    })

    await expect(
      useNavigationStore.getState().createAgentGroup('', {
        name: 'Review',
        description: 'Agents in this group can receive tasks from the board.',
      })
    ).rejects.toMatchObject({ statusCode: '422' })

    expectBeginnerError(
      useNavigationStore.getState().error,
      'Choose the project that should hold this task queue, then try again.'
    )
    expect(useNavigationStore.getState().error).not.toContain('project is required')
  })
})
