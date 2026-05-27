import { create } from 'zustand'
import type {
  LlmProviderConfig,
  CreateProviderInput,
  ApiKeyRecord,
  CreateApiKeyResult,
  CliTool,
  RuntimeSettings,
  RuntimeType,
} from '@app/shared/api/legacy/settingsApi'
import type {
  GitCredential,
  GitProvider,
  UserSshKey,
  ResourceProfileOption,
} from '@app/shared/api/legacy/AgentAPI'
import { getSettingsApi, getAgentApi } from '@app/shared/api/legacy'

// ============================================================================
// Types
// ============================================================================

export const SETTINGS_DEFAULT_SECTION = 'providers'

export const SETTINGS_SECTIONS = [
  'providers',
  'keys',
  'git-credentials',
  'ssh-keys',
  'resources',
  'runtime',
  'account',
  'teams',
  'projects',
  'about',
] as const

export type SettingsSection = (typeof SETTINGS_SECTIONS)[number]

const SETTINGS_SECTION_ALIASES: Record<string, SettingsSection> = {
  'api-keys': 'keys',
  api: 'keys',
  git: 'git-credentials',
  ssh: 'ssh-keys',
  'ssh-credentials': 'ssh-keys',
  workspace: 'resources',
  workspaces: 'resources',
  profile: 'account',
  user: 'account',
  organization: 'teams',
  organizations: 'teams',
}

export function normalizeSettingsSection(value: unknown): SettingsSection | null {
  if (typeof value !== 'string') return null
  const normalized = value.trim().toLowerCase()
  if (!normalized) return null
  if ((SETTINGS_SECTIONS as readonly string[]).includes(normalized)) {
    return normalized as SettingsSection
  }
  return SETTINGS_SECTION_ALIASES[normalized] ?? null
}

type SettingsErrorArea =
  | 'providers'
  | 'apiKeys'
  | 'gitCredentials'
  | 'sshKeys'
  | 'resourceProfiles'
  | 'runtime'

type SettingsErrorAction = 'load' | 'save' | 'delete' | 'create' | 'revoke' | 'update'

const SETTINGS_AREA_LABELS: Record<SettingsErrorArea, string> = {
  providers: 'provider settings',
  apiKeys: 'platform API keys',
  gitCredentials: 'Git credentials',
  sshKeys: 'SSH keys',
  resourceProfiles: 'resource profiles',
  runtime: 'runtime settings',
}

const SETTINGS_ITEM_LABELS: Record<SettingsErrorArea, string> = {
  providers: 'provider',
  apiKeys: 'platform API key',
  gitCredentials: 'Git credential',
  sshKeys: 'SSH key',
  resourceProfiles: 'resource profile',
  runtime: 'runtime setting',
}

function settingsActionPhrase(area: SettingsErrorArea, action: SettingsErrorAction): string {
  const areaLabel = SETTINGS_AREA_LABELS[area]
  const itemLabel = SETTINGS_ITEM_LABELS[area]
  switch (action) {
    case 'load':
      return `load ${areaLabel}`
    case 'save':
      return `save the ${itemLabel}`
    case 'delete':
      return `delete the ${itemLabel}`
    case 'create':
      return `create the ${itemLabel}`
    case 'revoke':
      return `revoke the ${itemLabel}`
    case 'update':
      return `update ${areaLabel}`
  }
}

