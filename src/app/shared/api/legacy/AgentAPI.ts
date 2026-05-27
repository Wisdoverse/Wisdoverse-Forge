/**
 * AgentAPI - Pure API layer for agent management
 *
 * All functions are pure HTTP calls with no DOM/state dependencies.
 * UI logic and state updates are handled by the caller (main.ts).
 */

import type {
  ManagedAgent,
  AgentMessageRow,
  ImageAttachment,
  CliTool,
  LlmProviderKey,
  WorkspaceProject,
} from '@shared/types'

export interface AgentFlags {
  skipPermissions?: boolean
  chrome?: boolean
}

/**
 * Standardized error fields returned by the server's error-handler plugin.
 * All API responses may include these when ok=false.
 */
export interface ApiErrorFields {
  error?: string
  message?: string
  details?: { reason?: string; issues?: Array<{ path: string; message: string }> }
  requestId?: string
}

export interface CreateAgentResponse extends ApiErrorFields {
  ok: boolean
  agent?: ManagedAgent
}

export interface SimpleResponse extends ApiErrorFields {
  ok: boolean
}

export interface StartAgentResponse extends ApiErrorFields {
  ok: boolean
  container_id?: string
  containerId?: string
  status?: string
}

export interface HostAgentEnrollment {
  agentId: string
  runtimeId: string
  cliTool: CliTool
  env: Record<string, string>
  shellExports: string
  sidecarCommand: string
  serverUrl?: string | null
}

export interface LocalAgentEnrollmentResponse extends ApiErrorFields {
  ok: boolean
  agent?: ManagedAgent
  enrollment?: HostAgentEnrollment
}

/**
 * Extract a human-readable error message from any API error response.
 * Priority: details.reason > message > error code > fallback.
 */
export function extractApiError(data: ApiErrorFields, fallback = 'Unknown error'): string {
  const rawError = (data as Record<string, unknown>).error
  const nestedError =
    rawError && typeof rawError === 'object' && !Array.isArray(rawError)
      ? (rawError as Record<string, unknown>)
      : null
  const nestedMessage =
    typeof nestedError?.message === 'string'
      ? nestedError.message
      : typeof nestedError?.code === 'string'
        ? nestedError.code
        : undefined
  return (
    data.details?.reason ||
    data.message ||
    (typeof rawError === 'string' ? rawError : nestedMessage) ||
    fallback
  )
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {}
}

function payloadArray(data: Record<string, unknown>, key: string): unknown[] {
  const direct = data[key]
  if (Array.isArray(direct)) return direct
  const nested = data.data
  if (Array.isArray(nested)) return nested
  const nestedRecord = asRecord(nested)
  const nestedValue = nestedRecord[key]
  return Array.isArray(nestedValue) ? nestedValue : []
}

function payloadObject(data: Record<string, unknown>, key: string): Record<string, unknown> {
  const direct = data[key]
  if (direct && typeof direct === 'object' && !Array.isArray(direct))
    return direct as Record<string, unknown>
  const nested = data.data
  if (nested && typeof nested === 'object' && !Array.isArray(nested))
    return nested as Record<string, unknown>
  return data
}

function stringField(data: Record<string, unknown>, ...keys: string[]): string | undefined {
  for (const key of keys) {
    const value = data[key]
    if (typeof value === 'string') return value
  }
  return undefined
}

function numberField(data: Record<string, unknown>, fallback: number, ...keys: string[]): number {
  for (const key of keys) {
    const value = data[key]
    if (typeof value === 'number') return value
  }
  return fallback
}

function mapResourceProfile(value: unknown): ResourceProfileOption {
  const data = asRecord(value)
  const cpu = numberField(
    data,
    numberField(data, 0, 'cpu_millicores', 'cpuMillicores') / 1000,
    'cpu'
  )
  return {
    id: stringField(data, 'id') ?? '',
    name: stringField(data, 'name') ?? '',
    cpu,
    memoryMb: numberField(data, 0, 'memoryMb', 'memory_mb'),
  }
}

function mapUserSshKey(value: unknown): UserSshKey {
  const data = asRecord(value)
  const createdAt = stringField(data, 'createdAt', 'created_at') ?? ''
  return {
    id: stringField(data, 'id') ?? '',
    label: stringField(data, 'label', 'name') ?? '',
    fingerprint: stringField(data, 'fingerprint') ?? '',
    keyType: stringField(data, 'keyType', 'key_type') ?? '',
    publicKey: stringField(data, 'publicKey', 'public_key') ?? '',
    createdAt,
    updatedAt: stringField(data, 'updatedAt', 'updated_at') ?? createdAt,
  }
}

function mapGitCredential(value: unknown): GitCredential {
  const data = asRecord(value)
  const provider = stringField(data, 'provider') === 'gitlab' ? 'gitlab' : 'github'
  const createdAt = stringField(data, 'createdAt', 'created_at') ?? ''
  return {
    id: stringField(data, 'id') ?? '',
    provider,
    host: stringField(data, 'host', 'remote_url') ?? null,
    createdAt,
    updatedAt: stringField(data, 'updatedAt', 'updated_at') ?? createdAt,
  }
}

