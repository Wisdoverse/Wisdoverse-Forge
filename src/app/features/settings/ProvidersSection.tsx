import { useEffect, useState } from 'react'
import { Activity } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type {
  LlmProvider,
  LlmProviderConfig,
  CreateProviderInput,
  ProviderInfo,
  TestConnectionResult,
} from '@app/shared/api/legacy/settingsApi'
import { getSettingsApi } from '@app/shared/api/legacy'

// ============================================================================
// Types
// ============================================================================

interface AddProviderForm {
  provider: LlmProvider
  displayName: string
  model: string
  apiKey: string
  baseUrl: string
}

const DEFAULT_FORM: AddProviderForm = {
  provider: 'anthropic',
  displayName: '',
  model: 'claude-sonnet-4-20250514',
  apiKey: '',
  baseUrl: '',
}

const FALLBACK_SUPPORTED_PROVIDERS: ProviderInfo[] = [
  {
    provider: 'anthropic',
    displayName: 'Anthropic',
    models: [{ model: 'claude-sonnet-4-20250514', displayName: 'Claude Sonnet 4' }],
  },
  {
    provider: 'openai',
    displayName: 'OpenAI',
    models: [{ model: 'gpt-5.4', displayName: 'GPT-5.4' }],
  },
  {
    provider: 'google',
    displayName: 'Google',
    models: [{ model: 'gemini-2.5-pro', displayName: 'Gemini 2.5 Pro' }],
  },
  {
    provider: 'ollama',
    displayName: 'Ollama',
    models: [{ model: 'llama3', displayName: 'Llama 3' }],
  },
  {
    provider: 'groq',
    displayName: 'Groq',
    models: [{ model: 'llama-3.3-70b-versatile', displayName: 'Llama 3.3 70B' }],
  },
  {
    provider: 'deepseek',
    displayName: 'DeepSeek',
    models: [{ model: 'deepseek-chat', displayName: 'DeepSeek Chat' }],
  },
  {
    provider: 'xai',
    displayName: 'xAI',
    models: [{ model: 'grok-3-mini', displayName: 'Grok 3 Mini' }],
  },
  {
    provider: 'openrouter',
    displayName: 'OpenRouter',
    models: [{ model: 'openai/gpt-4o-mini', displayName: 'OpenAI GPT-4o Mini' }],
  },
  {
    provider: 'together',
    displayName: 'Together AI',
    models: [{ model: 'openai/gpt-oss-20b', displayName: 'GPT OSS 20B' }],
  },
  {
    provider: 'fireworks',
    displayName: 'Fireworks AI',
    models: [
      {
        model: 'accounts/fireworks/models/qwen3-30b-a3b',
        displayName: 'Qwen3 30B A3B',
      },
    ],
  },
]

function baseUrlPlaceholder(provider: LlmProvider): string {
  switch (provider) {
    case 'ollama':
      return 'http://localhost:11434'
    case 'groq':
      return 'https://api.groq.com/openai'
    case 'openrouter':
      return 'https://openrouter.ai/api'
    case 'together':
      return 'https://api.together.xyz'
    case 'fireworks':
      return 'https://api.fireworks.ai/inference'
    default:
      return 'https://api.example.com'
  }
}

function providerNeedsApiKey(provider: LlmProvider): boolean {
  return provider !== 'ollama'
}

// ============================================================================
// Provider Card
// ============================================================================

interface ProviderCardProps {
  id: string
  provider: string
  displayName: string
  model: string
  isEnabled: boolean
  isDefault: boolean
  apiKeyPrefix?: string
  lastTestStatus?: LlmProviderConfig['lastTestStatus']
  lastTestErrorMessage?: string
  onTest: (id: string) => Promise<TestConnectionResult>
  onDelete: (id: string) => void
}

