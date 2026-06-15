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

interface ProviderFilterEmptyState {
  title: string
  detail: string
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
  { label: 'Choose AI account', value: 'Pick the service your team already pays for or runs.' },
  {
    label: 'Paste service access key',
    value: 'Open that account, copy its access key, and paste it here.',
  },
  {
    label: 'Save and check',
    value: 'Click Check after saving. Ready means agents can use this service.',
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
  // Region-aware vendors use the default address unless an owner provides
  // a different service guide address.
  {
    provider: 'zhipu',
    displayName: 'Zhipu GLM',
    defaultModel: 'glm-4.7',
    defaultBaseUrl: 'https://open.bigmodel.cn/api/paas/v4',
    globalBaseUrl: 'https://api.z.ai/api/paas/v4',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'glm-4.7', displayName: 'GLM-4.7' }],
  },
  {
    provider: 'zhipu_coding',
    displayName: 'Zhipu GLM Coding Plan',
    defaultModel: 'glm-4.7',
    defaultBaseUrl: 'https://open.bigmodel.cn/api/anthropic',
    globalBaseUrl: 'https://api.z.ai/api/anthropic',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'glm-4.7', displayName: 'GLM-4.7' }],
  },
  {
    provider: 'minimax',
    displayName: 'MiniMax',
    defaultModel: 'MiniMax-M3',
    defaultBaseUrl: 'https://api.minimaxi.com/v1',
    globalBaseUrl: 'https://api.minimax.io/v1',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'MiniMax-M3', displayName: 'MiniMax M3' }],
  },
  {
    provider: 'minimax_coding',
    displayName: 'MiniMax Coding Plan',
    defaultModel: 'MiniMax-M3',
    defaultBaseUrl: 'https://api.minimaxi.com/anthropic',
    globalBaseUrl: 'https://api.minimax.io/anthropic',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'MiniMax-M3', displayName: 'MiniMax M3' }],
  },
  {
    provider: 'moonshot',
    displayName: 'Moonshot Kimi',
    defaultModel: 'kimi-k2.5',
    defaultBaseUrl: 'https://api.moonshot.cn/v1',
    globalBaseUrl: 'https://api.moonshot.ai/v1',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'kimi-k2.5', displayName: 'Kimi K2.5' }],
  },
  {
    provider: 'moonshot_coding',
    displayName: 'Moonshot Kimi Coding Plan',
    defaultModel: 'kimi-k2.5',
    defaultBaseUrl: 'https://api.moonshot.cn/anthropic',
    globalBaseUrl: 'https://api.moonshot.ai/anthropic',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'kimi-k2.5', displayName: 'Kimi K2.5' }],
  },
  {
    provider: 'dashscope',
    displayName: 'Alibaba Qwen (DashScope)',
    defaultModel: 'qwen3-coder-plus',
    defaultBaseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    globalBaseUrl: 'https://dashscope-intl.aliyuncs.com/compatible-mode/v1',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'qwen3-coder-plus', displayName: 'Qwen3 Coder Plus' }],
  },
  {
    provider: 'dashscope_coding',
    displayName: 'Alibaba Qwen Coding Plan',
    defaultModel: 'qwen3-coder-plus',
    defaultBaseUrl: 'https://coding.dashscope.aliyuncs.com/apps/anthropic',
    globalBaseUrl: 'https://coding-intl.dashscope.aliyuncs.com/apps/anthropic',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'qwen3-coder-plus', displayName: 'Qwen3 Coder Plus' }],
  },
  {
    provider: 'hunyuan',
    displayName: 'Tencent Hunyuan',
    defaultModel: 'hunyuan-turbo-latest',
    defaultBaseUrl: 'https://api.hunyuan.cloud.tencent.com/v1',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'hunyuan-turbo-latest', displayName: 'Hunyuan Turbo' }],
  },
  {
    provider: 'xiaomi',
    displayName: 'Xiaomi MiMo',
    defaultModel: 'mimo-v2.5-pro',
    defaultBaseUrl: 'https://api.xiaomimimo.com/v1',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'mimo-v2.5-pro', displayName: 'MiMo V2.5 Pro' }],
  },
  {
    provider: 'xiaomi_coding',
    displayName: 'Xiaomi MiMo Coding Plan',
    defaultModel: 'mimo-v2.5-pro',
    defaultBaseUrl: 'https://api.xiaomimimo.com/anthropic',
    requiresApiKey: true,
    allowCustomModels: true,
    models: [{ model: 'mimo-v2.5-pro', displayName: 'MiMo V2.5 Pro' }],
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

function serviceAddressPlaceholder(needsBaseUrl: boolean): string {
  return needsBaseUrl ? 'Paste the service address from your guide' : 'Usually leave this blank'
}

function serviceAddressHelp(selectedProvider?: ProviderInfo): string {
  if (selectedProvider?.globalBaseUrl) {
    return 'Leave blank to use the default regional address. Fill it only when your service guide or owner gives you a global address.'
  }
  return 'Most users leave this blank. Fill it only when an owner gives you a custom AI service address.'
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
      title: 'Next: choose the model to use',
      detail:
        'The suggested model is safe to start with. Change it only if your service guide gave you another model name.',
      error: 'Choose the model to use before saving this AI service.',
      fieldId: modelInputId,
    }
  }

  if (needsApiKey && !form.apiKey.trim()) {
    return {
      ready: false,
      title: 'Next: paste the service access key',
      detail:
        'Open your AI service account, copy its access key, and paste it here. Some services call this an API key. Forge hides it after saving.',
      error: 'Paste the service access key before saving this AI service.',
      fieldId: apiKeyInputId,
    }
  }

  if (needsBaseUrl && !form.baseUrl.trim()) {
    return {
      ready: false,
      title: 'Next: add the service address',
      detail:
        'Most users leave this blank. Fill it only when an owner gives you a custom AI service address.',
      error: 'Add the service address before saving this AI service.',
      fieldId: baseUrlInputId,
    }
  }

  return {
    ready: true,
    title: 'Ready to save this service',
    detail: 'Save it. When it appears in the list, click Check; Ready means agents can use it.',
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

function providerFilterEmptyState(
  filter: ProviderFilter,
  search: string
): ProviderFilterEmptyState {
  const hasSearch = search.trim().length > 0
  const hasFilter = filter !== 'all'

  if (hasSearch && hasFilter) {
    return {
      title: 'Clear search or show all AI services',
      detail: 'Your AI services exist, but the current search and filter hide them.',
    }
  }

  if (hasSearch) {
    return {
      title: 'Clear search to see AI services',
      detail: 'Your AI services exist, but this search hides them. Try a broader name.',
    }
  }

  return {
    title: 'Choose All to see AI services',
    detail: 'Your AI services exist, but this filter has no results yet.',
  }
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
      title: 'Add your first AI service',
      detail:
        'An AI service is the account agents use to answer. Pick a service, paste the service access key, save it, then click Check.',
      success: 'At least 1 AI service is saved and ready for a connection check.',
      ready: false,
      action: 'add-provider',
      actionLabel: 'Add AI service',
    }
  }

  if (needsTestProviders.length > 0) {
    const firstProvider = needsTestProviders[0]
    return {
      title: 'Check the AI service connection',
      detail: `Click Check for ${firstProvider.displayName} before assigning work so agents do not fail on the first answer.`,
      success: 'The AI service shows Ready and can be used by simple chat agents.',
      ready: false,
      action: 'show-needs-test',
      actionLabel: 'Show needs check',
    }
  }

  if (readyProviders.length === 0) {
    return {
      title: 'Add a working AI service',
      detail:
        'All saved AI services are disabled. Add one working AI service so agents have an account to use.',
      success: 'At least 1 enabled AI service is tested and marked Ready.',
      ready: false,
      action: 'add-provider',
      actionLabel: 'Add AI service',
    }
  }

  if (!defaultProvider && readyProviders.length > 0) {
    return {
      title: 'Ready AI service available',
      detail: `${readyProviders[0].displayName} is ready. Choose it when creating a simple chat agent.`,
      success: 'New simple chat agents can select a tested AI service.',
      ready: true,
    }
  }

  return {
    title: 'Ready to create simple chat agents',
    detail: `${defaultProvider?.displayName ?? readyProviders[0]?.displayName ?? 'An AI service'} is ready for agents that answer in chat.`,
    success: 'Open Agents, choose Create Agent, then select Simple chat agent.',
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
              role={visibleTestResult.ok ? 'status' : 'alert'}
              aria-live="polite"
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
          aria-label={`Check ${displayName} AI service connection`}
          title="Check AI service connection"
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
            'shrink-0',
            confirming ? uiStyles.dangerConfirmButton : uiStyles.dangerButton
          )}
        >
          {confirming ? 'Confirm remove' : 'Remove AI service'}
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
  const defaultProviderLabel =
    defaultProvider?.displayName ??
    (total === 0 ? 'add an AI service first' : 'choose a ready AI service')
  const defaultProviderMetric =
    defaultProvider?.displayName ?? (total === 0 ? 'Add first service' : 'Choose a default')
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
              {allReady
                ? 'AI services ready for agent creation'
                : 'AI service setup needs attention'}
            </h3>
          </div>
          <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
            {total === 0
              ? 'Add and check an AI service before creating simple chat agents.'
              : providerReadinessSummary(ready, needsTest, disabled)}
          </p>
        </div>
        <span className="shrink-0 rounded-full bg-black/[0.04] px-2.5 py-1 text-ui-caption font-semibold text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
          Default: {defaultProviderLabel}
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
          label="Default AI service"
          value={defaultProviderMetric}
          ready={Boolean(defaultProvider)}
        />
      </div>
    </section>
  )
}

