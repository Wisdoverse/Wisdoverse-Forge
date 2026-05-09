import { create } from 'zustand'
import {
  agentGroupApi,
  type CreateAgentGroupInput,
  type NavAgentGroup,
} from '@app/entities/agent-group'
import { organizationApi, type NavOrg } from '@app/entities/organization'
import { projectApi, type NavProject, type UpdateProjectInput } from '@app/entities/project'
import { teamApi, type NavTeam, type UpdateTeamInput } from '@app/entities/team'
import { useBoardStore } from '@app/shared/model/board.store'

interface NavigationState {
  orgs: NavOrg[]
  selectedOrgId: string | null
  teams: NavTeam[]
  projects: Record<string, NavProject[]>
  agentGroups: NavAgentGroup[]
  selectedProjectId: string | null
  sidebarExpanded: boolean
  expandedTeams: string[]
  loading: boolean
  error: string | null

  loadOrgs: () => Promise<void>
  selectOrg: (orgId: string) => Promise<void>
  updateOrg: (orgId: string, input: { name?: string }) => Promise<void>
  updateTeam: (teamId: string, input: UpdateTeamInput) => Promise<void>
  deleteTeam: (teamId: string) => Promise<void>
  updateProject: (projectId: string, input: UpdateProjectInput) => Promise<void>
  deleteProject: (projectId: string) => Promise<void>
  selectProject: (projectId: string) => Promise<void>
  createAgentGroup: (
    projectId: string,
    input: Omit<CreateAgentGroupInput, 'projectId'>
  ) => Promise<NavAgentGroup>
  toggleSidebar: () => void
  toggleTeam: (teamId: string) => void
  reset: () => void
}

const LS_ORG = 'af:nav:orgId'
const LS_PROJECT = 'af:nav:projectId'
const LS_SIDEBAR = 'af:nav:sidebarExpanded'
const LS_TEAMS = 'af:nav:expandedTeams'

function lsGet(key: string): string | null {
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}

function lsSet(key: string, value: string) {
  try {
    localStorage.setItem(key, value)
  } catch {
    /* noop */
  }
}

const initialState = {
  orgs: [] as NavOrg[],
  selectedOrgId: null as string | null,
  teams: [] as NavTeam[],
  projects: {} as Record<string, NavProject[]>,
  agentGroups: [] as NavAgentGroup[],
  selectedProjectId: null as string | null,
  sidebarExpanded: lsGet(LS_SIDEBAR) !== 'false',
  expandedTeams: JSON.parse(lsGet(LS_TEAMS) || '[]') as string[],
  loading: false,
  error: null as string | null,
}