function statusFromSettingsError(error: unknown): number | null {
  if (error && typeof error === 'object' && 'statusCode' in error) {
    const statusCode = (error as { statusCode?: unknown }).statusCode
    if (typeof statusCode === 'number') return statusCode
  }

  const message = settingsErrorDetail(error)
  const match = message?.match(/\b(?:HTTP|Server error \()? ?(\d{3})\b/)
  return match ? Number(match[1]) : null
}

function settingsErrorDetail(error: unknown): string | null {
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

function isRawSettingsFailure(detail: string | null): boolean {
  if (!detail) return true
  return (
    /^HTTP \d{3}/i.test(detail) ||
    /^Server error \(\d{3}\)$/i.test(detail) ||
    /^Network error$/i.test(detail) ||
    /^Failed to fetch$/i.test(detail)
  )
}

export function settingsActionErrorMessage(
  area: SettingsErrorArea,
  action: SettingsErrorAction,
  error?: unknown
): string {
  const actionPhrase = settingsActionPhrase(area, action)
  const status = statusFromSettingsError(error)
  const detail = settingsErrorDetail(error)
  const suffix = !isRawSettingsFailure(detail) ? ` Details: ${detail}` : ''

  if (!status) {
    if (!isRawSettingsFailure(detail)) {
      return `Settings could not ${actionPhrase}. Review the message and try again.${suffix}`
    }
    return `Settings could not ${actionPhrase} because the browser could not reach the server. Check your connection and try again.${suffix}`
  }

  const statusText = `Code: ${status}.`
  if (status === 401) {
    return `Sign in again, then ${actionPhrase}. ${statusText}${suffix}`
  }
  if (status === 403) {
    return `You do not have permission to ${actionPhrase}. Ask an admin to update your role. ${statusText}${suffix}`
  }
  if (status === 404) {
    return `The settings service for ${SETTINGS_AREA_LABELS[area]} is not available. Refresh after the backend is deployed. ${statusText}${suffix}`
  }
  if (status === 409) {
    return `This setting changed or already exists. Refresh the list, review the current value, then try again. ${statusText}${suffix}`
  }
  if (status === 422) {
    return `Check the required fields for ${SETTINGS_ITEM_LABELS[area]}, then try again. ${statusText}${suffix}`
  }
  if (status === 429) {
    return `The settings service is busy. Wait a moment, then ${actionPhrase}. ${statusText}${suffix}`
  }
  if (status >= 500) {
    return `The settings service had a server problem. Try again after the backend is healthy. ${statusText}${suffix}`
  }

  return `Settings could not ${actionPhrase}. Refresh the page and try again. ${statusText}${suffix}`
}

interface SettingsState {
  // Navigation
  activeSection: SettingsSection

  // Providers
  providers: LlmProviderConfig[]
  providersLoading: boolean
  providersError: string | null

  // API Keys
  apiKeys: ApiKeyRecord[]
  keysLoading: boolean
  keysError: string | null

  // Git Credentials
  gitCredentials: GitCredential[]
  gitCredentialsLoading: boolean
  gitCredentialsError: string | null

  // SSH Keys
  sshKeys: UserSshKey[]
  sshKeysLoading: boolean
  sshKeysError: string | null

  // Resource Profiles
  resourceProfiles: ResourceProfileOption[]
  resourceProfilesLoading: boolean
  resourceProfilesError: string | null

  // Runtime Settings
  runtimeSettings: RuntimeSettings | null
  runtimeLoading: boolean
  runtimeError: string | null

  // Setters
  setActiveSection: (section: SettingsSection) => void

  // Provider actions
  loadProviders: () => Promise<void>
  saveProvider: (input: CreateProviderInput) => Promise<LlmProviderConfig | null>
  deleteProvider: (id: string) => Promise<boolean>

  // API Key actions
  loadApiKeys: () => Promise<void>
  createApiKey: (name: string) => Promise<CreateApiKeyResult | null>
  revokeApiKey: (id: string) => Promise<boolean>

  // Git Credential actions
  loadGitCredentials: () => Promise<void>
  saveGitCredential: (provider: GitProvider, token: string, host?: string) => Promise<boolean>
  deleteGitCredential: (id: string) => Promise<boolean>

  // SSH Key actions
  loadSshKeys: () => Promise<void>
  createSshKey: (label: string, publicKey: string) => Promise<boolean>
  deleteSshKey: (id: string) => Promise<boolean>

  // Resource Profile actions
  loadResourceProfiles: () => Promise<void>

  // Runtime actions
  loadRuntimeSettings: () => Promise<void>
  updateRuntimeSettings: (settings: {
    defaultRuntime?: RuntimeType
    defaultCliTool?: CliTool
  }) => Promise<boolean>
}

// ============================================================================
// Store
// ============================================================================

const initialState = {
  activeSection: SETTINGS_DEFAULT_SECTION as SettingsSection,
  providers: [] as LlmProviderConfig[],
  providersLoading: false,
  providersError: null as string | null,
  apiKeys: [] as ApiKeyRecord[],
  keysLoading: false,
  keysError: null as string | null,
  gitCredentials: [] as GitCredential[],
  gitCredentialsLoading: false,
  gitCredentialsError: null as string | null,
  sshKeys: [] as UserSshKey[],
  sshKeysLoading: false,
  sshKeysError: null as string | null,
  resourceProfiles: [] as ResourceProfileOption[],
  resourceProfilesLoading: false,
  resourceProfilesError: null as string | null,
  runtimeSettings: null as RuntimeSettings | null,
  runtimeLoading: false,
  runtimeError: null as string | null,
}

export const useSettingsStore = create<SettingsState>((set) => ({
  ...initialState,

  setActiveSection: (activeSection) => set({ activeSection }),

  // ---------------------------------------------------------------------------
  // Provider actions
  // ---------------------------------------------------------------------------

  loadProviders: async () => {
    set({ providersLoading: true, providersError: null })
    try {
      const providers = await getSettingsApi().getProviders()
      set({ providers, providersLoading: false })
    } catch (err) {
      set({
        providersLoading: false,
        providersError: settingsActionErrorMessage('providers', 'load', err),
      })
    }
  },

  saveProvider: async (input) => {
    set({ providersError: null })
    try {
      const provider = await getSettingsApi().createProvider(input)
      set((state) => ({ providers: [...state.providers, provider] }))
      return provider
    } catch (err) {
      set({ providersError: settingsActionErrorMessage('providers', 'save', err) })
      return null
    }
  },

  deleteProvider: async (id) => {
    set({ providersError: null })
    try {
      await getSettingsApi().deleteProvider(id)
      set((state) => ({ providers: state.providers.filter((p) => p.id !== id) }))
      return true
    } catch (err) {
      set({ providersError: settingsActionErrorMessage('providers', 'delete', err) })
      return false
    }
  },

  // ---------------------------------------------------------------------------
  // API Key actions
  // ---------------------------------------------------------------------------

  loadApiKeys: async () => {
    set({ keysLoading: true, keysError: null })
    try {
      const apiKeys = await getSettingsApi().getApiKeys()
      set({ apiKeys, keysLoading: false })
    } catch (err) {
      set({ keysLoading: false, keysError: settingsActionErrorMessage('apiKeys', 'load', err) })
    }
  },

  createApiKey: async (name) => {
    set({ keysError: null })
    try {
      const result = await getSettingsApi().createApiKey(name)
      set((state) => ({ apiKeys: [result.apiKey, ...state.apiKeys] }))
      return result
    } catch (err) {
      set({ keysError: settingsActionErrorMessage('apiKeys', 'create', err) })
      return null
    }
  },

  revokeApiKey: async (id) => {
    set({ keysError: null })
    try {
      await getSettingsApi().revokeApiKey(id)
      set((state) => ({ apiKeys: state.apiKeys.filter((k) => k.id !== id) }))
      return true
    } catch (err) {
      set({ keysError: settingsActionErrorMessage('apiKeys', 'revoke', err) })
      return false
    }
  },

  // ---------------------------------------------------------------------------
  // Git Credential actions
  // ---------------------------------------------------------------------------

  loadGitCredentials: async () => {
    set({ gitCredentialsLoading: true, gitCredentialsError: null })
    const result = await getAgentApi().getGitCredentials()
    if (result.ok) {
      set({ gitCredentials: result.credentials, gitCredentialsLoading: false })
    } else {
      set({
        gitCredentialsLoading: false,
        gitCredentialsError: settingsActionErrorMessage('gitCredentials', 'load', result),
      })
    }
  },

  saveGitCredential: async (provider, token, host) => {
    set({ gitCredentialsError: null })
    const result = await getAgentApi().upsertGitCredential(provider, token, host)
    if (result.ok) {
      // Reload to get the updated list
      const listResult = await getAgentApi().getGitCredentials()
      if (listResult.ok) {
        set({ gitCredentials: listResult.credentials })
      }
      return true
    } else {
      set({
        gitCredentialsError: settingsActionErrorMessage('gitCredentials', 'save', result.error),
      })
      return false
    }
  },

  deleteGitCredential: async (id) => {
    set({ gitCredentialsError: null })
    const result = await getAgentApi().deleteGitCredential(id)
    if (result.ok) {
      set((state) => ({
        gitCredentials: state.gitCredentials.filter((c) => c.id !== id),
      }))
      return true
    } else {
      set({
        gitCredentialsError: settingsActionErrorMessage('gitCredentials', 'delete', result.error),
      })
      return false
    }
  },

  // ---------------------------------------------------------------------------
  // SSH Key actions
  // ---------------------------------------------------------------------------

  loadSshKeys: async () => {
    set({ sshKeysLoading: true, sshKeysError: null })
    const result = await getAgentApi().getUserSshKeys()
    if (result.ok) {
      set({ sshKeys: result.keys, sshKeysLoading: false })
    } else {
      set({
        sshKeysLoading: false,
        sshKeysError: settingsActionErrorMessage('sshKeys', 'load', result),
      })
    }
  },

  createSshKey: async (label, publicKey) => {
    set({ sshKeysError: null })
    const result = await getAgentApi().createUserSshKey({ label, publicKey })
    const key = result.key
    if (result.ok && key) {
      set((state) => ({ sshKeys: [...state.sshKeys, key] }))
      return true
    } else {
      set({ sshKeysError: settingsActionErrorMessage('sshKeys', 'create', result.error) })
      return false
    }
  },

  deleteSshKey: async (id) => {
    set({ sshKeysError: null })
    const result = await getAgentApi().deleteUserSshKey(id)
    if (result.ok) {
      set((state) => ({ sshKeys: state.sshKeys.filter((k) => k.id !== id) }))
      return true
    } else {
      set({ sshKeysError: settingsActionErrorMessage('sshKeys', 'delete', result.error) })
      return false
    }
  },

  // ---------------------------------------------------------------------------
  // Resource Profile actions
  // ---------------------------------------------------------------------------

  loadResourceProfiles: async () => {
    set({ resourceProfilesLoading: true, resourceProfilesError: null })
    try {
      const profiles = await getAgentApi().getResourceProfiles()
      set({ resourceProfiles: profiles, resourceProfilesLoading: false })
    } catch (err) {
      set({
        resourceProfilesLoading: false,
        resourceProfilesError: settingsActionErrorMessage('resourceProfiles', 'load', err),
      })
    }
  },

  // ---------------------------------------------------------------------------
  // Runtime actions
  // ---------------------------------------------------------------------------

  loadRuntimeSettings: async () => {
    set({ runtimeLoading: true, runtimeError: null })
    try {
      const settings = await getSettingsApi().getRuntimeSettings()
      set({ runtimeSettings: settings, runtimeLoading: false })
    } catch (err) {
      set({
        runtimeLoading: false,
        runtimeError: settingsActionErrorMessage('runtime', 'load', err),
      })
    }
  },

  updateRuntimeSettings: async (settings) => {
    set({ runtimeError: null })
    try {
      const updated = await getSettingsApi().updateRuntimeSettings(settings)
      set({ runtimeSettings: updated })
      return true
    } catch (err) {
      set({ runtimeError: settingsActionErrorMessage('runtime', 'update', err) })
      return false
    }
  },
}))
