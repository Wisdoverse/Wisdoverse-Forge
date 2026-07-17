import { create } from 'zustand'
import { authFetch } from '@app/shared/api/authFetch'

// ============================================================================
// Types
// ============================================================================

export interface Skill {
  id?: string
  name: string
  description: string
  plugin: string
  pluginAuthor: string
  content: string
  path: string
  installed: boolean
  marketplace: string
  cliTool: string
  triggerPattern: string
}

type ApiSkill = Partial<Skill> & {
  id?: string
  organization_id?: string | null
  workspace_id?: string | null
  scope_kind?: 'org' | 'user' | 'team' | 'project' | null
  scope_id?: string | null
  state?: 'candidate' | 'active' | 'deprecated' | 'revoked'
  version?: number
  owner_user_id?: string | null
  sensitivity?: 'public' | 'internal' | 'confidential' | 'secret_detected'
  trigger_pattern?: string | null
  enabled?: boolean
}

export interface InstalledSkill {
  name: string
  description: string
  content: string
  source: string
  scope: 'global' | 'project'
  projectName?: string
  installedAt: number
}

export interface CreateSkillInput {
  name: string
  description?: string
  trigger_pattern?: string
  content: string
}

interface SkillsState {
  skills: Skill[]
  installedSkills: InstalledSkill[]
  loading: boolean
  error: string | null
  searchQuery: string

  // Computed (derived from skills + searchQuery)
  filteredSkills: () => Skill[]

  // Actions
  setSearchQuery: (query: string) => void
  loadSkills: () => Promise<void>
  createSkill: (input: CreateSkillInput) => Promise<Skill>
  reset: () => void
}

type SkillsResponse = {
  ok: boolean
  skills?: ApiSkill[]
  installedSkills?: InstalledSkill[]
  data?: ApiSkill[] | { skills?: ApiSkill[]; installedSkills?: InstalledSkill[] }
  error?: { message?: string } | string
  message?: string
}

type SkillAction = 'load' | 'create'

class SkillUserFacingError extends Error {}

function userFacingError(message: string): SkillUserFacingError {
  return new SkillUserFacingError(message)
}

// ============================================================================
// Store
// ============================================================================

const initialState = {
  skills: [] as Skill[],
  installedSkills: [] as InstalledSkill[],
  loading: false,
  error: null as string | null,
  searchQuery: '',
}

function normalizeSkill(skill: ApiSkill): Skill {
  const name = skill.name ?? 'Untitled skill'
  const globalSkill = !skill.organization_id && !skill.scope_kind
  const marketplace =
    skill.scope_kind === 'project' ? 'project' : globalSkill ? 'global' : 'workspace'
  const source =
    skill.scope_kind === 'project'
      ? 'Project skills'
      : globalSkill
        ? 'Global skills'
        : 'Team space skills'
  return {
    id: skill.id,
    name,
    description: skill.description ?? skill.trigger_pattern ?? '',
    plugin: skill.plugin ?? source,
    pluginAuthor: skill.pluginAuthor ?? '',
    content: skill.content ?? '',
    path: skill.path ?? skill.id ?? name,
    installed: skill.installed ?? skill.enabled ?? true,
    marketplace: skill.marketplace ?? marketplace,
    cliTool: skill.cliTool ?? '',
    triggerPattern: skill.trigger_pattern ?? '',
  }
}

function extractSkillsPayload(data: SkillsResponse): {
  skills: Skill[]
  installedSkills: InstalledSkill[]
} {
  const rawSkills = Array.isArray(data.skills)
    ? data.skills
    : Array.isArray(data.data)
      ? data.data
      : (data.data?.skills ?? [])
  const installedSkills =
    data.installedSkills ?? (!Array.isArray(data.data) ? data.data?.installedSkills : undefined)

  return {
    skills: rawSkills.map(normalizeSkill),
    installedSkills: installedSkills ?? [],
  }
}

function errorDetail(data: SkillsResponse | Record<string, unknown>): string | null {
  if (typeof data.error === 'string' && data.error.trim()) return data.error.trim()
  if (
    data.error &&
    typeof data.error === 'object' &&
    'message' in data.error &&
    typeof data.error.message === 'string' &&
    data.error.message.trim()
  ) {
    return data.error.message.trim()
  }
  if (typeof data.message === 'string' && data.message.trim()) return data.message.trim()
  return null
}

async function readErrorPayload(res: Response): Promise<Record<string, unknown>> {
  return ((await res.json().catch(() => ({}))) ?? {}) as Record<string, unknown>
}

