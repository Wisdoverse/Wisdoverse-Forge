import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { organizationApi } from '@app/entities/organization'
import { projectApi } from '@app/entities/project'
import { agentGroupApi } from '@app/entities/agent-group'
import { teamApi } from '@app/entities/team'
import { userApi } from '@app/entities/user'

// Save and override global fetch — avoid vi.stubGlobal which restoreMocks undoes
const originalFetch = globalThis.fetch
const mockFetch = vi.fn()

beforeEach(() => {
  globalThis.fetch = mockFetch as any
  mockFetch.mockReset()
})

afterEach(() => {
  globalThis.fetch = originalFetch
})

describe('navigation entity APIs', () => {
  it('getOrgs returns parsed orgs array', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () =>
        Promise.resolve({
          ok: true,
          orgs: [{ id: 'org1', name: 'Org One', slug: 'org-one', plan: 'pro', role: 'owner' }],
        }),
    })

    const orgs = await organizationApi.getOrgs()

    expect(orgs).toEqual([
      { id: 'org1', name: 'Org One', slug: 'org-one', plan: 'pro', role: 'owner' },
    ])
    expect(mockFetch).toHaveBeenCalledWith('/api/v1/orgs', expect.any(Object))
  })

  it('getTeams returns teams for org', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () =>
        Promise.resolve({
          ok: true,
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
        }),
    })

    const teams = await teamApi.getTeams('org1')

    expect(teams).toHaveLength(1)
    expect(teams[0].name).toBe('Team A')
    expect(mockFetch).toHaveBeenCalledWith('/api/v1/orgs/org1/teams', expect.any(Object))
  })

  it('createTeam posts to the org team hierarchy', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () =>
        Promise.resolve({
          ok: true,
          team: {
            id: 't1',
            orgId: 'org1',
            name: 'Team A',
            slug: 'team-a',
            visibility: 'private',
            description: '',
          },
        }),
    })

    const team = await teamApi.createTeam('org1', { name: 'Team A', slug: 'team-a' })

    expect(team.id).toBe('t1')
    expect(mockFetch).toHaveBeenCalledWith(
      '/api/v1/orgs/org1/teams',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ name: 'Team A', slug: 'team-a' }),
      })
    )
  })

  it('getProjects returns projects for team', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () =>
        Promise.resolve({
          ok: true,
          projects: [
            {
              id: 'p1',
              teamId: 't1',
              name: 'Project X',
              slug: 'project-x',
              color: '#007AFF',
              description: '',
            },
          ],
        }),
    })

    const projects = await projectApi.getProjects('t1')

    expect(projects).toHaveLength(1)
    expect(projects[0].name).toBe('Project X')
    expect(mockFetch).toHaveBeenCalledWith('/api/v1/teams/t1/projects', expect.any(Object))
  })

  it('createProject posts to the team project hierarchy', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () =>
        Promise.resolve({
          ok: true,
          project: {
            id: 'p1',
            teamId: 't1',
            name: 'Project X',
            slug: 'project-x',
            color: '#007AFF',
            description: '',
          },
        }),
    })

    const project = await projectApi.createProject('t1', {
      name: 'Project X',
      slug: 'project-x',
    })

    expect(project.id).toBe('p1')
    expect(mockFetch).toHaveBeenCalledWith(
      '/api/v1/teams/t1/projects',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ name: 'Project X', slug: 'project-x' }),
      })
    )
  })

  it('team member APIs use org-scoped team member routes', async () => {
    mockFetch
      .mockResolvedValueOnce({
        ok: true,
        json: () =>
          Promise.resolve({
            ok: true,
            members: [{ userId: 'u1', email: 'dev@example.com', username: 'Dev', role: 'member' }],
          }),
      })
      .mockResolvedValueOnce({
        ok: true,
        json: () =>
          Promise.resolve({
            ok: true,
            member: { userId: 'u2', email: 'ops@example.com', username: 'Ops', role: 'maintainer' },
          }),
      })

    const members = await teamApi.getMembers('org1', 't1')
    const added = await teamApi.addMember('org1', 't1', { userId: 'u2', role: 'maintainer' })

    expect(members[0].userId).toBe('u1')
    expect(added.role).toBe('maintainer')
    expect(mockFetch).toHaveBeenNthCalledWith(
      1,
      '/api/v1/orgs/org1/teams/t1/members',
      expect.any(Object)
    )
    expect(mockFetch).toHaveBeenNthCalledWith(
      2,
      '/api/v1/orgs/org1/teams/t1/members',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ userId: 'u2', role: 'maintainer' }),
      })
    )
  })

  it('project member APIs use project member routes', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () =>
        Promise.resolve({
          ok: true,
          member: { userId: 'u2', email: 'ops@example.com', username: 'Ops', role: 'admin' },
        }),
    })

    const updated = await projectApi.updateMember('p1', 'u2', { role: 'admin' })

    expect(updated.userId).toBe('u2')
    expect(mockFetch).toHaveBeenCalledWith(
      '/api/v1/projects/p1/members/u2',
      expect.objectContaining({
        method: 'PATCH',
        body: JSON.stringify({ role: 'admin' }),
      })
    )
  })

  it('userApi normalizes org users for member pickers', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () =>
        Promise.resolve({
          ok: true,
          data: [
            {
              id: 'u1',
              email: 'dev@example.com',
              display_name: 'Dev User',
            },
          ],
        }),
    })

    const users = await userApi.getUsers()

    expect(users).toEqual([{ id: 'u1', email: 'dev@example.com', username: 'Dev User' }])
    expect(mockFetch).toHaveBeenCalledWith('/api/v1/users?limit=100', expect.any(Object))
  })

  it('getGroups returns groups for project', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () =>
        Promise.resolve({
          ok: true,
          groups: [{ id: 'g1', name: 'Default', projectId: 'p1' }],
        }),
    })

    const groups = await agentGroupApi.getGroups('p1')

    expect(groups).toHaveLength(1)
    expect(groups[0].id).toBe('g1')
    expect(mockFetch).toHaveBeenCalledWith('/api/v1/groups?projectId=p1', expect.any(Object))
  })

  it('createGroup posts projectId and normalizes the created group', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () =>
        Promise.resolve({
          ok: true,
          data: { id: 'g1', name: 'Default Task Group', project_id: 'p1' },
        }),
    })

    const group = await agentGroupApi.createGroup({
      projectId: 'p1',
      name: 'Default Task Group',
      description: 'Agents in this group can receive tasks from the board.',
    })

    expect(group).toEqual({ id: 'g1', name: 'Default Task Group', projectId: 'p1' })
    expect(mockFetch).toHaveBeenCalledWith(
      '/api/v1/groups',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({
          projectId: 'p1',
          name: 'Default Task Group',
          description: 'Agents in this group can receive tasks from the board.',
        }),
      })
    )
  })

  it('createGroup explains missing created task queue without API wording', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ ok: true, group: null }),
    })

    await expect(
      agentGroupApi.createGroup({
        projectId: 'p1',
        name: 'Review',
        description: 'Agents in this group can receive tasks from the board.',
      })
    ).rejects.toThrow(
      'Check the task queue name and project, then create the queue again. Task queue was not created.'
    )
  })

  it('updateOrg sends PATCH with JSON body and returns org', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () =>
        Promise.resolve({
          ok: true,
          org: { id: 'org1', name: 'Renamed', slug: 'org-one', plan: 'pro', role: 'owner' },
        }),
    })

    const org = await organizationApi.updateOrg('org1', { name: 'Renamed' })

    expect(org.name).toBe('Renamed')
    expect(mockFetch).toHaveBeenCalledWith(
      '/api/v1/orgs/org1',
      expect.objectContaining({
        method: 'PATCH',
        body: JSON.stringify({ name: 'Renamed' }),
      })
    )
  })

  it('throws on non-ok response', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: false,
      status: 403,
      text: () => Promise.resolve('Forbidden'),
    })

    await expect(organizationApi.getOrgs()).rejects.toThrow('API 403')
  })
})