function providerReadinessSummary(ready: number, needsTest: number, disabled: number): string {
  const readyText =
    ready === 0
      ? needsTest > 0
        ? 'Run a connection check before agents use these AI services'
        : 'Enable or add an AI service before agents can use one'
      : `${providerCount(ready)} ${ready === 1 ? 'is' : 'are'} ready to use`
  const needsTestText =
    needsTest === 0
      ? 'no connection checks are needed'
      : `${providerCount(needsTest)} ${needsTest === 1 ? 'needs' : 'need'} a connection check`
  const disabledText =
    disabled === 0
      ? 'none are disabled'
      : `${providerCount(disabled)} ${disabled === 1 ? 'is' : 'are'} disabled`

  return `${readyText}. ${needsTestText}, and ${disabledText}.`
}

function providerCount(count: number): string {
  return `${count} AI service${count === 1 ? '' : 's'}`
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
              {step.ready ? 'Ready' : 'Do this next'}
            </p>
          </div>
          <h3 className="mt-1 text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            {step.title}
          </h3>
          <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
            {step.detail}
          </p>
          <p className="mt-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
            What success looks like: {step.success}
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
  const modelHelpId = 'provider-form-model-help'
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
          3 steps to connect an AI account
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
            Model to use
          </label>
          <p
            id={modelHelpId}
            className="mb-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            The suggested model is safe to start with. Change it only if your service guide gave you
            another model name.
          </p>
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
                aria-describedby={`${formStatusId} ${modelHelpId}${
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
                aria-describedby={`${formStatusId} ${modelHelpId}${
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
                aria-describedby={`${formStatusId} ${modelHelpId}${
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

        {/* Display name */}
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
            placeholder="My AI service…"
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
            Paste the access key from your AI service account. Some services call this an API key.
            Forge hides it after saving.
          </p>
          <input
            id={apiKeyInputId}
            type="password"
            name="apiKey"
            value={form.apiKey}
            onChange={(e) => setForm({ ...form, apiKey: e.target.value })}
            placeholder={needsApiKey ? 'sk-…' : 'not required…'}
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
            {serviceAddressHelp(selectedProvider)}
          </p>
          <input
            id={baseUrlInputId}
            type="url"
            name="baseUrl"
            value={form.baseUrl}
            onChange={(e) => setForm({ ...form, baseUrl: e.target.value })}
            placeholder={`${serviceAddressPlaceholder(needsBaseUrl)}…`}
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

      <div className="flex items-center justify-between gap-2">
        <div
          id={formStatusId}
          data-testid="provider-form-status"
          className="text-ui-caption text-secondary-light dark:text-secondary-dark"
        >
          <span className="block font-semibold text-foreground-light dark:text-foreground-dark">
            {readiness.title}
          </span>
          <span className="block">{readiness.detail}</span>
        </div>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={onCancel}
            disabled={saving}
            className={uiStyles.secondaryButton}
          >
            Cancel
          </button>
          <button type="submit" disabled={saving} className={uiStyles.primaryButton}>
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
  const filterEmptyState = providerFilterEmptyState(providerFilter, providerSearch)

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

  function resetProviderFilters() {
    setProviderSearch('')
    setProviderFilter('all')
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
            Connect the AI accounts that simple chat agents use to answer questions.
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
          {providersError}
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

      {/* Provider list */}
      <div className={uiStyles.card}>
        {providersLoading && providers.length === 0 ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            Loading AI services…
          </div>
        ) : providers.length === 0 && !showForm ? (
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              Add your first AI service
            </p>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Use the step above to add one AI account, then click Check so agents can answer
              without setup surprises.
            </p>
          </div>
        ) : filteredProviders.length === 0 && !showForm ? (
          <div data-testid="provider-filter-empty" className="px-4 py-6 text-center">
            <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
              {filterEmptyState.title}
            </p>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              {filterEmptyState.detail}
            </p>
            <button
              type="button"
              onClick={resetProviderFilters}
              className="mt-3 inline-flex h-8 items-center rounded-full px-3 text-ui-button font-medium text-apple-blue transition-colors hover:bg-apple-blue/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
            >
              Show all AI services
            </button>
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