export const useNavigationStore = create<NavigationState>((set, get) => ({
  ...initialState,

  loadOrgs: async () => {
    set({ loading: true, error: null })
    try {
      const orgs = await organizationApi.getOrgs()
      set({ orgs, loading: false })

      if (orgs.length > 0) {
        const savedOrg = lsGet(LS_ORG)
        const targetOrg = savedOrg && orgs.some((o) => o.id === savedOrg) ? savedOrg : orgs[0].id
        await get().selectOrg(targetOrg)

        const savedProject = lsGet(LS_PROJECT)
        if (savedProject) {
          const allProjects = Object.values(get().projects).flat()
          if (allProjects.find((p) => p.id === savedProject)) {
            await get().selectProject(savedProject)
          }
        }
      }
    } catch (err) {
      set({ error: err instanceof Error ? err.message : 'Failed to load orgs', loading: false })
    }
  },

  selectOrg: async (orgId: string) => {
    set({ selectedOrgId: orgId, selectedProjectId: null, teams: [], projects: {}, agentGroups: [] })
    useBoardStore.getState().setSelectedGroupId(null)
    lsSet(LS_ORG, orgId)

    try {
      const teams = await teamApi.getTeams(orgId)
      const projectMap: Record<string, NavProject[]> = {}
      const results = await Promise.all(teams.map((t) => projectApi.getProjects(t.id)))
      teams.forEach((t, i) => {
        projectMap[t.id] = results[i]
      })
      set({ teams, projects: projectMap })
    } catch (err) {
      set({ error: err instanceof Error ? err.message : 'Failed to load teams' })
    }
  },

  updateOrg: async (orgId: string, input: { name?: string }) => {
    const updated = await organizationApi.updateOrg(orgId, input)
    set({
      orgs: get().orgs.map((o) => (o.id === orgId ? { ...o, ...updated } : o)),
    })
  },

  updateTeam: async (teamId: string, input: UpdateTeamInput) => {
    const team = get().teams.find((item) => item.id === teamId)
    if (!team) throw new Error('Team not found')

    const updated = await teamApi.updateTeam(team.orgId, teamId, input)
    set({
      teams: get().teams.map((item) => (item.id === teamId ? { ...item, ...updated } : item)),
    })
  },

  deleteTeam: async (teamId: string) => {
    const team = get().teams.find((item) => item.id === teamId)
    if (!team) throw new Error('Team not found')

    await teamApi.deleteTeam(team.orgId, teamId)

    const removedProjects = get().projects[teamId] ?? []
    const selectedProjectWasRemoved = removedProjects.some(
      (project) => project.id === get().selectedProjectId
    )
    const nextProjects = { ...get().projects }
    delete nextProjects[teamId]

    set({
      teams: get().teams.filter((item) => item.id !== teamId),
      expandedTeams: get().expandedTeams.filter((id) => id !== teamId),
      projects: nextProjects,
      selectedProjectId: selectedProjectWasRemoved ? null : get().selectedProjectId,
      agentGroups: selectedProjectWasRemoved ? [] : get().agentGroups,
    })
    if (selectedProjectWasRemoved) {
      useBoardStore.getState().setSelectedGroupId(null)
    }
  },

  updateProject: async (projectId: string, input: UpdateProjectInput) => {
    const projectEntry = Object.entries(get().projects).find(([, projects]) =>
      projects.some((project) => project.id === projectId)
    )
    if (!projectEntry) throw new Error('Project not found')

    const [projectTeamId] = projectEntry
    const updated = await projectApi.updateProject(projectTeamId, projectId, input)
    set({
      projects: Object.fromEntries(
        Object.entries(get().projects).map(([teamId, projects]) => [
          teamId,
          projects.map((project) =>
            project.id === projectId ? { ...project, ...updated } : project
          ),
        ])
      ),
    })
  },

  deleteProject: async (projectId: string) => {
    const projectEntry = Object.entries(get().projects).find(([, projects]) =>
      projects.some((project) => project.id === projectId)
    )
    if (!projectEntry) throw new Error('Project not found')

    const [projectTeamId] = projectEntry
    await projectApi.deleteProject(projectTeamId, projectId)
    set({
      selectedProjectId: get().selectedProjectId === projectId ? null : get().selectedProjectId,
      agentGroups: get().selectedProjectId === projectId ? [] : get().agentGroups,
      projects: Object.fromEntries(
        Object.entries(get().projects).map(([teamId, projects]) => [
          teamId,
          projects.filter((project) => project.id !== projectId),
        ])
      ),
    })
    if (get().selectedProjectId === null) {
      useBoardStore.getState().setSelectedGroupId(null)
    }
  },

  selectProject: async (projectId: string) => {
    set({ selectedProjectId: projectId, agentGroups: [] })
    useBoardStore.getState().setSelectedGroupId(null)
    lsSet(LS_PROJECT, projectId)

    try {
      const groups = await agentGroupApi.getGroups(projectId)
      set({ agentGroups: groups })
      if (groups.length > 0) {
        useBoardStore.getState().setSelectedGroupId(groups[0].id)
      } else {
        useBoardStore.getState().setSelectedGroupId(null)
      }
    } catch (err) {
      set({ error: err instanceof Error ? err.message : 'Failed to load groups' })
    }
  },

  createAgentGroup: async (projectId, input) => {
    try {
      const group = await agentGroupApi.createGroup({ projectId, ...input })
      if (get().selectedProjectId === projectId) {
        set({
          agentGroups: get().agentGroups.some((item) => item.id === group.id)
            ? get().agentGroups.map((item) => (item.id === group.id ? group : item))
            : [...get().agentGroups, group],
        })
        useBoardStore.getState().setSelectedGroupId(group.id)
      }
      return group
    } catch (err) {
      set({ error: err instanceof Error ? err.message : 'Failed to create group' })
      throw err
    }
  },

  toggleSidebar: () => {
    const next = !get().sidebarExpanded
    set({ sidebarExpanded: next })
    lsSet(LS_SIDEBAR, String(next))
  },

  toggleTeam: (teamId: string) => {
    const current = get().expandedTeams
    const next = current.includes(teamId)
      ? current.filter((id) => id !== teamId)
      : [...current, teamId]
    set({ expandedTeams: next })
    lsSet(LS_TEAMS, JSON.stringify(next))
  },

  reset: () => set(initialState),
}))
