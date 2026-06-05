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

type NavigationErrorArea = 'organizations' | 'teamProjects' | 'workLanes' | 'workLane'
type NavigationErrorAction = 'load' | 'create'

const NAVIGATION_AREA_LABELS: Record<NavigationErrorArea, string> = {
  organizations: 'organizations',
  teamProjects: 'teams and projects',
  workLanes: 'work lanes',
  workLane: 'work lane',
}

function navigationActionPhrase(area: NavigationErrorArea, action: NavigationErrorAction): string {
  switch (action) {
    case 'load':
      return `load ${NAVIGATION_AREA_LABELS[area]}`
    case 'create':
      return `create the ${NAVIGATION_AREA_LABELS[area]}`
  }
}

function rawNavigationErrorMessage(error: unknown): string | null {
  if (typeof error === 'string' && error.trim()) return error.trim()
  if (error instanceof Error && error.message.trim()) return error.message.trim()
  if (error && typeof error === 'object' && 'error' in error) {
    const value = (error as { error?: unknown }).error
    if (typeof value === 'string' && value.trim()) return value.trim()
  }
  if (error && typeof error === 'object' && 'message' in error) {
    const value = (error as { message?: unknown }).message
    if (typeof value === 'string' && value.trim()) return value.trim()
  }
  return null
}

function detailFromPayload(payload: unknown): string | null {
  if (!payload || typeof payload !== 'object') return null
  const record = payload as Record<string, unknown>
  const nestedError = record.error
  if (nestedError && typeof nestedError === 'object') {
    const message = (nestedError as { message?: unknown }).message
    if (typeof message === 'string' && message.trim()) return message.trim()
  }
  for (const key of ['error', 'message', 'detail']) {
    const value = record[key]
    if (typeof value === 'string' && value.trim()) return value.trim()
  }
  return null
}

function navigationErrorDetail(error: unknown): string | null {
  const raw = rawNavigationErrorMessage(error)
  if (!raw) return null

  const apiMatch = raw.match(/\b(?:API|HTTP)\s+\d{3}:?\s*(.*)$/i)
  const body = apiMatch?.[1]?.trim()
  if (body) {
    try {
      const parsed = JSON.parse(body) as unknown
      const detail = detailFromPayload(parsed)
      if (detail) return detail
    } catch {
      return body
    }
  }

  return raw
}

function navigationErrorStatus(error: unknown): number | null {
  if (error && typeof error === 'object' && 'statusCode' in error) {
    const statusCode = (error as { statusCode?: unknown }).statusCode
    if (typeof statusCode === 'number') return statusCode
  }

  const raw = rawNavigationErrorMessage(error)
  const match = raw?.match(/\b(?:API|HTTP|Server error \()? ?(\d{3})\b/)
  return match ? Number(match[1]) : null
}

function isRawNavigationFailure(detail: string | null): boolean {
  if (!detail) return true
  return (
    /^API \d{3}/i.test(detail) ||
    /^HTTP \d{3}/i.test(detail) ||
    /^Server error \(\d{3}\)$/i.test(detail) ||
    /^Network error$/i.test(detail) ||
    /^Failed to fetch$/i.test(detail)
  )
}

export function navigationActionErrorMessage(
  area: NavigationErrorArea,
  action: NavigationErrorAction,
  error?: unknown
): string {
  const actionPhrase = navigationActionPhrase(area, action)
  const status = navigationErrorStatus(error)
  const detail = navigationErrorDetail(error)

  if (!status) {
    if (!isRawNavigationFailure(detail)) {
      return navigationValidationMessage(area, action, detail)
    }
    return `Navigation could not ${actionPhrase}. Forge could not connect while loading the sidebar. Check your connection, then refresh the page.`
  }

  if (status === 401) {
    return `Sign in again, then open the workspace sidebar and try to ${actionPhrase} again.`
  }
  if (status === 403) {
    return `You do not have permission to ${actionPhrase}. Ask an owner or admin to update your workspace access.`
  }
  if (status === 404) {
    return `Workspace navigation for ${NAVIGATION_AREA_LABELS[area]} is not ready yet. Refresh the sidebar, then try again.`
  }
  if (status === 409) {
    return 'The workspace navigation changed while you were working. Refresh the sidebar, review the current teams and projects, then try again.'
  }
  if (status === 422) {
    return navigationValidationMessage(area, action, detail)
  }
  if (status === 429) {
    return `The sidebar is busy. Wait a moment, then try to ${actionPhrase} again.`
  }
  if (status >= 500) {
    return 'Forge could not load workspace navigation right now. Refresh the sidebar, then try again. If it still fails, ask an owner or admin to check workspace navigation.'
  }

  return `Navigation could not ${actionPhrase}. Refresh the sidebar, then try again.`
}

function navigationValidationMessage(
  area: NavigationErrorArea,
  action: NavigationErrorAction,
  detail: string | null
): string {
  const normalized = detail?.toLowerCase() ?? ''

  if (area === 'workLane' || area === 'workLanes') {
    if (normalized.includes('name') || normalized.includes('title')) {
      return 'Name this work lane, choose its project, then create it again.'
    }
    if (normalized.includes('project')) {
      return 'Choose the project that should hold this work lane, then try again.'
    }
    return action === 'create'
      ? 'Check the work lane name and project, then create it again.'
      : 'Refresh the selected project, then load work lanes again.'
  }

  if (area === 'teamProjects') {
    return 'Choose an organization you can access, refresh the sidebar, then load its teams and projects again.'
  }

  return `Check the ${NAVIGATION_AREA_LABELS[area]} selection, refresh the sidebar, then try again.`
}

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
      set({
        error: navigationActionErrorMessage('organizations', 'load', err),
        loading: false,
      })
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
      set({ error: navigationActionErrorMessage('teamProjects', 'load', err) })
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
      set({ error: navigationActionErrorMessage('workLanes', 'load', err) })
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
      set({ error: navigationActionErrorMessage('workLane', 'create', err) })
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
