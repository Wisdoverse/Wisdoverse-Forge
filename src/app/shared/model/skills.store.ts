import { create } from 'zustand'

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
  const globalSkill = !skill.organization_id
  return {
    id: skill.id,
    name,
    description: skill.description ?? skill.trigger_pattern ?? '',
    plugin: skill.plugin ?? (globalSkill ? 'Global skills' : 'Workspace skills'),
    pluginAuthor: skill.pluginAuthor ?? '',
    content: skill.content ?? '',
    path: skill.path ?? skill.id ?? name,
    installed: skill.installed ?? skill.enabled ?? true,
    marketplace: skill.marketplace ?? (globalSkill ? 'global' : 'workspace'),
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
  const actionText = action === 'create' ? 'create the skill' : 'refresh Skills'

  if (status === 401) {
    return `Sign in again, then ${actionText}.`
  }
  if (status === 403) {
    return action === 'create'
      ? 'You do not have permission to create workspace skills. Ask an owner or admin to let you create reusable skills.'
      : 'You do not have permission to view workspace skills. Ask an owner or admin to update your workspace access.'
  }
  if (status === 404) {
    return 'Skills could not be opened from this page. Refresh Skills, then try again.'
  }
  if (status === 409) {
    return 'A skill with this name or trigger may already exist. Review the existing skills, then try again.'
  }
  if (status === 422) {
    return skillValidationMessage(detail)
  }
  if (status === 429) {
    return `Skill setup is busy. Wait a moment, then ${actionText}.`
  }
  if (status >= 500) {
    return action === 'create'
      ? 'Forge could not create the skill right now. Refresh Skills, then try again. If it still fails, ask an owner or admin to check skill setup.'
      : 'Forge could not load Skills right now. Refresh Skills, then try again. If it still fails, ask an owner or admin to check skill setup.'
  }

  return action === 'create'
    ? 'The skill could not be created. Review the fields and try again.'
    : 'Skills could not load. Refresh Skills and try again.'
}

function skillNetworkErrorMessage(action: SkillAction): string {
  return action === 'create'
    ? 'Forge could not connect while creating this skill. Check your connection, then try again.'
    : 'Forge could not connect while loading Skills. Check your connection, then refresh the page.'
}

function skillResponseErrorMessage(
  action: SkillAction,
  data: SkillsResponse | Record<string, unknown>
): string {
  const detail = errorDetail(data)
  if (detail)
    return action === 'create'
      ? skillValidationMessage(detail)
      : 'Skills could not load. Refresh Skills and try again.'
  return action === 'create'
    ? 'The skill could not be created. Review the fields and try again.'
    : 'Skills could not load. Refresh the page and try again.'
}

function skillValidationMessage(detail: string | null): string {
  const normalized = detail?.toLowerCase() ?? ''
  if (normalized.includes('trigger')) {
    return 'Check the trigger pattern, then try again.'
  }
  if (normalized.includes('name')) {
    return 'Enter a skill name, then try again.'
  }
  if (normalized.includes('content') || normalized.includes('instruction')) {
    return 'Enter the skill instructions, then try again.'
  }
  return 'Check the skill name, trigger pattern, and content, then try again.'
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
      const token = typeof window !== 'undefined' ? localStorage.getItem('af:auth:access') : null
      const res = await fetch('/api/v1/skills', {
        headers: {
          ...(token ? { Authorization: `Bearer ${token}` } : {}),
        },
      })
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
      const token = typeof window !== 'undefined' ? localStorage.getItem('af:auth:access') : null
      const res = await fetch('/api/v1/skills', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(token ? { Authorization: `Bearer ${token}` } : {}),
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
