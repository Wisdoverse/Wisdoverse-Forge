import { useEffect, useMemo, useState } from 'react'
import { Activity, AlertTriangle, CheckCircle2, Plus, Search } from 'lucide-react'
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

type ProviderFilter = 'all' | 'ready' | 'needs-test' | 'disabled'
type ProviderNextAction = 'add-provider' | 'show-needs-test'

interface ProviderNextStep {
  title: string
  detail: string
  success: string
  ready: boolean
  action?: ProviderNextAction
  actionLabel?: string
}

const PROVIDER_FILTERS: { id: ProviderFilter; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'ready', label: 'Ready' },
  { id: 'needs-test', label: 'Needs Test' },
  { id: 'disabled', label: 'Disabled' },
]

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
    defaultModel: 'claude-sonnet-4-20250514',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'claude-sonnet-4-20250514', displayName: 'Claude Sonnet 4' }],
  },
  {
    provider: 'openai',
    displayName: 'OpenAI',
    defaultModel: 'gpt-5.4',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'gpt-5.4', displayName: 'GPT-5.4' }],
  },
  {
    provider: 'google',
    displayName: 'Google',
    defaultModel: 'gemini-2.5-pro',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'gemini-2.5-pro', displayName: 'Gemini 2.5 Pro' }],
  },
  {
    provider: 'ollama',
    displayName: 'Ollama',
    defaultModel: 'llama3',
    requiresApiKey: false,
    allowCustomModels: true,
    models: [{ model: 'llama3', displayName: 'Llama 3' }],
  },
  {
    provider: 'groq',
    displayName: 'Groq',
    defaultModel: 'llama-3.3-70b-versatile',
    defaultBaseUrl: 'https://api.groq.com/openai',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'llama-3.3-70b-versatile', displayName: 'Llama 3.3 70B' }],
  },
  {
    provider: 'deepseek',
    displayName: 'DeepSeek',
    defaultModel: 'deepseek-chat',
    defaultBaseUrl: 'https://api.deepseek.com',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'deepseek-chat', displayName: 'DeepSeek Chat' }],
  },
  {
    provider: 'xai',
    displayName: 'xAI',
    defaultModel: 'grok-3-mini',
    defaultBaseUrl: 'https://api.x.ai',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'grok-3-mini', displayName: 'Grok 3 Mini' }],
  },
  {
    provider: 'openrouter',
    displayName: 'OpenRouter',
    defaultModel: 'openai/gpt-4o-mini',
    defaultBaseUrl: 'https://openrouter.ai/api',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'openai/gpt-4o-mini', displayName: 'OpenAI GPT-4o Mini' }],
  },
  {
    provider: 'together',
    displayName: 'Together AI',
    defaultModel: 'openai/gpt-oss-20b',
    defaultBaseUrl: 'https://api.together.xyz',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'openai/gpt-oss-20b', displayName: 'GPT OSS 20B' }],
  },
  {
    provider: 'fireworks',
    displayName: 'Fireworks AI',
    defaultModel: 'accounts/fireworks/models/qwen3-30b-a3b',
    defaultBaseUrl: 'https://api.fireworks.ai/inference',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [
      {
        model: 'accounts/fireworks/models/qwen3-30b-a3b',
        displayName: 'Qwen3 30B A3B',
      },
    ],
  },
  {
    provider: 'litellm',
    displayName: 'LiteLLM Gateway',
    defaultModel: 'gpt-4o-mini',
    defaultBaseUrl: 'http://litellm:4000',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'gpt-4o-mini', displayName: 'Gateway alias: gpt-4o-mini' }],
  },
  {
    provider: 'openai_compatible',
    displayName: 'OpenAI-Compatible',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [],
  },
]

function baseUrlPlaceholder(provider: LlmProvider, info?: ProviderInfo): string {
  if (info?.defaultBaseUrl) return info.defaultBaseUrl
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
    case 'openai_compatible':
      return 'https://api.example.com'
    default:
      return 'https://api.example.com'
  }
}