export interface ServerInfoResponse {
  ok: boolean
  cwd?: string
  error?: string
}

export type AuthHeaderProvider = () => Record<string, string>

/**
 * Options for `createAgent`. Replaces the legacy 9-positional-arg signature
 * now that we need a 10th field (`systemPrompt`). All fields are optional —
 * provider+prompt agents need { provider, model, systemPrompt? }; CLI agents
 * need { cliTool }; both use { name, cwd, projectId, ... }.
 */
export interface CreateAgentOptions {
  name?: string
  cwd?: string
  flags?: AgentFlags
  workspaceId?: string
  projectId?: string
  cliTool?: CliTool
  profileId?: string
  groupId?: string
  provider?: string
  model?: string
  systemPrompt?: string
}

export interface LocalAgentEnrollmentOptions {
  name?: string
  cliTool: CliTool
  model?: string
  cwd?: string
  workspaceId?: string
  projectId?: string
}

/**
 * Create an AgentAPI instance bound to a specific API URL
 * Optionally accepts an auth header provider for authenticated requests
 */
export function createAgentAPI(
  apiUrl: string,
  getAuthHeaders?: AuthHeaderProvider,
  fetchFn: typeof fetch = fetch
) {
  function headers(extra?: Record<string, string>): Record<string, string> {
    return {
      'Content-Type': 'application/json',
      ...(getAuthHeaders?.() ?? {}),
      ...extra,
    }
  }

  function headersNoBody(): Record<string, string> {
    return getAuthHeaders?.() ?? {}
  }

  function buildAnalyticsQs(params?: AnalyticsParams): string {
    if (!params) return ''
    const qs = new URLSearchParams()
    if (params.hours) qs.set('hours', String(params.hours))
    if (params.scope) qs.set('scope', params.scope)
    if (params.teamId) qs.set('teamId', params.teamId)
    if (params.projectId) qs.set('projectId', params.projectId)
    if (params.userId) qs.set('userId', params.userId)
    const str = qs.toString()
    return str ? `?${str}` : ''
  }

  return {
    /**
     * List all managed agents (org-scoped)
     */
    async getAgents(): Promise<{ ok: boolean; agents: ManagedAgent[] }> {
      try {
        const response = await fetchFn(`${apiUrl}/agents`, { headers: headersNoBody() })
        if (!response.ok) {
          console.error(`Error fetching agents: HTTP ${response.status}`)
          return { ok: false, agents: [] }
        }
        return await response.json()
      } catch (e) {
        console.error('Error fetching agents:', e)
        return { ok: false, agents: [] }
      }
    },

    /**
     * Create a new managed agent.
     *
     * Two agent kinds share this endpoint:
     *   - Container CLI: pass `cliTool` (claude/codex/gemini/opencode), omit provider/model.
     *   - Provider+Prompt: omit `cliTool`, pass `provider` and `model`.
     * The backend distinguishes them by `cli_tool` being NULL.
     */
    async createAgent(opts: CreateAgentOptions): Promise<CreateAgentResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/agents`, {
          method: 'POST',
          headers: headers(),
          body: JSON.stringify({
            name: opts.name,
            cwd: opts.cwd,
            chromeEnabled: opts.flags?.chrome,
            skipPermissions: opts.flags?.skipPermissions,
            workspaceId: opts.workspaceId,
            projectId: opts.projectId,
            cliTool: opts.cliTool,
            profileId: opts.profileId,
            groupId: opts.groupId,
            provider: opts.provider,
            model: opts.model,
            systemPrompt: opts.systemPrompt,
          }),
        })
        return await response.json()
      } catch (e) {
        console.error('Error creating agent:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    /**
     * Enroll a local CLI process as a managed agent and return the one-time
     * sidecar environment the operator can run on that machine.
     */
    async enrollLocalAgent(
      opts: LocalAgentEnrollmentOptions
    ): Promise<LocalAgentEnrollmentResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/agents/local-enroll`, {
          method: 'POST',
          headers: headers(),
          body: JSON.stringify({
            name: opts.name,
            cliTool: opts.cliTool,
            model: opts.model,
            cwd: opts.cwd,
            workspaceId: opts.workspaceId,
            projectId: opts.projectId,
          }),
        })
        return await response.json()
      } catch (e) {
        console.error('Error enrolling local agent:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    /**
     * Fetch server info (cwd, etc.)
     */
    async getServerInfo(): Promise<ServerInfoResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/info`, { headers: headersNoBody() })
        return await response.json()
      } catch (e) {
        console.error('Error fetching server info:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    /**
     * Rename a managed agent
     */
    async renameAgent(agentId: string, name: string): Promise<SimpleResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/agents/${agentId}`, {
          method: 'PATCH',
          headers: headers(),
          body: JSON.stringify({ name }),
        })
        return await response.json()
      } catch (e) {
        console.error('Error renaming agent:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    /**
     * Save zone position for a managed agent
     */
    async saveZonePosition(
      agentId: string,
      position: { q: number; r: number }
    ): Promise<SimpleResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/agents/${agentId}`, {
          method: 'PATCH',
          headers: headers(),
          body: JSON.stringify({ zonePosition: position }),
        })
        return await response.json()
      } catch (e) {
        console.error('Error saving zone position:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    /**
     * Delete a managed agent
     */
    async deleteAgent(agentId: string): Promise<SimpleResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/agents/${agentId}`, {
          method: 'DELETE',
          headers: headersNoBody(),
        })
        return await response.json()
      } catch (e) {
        console.error('Error deleting agent:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    /**
     * Start a container for a CLI-backed agent that does not have one yet.
     */
    async startAgent(agentId: string): Promise<StartAgentResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/agents/${agentId}/start`, {
          method: 'POST',
          headers: headersNoBody(),
        })
        return await response.json()
      } catch (e) {
        console.error('Error starting agent:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    /**
     * Restart an offline agent
     */
    async restartAgent(agentId: string): Promise<SimpleResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/agents/${agentId}/restart`, {
          method: 'POST',
          headers: headersNoBody(),
        })
        return await response.json()
      } catch (e) {
        console.error('Error restarting agent:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    /**
     * Send a prompt to a managed agent.
     *
     * Backend `PromptRequest` (rust/crates/api/src/routes/agents.rs) expects
     * the body field named `content`. The `prompt` parameter here is the
     * frontend-facing label; it is serialized as `content` on the wire.
     *
     * Optionally includes image attachments (currently rejected server-side
     * with 422 — wire reserved for future multimodal support).
     */
    async sendPrompt(
      agentId: string,
      prompt: string,
      images?: ImageAttachment[]
    ): Promise<SimpleResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/agents/${agentId}/prompt`, {
          method: 'POST',
          headers: headers(),
          body: JSON.stringify({ content: prompt, images }),
        })
        return await response.json()
      } catch (e) {
        console.error('Error sending prompt:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    /**
     * Update agent fields. `systemPrompt: null` / omitted → no change;
     * `systemPrompt: ""` → clear the stored prompt (backend wipes the column).
     */
    async updateAgent(
      id: string,
      patch: { name?: string; model?: string; provider?: string; systemPrompt?: string | null }
    ): Promise<{ ok: boolean; data?: unknown; error?: string }> {
      try {
        const r = await fetchFn(`${apiUrl}/agents/${id}`, {
          method: 'PATCH',
          headers: headers(),
          body: JSON.stringify({
            name: patch.name,
            model: patch.model,
            provider: patch.provider,
            systemPrompt: patch.systemPrompt,
          }),
        })
        return await r.json()
      } catch (e) {
        console.error('updateAgent failed:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    /**
     * Fetch chat history for an agent. Returns a page of messages in
     * chronological oldest-first order. `before` (ISO 8601) cursor pages
     * further back in time (strictly earlier than that timestamp). `limit`
     * defaults to 50 server-side and is clamped server-side to `[1, 200]` —
     * no client clamping.
     */
    async fetchMessages(
      agentId: string,
      params?: { limit?: number; before?: string }
    ): Promise<{ ok: boolean; messages?: AgentMessageRow[]; hasMore?: boolean; error?: string }> {
      try {
        const q = new URLSearchParams()
        if (params?.limit !== undefined) q.set('limit', String(params.limit))
        if (params?.before) q.set('before', params.before)
        const qs = q.toString()
        const url = qs
          ? `${apiUrl}/agents/${agentId}/messages?${qs}`
          : `${apiUrl}/agents/${agentId}/messages`
        const r = await fetchFn(url, { headers: headersNoBody() })
        return await r.json()
      } catch (e) {
        console.error('fetchMessages failed:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    async deleteMessages(
      agentId: string
    ): Promise<{ ok: boolean; deleted?: number; error?: string }> {
      try {
        const r = await fetchFn(`${apiUrl}/agents/${agentId}/messages`, {
          method: 'DELETE',
          headers: headersNoBody(),
        })
        return await r.json()
      } catch (e) {
        console.error('deleteMessages failed:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    /**
     * Returns the raw `Response` so callers can consume the SSE body stream via
     * `response.body?.getReader()`. Content-Type is `text/event-stream` for
     * provider+prompt agents (cli_tool = null) and `application/json` for
     * CLI-tool agents — the caller branches on the header.
     *
     * Callers MUST check `response.ok` first: non-2xx responses (e.g. 409
     * `agent_busy`, 400 validation errors, 401 auth) are `application/json`
     * error envelopes, NOT SSE. Reading `body.getReader()` on an error response
     * surfaces raw JSON bytes as pseudo-stream content.
     */
    async streamPrompt(agentId: string, content: string, signal: AbortSignal): Promise<Response> {
      return fetchFn(`${apiUrl}/agents/${agentId}/prompt`, {
        method: 'POST',
        headers: headers(),
        body: JSON.stringify({ content }),
        signal,
      })
    },

    async interruptPrompt(agentId: string): Promise<{ ok: boolean; error?: string }> {
      try {
        const r = await fetchFn(`${apiUrl}/agents/${agentId}/prompt/interrupt`, {
          method: 'POST',
          headers: headersNoBody(),
        })
        return await r.json()
      } catch (e) {
        console.error('interruptPrompt failed:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    /**
     * Link a Claude agent ID to a managed agent
     */
    async linkAgent(managedId: string, cliSessionId: string): Promise<void> {
      try {
        await fetchFn(`${apiUrl}/agents/${managedId}/link`, {
          method: 'POST',
          headers: headers(),
          body: JSON.stringify({ cliSessionId }),
        })
      } catch (e) {
        console.error('Failed to link agent on server:', e)
      }
    },

    /**
     * Trigger a health check / refresh of all agents
     */
    async refreshAgents(): Promise<void> {
      try {
        await fetchFn(`${apiUrl}/agents/refresh`, {
          method: 'POST',
          headers: headersNoBody(),
        })
      } catch (e) {
        console.error('Error refreshing agents:', e)
      }
    },

    // =========================================================================
    // Workspace Projects
    // =========================================================================

    // =========================================================================
    // Resource Profiles
    // =========================================================================

    async getResourceProfiles(): Promise<ResourceProfileOption[]> {
      try {
        const response = await fetchFn(`${apiUrl}/resource-profiles`, {
          headers: headersNoBody(),
        })
        if (!response.ok) {
          console.error(`Error fetching resource profiles: HTTP ${response.status}`)
          return []
        }
        const data = await response.json()
        return payloadArray(data, 'profiles').map(mapResourceProfile)
      } catch (e) {
        console.error('Error fetching resource profiles:', e)
        return []
      }
    },

    async getWorkspaceProjects(): Promise<WorkspaceProject[]> {
      try {
        const response = await fetchFn(`${apiUrl}/workspace/projects`, {
          headers: headersNoBody(),
        })
        if (!response.ok) {
          console.error(`Error fetching workspace projects: HTTP ${response.status}`)
          return []
        }
        const data = await response.json()
        return data?.projects ?? []
      } catch (e) {
        console.error('Error fetching workspace projects:', e)
        return []
      }
    },

    // =========================================================================
    // User Preferences
    // =========================================================================

    async getUserPreferences(): Promise<UserPreferencesResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/users/me/preferences`, {
          headers: headersNoBody(),
        })
        if (!response.ok) {
          console.error(`Error fetching user preferences: HTTP ${response.status}`)
          return { ok: false, preferences: {} }
        }
        return await response.json()
      } catch (e) {
        console.error('Error fetching user preferences:', e)
        return { ok: false, preferences: {} }
      }
    },

    async updateUserPreferences(prefs: UserPreferences): Promise<UserPreferencesResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/users/me/preferences`, {
          method: 'PATCH',
          headers: headers(),
          body: JSON.stringify(prefs),
        })
        if (!response.ok) {
          console.error(`Error updating user preferences: HTTP ${response.status}`)
          return { ok: false, preferences: {} }
        }
        return await response.json()
      } catch (e) {
        console.error('Error updating user preferences:', e)
        return { ok: false, preferences: {} }
      }
    },

    // =========================================================================
    // User LLM Configs (BYOK)
    // =========================================================================

    async getUserLlmConfigs(): Promise<UserLlmConfigsResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/user/llm-configs`, {
          headers: headersNoBody(),
        })
        if (!response.ok) {
          console.error(`Error fetching user LLM configs: HTTP ${response.status}`)
          return { ok: false, configs: [] }
        }
        return await response.json()
      } catch (e) {
        console.error('Error fetching user LLM configs:', e)
        return { ok: false, configs: [] }
      }
    },

    async createUserLlmConfig(input: CreateUserLlmConfigInput): Promise<UserLlmConfigResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/user/llm-configs`, {
          method: 'POST',
          headers: headers(),
          body: JSON.stringify(input),
        })
        if (!response.ok) {
          const body = await response.json().catch(() => ({}))
          console.error(`Error creating user LLM config: HTTP ${response.status}`, body)
          return { ok: false, error: body?.error ?? `Server error (${response.status})` }
        }
        return await response.json()
      } catch (e) {
        console.error('Error creating user LLM config:', e)
        return { ok: false }
      }
    },

    async deleteUserLlmConfig(id: string): Promise<SimpleResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/user/llm-configs/${id}`, {
          method: 'DELETE',
          headers: headersNoBody(),
        })
        if (!response.ok) {
          const body = await response.json().catch(() => ({}))
          console.error(`Error deleting user LLM config: HTTP ${response.status}`, body)
          return { ok: false, error: body?.error ?? `Server error (${response.status})` }
        }
        return await response.json()
      } catch (e) {
        console.error('Error deleting user LLM config:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    async testUserLlmConfig(id: string): Promise<SimpleResponse & { latencyMs?: number }> {
      try {
        const response = await fetchFn(`${apiUrl}/user/llm-configs/${id}/test`, {
          method: 'POST',
          headers: headersNoBody(),
        })
        if (!response.ok) {
          const body = await response.json().catch(() => ({}))
          console.error(`Error testing user LLM config: HTTP ${response.status}`, body)
          return { ok: false, error: body?.error ?? `Server error (${response.status})` }
        }
        return await response.json()
      } catch (e) {
        console.error('Error testing user LLM config:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    // =========================================================================
    // User SSH Keys
    // =========================================================================

    async getUserSshKeys(): Promise<UserSshKeysResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/ssh-keys`, {
          headers: headersNoBody(),
        })
        if (!response.ok) {
          console.error(`Error fetching SSH keys: HTTP ${response.status}`)
          return { ok: false, keys: [] }
        }
        const data = await response.json()
        return { ok: Boolean(data?.ok), keys: payloadArray(data, 'keys').map(mapUserSshKey) }
      } catch (e) {
        console.error('Error fetching SSH keys:', e)
        return { ok: false, keys: [] }
      }
    },

    async createUserSshKey(input: CreateUserSshKeyInput): Promise<UserSshKeyResponse> {
      try {
        const publicKey = input.publicKey ?? input.privateKey
        const response = await fetchFn(`${apiUrl}/ssh-keys`, {
          method: 'POST',
          headers: headers(),
          body: JSON.stringify({ name: input.label, public_key: publicKey }),
        })
        if (!response.ok) {
          const body = await response.json().catch(() => ({}))
          console.error(`Error creating SSH key: HTTP ${response.status}`, body)
          return {
            ok: false,
            error: extractApiError(body as ApiErrorFields, `Server error (${response.status})`),
          }
        }
        const data = await response.json()
        return { ok: Boolean(data?.ok), key: mapUserSshKey(payloadObject(data, 'key')) }
      } catch (e) {
        console.error('Error creating SSH key:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    async deleteUserSshKey(id: string): Promise<SimpleResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/ssh-keys/${id}`, {
          method: 'DELETE',
          headers: headersNoBody(),
        })
        if (!response.ok) {
          const body = await response.json().catch(() => ({}))
          console.error(`Error deleting SSH key: HTTP ${response.status}`, body)
          return {
            ok: false,
            error: extractApiError(body as ApiErrorFields, `Server error (${response.status})`),
          }
        }
        const data = await response.json()
        return { ok: Boolean(data?.ok) }
      } catch (e) {
        console.error('Error deleting SSH key:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    // =========================================================================
    // Git Credentials (glab/gh CLI tokens)
    // =========================================================================

    async getGitCredentials(): Promise<GitCredentialsResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/git-credentials`, {
          headers: headersNoBody(),
        })
        if (!response.ok) {
          console.error(`Error fetching git credentials: HTTP ${response.status}`)
          return { ok: false, credentials: [] }
        }
        const data = await response.json()
        return {
          ok: Boolean(data?.ok),
          credentials: payloadArray(data, 'credentials').map(mapGitCredential),
        }
      } catch (e) {
        console.error('Error fetching git credentials:', e)
        return { ok: false, credentials: [] }
      }
    },

    async upsertGitCredential(
      provider: GitProvider,
      token: string,
      host?: string
    ): Promise<SimpleResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/git-credentials`, {
          method: 'POST',
          headers: headers(),
          body: JSON.stringify({
            name: `${provider}${host ? ` (${host})` : ''}`,
            provider,
            credential_type: 'token',
            remote_url: host,
            token,
          }),
        })
        if (!response.ok) {
          const body = await response.json().catch(() => ({}))
          return {
            ok: false,
            error: extractApiError(body as ApiErrorFields, `Server error (${response.status})`),
          }
        }
        return await response.json()
      } catch (e) {
        console.error('Error saving git credential:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    async deleteGitCredential(id: string): Promise<SimpleResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/git-credentials/${id}`, {
          method: 'DELETE',
          headers: headersNoBody(),
        })
        if (!response.ok) {
          const body = await response.json().catch(() => ({}))
          return {
            ok: false,
            error: extractApiError(body as ApiErrorFields, `Server error (${response.status})`),
          }
        }
        return await response.json()
      } catch (e) {
        console.error('Error deleting git credential:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    // =========================================================================
    // CLI Auth Proxy (server-side auth proxy for CLI tools)
    // =========================================================================

    async getCliAuthProxyProviders(): Promise<CliAuthProxyProvidersResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/cli-auth-proxy/providers`, {
          headers: headersNoBody(),
        })
        if (!response.ok) {
          console.error(`[AgentAPI] getCliAuthProxyProviders failed: HTTP ${response.status}`)
          return { ok: false, providers: [] }
        }
        return await response.json()
      } catch (e) {
        console.error('Error fetching CLI auth proxy providers:', e)
        return { ok: false, providers: [] }
      }
    },

    async getCliAuthProxyStatus(): Promise<CliAuthProxyStatusResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/cli-auth-proxy/status`, {
          headers: headersNoBody(),
        })
        if (!response.ok) {
          console.error(`[AgentAPI] getCliAuthProxyStatus failed: HTTP ${response.status}`)
          return { ok: false, statuses: [] }
        }
        return await response.json()
      } catch (e) {
        console.error('Error fetching CLI auth proxy status:', e)
        return { ok: false, statuses: [] }
      }
    },

    async startCliAuthProxyLogin(
      provider: string
    ): Promise<{ ok: boolean; url?: string; error?: string }> {
      try {
        const response = await fetchFn(
          `${apiUrl}/cli-auth-proxy/${encodeURIComponent(provider)}/authorize`,
          { method: 'POST', headers: headersNoBody() }
        )
        if (!response.ok) {
          const body = await response.json().catch(() => ({}))
          return { ok: false, error: body?.error ?? `Server error (${response.status})` }
        }
        return await response.json()
      } catch (e) {
        console.error('Error starting CLI auth proxy login:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    async completeCliAuthProxyLogin(
      provider: string,
      callbackUrl: string
    ): Promise<{ ok: boolean; error?: string }> {
      try {
        // Extract code and state from the pasted callback URL
        let parsed: URL
        try {
          // Handle URLs with localhost that may use http:// or have no protocol
          const normalized = callbackUrl.startsWith('http') ? callbackUrl : `https://${callbackUrl}`
          parsed = new URL(normalized)
        } catch {
          return { ok: false, error: 'Invalid URL — paste the full address from your browser' }
        }

        const code = parsed.searchParams.get('code')
        const state = parsed.searchParams.get('state')
        if (!code || !state) {
          return {
            ok: false,
            error: 'URL is missing the login code. Make sure you copied the entire address.',
          }
        }

        // Call the existing callback endpoint which processes code+state server-side
        const cbUrl = `${apiUrl}/cli-auth-proxy/${encodeURIComponent(provider)}/callback?code=${encodeURIComponent(code)}&state=${encodeURIComponent(state)}`
        const response = await fetchFn(cbUrl)

        // The callback returns HTML — check status code for success
        if (!response.ok) {
          return { ok: false, error: `Server error (${response.status})` }
        }

        // Parse the HTML response to extract the postMessage payload
        const html = await response.text()
        if (html.includes('"ok":true')) {
          return { ok: true }
        }

        // Try to extract error from the HTML payload
        const errorMatch = html.match(/"error":"([^"]*)"/)
        return { ok: false, error: errorMatch?.[1] ?? 'Connection failed' }
      } catch (e) {
        console.error('Error completing CLI auth proxy login:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    async disconnectCliAuthProxy(provider: string): Promise<SimpleResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/cli-auth-proxy/${encodeURIComponent(provider)}`, {
          method: 'DELETE',
          headers: headersNoBody(),
        })
        if (!response.ok) {
          const body = await response.json().catch(() => ({}))
          return { ok: false, error: body?.error ?? `Server error (${response.status})` }
        }
        return await response.json()
      } catch (e) {
        console.error('Error disconnecting CLI auth proxy:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    // =========================================================================
    // Analytics
    // =========================================================================

    async getAnalyticsSummary(params?: AnalyticsParams): Promise<AnalyticsSummaryResponse> {
      try {
        const qs = buildAnalyticsQs(params)
        const response = await fetchFn(`${apiUrl}/analytics/summary${qs}`, {
          headers: headersNoBody(),
        })
        if (!response.ok) {
          console.error(`[AgentAPI] getAnalyticsSummary failed: HTTP ${response.status}`)
          return { ok: false }
        }
        return await response.json()
      } catch (e) {
        console.error('[AgentAPI] getAnalyticsSummary error:', e)
        return { ok: false }
      }
    },

    async getAnalyticsTools(params?: AnalyticsParams): Promise<AnalyticsToolsResponse> {
      try {
        const qs = buildAnalyticsQs(params)
        const response = await fetchFn(`${apiUrl}/analytics/tools${qs}`, {
          headers: headersNoBody(),
        })
        if (!response.ok) {
          console.error(`[AgentAPI] getAnalyticsTools failed: HTTP ${response.status}`)
          return { ok: false, tools: [] }
        }
        return await response.json()
      } catch (e) {
        console.error('[AgentAPI] getAnalyticsTools error:', e)
        return { ok: false, tools: [] }
      }
    },

    async getAnalyticsActivity(params?: AnalyticsParams): Promise<AnalyticsActivityResponse> {
      try {
        const qs = buildAnalyticsQs(params)
        const response = await fetchFn(`${apiUrl}/analytics/activity${qs}`, {
          headers: headersNoBody(),
        })
        if (!response.ok) {
          console.error(`[AgentAPI] getAnalyticsActivity failed: HTTP ${response.status}`)
          return { ok: false, activity: [] }
        }
        return await response.json()
      } catch (e) {
        console.error('[AgentAPI] getAnalyticsActivity error:', e)
        return { ok: false, activity: [] }
      }
    },

    async getAnalyticsSessions(params?: AnalyticsParams): Promise<AnalyticsSessionsResponse> {
      try {
        const qs = buildAnalyticsQs(params)
        const response = await fetchFn(`${apiUrl}/analytics/agents${qs}`, {
          headers: headersNoBody(),
        })
        if (!response.ok) {
          console.error(`[AgentAPI] getAnalyticsSessions failed: HTTP ${response.status}`)
          return { ok: false, agents: [] }
        }
        return await response.json()
      } catch (e) {
        console.error('[AgentAPI] getAnalyticsSessions error:', e)
        return { ok: false, agents: [] }
      }
    },

    async getAnalyticsHeatmap(params?: AnalyticsParams): Promise<AnalyticsHeatmapResponse> {
      try {
        const qs = buildAnalyticsQs(params)
        const response = await fetchFn(`${apiUrl}/analytics/heatmap${qs}`, {
          headers: headersNoBody(),
        })
        if (!response.ok) {
          console.error(`[AgentAPI] getAnalyticsHeatmap failed: HTTP ${response.status}`)
          return { ok: false, days: [] }
        }
        return await response.json()
      } catch (e) {
        console.error('[AgentAPI] getAnalyticsHeatmap error:', e)
        return { ok: false, days: [] }
      }
    },

    async getAnalyticsScopes(): Promise<AnalyticsScopeInfo> {
      try {
        const response = await fetchFn(`${apiUrl}/analytics/scopes`, {
          headers: headersNoBody(),
        })
        if (!response.ok) {
          console.error(`[AgentAPI] getAnalyticsScopes failed: HTTP ${response.status}`)
          return { ok: false }
        }
        return await response.json()
      } catch (e) {
        console.error('[AgentAPI] getAnalyticsScopes error:', e)
        return { ok: false }
      }
    },

    // =========================================================================
    // Agent Collaborators
    // =========================================================================

    async getAgentCollaborators(
      agentId: string
    ): Promise<{ ok: boolean; collaborators: AgentCollaborator[] }> {
      try {
        const response = await fetchFn(`${apiUrl}/agents/${agentId}/collaborators`, {
          headers: headersNoBody(),
        })
        if (!response.ok) return { ok: false, collaborators: [] }
        return await response.json()
      } catch (e) {
        console.error('Error fetching collaborators:', e)
        return { ok: false, collaborators: [] }
      }
    },

    async addAgentCollaborator(
      agentId: string,
      userId: string,
      permission: 'view' | 'prompt' | 'manage'
    ): Promise<{ ok: boolean; collaborator?: AgentCollaborator; error?: string }> {
      try {
        const response = await fetchFn(`${apiUrl}/agents/${agentId}/collaborators`, {
          method: 'POST',
          headers: headers(),
          body: JSON.stringify({ userId, permission }),
        })
        return await response.json()
      } catch (e) {
        console.error('Error adding collaborator:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    async updateAgentCollaboratorPermission(
      agentId: string,
      userId: string,
      permission: 'view' | 'prompt' | 'manage'
    ): Promise<SimpleResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/agents/${agentId}/collaborators/${userId}`, {
          method: 'PATCH',
          headers: headers(),
          body: JSON.stringify({ permission }),
        })
        return await response.json()
      } catch (e) {
        console.error('Error updating collaborator:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    async removeAgentCollaborator(agentId: string, userId: string): Promise<SimpleResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/agents/${agentId}/collaborators/${userId}`, {
          method: 'DELETE',
          headers: headersNoBody(),
        })
        return await response.json()
      } catch (e) {
        console.error('Error removing collaborator:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    async transferAgentOwnership(agentId: string, newOwnerId: string): Promise<SimpleResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/agents/${agentId}/transfer-ownership`, {
          method: 'POST',
          headers: headers(),
          body: JSON.stringify({ newOwnerId }),
        })
        const data = await response.json()
        if (!response.ok) {
          return { ok: false, error: data.message ?? data.error ?? '转让失败' }
        }
        return data
      } catch (e) {
        console.error('Error transferring ownership:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    // =========================================================================
    // Invites
    // =========================================================================

    async inviteToTeam(
      orgId: string,
      teamId: string,
      email: string,
      role = 'editor'
    ): Promise<SimpleResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/orgs/${orgId}/teams/${teamId}/invites`, {
          method: 'POST',
          headers: headers(),
          body: JSON.stringify({ email, role }),
        })
        return await response.json()
      } catch (e) {
        console.error('Error inviting to team:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    async inviteToProject(
      projectId: string,
      email: string,
      role = 'editor'
    ): Promise<SimpleResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/projects/${projectId}/invites`, {
          method: 'POST',
          headers: headers(),
          body: JSON.stringify({ email, role }),
        })
        return await response.json()
      } catch (e) {
        console.error('Error inviting to project:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    async acceptInvite(token: string): Promise<SimpleResponse> {
      try {
        const response = await fetchFn(`${apiUrl}/invites/accept`, {
          method: 'POST',
          headers: headers(),
          body: JSON.stringify({ token }),
        })
        return await response.json()
      } catch (e) {
        console.error('Error accepting invite:', e)
        return { ok: false, error: 'Network error' }
      }
    },

    // =========================================================================
    // User Search
    // =========================================================================

    async searchUsers(query: string): Promise<{
      ok: boolean
      members: Array<{ userId: string; email: string; username: string; role: string }>
    }> {
      try {
        const response = await fetchFn(`${apiUrl}/users/search?q=${encodeURIComponent(query)}`, {
          headers: headersNoBody(),
        })
        if (!response.ok) return { ok: false, members: [] }
        return await response.json()
      } catch (e) {
        console.error('Error searching users:', e)
        return { ok: false, members: [] }
      }
    },
  }
}

export type AgentAPI = ReturnType<typeof createAgentAPI>

// ============================================================================
// Resource Profile Types (frontend-friendly subset)
// ============================================================================

export interface ResourceProfileOption {
  id: string
  name: string
  cpu: number
  memoryMb: number
}

export type { WorkspaceProject }

// ============================================================================
// User Preferences Types
// ============================================================================

export interface UserPreferences {
  defaultCliTool?: CliTool
}

export interface UserPreferencesResponse {
  ok: boolean
  preferences: UserPreferences
}

// ============================================================================
// User LLM Config Types (BYOK)
// ============================================================================

export interface UserLlmConfig {
  id: string
  provider: LlmProviderKey
  model?: string
  displayName?: string
  baseUrl?: string
  apiKeyPrefix?: string
  isDefault: boolean
  isEnabled: boolean
  createdAt: string
  updatedAt: string
}

export interface UserLlmConfigsResponse {
  ok: boolean
  configs: UserLlmConfig[]
}

export interface UserLlmConfigResponse {
  ok: boolean
  config?: UserLlmConfig
  error?: string
}

export interface CreateUserLlmConfigInput {
  provider: LlmProviderKey
  apiKey: string
  model?: string
  displayName?: string
  baseUrl?: string
  isDefault?: boolean
}

// ============================================================================
// User SSH Key Types
// ============================================================================

export interface UserSshKey {
  id: string
  label: string
  fingerprint: string
  keyType: string
  publicKey: string
  createdAt: string
  updatedAt: string
}

export interface UserSshKeysResponse {
  ok: boolean
  keys: UserSshKey[]
}

export interface UserSshKeyResponse {
  ok: boolean
  key?: UserSshKey
  error?: string
}

export interface CreateUserSshKeyInput {
  label: string
  publicKey?: string
  privateKey?: string
}

// ============================================================================
// Git Credential Types
// ============================================================================

export type GitProvider = 'gitlab' | 'github'

export interface GitCredential {
  id: string
  provider: GitProvider
  host: string | null
  createdAt: string
  updatedAt: string
}

export interface GitCredentialsResponse {
  ok: boolean
  credentials: GitCredential[]
}

// ============================================================================
// CLI Auth Proxy Types
// ============================================================================

export interface CliAuthProxyProvider {
  name: string
  displayName: string
  cliTool: string
}

export interface CliAuthProxyProvidersResponse {
  ok: boolean
  providers: CliAuthProxyProvider[]
}

export interface CliAuthProxyStatusEntry {
  provider: string
  displayName: string
  cliTool: string
  connected: boolean
  lastRefresh?: string
  revokedAt?: string
  revokeReason?: string
  refreshFailCount?: number
}

export interface CliAuthProxyStatusResponse {
  ok: boolean
  statuses: CliAuthProxyStatusEntry[]
}

// ============================================================================
// Analytics Types
// ============================================================================

export interface AnalyticsParams {
  hours?: number
  scope?: 'user' | 'org' | 'team' | 'project' | 'user_detail'
  teamId?: string
  projectId?: string
  userId?: string
}

export interface AnalyticsSummaryResponse {
  ok: boolean
  totalEvents?: number
  toolCalls?: number
  prompts?: number
  responses?: number
  uniqueAgents?: number
  timeSpanHours?: number
}

export interface AnalyticsToolStat {
  tool: string
  count: number
  successRate: number
  avgDurationMs: number | null
}

export interface AnalyticsToolsResponse {
  ok: boolean
  tools: AnalyticsToolStat[]
}

export interface AnalyticsHourlyActivity {
  hour: number
  count: number
}

export interface AnalyticsActivityResponse {
  ok: boolean
  activity: AnalyticsHourlyActivity[]
}

export interface AnalyticsSessionStat {
  cliSessionId: string
  agentName: string | null
  eventCount: number
  toolUses: number
  prompts: number
}

export interface AnalyticsSessionsResponse {
  ok: boolean
  agents: AnalyticsSessionStat[]
}

export interface AnalyticsDailyActivity {
  date: string
  count: number
}

export interface AnalyticsHeatmapResponse {
  ok: boolean
  days: AnalyticsDailyActivity[]
}

export interface AnalyticsScopeInfo {
  ok: boolean
  canViewOrg?: boolean
  teams?: Array<{ id: string; name: string }>
  projects?: Array<{ id: string; name: string }>
}

// ============================================================================
// Agent Collaborator Types
// ============================================================================

export interface AgentCollaborator {
  id: string
  agentId: string
  userId: string
  email: string
  username: string
  permission: 'view' | 'prompt' | 'manage'
  grantedBy: string | null
  grantedAt: number
}
