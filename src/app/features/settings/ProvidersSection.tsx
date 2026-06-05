import { useEffect, useMemo, useState, type FormEvent } from 'react'
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
import { providerTestErrorMessage } from './providerTestErrorMessage'
import { providerSettingsErrorMessage } from './providerSettingsErrorMessage'

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

interface ProviderFormReadiness {
  ready: boolean
  title: string
  detail: string
  error: string | null
  fieldId: string | null
}

const PROVIDER_FILTERS: { id: ProviderFilter; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'ready', label: 'Ready' },
  { id: 'needs-test', label: 'Needs check' },
  { id: 'disabled', label: 'Disabled' },
]

const DEFAULT_FORM: AddProviderForm = {
  provider: 'anthropic',
  displayName: '',
  model: 'claude-sonnet-4-20250514',
  apiKey: '',
  baseUrl: '',
}

const PROVIDER_SETUP_STEPS = [
  {
    label: 'Choose AI service',
    value: 'Pick the company or gateway that provides the model.',
  },
  {
    label: 'Add service access key',
    value: 'Paste the service access key from your AI service. It stays hidden after saving.',
  },
  {
    label: 'Save and check',
    value: 'Run Check before using this service with agents.',
  },
]

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

function providerFormReadiness({
  form,
  needsApiKey,
  needsBaseUrl,
  modelInputId,
  apiKeyInputId,
  baseUrlInputId,
}: {
  form: AddProviderForm
  needsApiKey: boolean
  needsBaseUrl: boolean
  modelInputId: string
  apiKeyInputId: string
  baseUrlInputId: string
}): ProviderFormReadiness {
  if (!form.model.trim()) {
    return {
      ready: false,
      title: 'Next: Choose a model',
      detail: 'Use the suggested model, or choose one from the list.',
      error: 'Choose a model before saving this AI service.',
      fieldId: modelInputId,
    }
  }

  if (needsApiKey && !form.apiKey.trim()) {
    return {
      ready: false,
      title: 'Next: Add the service access key',
      detail:
        'Paste the service access key from the selected AI service. Do not paste your account password.',
      error: 'Add the service access key before saving this AI service.',
      fieldId: apiKeyInputId,
    }
  }

  if (needsBaseUrl && !form.baseUrl.trim()) {
    return {
      ready: false,
      title: 'Next: Add the service address',
      detail: 'Paste the web address for your compatible or local AI service.',
      error: 'Add the service address before saving this AI service.',
      fieldId: baseUrlInputId,
    }
  }

  return {
    ready: true,
    title: 'Ready to save',
    detail: 'Save this AI service, then check the connection so agents can use it safely.',
    error: null,
    fieldId: null,
  }
}

function providerConnectionState(provider: LlmProviderConfig): ProviderFilter {
  if (!provider.isEnabled) return 'disabled'
  return provider.lastTestStatus === 'passed' ? 'ready' : 'needs-test'
}

