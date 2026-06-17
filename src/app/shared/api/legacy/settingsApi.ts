/**
 * SettingsAPI - Pure API layer for settings management
 *
 * All functions are pure HTTP calls with no DOM/state dependencies.
 * Handles model provider configuration, agent work settings, and gateway settings.
 *
 * Error handling: Methods throw SettingsApiError on network/server errors.
 * UI components should catch errors and show appropriate error states.
 */

import type { AuthHeaderProvider } from './AgentAPI'

// ============================================================================
// Types
// ============================================================================

import type { CliTool, LlmProviderKey } from '@shared/types.js'

export type LlmProvider = LlmProviderKey
export type RuntimeType = 'cli' | 'api' | 'container'
export type RoutingStrategy = 'specified' | 'cost' | 'latency' | 'failover'

// Re-export CliTool for consumers of this API
export type { CliTool }

export interface LlmProviderConfig {
  id: string
  provider: LlmProvider
  displayName: string
  model: string
  baseUrl?: string
  apiKeyPrefix?: string
  priority: number
  isEnabled: boolean
  isDefault: boolean
  lastTestStatus?: 'passed' | 'failed' | 'untested'
  lastTestErrorCode?: string
  lastTestErrorMessage?: string
  lastTestedAt?: string
}

export interface ProviderInfo {
  provider: LlmProvider
  displayName: string
  defaultModel?: string
  defaultBaseUrl?: string
  /** Alternate-region endpoint hint (e.g. global host when the default base URL is the China-region endpoint). */
  globalBaseUrl?: string
  requiresApiKey: boolean
  allowCustomModels: boolean
  models: { model: string; displayName: string }[]
}

export interface CreateProviderInput {
  provider: LlmProvider
  displayName: string
  model: string
  apiKey?: string
  baseUrl?: string
}

export interface GatewaySettings {
  routingStrategy: RoutingStrategy
  circuitBreakerThreshold: number
  circuitBreakerResetMs: number
}

export interface RuntimeSettings {
  defaultRuntime: RuntimeType
  availableRuntimes: RuntimeType[]
  defaultCliTool: CliTool
  availableCliTools: CliTool[]
  cliToolDetails: RuntimeCliToolDetail[]
}

export interface RuntimeCliToolDetail {
  cliTool: CliTool
  image: string
  version?: string
  imagePresent: boolean
  versionSource: 'docker-label' | 'image-tag' | 'not-reported' | string
}

export interface TestConnectionResult {
  ok: boolean
  latencyMs?: number
  error?: string
  responsePreview?: string
}

// API Key types (/api-keys endpoints)
export interface ApiKeyRecord {
  id: string
  orgId: string
  userId: string
  name: string
  keyPrefix: string
  createdAt: string
  lastUsedAt: string | null
  expiresAt: string | null
}

export interface CreateApiKeyResult {
  key: string
  apiKey: ApiKeyRecord
}

// ============================================================================
// Error Class
// ============================================================================

export class SettingsApiError extends Error {
  constructor(
    message: string,
    public readonly statusCode?: number,
    public readonly serverError?: string
  ) {
    super(message)
    this.name = 'SettingsApiError'
  }
}

// ============================================================================
// Response Extractors
// ============================================================================

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {}
}