function ProviderCard({
  id,
  displayName,
  model,
  isEnabled,
  isDefault,
  apiKeyPrefix,
  lastTestStatus,
  lastTestErrorMessage,
  onTest,
  onDelete,
}: ProviderCardProps) {
  const [confirming, setConfirming] = useState(false)
  const [testing, setTesting] = useState(false)
  const [testResult, setTestResult] = useState<{ ok: boolean; message: string } | null>(null)
  const persistedTestResult =
    lastTestStatus === 'passed'
      ? { ok: true, message: 'Connection ready' }
      : lastTestStatus === 'failed'
        ? { ok: false, message: lastTestErrorMessage || 'Connection failed' }
        : null
  const visibleTestResult = testResult ?? persistedTestResult

  function handleDelete() {
    if (!confirming) {
      setConfirming(true)
      return
    }
    onDelete(id)
    setConfirming(false)
  }

  async function handleTest() {
    setTesting(true)
    setTestResult(null)
    try {
      const result = await onTest(id)
      setTestResult({
        ok: result.ok,
        message: result.ok ? 'Connection ready' : result.error || 'Connection failed',
      })
    } catch (err) {
      setTestResult({
        ok: false,
        message: err instanceof Error ? err.message : 'Connection failed',
      })
    } finally {
      setTesting(false)
    }
  }

  return (
    <div
      className={cn(
        'flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between',
        uiStyles.row
      )}
    >
      <div className="flex min-w-0 items-center gap-3">
        {/* Status dot */}
        <div
          className={cn(
            'h-2 w-2 shrink-0 rounded-full',
            isEnabled ? 'bg-apple-blue' : 'bg-[#d2d2d7]'
          )}
        />
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="truncate text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
              {displayName}
            </span>
            {isDefault && <span className={uiStyles.activeBadge}>default</span>}
          </div>
          <div className="flex items-center gap-2 mt-0.5">
            <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
              {model}
            </span>
            {apiKeyPrefix && (
              <span className="font-mono text-ui-caption text-secondary-light dark:text-secondary-dark">
                {apiKeyPrefix}••••
              </span>
            )}
          </div>
          {visibleTestResult && (
            <div
              className={cn(
                'mt-1 text-ui-caption',
                visibleTestResult.ok ? 'text-apple-green' : 'text-apple-red'
              )}
            >
              {visibleTestResult.message}
            </div>
          )}
        </div>
      </div>

      <div className="flex w-full shrink-0 items-center justify-end gap-2 sm:w-auto">
        <button
          type="button"
          onClick={handleTest}
          disabled={testing || !isEnabled}
          className={uiStyles.secondaryButton}
          aria-label={`Test ${displayName} connection`}
          title="Test connection"
        >
          <Activity className="h-4 w-4" aria-hidden="true" />
          <span>{testing ? 'Testing' : 'Test'}</span>
        </button>
        <button
          type="button"
          onClick={handleDelete}
          className={cn(
            'shrink-0',
            confirming ? uiStyles.dangerConfirmButton : uiStyles.dangerButton
          )}
        >
          {confirming ? 'Confirm?' : 'Delete'}
        </button>
      </div>
    </div>
  )
}

// ============================================================================
// Add Provider Form
// ============================================================================

interface AddProviderFormProps {
  supportedProviders: ProviderInfo[]
  onSave: (input: CreateProviderInput) => Promise<void>
  onCancel: () => void
  saving: boolean
}

function AddProviderFormPanel({
  supportedProviders,
  onSave,
  onCancel,
  saving,
}: AddProviderFormProps) {
  const [form, setForm] = useState<AddProviderForm>(DEFAULT_FORM)

  const providerOptions =
    supportedProviders.length > 0 ? supportedProviders : FALLBACK_SUPPORTED_PROVIDERS
  const selectedProvider = providerOptions.find((p) => p.provider === form.provider)
  const models = selectedProvider?.models ?? []
  const needsApiKey = providerNeedsApiKey(form.provider)
  const canSubmit = Boolean(form.model.trim() && (!needsApiKey || form.apiKey.trim()))

  function handleProviderChange(provider: LlmProvider) {
    const info = providerOptions.find((p) => p.provider === provider)
    setForm({
      ...DEFAULT_FORM,
      provider,
      displayName: info?.displayName ?? '',
      model: info?.models[0]?.model ?? '',
    })
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (!canSubmit) return
    await onSave({
      provider: form.provider,
      displayName: form.displayName || selectedProvider?.displayName || form.provider,
      model: form.model,
      apiKey: form.apiKey.trim() || undefined,
      baseUrl: form.baseUrl || undefined,
    })
  }

  return (
    <form
      onSubmit={handleSubmit}
      className={cn(
        'border-t border-black/[0.06] p-4 dark:border-white/[0.08]',
        'bg-black/[0.015] dark:bg-white/[0.025]'
      )}
    >
      <div className="mb-3 grid grid-cols-1 gap-3 sm:grid-cols-2">
        {/* Provider */}
        <div>
          <label className={uiStyles.label}>Provider</label>
          <select
            value={form.provider}
            onChange={(e) => handleProviderChange(e.target.value as LlmProvider)}
            className={cn(uiStyles.select, 'w-full')}
          >
            {providerOptions.map((p) => (
              <option key={p.provider} value={p.provider}>
                {p.displayName}
              </option>
            ))}
          </select>
        </div>

        {/* Model */}
        <div>
          <label className={uiStyles.label}>Model</label>
          {models.length > 0 ? (
            <select
              value={form.model}
              onChange={(e) => setForm({ ...form, model: e.target.value })}
              className={cn(uiStyles.select, 'w-full')}
            >
              {models.map((m) => (
                <option key={m.model} value={m.model}>
                  {m.displayName}
                </option>
              ))}
            </select>
          ) : (
            <input
              type="text"
              value={form.model}
              onChange={(e) => setForm({ ...form, model: e.target.value })}
              placeholder="e.g. llama3"
              className={uiStyles.input}
            />
          )}
        </div>

        {/* Display Name */}
        <div>
          <label className={uiStyles.label}>Display Name</label>
          <input
            type="text"
            value={form.displayName}
            onChange={(e) => setForm({ ...form, displayName: e.target.value })}
            placeholder="My Provider"
            className={uiStyles.input}
          />
        </div>

        {/* API Key */}
        <div>
          <label className={uiStyles.label}>
            API Key {needsApiKey && <span className="text-red-500">*</span>}
          </label>
          <input
            type="password"
            value={form.apiKey}
            onChange={(e) => setForm({ ...form, apiKey: e.target.value })}
            placeholder={needsApiKey ? 'sk-...' : 'not required'}
            required={needsApiKey}
            className={uiStyles.input}
          />
        </div>

        {/* Base URL (optional) */}
        <div className="sm:col-span-2">
          <label className={uiStyles.label}>Base URL</label>
          <input
            type="url"
            value={form.baseUrl}
            onChange={(e) => setForm({ ...form, baseUrl: e.target.value })}
            placeholder={baseUrlPlaceholder(form.provider)}
            className={uiStyles.input}
          />
        </div>
      </div>

      <div className="flex gap-2 justify-end">
        <button
          type="button"
          onClick={onCancel}
          disabled={saving}
          className={uiStyles.secondaryButton}
        >
          Cancel
        </button>
        <button type="submit" disabled={saving || !canSubmit} className={uiStyles.primaryButton}>
          {saving ? 'Saving...' : 'Save Provider'}
        </button>
      </div>
    </form>
  )
}