function providerStatusLabel(provider: LlmProviderConfig): string {
  if (!provider.isEnabled) return 'Disabled'
  if (provider.lastTestStatus === 'passed') return 'Ready'
  return 'Needs check'
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
      title: 'Add Your First AI Service',
      detail:
        'An AI service gives chat-only agents a model to use. Choose the service, confirm the model, add its access key, then check the connection.',
      success: 'At least 1 AI service is saved and ready for a connection check.',
      ready: false,
      action: 'add-provider',
      actionLabel: 'Add AI service',
    }
  }

  if (needsTestProviders.length > 0) {
    const firstProvider = needsTestProviders[0]
    return {
      title: 'Check AI Service Connection',
      detail: `Check ${firstProvider.displayName} before assigning work so agent creation does not fail on the first run.`,
      success: 'The AI service shows Ready and can be used by chat-only agents.',
      ready: false,
      action: 'show-needs-test',
      actionLabel: 'Show services needing check',
    }
  }

  if (readyProviders.length === 0) {
    return {
      title: 'Add an Active AI Service',
      detail:
        'All saved AI services are disabled. Add a working AI service so agents have a model to use.',
      success: 'At least 1 enabled AI service is checked and marked Ready.',
      ready: false,
      action: 'add-provider',
      actionLabel: 'Add AI service',
    }
  }

  if (!defaultProvider && readyProviders.length > 0) {
    return {
      title: 'Ready AI Service Available',
      detail: `${readyProviders[0].displayName} is ready. Use it when creating a chat-only agent.`,
      success: 'New chat-only agents can select a checked AI service.',
      ready: true,
    }
  }

  return {
    title: 'Ready to Create Chat-Only Agents',
    detail: `${defaultProvider?.displayName ?? readyProviders[0]?.displayName ?? 'An AI service'} is ready for chat-only agents.`,
    success: 'Open Agents, choose New Agent, then select Chat-only agent.',
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
        ? { ok: false, message: providerTestErrorMessage(lastTestErrorMessage, displayName) }
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
        message: result.ok
          ? 'Connection ready'
          : providerTestErrorMessage(result.error, displayName),
      })
    } catch (err) {
      setTestResult({
        ok: false,
        message: providerTestErrorMessage(err, displayName),
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
              role={visibleTestResult.ok ? undefined : 'alert'}
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
          className={cn(uiStyles.secondaryButton, 'flex-1 sm:flex-none')}
          aria-label={`Check ${displayName} connection`}
          title="Check connection"
        >
          <Activity className="h-4 w-4" aria-hidden="true" />
          <span>{testing ? 'Checking' : 'Check'}</span>
        </button>
        <button
          type="button"
          onClick={handleDelete}
          aria-label={
            confirming
              ? `Confirm removing ${displayName} AI service`
              : `Remove ${displayName} AI service`
          }
          className={cn(
            'flex-1 sm:flex-none',
            confirming ? uiStyles.dangerConfirmButton : uiStyles.dangerButton
          )}
        >
          {confirming ? 'Remove now' : 'Remove'}
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
              {allReady ? 'AI services ready for agents' : 'AI service setup needs attention'}
            </h3>
          </div>
          <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
            {total === 0
              ? 'Add and check an AI service before creating chat-only agents.'
              : `${ready}/${total} AI service${total === 1 ? '' : 's'} ready, ${needsTest} need${
                  needsTest === 1 ? 's' : ''
                } a connection check, ${disabled} disabled.`}
          </p>
        </div>
        <span className="shrink-0 rounded-full bg-black/[0.04] px-2.5 py-1 text-ui-caption font-semibold text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
          Default AI service: {defaultProvider?.displayName ?? 'None'}
        </span>
      </div>

      <div className="mt-4 grid gap-3 sm:grid-cols-4">
        <ProviderReadinessMetric label="Ready" value={String(ready)} ready={ready > 0} />
        <ProviderReadinessMetric
          label="Needs check"
          value={String(needsTest)}
          ready={needsTest === 0}
        />
        <ProviderReadinessMetric label="Disabled" value={String(disabled)} ready={disabled === 0} />
        <ProviderReadinessMetric
          label="Default service"
          value={defaultProvider?.displayName ?? 'Not set'}
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
              {step.ready ? 'Ready' : 'Next step'}
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
  const [submitAttempted, setSubmitAttempted] = useState(false)

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
  const apiKeyHelpId = 'provider-form-api-key-help'
  const baseUrlInputId = 'provider-form-base-url'
  const baseUrlHelpId = 'provider-form-base-url-help'
  const readiness = providerFormReadiness({
    form,
    needsApiKey,
    needsBaseUrl,
    modelInputId,
    apiKeyInputId,
    baseUrlInputId,
  })
  const visibleError = submitAttempted && !readiness.ready ? readiness.error : null
  const formStatusId = 'provider-form-status'
  const modelErrorId = 'provider-form-model-error'
  const apiKeyErrorId = 'provider-form-api-key-error'
  const baseUrlErrorId = 'provider-form-base-url-error'

  function handleProviderChange(provider: LlmProvider) {
    const info = providerOptions.find((p) => p.provider === provider)
    setSubmitAttempted(false)
    setForm({
      ...DEFAULT_FORM,
      provider,
      displayName: info?.displayName ?? '',
      model: info?.defaultModel ?? info?.models[0]?.model ?? '',
    })
  }

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    setSubmitAttempted(true)
    if (!readiness.ready) {
      if (readiness.fieldId) document.getElementById(readiness.fieldId)?.focus()
      return
    }
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
      noValidate
    >
      <div className="mb-3 rounded-lg border border-black/[0.06] bg-white px-3 py-2.5 dark:border-white/[0.08] dark:bg-black/20">
        <div className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
          AI service setup
        </div>
        <div className="mt-2 grid gap-1.5 sm:grid-cols-3">
          {PROVIDER_SETUP_STEPS.map((step) => (
            <div
              key={step.label}
              className="min-w-0 rounded-md bg-black/[0.025] px-2 py-1.5 dark:bg-white/[0.04]"
            >
              <span className="block text-[10px] font-medium text-secondary-light dark:text-secondary-dark">
                {step.label}
              </span>
              <span className="mt-0.5 block text-ui-caption text-foreground-light dark:text-foreground-dark">
                {step.value}
              </span>
            </div>
          ))}
        </div>
      </div>

      <div className="mb-3 grid grid-cols-1 gap-3 sm:grid-cols-2">
        {/* AI service */}
        <div>
          <label htmlFor={providerInputId} className={uiStyles.label}>
            AI service
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
                aria-invalid={visibleError !== null && readiness.fieldId === modelInputId}
                aria-describedby={`${formStatusId}${
                  visibleError !== null && readiness.fieldId === modelInputId
                    ? ` ${modelErrorId}`
                    : ''
                }`}
                className={uiStyles.input}
              />
              {visibleError !== null && readiness.fieldId === modelInputId && (
                <p id={modelErrorId} className="mt-1 text-ui-caption text-apple-red">
                  {visibleError}
                </p>
              )}
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
            <>
              <select
                id={modelInputId}
                name="model"
                value={form.model}
                onChange={(e) => setForm({ ...form, model: e.target.value })}
                aria-invalid={visibleError !== null && readiness.fieldId === modelInputId}
                aria-describedby={`${formStatusId}${
                  visibleError !== null && readiness.fieldId === modelInputId
                    ? ` ${modelErrorId}`
                    : ''
                }`}
                className={cn(uiStyles.select, 'w-full')}
              >
                {models.map((m) => (
                  <option key={m.model} value={m.model}>
                    {m.displayName}
                  </option>
                ))}
              </select>
              {visibleError !== null && readiness.fieldId === modelInputId && (
                <p id={modelErrorId} className="mt-1 text-ui-caption text-apple-red">
                  {visibleError}
                </p>
              )}
            </>
          ) : (
            <>
              <input
                id={modelInputId}
                type="text"
                name="model"
                value={form.model}
                onChange={(e) => setForm({ ...form, model: e.target.value })}
                placeholder="e.g. llama3…"
                autoComplete="off"
                aria-invalid={visibleError !== null && readiness.fieldId === modelInputId}
                aria-describedby={`${formStatusId}${
                  visibleError !== null && readiness.fieldId === modelInputId
                    ? ` ${modelErrorId}`
                    : ''
                }`}
                className={uiStyles.input}
              />
              {visibleError !== null && readiness.fieldId === modelInputId && (
                <p id={modelErrorId} className="mt-1 text-ui-caption text-apple-red">
                  {visibleError}
                </p>
              )}
            </>
          )}
        </div>

        {/* Display Name */}
        <div>
          <label htmlFor={displayNameInputId} className={uiStyles.label}>
            Name in Forge
          </label>
          <input
            id={displayNameInputId}
            type="text"
            name="displayName"
            value={form.displayName}
            onChange={(e) => setForm({ ...form, displayName: e.target.value })}
            placeholder="Team AI service…"
            autoComplete="off"
            className={uiStyles.input}
          />
        </div>

        {/* Service access key */}
        <div>
          <label htmlFor={apiKeyInputId} className={uiStyles.label}>
            Service access key {needsApiKey && <span className="text-red-500">*</span>}
          </label>
          <p
            id={apiKeyHelpId}
            className="mb-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            Paste the service access key from the selected AI service. Forge keeps it hidden after
            saving.
          </p>
          <input
            id={apiKeyInputId}
            type="password"
            name="apiKey"
            value={form.apiKey}
            onChange={(e) => setForm({ ...form, apiKey: e.target.value })}
            placeholder={needsApiKey ? 'Paste the service access key' : 'No key required'}
            autoComplete="off"
            spellCheck={false}
            aria-invalid={visibleError !== null && readiness.fieldId === apiKeyInputId}
            aria-describedby={`${formStatusId}${
              visibleError !== null && readiness.fieldId === apiKeyInputId
                ? ` ${apiKeyErrorId}`
                : ''
            } ${apiKeyHelpId}`}
            className={uiStyles.input}
          />
          {visibleError !== null && readiness.fieldId === apiKeyInputId && (
            <p id={apiKeyErrorId} className="mt-1 text-ui-caption text-apple-red">
              {visibleError}
            </p>
          )}
        </div>

        {/* Service address (optional) */}
        <div className="sm:col-span-2">
          <label htmlFor={baseUrlInputId} className={uiStyles.label}>
            Service address {needsBaseUrl && <span className="text-red-500">*</span>}
          </label>
          <p
            id={baseUrlHelpId}
            className="mb-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            Leave this alone unless you use a local model server or compatible gateway.
          </p>
          <input
            id={baseUrlInputId}
            type="url"
            name="baseUrl"
            value={form.baseUrl}
            onChange={(e) => setForm({ ...form, baseUrl: e.target.value })}
            placeholder={`${baseUrlPlaceholder(form.provider, selectedProvider)}…`}
            autoComplete="off"
            aria-invalid={visibleError !== null && readiness.fieldId === baseUrlInputId}
            aria-describedby={`${formStatusId}${
              visibleError !== null && readiness.fieldId === baseUrlInputId
                ? ` ${baseUrlErrorId}`
                : ''
            } ${baseUrlHelpId}`}
            className={uiStyles.input}
          />
          {visibleError !== null && readiness.fieldId === baseUrlInputId && (
            <p id={baseUrlErrorId} className="mt-1 text-ui-caption text-apple-red">
              {visibleError}
            </p>
          )}
        </div>
      </div>

      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <p
          id={formStatusId}
          data-testid="provider-form-status"
          className="text-ui-caption text-secondary-light dark:text-secondary-dark"
        >
          <span className="font-medium text-foreground-light dark:text-foreground-dark">
            {readiness.title}
          </span>
          <span> {readiness.detail}</span>
        </p>
        <div className="flex gap-2 sm:shrink-0">
          <button
            type="button"
            onClick={onCancel}
            disabled={saving}
            className={cn(uiStyles.secondaryButton, 'flex-1 sm:flex-none')}
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={saving}
            className={cn(uiStyles.primaryButton, 'flex-1 sm:flex-none')}
          >
            {saving ? 'Saving…' : 'Save AI service'}
          </button>
        </div>
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
          <h2 className={uiStyles.sectionTitle}>AI services</h2>
          <p className={uiStyles.sectionDescription}>
            Connect an AI service so chat-only agents can answer messages.
          </p>
        </div>
        {!showForm && (
          <button
            type="button"
            onClick={() => setShowForm(true)}
            className={uiStyles.primaryButton}
          >
            <Plus size={14} strokeWidth={2.25} aria-hidden="true" />
            <span>Add AI service</span>
          </button>
        )}
      </div>

      {/* Error */}
      {providersError && (
        <div role="alert" aria-live="polite" className={uiStyles.error}>
          {providerSettingsErrorMessage(providersError)}
        </div>
      )}

      <ProviderReadinessPanel providers={providers} />
      <ProviderNextStepPanel step={nextStep} onAction={handleNextStepAction} />

      <div className="mb-4 flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
        <label className="relative min-w-0 flex-1">
          <span className="sr-only">Search AI services</span>
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
            placeholder="Search AI services…"
            autoComplete="off"
            className={cn(uiStyles.input, 'pl-9')}
          />
        </label>
        <div
          className="flex flex-wrap gap-2"
          role="group"
          aria-label="Filter AI services by status"
        >
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

      {/* AI service list */}
      <div className={uiStyles.card}>
        {providersLoading && providers.length === 0 ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            Loading AI services…
          </div>
        ) : providers.length === 0 && !showForm ? (
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              No AI services connected
            </p>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Add one AI service, confirm the model, add its access key, save it, then run Check
              before creating chat-only agents.
            </p>
          </div>
        ) : filteredProviders.length === 0 && !showForm ? (
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              No AI services match this view
            </p>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Clear search or switch filters to review every AI service.
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