function providerNeedsApiKey(provider: LlmProvider, info?: ProviderInfo): boolean {
  return info?.requiresApiKey ?? provider !== 'ollama'
}

function providerNeedsBaseUrl(provider: LlmProvider, info?: ProviderInfo): boolean {
  return provider === 'openai_compatible' && !info?.defaultBaseUrl
}

function providerConnectionState(provider: LlmProviderConfig): ProviderFilter {
  if (!provider.isEnabled) return 'disabled'
  return provider.lastTestStatus === 'passed' ? 'ready' : 'needs-test'
}

function providerStatusLabel(provider: LlmProviderConfig): string {
  if (!provider.isEnabled) return 'Disabled'
  if (provider.lastTestStatus === 'passed') return 'Ready'
  if (provider.lastTestStatus === 'failed') return 'Failed'
  return 'Needs Test'
}

function providerStatusTone(provider: LlmProviderConfig): string {
  if (!provider.isEnabled) {
    return 'bg-apple-gray-5 text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark'
  }
  if (provider.lastTestStatus === 'passed') return 'bg-apple-green/10 text-apple-green'
  return 'bg-apple-orange/12 text-apple-orange'
}

function providerMatchesSearch(provider: LlmProviderConfig, search: string): boolean {
  const query = search.trim().toLowerCase()
  if (!query) return true
  return [provider.displayName, provider.provider, provider.model, provider.apiKeyPrefix]
    .filter((value): value is string => Boolean(value))
    .some((value) => value.toLowerCase().includes(query))
}

function providerNextStep(providers: LlmProviderConfig[]): ProviderNextStep {
  const total = providers.length
  const readyProviders = providers.filter(
    (provider) => providerConnectionState(provider) === 'ready'
  )
  const needsTestProviders = providers.filter(
    (provider) => providerConnectionState(provider) === 'needs-test'
  )
  const defaultProvider = providers.find((provider) => provider.isDefault)

  if (total === 0) {
    return {
      title: 'Add Your First Provider',
      detail:
        'A provider gives agents a model to use. Pick a provider, paste the key, save it, then run a connection test.',
      success: 'At least 1 provider is saved and ready for a test.',
      ready: false,
      action: 'add-provider',
      actionLabel: 'Add Provider',
    }
  }

  if (needsTestProviders.length > 0) {
    const firstProvider = needsTestProviders[0]
    return {
      title: 'Test Provider Connection',
      detail: `Test ${firstProvider.displayName} before assigning work so agent creation does not fail on the first run.`,
      success: 'The provider shows Ready and can be used by Provider + Prompt agents.',
      ready: false,
      action: 'show-needs-test',
      actionLabel: 'Show Needs Test',
    }
  }

  if (readyProviders.length === 0) {
    return {
      title: 'Add an Active Provider',
      detail:
        'All saved providers are disabled. Add a working provider so agents have a model to use.',
      success: 'At least 1 enabled provider is tested and marked Ready.',
      ready: false,
      action: 'add-provider',
      actionLabel: 'Add Provider',
    }
  }

  if (!defaultProvider && readyProviders.length > 0) {
    return {
      title: 'Ready Provider Available',
      detail: `${readyProviders[0].displayName} is ready. Use it when creating a Provider + Prompt agent.`,
      success: 'New Provider + Prompt agents can select a tested provider.',
      ready: true,
    }
  }

  return {
    title: 'Ready to Create Provider Agents',
    detail: `${defaultProvider?.displayName ?? readyProviders[0]?.displayName ?? 'A provider'} is ready for Provider + Prompt agents.`,
    success: 'Open Agents, choose New Agent, then select Provider + Prompt.',
    ready: true,
  }
}

// ============================================================================
// Provider Card
// ============================================================================

interface ProviderCardProps {
  providerConfig: LlmProviderConfig
  onTest: (id: string) => Promise<TestConnectionResult>
  onDelete: (id: string) => void
}

