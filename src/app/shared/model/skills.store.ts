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
        throw new Error(`HTTP ${res.status}`)
      }
      const data = (await res.json()) as SkillsResponse
      if (!data.ok) {
        throw new Error('Server returned ok: false')
      }
      const payload = extractSkillsPayload(data)
      set({
        skills: payload.skills,
        installedSkills: payload.installedSkills,
        loading: false,
      })
    } catch {
      set({ loading: false, error: 'Failed to load skills' })
    }
  },

  createSkill: async (input) => {
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
      throw new Error(`HTTP ${res.status}`)
    }

    const data = (await res.json()) as {
      ok: boolean
      data?: ApiSkill
      error?: { message?: string }
    }
    if (!data.ok || !data.data) {
      throw new Error(data.error?.message ?? 'Failed to create skill')
    }

    const skill = normalizeSkill(data.data)
    set((state) => ({
      skills: [...state.skills, skill].sort((a, b) => a.name.localeCompare(b.name)),
    }))
    return skill
  },

  reset: () => set(initialState),
}))