// ============================================================================
// ProvidersSection
// ============================================================================

export function ProvidersSection() {
  const {
    providers,
    providersLoading,
    providersError,
    loadProviders,
    saveProvider,
    deleteProvider,
  } = useSettingsStore()
  const [showForm, setShowForm] = useState(false)
  const [saving, setSaving] = useState(false)
  const [supportedProviders, setSupportedProviders] = useState<ProviderInfo[]>([])

  useEffect(() => {
    void loadProviders()
    // Load supported providers for the form dropdown
    void getSettingsApi()
      .getSupportedProviders()
      .then(setSupportedProviders)
      .catch(() => {
        // Non-critical: form falls back to hardcoded list
      })
  }, [loadProviders])

  async function handleSave(input: CreateProviderInput) {
    setSaving(true)
    const result = await saveProvider(input)
    setSaving(false)
    if (result) {
      setShowForm(false)
    }
  }

  async function handleDelete(id: string) {
    await deleteProvider(id)
  }

  async function handleTest(id: string) {
    const result = await getSettingsApi().testProvider(id)
    await loadProviders()
    return result
  }

  return (
    <div>
      {/* Section header */}
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>LLM Providers</h2>
          <p className={uiStyles.sectionDescription}>Configure AI model providers and API keys</p>
        </div>
        {!showForm && (
          <button
            type="button"
            onClick={() => setShowForm(true)}
            className={uiStyles.primaryButton}
          >
            <span>+</span>
            <span>Add Provider</span>
          </button>
        )}
      </div>

      {/* Error */}
      {providersError && <div className={uiStyles.error}>{providersError}</div>}

      {/* Provider list */}
      <div className={uiStyles.card}>
        {providersLoading && providers.length === 0 ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            Loading providers...
          </div>
        ) : providers.length === 0 && !showForm ? (
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              No providers configured
            </p>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Add a provider to enable AI capabilities
            </p>
          </div>
        ) : (
          providers.map((provider) => (
            <ProviderCard
              key={provider.id}
              id={provider.id}
              provider={provider.provider}
              displayName={provider.displayName}
              model={provider.model}
              isEnabled={provider.isEnabled}
              isDefault={provider.isDefault}
              apiKeyPrefix={provider.apiKeyPrefix}
              lastTestStatus={provider.lastTestStatus}
              lastTestErrorMessage={provider.lastTestErrorMessage}
              onTest={handleTest}
              onDelete={handleDelete}
            />
          ))
        )}

        {/* Add form */}
        {showForm && (
          <AddProviderFormPanel
            supportedProviders={supportedProviders}
            onSave={handleSave}
            onCancel={() => setShowForm(false)}
            saving={saving}
          />
        )}
      </div>
    </div>
  )
}
