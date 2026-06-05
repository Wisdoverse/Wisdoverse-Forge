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
} from '@app/shared/api/agent-api-types'
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
  apiKeys: 'platform access keys',
  gitCredentials: 'Git credentials',
  sshKeys: 'SSH keys',
  resourceProfiles: 'resource profiles',
  runtime: 'agent work settings',
}

const SETTINGS_ITEM_LABELS: Record<SettingsErrorArea, string> = {
  providers: 'provider',
  apiKeys: 'platform access key',
  gitCredentials: 'Git credential',
  sshKeys: 'SSH key',
  resourceProfiles: 'resource profile',
  runtime: 'agent work setting',
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
  const match = message?.match(/\b(?:API|HTTP|Server error \()? ?(\d{3})\b/)
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
    /^API \d{3}/i.test(detail) ||
    /^HTTP \d{3}/i.test(detail) ||
    /^Server error \(\d{3}\)$/i.test(detail) ||
    /^Network error$/i.test(detail) ||
    /^Failed to fetch$/i.test(detail)
  )
}

function settingsConnectionMessage(actionPhrase: string, action: SettingsErrorAction): string {
  const operation = action === 'load' ? 'loading Settings' : 'updating Settings'
  return `Settings could not ${actionPhrase}. Forge could not connect while ${operation}. Check your connection, then try again.`
}

function settingsUnavailableMessage(actionPhrase: string, action: SettingsErrorAction): string {
  const operation = action === 'load' ? 'load Settings' : 'update Settings'
  return `Forge could not ${operation} right now. Refresh Settings, then try to ${actionPhrase} again. If it still fails, ask an owner or admin to check Settings.`
}

export function settingsActionErrorMessage(
  area: SettingsErrorArea,
  action: SettingsErrorAction,
  error?: unknown
): string {
  const actionPhrase = settingsActionPhrase(area, action)
  const status = statusFromSettingsError(error)
  const detail = settingsErrorDetail(error)

  if (!status) {
    if (!isRawSettingsFailure(detail)) {
      return settingsValidationMessage(area, action, detail)
    }
    return settingsConnectionMessage(actionPhrase, action)
  }

  if (status === 401) {
    return `Sign in again, then open Settings and try to ${actionPhrase} again.`
  }
  if (status === 403) {
    return `You do not have permission to ${actionPhrase}. Ask an owner or admin to give you access to ${SETTINGS_AREA_LABELS[area]}.`
  }
  if (status === 404) {
    return `Settings for ${SETTINGS_AREA_LABELS[area]} are not ready yet. Refresh Settings, then try again.`
  }
  if (status === 409) {
    return `This ${SETTINGS_ITEM_LABELS[area]} changed or already exists. Refresh the list, review the current value, then try again.`
  }
  if (status === 422) {
    return settingsValidationMessage(area, action, detail)
  }
  if (status === 429) {
    return `The Settings page is busy. Wait a moment, then try to ${actionPhrase} again.`
  }
  if (status >= 500) {
    return settingsUnavailableMessage(actionPhrase, action)
  }

  return `Settings could not ${actionPhrase}. Refresh Settings, then try again.`
}

function settingsValidationMessage(
  area: SettingsErrorArea,
  action: SettingsErrorAction,
  detail: string | null
): string {
  const normalized = detail?.toLowerCase() ?? ''

  if (area === 'providers') {
    if (
      normalized.includes('api key') ||
      normalized.includes('token') ||
      normalized.includes('key')
    ) {
      return 'Enter the service access key from the AI service, choose a model, then save again.'
    }
    if (normalized.includes('model')) {
      return 'Choose a supported model for this provider, then save the provider again.'
    }
    return 'Check the provider name, model, and secret key, then save the provider again.'
  }

  if (area === 'apiKeys') {
    return action === 'load'
      ? 'Refresh platform access keys. If they still do not load, ask an owner or admin for access.'
      : 'Name this platform access key, choose the allowed access, then create it again.'
  }

  if (area === 'gitCredentials') {
    if (normalized.includes('not configured') || normalized.includes('provider')) {
      return 'Repository access is not configured yet. Ask an owner or admin to configure the Git provider, then refresh repository tokens.'
    }
    return 'Choose the Git provider, add the repository token, then save repository access again.'
  }

  if (area === 'sshKeys') {
    return 'Add a label, paste a valid public SSH key, then save the SSH key again.'
  }

  if (area === 'resourceProfiles') {
    return 'Ask an owner or admin to add a resource profile, then refresh Settings.'
  }

  return 'Choose an available work location and local tool, then save agent work settings again.'
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
