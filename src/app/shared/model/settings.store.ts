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
      const message = err instanceof Error ? err.message : 'Failed to load providers'
      set({ providersLoading: false, providersError: message })
    }
  },

  saveProvider: async (input) => {
    set({ providersError: null })
    try {
      const provider = await getSettingsApi().createProvider(input)
      set((state) => ({ providers: [...state.providers, provider] }))
      return provider
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to save provider'
      set({ providersError: message })
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
      const message = err instanceof Error ? err.message : 'Failed to delete provider'
      set({ providersError: message })
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
      const message = err instanceof Error ? err.message : 'Failed to load API keys'
      set({ keysLoading: false, keysError: message })
    }
  },

  createApiKey: async (name) => {
    set({ keysError: null })
    try {
      const result = await getSettingsApi().createApiKey(name)
      set((state) => ({ apiKeys: [result.apiKey, ...state.apiKeys] }))
      return result
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to create API key'
      set({ keysError: message })
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
      const message = err instanceof Error ? err.message : 'Failed to revoke API key'
      set({ keysError: message })
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
      set({ gitCredentialsLoading: false, gitCredentialsError: 'Failed to load git credentials' })
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
      set({ gitCredentialsError: result.error ?? 'Failed to save git credential' })
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
      set({ gitCredentialsError: result.error ?? 'Failed to delete git credential' })
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
      set({ sshKeysLoading: false, sshKeysError: 'Failed to load SSH keys' })
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
      set({ sshKeysError: result.error ?? 'Failed to create SSH key' })
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
      set({ sshKeysError: result.error ?? 'Failed to delete SSH key' })
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
      const message = err instanceof Error ? err.message : 'Failed to load resource profiles'
      set({ resourceProfilesLoading: false, resourceProfilesError: message })
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
      const message = err instanceof Error ? err.message : 'Failed to load runtime settings'
      set({ runtimeLoading: false, runtimeError: message })
    }
  },

  updateRuntimeSettings: async (settings) => {
    set({ runtimeError: null })
    try {
      const updated = await getSettingsApi().updateRuntimeSettings(settings)
      set({ runtimeSettings: updated })
      return true
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to update runtime settings'
      set({ runtimeError: message })
      return false
    }
  },
}))