export function skillHttpErrorMessage(
  action: SkillAction,
  status: number,
  data: Record<string, unknown> = {}
): string {
  const detail = errorDetail(data)
  const actionText = action === 'create' ? 'save the skill again' : 'open Skills again'
  const createPermissionMessage =
    'Ask an owner or admin to let you save reusable guidance for this team space, then save the skill again.'
  const createConflictMessage =
    'Open Skills to check for a similar item, then change the name or matching words and save the skill again.'
  const createRateLimitMessage =
    'Wait a moment, then save the skill again. Forge is busy with skills right now.'
  const createServiceMessage =
    'Open Skills again, then save the skill again. If it still fails, ask an owner or admin to check Skills access.'
  const createDefaultMessage = 'Check the required fields, then save the skill again.'

  if (status === 401) {
    return `Sign in again, then ${actionText}.`
  }
  if (status === 403) {
    return action === 'create'
      ? createPermissionMessage
      : 'Ask an owner or admin to update your team space access, then open Skills again. You do not have access to skills for this team space.'
  }
  if (status === 404) {
    return action === 'create'
      ? 'Open Skills again, then save the skill again.'
      : 'Open Skills again to load the list.'
  }
  if (status === 409) {
    return createConflictMessage
  }
  if (status === 422) {
    return skillValidationMessage(detail)
  }
  if (status === 429) {
    return action === 'create'
      ? createRateLimitMessage
      : `Wait a moment, then ${actionText}. Forge is busy with skills right now.`
  }
  if (status >= 500) {
    return action === 'create' ? createServiceMessage : 'Open Skills again to load the list.'
  }

  return action === 'create' ? createDefaultMessage : 'Open Skills again to load the list.'
}

function skillNetworkErrorMessage(action: SkillAction): string {
  return action === 'create'
    ? 'Check your connection, then save the skill again. Forge could not connect while saving it.'
    : 'Check your connection, then open Skills again to load the list.'
}

function skillResponseErrorMessage(
  action: SkillAction,
  data: SkillsResponse | Record<string, unknown>
): string {
  const detail = errorDetail(data)
  const normalized = detail?.toLowerCase() ?? ''
  if (
    normalized.includes('role required') ||
    normalized.includes('forbidden') ||
    normalized.includes('permission')
  ) {
    return skillHttpErrorMessage(action, 403)
  }
  if (detail)
    return action === 'create'
      ? skillValidationMessage(detail)
      : 'Open Skills again to load the list.'
  return action === 'create'
    ? 'Check the required fields, then save the skill again.'
    : 'Open Skills again to load the list.'
}

function skillValidationMessage(detail: string | null): string {
  const normalized = detail?.toLowerCase() ?? ''
  if (normalized.includes('trigger')) {
    return 'Check the matching words, then save the skill again.'
  }
  if (normalized.includes('name')) {
    return 'Enter a guidance name, then save the skill again.'
  }
  if (normalized.includes('content') || normalized.includes('instruction')) {
    return 'Enter the reusable guidance, then save the skill again.'
  }
  return 'Check the guidance name, matching words, and reusable guidance, then save the skill again.'
}

export const useSkillsStore = create<SkillsState>((set, get) => ({
  ...initialState,

  filteredSkills: () => {
    const { skills, searchQuery } = get()
    if (!searchQuery.trim()) return skills
    const q = searchQuery.toLowerCase()
    return skills.filter(
      (s) =>
        s.name.toLowerCase().includes(q) ||
        s.description.toLowerCase().includes(q) ||
        s.plugin.toLowerCase().includes(q)
    )
  },

  setSearchQuery: (searchQuery) => set({ searchQuery }),

  loadSkills: async () => {
    set({ loading: true, error: null })
    try {
      const res = await authFetch('/api/v1/skills')
      if (!res.ok) {
        throw userFacingError(
          skillHttpErrorMessage('load', res.status, await readErrorPayload(res))
        )
      }
      const data = (await res.json()) as SkillsResponse
      if (!data.ok) {
        throw userFacingError(skillResponseErrorMessage('load', data))
      }
      const payload = extractSkillsPayload(data)
      set({
        skills: payload.skills,
        installedSkills: payload.installedSkills,
        loading: false,
      })
    } catch (err) {
      set({
        loading: false,
        error: err instanceof SkillUserFacingError ? err.message : skillNetworkErrorMessage('load'),
      })
    }
  },

  createSkill: async (input) => {
    try {
      const res = await authFetch('/api/v1/skills', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          name: input.name,
          description: input.description || undefined,
          trigger_pattern: input.trigger_pattern || undefined,
          content: input.content,
        }),
      })

      if (!res.ok) {
        throw userFacingError(
          skillHttpErrorMessage('create', res.status, await readErrorPayload(res))
        )
      }

      const data = (await res.json()) as {
        ok: boolean
        data?: ApiSkill
        error?: { message?: string }
        message?: string
      }
      if (!data.ok || !data.data) {
        throw userFacingError(skillResponseErrorMessage('create', data))
      }

      const skill = normalizeSkill(data.data)
      set((state) => ({
        skills: [...state.skills, skill].sort((a, b) => a.name.localeCompare(b.name)),
      }))
      return skill
    } catch (err) {
      if (err instanceof SkillUserFacingError) throw err
      throw new Error(skillNetworkErrorMessage('create'), { cause: err })
    }
  },

  reset: () => set(initialState),
}))