function payloadRecord(data: Record<string, unknown>): Record<string, unknown> {
  const nested = data.data
  return nested && typeof nested === 'object' && !Array.isArray(nested)
    ? (nested as Record<string, unknown>)
    : data
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

function stringField(data: Record<string, unknown>, ...keys: string[]): string | undefined {
  for (const key of keys) {
    const value = data[key]
    if (typeof value === 'string') return value
  }
  return undefined
}

function boolField(data: Record<string, unknown>, fallback: boolean, ...keys: string[]): boolean {
  for (const key of keys) {
    const value = data[key]
    if (typeof value === 'boolean') return value
  }
  return fallback
}

function numberField(data: Record<string, unknown>, fallback: number, ...keys: string[]): number {
  for (const key of keys) {
    const value = data[key]
    if (typeof value === 'number') return value
  }
  return fallback
}

function errorMessage(data: Record<string, unknown>): string | undefined {
  const error = data.error
  if (typeof error === 'string') return error
  const errorRecord = asRecord(error)
  const nestedMessage = stringField(errorRecord, 'message', 'code')
  if (nestedMessage) return nestedMessage
  return stringField(data, 'message')
}

function mapProviderConfig(value: unknown, index = 0): LlmProviderConfig {
  const data = asRecord(value)
  const provider = (stringField(data, 'provider') ?? 'anthropic') as LlmProvider
  return {
    id: stringField(data, 'id') ?? '',
    provider,
    displayName: stringField(data, 'displayName', 'display_name') ?? provider,
    model: stringField(data, 'model') ?? '',
    baseUrl: stringField(data, 'baseUrl', 'base_url'),
    apiKeyPrefix: stringField(data, 'apiKeyPrefix', 'api_key_prefix'),
    priority: numberField(data, index + 1, 'priority'),
    isEnabled: boolField(data, true, 'isEnabled', 'is_enabled'),
    isDefault: boolField(data, false, 'isDefault', 'is_default'),
    lastTestStatus: stringField(data, 'lastTestStatus', 'last_test_status') as
      | LlmProviderConfig['lastTestStatus']
      | undefined,
    lastTestErrorCode: stringField(data, 'lastTestErrorCode', 'last_test_error_code'),
    lastTestErrorMessage: stringField(data, 'lastTestErrorMessage', 'last_test_error_message'),
    lastTestedAt: stringField(data, 'lastTestedAt', 'last_tested_at'),
  }
}

function mapProviderInfo(value: unknown): ProviderInfo {
  const data = asRecord(value)
  const provider = (stringField(data, 'provider') ?? 'anthropic') as LlmProvider
  const models = payloadArray(data, 'models').map((model) => {
    const modelRecord = asRecord(model)
    return {
      model: stringField(modelRecord, 'model') ?? '',
      displayName: stringField(modelRecord, 'displayName', 'display_name') ?? '',
    }
  })

  return {
    provider,
    displayName: stringField(data, 'displayName', 'display_name') ?? provider,
    defaultModel: stringField(data, 'defaultModel', 'default_model'),
    defaultBaseUrl: stringField(data, 'defaultBaseUrl', 'default_base_url'),
    globalBaseUrl: stringField(data, 'globalBaseUrl', 'global_base_url'),
    requiresApiKey: boolField(data, true, 'requiresApiKey', 'requires_api_key'),
    allowCustomModels: boolField(data, true, 'allowCustomModels', 'allow_custom_models'),
    models,
  }
}

function mapTestConnectionResult(value: unknown): TestConnectionResult {
  const data = asRecord(value)
  return {
    ok: boolField(data, false, 'ok'),
    latencyMs: numberField(data, 0, 'latencyMs', 'latency_ms') || undefined,
    error: errorMessage(data),
    responsePreview: stringField(data, 'responsePreview', 'response_preview'),
  }
}

function mapApiKeyRecord(value: unknown): ApiKeyRecord {
  const data = asRecord(value)
  return {
    id: stringField(data, 'id') ?? '',
    orgId: stringField(data, 'orgId', 'org_id', 'organization_id') ?? '',
    userId: stringField(data, 'userId', 'user_id') ?? '',
    name: stringField(data, 'name') ?? '',
    keyPrefix: stringField(data, 'keyPrefix', 'key_prefix') ?? '',
    createdAt: stringField(data, 'createdAt', 'created_at') ?? '',
    lastUsedAt: stringField(data, 'lastUsedAt', 'last_used_at') ?? null,
    expiresAt: stringField(data, 'expiresAt', 'expires_at') ?? null,
  }
}

function extractRuntimeSettings(data: Record<string, unknown>): RuntimeSettings {
  const payload = payloadRecord(data)
  const fallbackCliTools: CliTool[] = ['claude', 'codex', 'gemini', 'opencode']
  const availableCliTools = Array.isArray(payload.availableCliTools)
    ? (payload.availableCliTools as CliTool[])
    : fallbackCliTools
  const rawDetails = payloadArray(payload, 'cliToolDetails')
  return {
    defaultRuntime: (payload.defaultRuntime as RuntimeType) || 'container',
    availableRuntimes: Array.isArray(payload.availableRuntimes)
      ? (payload.availableRuntimes as RuntimeType[])
      : ['container', 'api'],
    defaultCliTool: (payload.defaultCliTool as CliTool) || 'claude',
    availableCliTools,
    cliToolDetails:
      rawDetails.length > 0
        ? rawDetails.map(mapRuntimeCliToolDetail)
        : availableCliTools.map((cliTool) => ({
            cliTool,
            image: `agentforge-agent:${cliTool}`,
            imagePresent: false,
            version: cliTool,
            versionSource: 'image-tag',
          })),
  }
}

function mapRuntimeCliToolDetail(value: unknown): RuntimeCliToolDetail {
  const data = asRecord(value)
  const cliTool = (stringField(data, 'cliTool', 'cli_tool') ?? 'claude') as CliTool
  const version = stringField(data, 'version')
  return {
    cliTool,
    image: stringField(data, 'image') ?? `agentforge-agent:${cliTool}`,
    version,
    imagePresent: boolField(data, false, 'imagePresent', 'image_present'),
    versionSource: stringField(data, 'versionSource', 'version_source') ?? 'not-reported',
  }
}

function extractGatewaySettings(data: Record<string, unknown>): GatewaySettings {
  const payload = payloadRecord(data)
  return {
    routingStrategy: (payload.routingStrategy as RoutingStrategy) || 'specified',
    circuitBreakerThreshold: numberField(payload, 5, 'circuitBreakerThreshold'),
    circuitBreakerResetMs: numberField(payload, 30000, 'circuitBreakerResetMs'),
  }
}

// ============================================================================
// API Factory
// ============================================================================

/**
 * Create a SettingsAPI instance bound to a specific API URL.
 * Optionally accepts an auth header provider for authenticated requests.
 *
 * @param apiUrl - Base API URL, e.g. '/api/v1'
 * @param getAuthHeaders - Optional auth header provider
 * @param fetchFn - Fetch function (default: global fetch)
 */
export function createSettingsAPI(
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

  /** Headers for requests without a body (GET, DELETE). */
  function headersNoBody(): Record<string, string> {
    return getAuthHeaders?.() ?? {}
  }

  /** Parse response and handle errors consistently. */
  async function parseResponse<T>(
    response: Response,
    extractData: (data: Record<string, unknown>) => T
  ): Promise<T> {
    if (!response.ok) {
      let serverError: string | undefined
      try {
        const errorData = await response.json()
        if (errorData) {
          serverError = errorMessage(errorData)
        }
      } catch {
        // Response body not JSON
      }
      throw new SettingsApiError(
        `HTTP ${response.status}: ${response.statusText}`,
        response.status,
        serverError
      )
    }

    const data = await response.json()
    if (!data.ok) {
      const serverError = errorMessage(data)
      throw new SettingsApiError(
        serverError || 'Server returned error',
        response.status,
        serverError
      )
    }

    return extractData(data)
  }

  return {
    // =========================================================================
    // LLM Provider Operations
    // =========================================================================

    async getSupportedProviders(): Promise<ProviderInfo[]> {
      const response = await fetchFn(`${apiUrl}/llm-providers/supported`, {
        headers: headersNoBody(),
      })
      return parseResponse(response, (data) =>
        payloadArray(data, 'providers').map((provider) => mapProviderInfo(provider))
      )
    },

    async getProviders(): Promise<LlmProviderConfig[]> {
      const response = await fetchFn(`${apiUrl}/llm-providers`, {
        headers: headersNoBody(),
      })
      return parseResponse(response, (data) =>
        payloadArray(data, 'providers').map((provider, index) => mapProviderConfig(provider, index))
      )
    },

    async createProvider(input: CreateProviderInput): Promise<LlmProviderConfig> {
      const response = await fetchFn(`${apiUrl}/llm-providers`, {
        method: 'POST',
        headers: headers(),
        body: JSON.stringify(input),
      })
      return parseResponse(response, (data) =>
        mapProviderConfig(data.provider ?? data.data ?? data)
      )
    },

    async updateProvider(
      id: string,
      input: Partial<CreateProviderInput> & { isEnabled?: boolean }
    ): Promise<LlmProviderConfig> {
      const response = await fetchFn(`${apiUrl}/llm-providers/${id}`, {
        method: 'PATCH',
        headers: headers(),
        body: JSON.stringify(input),
      })
      return parseResponse(response, (data) =>
        mapProviderConfig(data.provider ?? data.data ?? data)
      )
    },

    async deleteProvider(id: string): Promise<void> {
      const response = await fetchFn(`${apiUrl}/llm-providers/${id}`, {
        method: 'DELETE',
        headers: headersNoBody(),
      })
      await parseResponse(response, () => undefined)
    },

    /**
     * Test connection to an LLM provider.
     * Does not throw on test failure -- only on network error.
     */
    async testProvider(id: string): Promise<TestConnectionResult> {
      const response = await fetchFn(`${apiUrl}/llm-providers/${id}/test`, {
        method: 'POST',
        headers: headersNoBody(),
      })
      if (!response.ok) {
        let serverError: string | undefined
        try {
          const errorData = await response.json()
          if (errorData) serverError = errorData.error || errorData.message
        } catch {
          // Response body not JSON
        }
        throw new SettingsApiError(
          `HTTP ${response.status}: ${response.statusText}`,
          response.status,
          serverError
        )
      }
      const data = await response.json()
      return mapTestConnectionResult(data)
    },

    async setDefaultProvider(id: string): Promise<void> {
      const response = await fetchFn(`${apiUrl}/llm-providers/${id}/default`, {
        method: 'POST',
        headers: headersNoBody(),
      })
      await parseResponse(response, () => undefined)
    },

    // =========================================================================
    // Runtime Settings
    // =========================================================================

    async getRuntimeSettings(): Promise<RuntimeSettings> {
      const response = await fetchFn(`${apiUrl}/settings/runtime`, {
        headers: headersNoBody(),
      })
      return parseResponse(response, extractRuntimeSettings)
    },

    async updateRuntimeSettings(settings: {
      defaultRuntime?: RuntimeType
      defaultCliTool?: CliTool
    }): Promise<RuntimeSettings> {
      const response = await fetchFn(`${apiUrl}/settings/runtime`, {
        method: 'PATCH',
        headers: headers(),
        body: JSON.stringify(settings),
      })
      return parseResponse(response, extractRuntimeSettings)
    },

    // =========================================================================
    // Gateway Settings
    // =========================================================================

    async getGatewaySettings(): Promise<GatewaySettings> {
      const response = await fetchFn(`${apiUrl}/settings/gateway`, {
        headers: headersNoBody(),
      })
      return parseResponse(response, extractGatewaySettings)
    },

    async updateGatewaySettings(settings: Partial<GatewaySettings>): Promise<GatewaySettings> {
      const response = await fetchFn(`${apiUrl}/settings/gateway`, {
        method: 'PATCH',
        headers: headers(),
        body: JSON.stringify(settings),
      })
      return parseResponse(response, extractGatewaySettings)
    },

    // =========================================================================
    // API Key Operations (/api-keys)
    // =========================================================================

    async getApiKeys(): Promise<ApiKeyRecord[]> {
      const response = await fetchFn(`${apiUrl}/api-keys`, {
        headers: headersNoBody(),
      })
      return parseResponse(response, (data) =>
        payloadArray(data, 'apiKeys').map((key) => mapApiKeyRecord(key))
      )
    },

    async createApiKey(name: string): Promise<CreateApiKeyResult> {
      const response = await fetchFn(`${apiUrl}/api-keys`, {
        method: 'POST',
        headers: headers(),
        body: JSON.stringify({ name }),
      })
      return parseResponse(response, (data) => {
        const payload = payloadRecord(data)
        return {
          key: stringField(payload, 'plaintextKey', 'plaintext_key') ?? '',
          apiKey: mapApiKeyRecord(payload.apiKey ?? payload.api_key ?? payload.key),
        }
      })
    },

    async revokeApiKey(id: string): Promise<void> {
      const response = await fetchFn(`${apiUrl}/api-keys/${id}`, {
        method: 'DELETE',
        headers: headersNoBody(),
      })
      await parseResponse(response, () => undefined)
    },
  }
}

export type SettingsAPI = ReturnType<typeof createSettingsAPI>