function ProviderCard({ providerConfig, onTest, onDelete }: ProviderCardProps) {
  const {
    id,
    displayName,
    model,
    isEnabled,
    isDefault,
    apiKeyPrefix,
    lastTestStatus,
    lastTestErrorMessage,
  } = providerConfig
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
            <span
              className={cn(
                'rounded-full px-2 py-0.5 text-ui-caption font-semibold',
                providerStatusTone(providerConfig)
              )}
            >
              {providerStatusLabel(providerConfig)}
            </span>
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

function ProviderReadinessPanel({ providers }: { providers: LlmProviderConfig[] }) {
  const total = providers.length
  const ready = providers.filter((provider) => providerConnectionState(provider) === 'ready').length
  const disabled = providers.filter(
    (provider) => providerConnectionState(provider) === 'disabled'
  ).length
  const needsTest = providers.filter(
    (provider) => providerConnectionState(provider) === 'needs-test'
  ).length
  const defaultProvider = providers.find((provider) => provider.isDefault)
  const allReady = ready > 0 && ready === total - disabled && needsTest === 0

  return (
    <section
      data-testid="provider-readiness"
      className="mb-4 rounded-lg border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-[#2a2a2c]"
    >
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            {allReady ? (
              <CheckCircle2
                size={17}
                strokeWidth={2.25}
                className="text-apple-green"
                aria-hidden="true"
              />
            ) : (
              <AlertTriangle
                size={17}
                strokeWidth={2.25}
                className="text-apple-orange"
                aria-hidden="true"
              />
            )}
            <h3 className={uiStyles.sectionTitle}>
              {allReady ? 'Providers ready for agent creation' : 'Provider setup needs attention'}
            </h3>
          </div>
          <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
            {total === 0
              ? 'Add and test a provider before creating Provider + Prompt agents.'
              : `${ready}/${total} provider${total === 1 ? '' : 's'} ready, ${needsTest} need${
                  needsTest === 1 ? 's' : ''
                } a connection test, ${disabled} disabled.`}
          </p>
        </div>
        <span className="shrink-0 rounded-full bg-black/[0.04] px-2.5 py-1 text-ui-caption font-semibold text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
          Default: {defaultProvider?.displayName ?? 'None'}
        </span>
      </div>

      <div className="mt-4 grid gap-3 sm:grid-cols-4">
        <ProviderReadinessMetric label="Ready" value={String(ready)} ready={ready > 0} />
        <ProviderReadinessMetric
          label="Needs Test"
          value={String(needsTest)}
          ready={needsTest === 0}
        />
        <ProviderReadinessMetric label="Disabled" value={String(disabled)} ready={disabled === 0} />
        <ProviderReadinessMetric
          label="Default Route"
          value={defaultProvider?.displayName ?? 'Not Set'}
          ready={Boolean(defaultProvider)}
        />
      </div>
    </section>
  )
}

function ProviderNextStepPanel({
  step,
  onAction,
}: {
  step: ProviderNextStep
  onAction: (action: ProviderNextAction) => void
}) {
  const action = step.action

  return (
    <section
      data-testid="provider-next-step"
      className={cn(
        'mb-4 rounded-lg border px-4 py-3',
        step.ready
          ? 'border-apple-green/20 bg-apple-green/5'
          : 'border-apple-blue/20 bg-apple-blue/[0.04]'
      )}
    >
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            {step.ready ? (
              <CheckCircle2
                size={17}
                strokeWidth={2.25}
                className="shrink-0 text-apple-green"
                aria-hidden="true"
              />
            ) : (
              <AlertTriangle
                size={17}
                strokeWidth={2.25}
                className="shrink-0 text-apple-blue"
                aria-hidden="true"
              />
            )}
            <p className="text-ui-caption font-semibold uppercase text-secondary-light dark:text-secondary-dark">
              {step.ready ? 'Ready' : 'Do This Next'}
            </p>
          </div>
          <h3 className="mt-1 text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            {step.title}
          </h3>
          <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
            {step.detail}
          </p>
          <p className="mt-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Success: {step.success}
          </p>
        </div>
        {action && step.actionLabel && (
          <button
            type="button"
            onClick={() => onAction(action)}
            className={cn(uiStyles.secondaryButton, 'shrink-0')}
          >
            {step.actionLabel}
          </button>
        )}
      </div>
    </section>
  )
}

function ProviderReadinessMetric({
  label,
  value,
  ready,
}: {
  label: string
  value: string
  ready: boolean
}) {
  return (
    <div className="rounded-lg border border-black/[0.06] px-3 py-2 dark:border-white/[0.08]">
      <div className="flex items-center gap-2">
        <span
          className={cn('h-2 w-2 rounded-full', ready ? 'bg-apple-green' : 'bg-apple-orange')}
        />
        <span className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
          {label}
        </span>
      </div>
      <p className="mt-1 truncate text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
        {value}
      </p>
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
  const needsApiKey = providerNeedsApiKey(form.provider, selectedProvider)
  const needsBaseUrl = providerNeedsBaseUrl(form.provider, selectedProvider)
  const modelListId = `provider-models-${form.provider}`
  const providerInputId = 'provider-form-provider'
  const modelInputId = 'provider-form-model'
  const displayNameInputId = 'provider-form-display-name'
  const apiKeyInputId = 'provider-form-api-key'
  const baseUrlInputId = 'provider-form-base-url'
  const canSubmit = Boolean(
    form.model.trim() &&
    (!needsApiKey || form.apiKey.trim()) &&
    (!needsBaseUrl || form.baseUrl.trim())
  )

  function handleProviderChange(provider: LlmProvider) {
    const info = providerOptions.find((p) => p.provider === provider)
    setForm({
      ...DEFAULT_FORM,
      provider,
      displayName: info?.displayName ?? '',
      model: info?.defaultModel ?? info?.models[0]?.model ?? '',
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
          <label htmlFor={providerInputId} className={uiStyles.label}>
            Provider
          </label>
          <select
            id={providerInputId}
            name="provider"
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
          <label htmlFor={modelInputId} className={uiStyles.label}>
            Model
          </label>
          {(selectedProvider?.allowCustomModels ?? true) ? (
            <>
              <input
                id={modelInputId}
                type="text"
                name="model"
                value={form.model}
                onChange={(e) => setForm({ ...form, model: e.target.value })}
                placeholder={selectedProvider?.defaultModel ?? 'e.g. llama3…'}
                list={models.length > 0 ? modelListId : undefined}
                autoComplete="off"
                className={uiStyles.input}
              />
              {models.length > 0 && (
                <datalist id={modelListId}>
                  {models.map((m) => (
                    <option key={m.model} value={m.model}>
                      {m.displayName}
                    </option>
                  ))}
                </datalist>
              )}
            </>
          ) : models.length > 0 ? (
            <select
              id={modelInputId}
              name="model"
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
              id={modelInputId}
              type="text"
              name="model"
              value={form.model}
              onChange={(e) => setForm({ ...form, model: e.target.value })}
              placeholder="e.g. llama3…"
              autoComplete="off"
              className={uiStyles.input}
            />
          )}
        </div>

        {/* Display Name */}
        <div>
          <label htmlFor={displayNameInputId} className={uiStyles.label}>
            Display Name
          </label>
          <input
            id={displayNameInputId}
            type="text"
            name="displayName"
            value={form.displayName}
            onChange={(e) => setForm({ ...form, displayName: e.target.value })}
            placeholder="My Provider…"
            autoComplete="off"
            className={uiStyles.input}
          />
        </div>

        {/* API Key */}
        <div>
          <label htmlFor={apiKeyInputId} className={uiStyles.label}>
            API Key {needsApiKey && <span className="text-red-500">*</span>}
          </label>
          <input
            id={apiKeyInputId}
            type="password"
            name="apiKey"
            value={form.apiKey}
            onChange={(e) => setForm({ ...form, apiKey: e.target.value })}
            placeholder={needsApiKey ? 'sk-…' : 'not required…'}
            required={needsApiKey}
            autoComplete="off"
            spellCheck={false}
            className={uiStyles.input}
          />
        </div>

        {/* Base URL (optional) */}
        <div className="sm:col-span-2">
          <label htmlFor={baseUrlInputId} className={uiStyles.label}>
            Base URL {needsBaseUrl && <span className="text-red-500">*</span>}
          </label>
          <input
            id={baseUrlInputId}
            type="url"
            name="baseUrl"
            value={form.baseUrl}
            onChange={(e) => setForm({ ...form, baseUrl: e.target.value })}
            placeholder={`${baseUrlPlaceholder(form.provider, selectedProvider)}…`}
            required={needsBaseUrl}
            autoComplete="off"
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
          {saving ? 'Saving…' : 'Save Provider'}
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
  const [providerSearch, setProviderSearch] = useState('')
  const [providerFilter, setProviderFilter] = useState<ProviderFilter>('all')
  const nextStep = useMemo(() => providerNextStep(providers), [providers])
  const filteredProviders = useMemo(
    () =>
      providers.filter((provider) => {
        const matchesFilter =
          providerFilter === 'all' || providerConnectionState(provider) === providerFilter
        return matchesFilter && providerMatchesSearch(provider, providerSearch)
      }),
    [providerFilter, providerSearch, providers]
  )

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

  function handleNextStepAction(action: ProviderNextAction) {
    if (action === 'add-provider') {
      setShowForm(true)
      return
    }
    if (action === 'show-needs-test') {
      setProviderSearch('')
      setProviderFilter('needs-test')
    }
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
            <Plus size={14} strokeWidth={2.25} aria-hidden="true" />
            <span>Add Provider</span>
          </button>
        )}
      </div>

      {/* Error */}
      {providersError && <div className={uiStyles.error}>{providersError}</div>}

      <ProviderReadinessPanel providers={providers} />
      <ProviderNextStepPanel step={nextStep} onAction={handleNextStepAction} />

      <div className="mb-4 flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
        <label className="relative min-w-0 flex-1">
          <span className="sr-only">Search Providers</span>
          <Search
            size={14}
            strokeWidth={2}
            className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-secondary-light dark:text-secondary-dark"
            aria-hidden="true"
          />
          <input
            type="search"
            name="provider-search"
            value={providerSearch}
            onChange={(event) => setProviderSearch(event.target.value)}
            placeholder="Search providers…"
            autoComplete="off"
            className={cn(uiStyles.input, 'pl-9')}
          />
        </label>
        <div className="flex flex-wrap gap-2" role="group" aria-label="Filter providers by status">
          {PROVIDER_FILTERS.map((filter) => (
            <button
              key={filter.id}
              type="button"
              onClick={() => setProviderFilter(filter.id)}
              className={cn(
                'inline-flex h-8 items-center rounded-full px-3 text-ui-caption font-semibold transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
                providerFilter === filter.id
                  ? 'bg-apple-blue text-white'
                  : 'border border-black/[0.08] bg-white text-secondary-light hover:bg-black/[0.03] hover:text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark dark:hover:bg-white/[0.08] dark:hover:text-foreground-dark'
              )}
              aria-pressed={providerFilter === filter.id}
            >
              {filter.label}
            </button>
          ))}
        </div>
      </div>

      {/* Provider list */}
      <div className={uiStyles.card}>
        {providersLoading && providers.length === 0 ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            Loading providers…
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
        ) : filteredProviders.length === 0 && !showForm ? (
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              No providers match this view
            </p>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Clear search or switch filters to review every provider.
            </p>
          </div>
        ) : (
          filteredProviders.map((provider) => (
            <ProviderCard
              key={provider.id}
              providerConfig={provider}
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
