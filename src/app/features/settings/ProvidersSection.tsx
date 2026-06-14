import { useEffect, useMemo, useState, type FormEvent } from 'react'
import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  Plus,
  Search,
  SlidersHorizontal,
} from 'lucide-react'
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
/** Which "Add" experience is visible: the curated catalog or the bring-your-own gateway form. */
type AddBucket = 'catalog' | 'custom'

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

/**
 * A built-in catalog vendor. The `xxx` (API) and `xxx_coding` (Coding Plan)
 * provider keys are collapsed into one card so operators pick the vendor once,
 * then toggle Plan / Region instead of scrolling a 25-entry dropdown.
 */
interface CatalogVendor {
  /** Stable grouping key (the base provider key, e.g. `zhipu`). */
  key: string
  /** Vendor display name with the " Coding Plan" suffix stripped. */
  displayName: string
  api?: ProviderInfo
  coding?: ProviderInfo
}

const PROVIDER_FILTERS: { id: ProviderFilter; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'ready', label: 'Ready' },
  { id: 'needs-test', label: 'Needs Test' },
  { id: 'disabled', label: 'Disabled' },
]

/** Provider keys that are bring-your-own endpoints rather than curated vendors. */
const GATEWAY_PROVIDERS: ReadonlySet<LlmProvider> = new Set<LlmProvider>([
  'openai_compatible',
  'litellm',
  'openrouter',
])

const DEFAULT_FORM: AddProviderForm = {
  provider: 'openai_compatible',
  displayName: '',
  model: '',
  apiKey: '',
  baseUrl: '',
}

const PROVIDER_SETUP_STEPS = [
  { label: 'Choose endpoint', value: 'Point at your OpenAI-compatible or gateway service.' },
  { label: 'Paste key', value: 'Use the provider key from that account. It is stored encrypted.' },
  { label: 'Save, then test', value: 'Run Test before creating Provider + Prompt agents.' },
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
  // Mainstream China-region vendors: the default Base URL is the China
  // endpoint; globalBaseUrl is the Global region endpoint surfaced as a toggle.
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

/** Strip the " Coding Plan" suffix so both variants share one vendor name. */
function vendorDisplayName(info: ProviderInfo): string {
  return info.displayName.replace(/\s+Coding Plan$/i, '').trim()
}

/**
 * Collapse a flat `ProviderInfo[]` into curated catalog vendors plus the
 * bring-your-own gateway list. `xxx` + `xxx_coding` keys fold into one vendor.
 */
function deriveCatalog(providers: ProviderInfo[]): {
  vendors: CatalogVendor[]
  gateways: ProviderInfo[]
} {
  const vendorMap = new Map<string, CatalogVendor>()
  const vendors: CatalogVendor[] = []
  const gateways: ProviderInfo[] = []

  for (const info of providers) {
    if (GATEWAY_PROVIDERS.has(info.provider)) {
      gateways.push(info)
      continue
    }
    const isCoding = info.provider.endsWith('_coding')
    const key = isCoding ? info.provider.slice(0, -'_coding'.length) : info.provider
    let vendor = vendorMap.get(key)
    if (!vendor) {
      vendor = { key, displayName: vendorDisplayName(info) }
      vendorMap.set(key, vendor)
      vendors.push(vendor)
    }
    if (isCoding) {
      vendor.coding = info
    } else {
      vendor.api = info
      // Prefer the API variant's (suffix-free) name when both exist.
      vendor.displayName = vendorDisplayName(info)
    }
  }

  return { vendors, gateways }
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
      title: 'Next: Add Model',
      detail: 'Choose a model from the list or keep the suggested default.',
      error: 'Add a model before saving this provider.',
      fieldId: modelInputId,
    }
  }

  if (needsApiKey && !form.apiKey.trim()) {
    return {
      ready: false,
      title: 'Next: Paste API Key',
      detail: 'Paste the key from your provider account. It will be stored as a secret.',
      error: 'Add the API key before saving this provider.',
      fieldId: apiKeyInputId,
    }
  }

  if (needsBaseUrl && !form.baseUrl.trim()) {
    return {
      ready: false,
      title: 'Next: Add Base URL',
      detail: 'Paste the HTTPS endpoint for your OpenAI-compatible service.',
      error: 'Add the Base URL before saving this provider.',
      fieldId: baseUrlInputId,
    }
  }

  return {
    ready: true,
    title: 'Ready to Save',
    detail: 'Save this provider, then run Test so agents can use it safely.',
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
// Segmented toggle (Plan / Region)
// ============================================================================

interface SegmentedToggleProps<T extends string> {
  label: string
  value: T
  options: { value: T; label: string }[]
  onChange: (value: T) => void
}

function SegmentedToggle<T extends string>({
  label,
  value,
  options,
  onChange,
}: SegmentedToggleProps<T>) {
  return (
    <div>
      <span className={uiStyles.label}>{label}</span>
      <div className="inline-flex gap-1" role="group" aria-label={label}>
        {options.map((option) => (
          <button
            key={option.value}
            type="button"
            aria-pressed={value === option.value}
            onClick={() => onChange(option.value)}
            className={cn(
              'inline-flex h-8 items-center rounded-full px-3 text-ui-caption font-semibold transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
              value === option.value
                ? 'bg-apple-blue text-white'
                : 'border border-black/[0.08] bg-white text-secondary-light hover:bg-black/[0.03] hover:text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark dark:hover:bg-white/[0.08] dark:hover:text-foreground-dark'
            )}
          >
            {option.label}
          </button>
        ))}
      </div>
    </div>
  )
}

// ============================================================================
// Catalog: vendor grid + inline minimal config
// ============================================================================

type PlanVariant = 'api' | 'coding'
type RegionVariant = 'cn' | 'global'

interface CatalogConfigPanelProps {
  vendor: CatalogVendor
  onSave: (input: CreateProviderInput) => Promise<void>
  onCancel: () => void
  saving: boolean
}

function CatalogConfigPanel({ vendor, onSave, onCancel, saving }: CatalogConfigPanelProps) {
  const [plan, setPlan] = useState<PlanVariant>(vendor.api ? 'api' : 'coding')
  const [region, setRegion] = useState<RegionVariant>('cn')
  const [apiKey, setApiKey] = useState('')
  const [model, setModel] = useState('')
  const [submitAttempted, setSubmitAttempted] = useState(false)

  const variant = plan === 'coding' ? (vendor.coding ?? vendor.api) : (vendor.api ?? vendor.coding)
  const hasPlanToggle = Boolean(vendor.api && vendor.coding)
  const hasRegionToggle = Boolean(variant?.globalBaseUrl)
  const needsApiKey = variant ? providerNeedsApiKey(variant.provider, variant) : true
  const allowCustomModels = variant?.allowCustomModels ?? true
  const models = variant?.models ?? []

  const modelListId = `catalog-models-${vendor.key}`
  const apiKeyInputId = 'provider-form-api-key'
  const modelInputId = 'provider-form-model'

  // Reset the inline config whenever the operator picks a different variant so
  // the prefilled model always matches the selected Plan.
  useEffect(() => {
    setModel(variant?.defaultModel ?? variant?.models[0]?.model ?? '')
    setSubmitAttempted(false)
  }, [variant])

  const trimmedModel = model.trim()
  const missingModel = !trimmedModel
  const missingApiKey = needsApiKey && !apiKey.trim()
  const ready = Boolean(variant) && !missingModel && !missingApiKey
  const statusTitle = missingModel
    ? 'Next: Add Model'
    : missingApiKey
      ? 'Next: Paste API Key'
      : 'Ready to Save'

  const modelError = submitAttempted && missingModel
  const apiKeyError = submitAttempted && !missingModel && missingApiKey
  const formStatusId = 'provider-form-status'

  function resolveBaseUrl(): string | undefined {
    if (!variant) return undefined
    if (region === 'global' && variant.globalBaseUrl) return variant.globalBaseUrl
    // Catalog vendors carry their endpoint as defaultBaseUrl; leaving it
    // undefined lets the backend default apply for vendors with no override.
    return variant.defaultBaseUrl
  }

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    setSubmitAttempted(true)
    if (!variant) return
    if (missingModel) {
      document.getElementById(modelInputId)?.focus()
      return
    }
    if (missingApiKey) {
      document.getElementById(apiKeyInputId)?.focus()
      return
    }
    await onSave({
      provider: variant.provider,
      displayName: variant.displayName,
      model: trimmedModel,
      apiKey: apiKey.trim() || undefined,
      baseUrl: resolveBaseUrl(),
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
      <div className="mb-3 flex items-center justify-between gap-2">
        <div className="min-w-0">
          <p className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            {vendor.displayName}
          </p>
          <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
            Base URL and model are derived from this vendor. Paste your key and save.
          </p>
        </div>
        <button
          type="button"
          onClick={onCancel}
          className="text-ui-caption font-medium text-apple-blue hover:underline"
        >
          Back to catalog
        </button>
      </div>

      {(hasPlanToggle || hasRegionToggle) && (
        <div className="mb-3 flex flex-wrap gap-4">
          {hasPlanToggle && (
            <SegmentedToggle<PlanVariant>
              label="Plan"
              value={plan}
              onChange={setPlan}
              options={[
                { value: 'api', label: 'API' },
                { value: 'coding', label: 'Coding Plan' },
              ]}
            />
          )}
          {hasRegionToggle && (
            <SegmentedToggle<RegionVariant>
              label="Region"
              value={region}
              onChange={setRegion}
              options={[
                { value: 'cn', label: 'CN' },
                { value: 'global', label: 'Global' },
              ]}
            />
          )}
        </div>
      )}

      <div className="mb-3 grid grid-cols-1 gap-3 sm:grid-cols-2">
        {/* Model */}
        <div>
          <label htmlFor={modelInputId} className={uiStyles.label}>
            Model
          </label>
          {allowCustomModels ? (
            <>
              <input
                id={modelInputId}
                type="text"
                name="model"
                value={model}
                onChange={(e) => setModel(e.target.value)}
                placeholder={variant?.defaultModel ?? 'e.g. model name…'}
                list={models.length > 0 ? modelListId : undefined}
                autoComplete="off"
                aria-invalid={modelError}
                aria-describedby={formStatusId}
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
          ) : (
            <select
              id={modelInputId}
              name="model"
              value={model}
              onChange={(e) => setModel(e.target.value)}
              aria-invalid={modelError}
              aria-describedby={formStatusId}
              className={cn(uiStyles.select, 'w-full')}
            >
              {models.map((m) => (
                <option key={m.model} value={m.model}>
                  {m.displayName}
                </option>
              ))}
            </select>
          )}
          {modelError && (
            <p className="mt-1 text-ui-caption text-apple-red">
              Add a model before saving this provider.
            </p>
          )}
        </div>

        {/* API Key */}
        {needsApiKey ? (
          <div>
            <label htmlFor={apiKeyInputId} className={uiStyles.label}>
              API Key <span className="text-red-500">*</span>
            </label>
            <input
              id={apiKeyInputId}
              type="password"
              name="apiKey"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="sk-…"
              autoComplete="off"
              spellCheck={false}
              aria-invalid={apiKeyError}
              aria-describedby={formStatusId}
              className={uiStyles.input}
            />
            {apiKeyError && (
              <p className="mt-1 text-ui-caption text-apple-red">
                Add the API key before saving this provider.
              </p>
            )}
          </div>
        ) : (
          <div className={uiStyles.note}>
            This vendor runs locally and needs no API key. Save to add it.
          </div>
        )}
      </div>

      <div className="flex items-center justify-between gap-2">
        <p
          id={formStatusId}
          data-testid="provider-form-status"
          className="text-ui-caption text-secondary-light dark:text-secondary-dark"
        >
          {statusTitle}
        </p>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={onCancel}
            disabled={saving}
            className={uiStyles.secondaryButton}
          >
            Cancel
          </button>
          <button type="submit" disabled={saving || !ready} className={uiStyles.primaryButton}>
            {saving ? 'Saving…' : 'Save Provider'}
          </button>
        </div>
      </div>
    </form>
  )
}

interface CatalogGridProps {
  vendors: CatalogVendor[]
  configuredProviders: LlmProviderConfig[]
  selectedVendorKey: string | null
  onSelect: (key: string) => void
}

function CatalogGrid({
  vendors,
  configuredProviders,
  selectedVendorKey,
  onSelect,
}: CatalogGridProps) {
  // A vendor counts as configured when either its API or Coding Plan key exists.
  const configuredKeys = useMemo(() => {
    const keys = new Set<string>()
    for (const config of configuredProviders) {
      keys.add(config.provider.replace(/_coding$/, ''))
    }
    return keys
  }, [configuredProviders])

  return (
    <div
      role="group"
      aria-label="Built-in provider catalog"
      className="grid gap-2 p-4 sm:grid-cols-2 lg:grid-cols-3"
    >
      {vendors.map((vendor) => {
        const selected = selectedVendorKey === vendor.key
        const configured = configuredKeys.has(vendor.key)
        return (
          <button
            key={vendor.key}
            type="button"
            aria-pressed={selected}
            onClick={() => onSelect(vendor.key)}
            className={cn(
              'flex min-h-14 flex-col items-start gap-1 rounded-lg border px-3 py-2.5 text-left transition-colors',
              selected
                ? 'border-apple-blue/40 bg-apple-blue/10'
                : 'border-black/[0.08] bg-white hover:bg-black/[0.03] dark:border-white/[0.1] dark:bg-white/[0.04] dark:hover:bg-white/[0.08]'
            )}
          >
            <span className="flex w-full items-center justify-between gap-2">
              <span className="truncate text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
                {vendor.displayName}
              </span>
              {configured && (
                <span className="shrink-0 rounded-full bg-apple-green/10 px-2 py-0.5 text-ui-caption font-medium text-apple-green">
                  Configured
                </span>
              )}
            </span>
            <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
              {vendor.coding ? 'API · Coding Plan' : 'API'}
              {vendor.api?.globalBaseUrl || vendor.coding?.globalBaseUrl ? ' · CN/Global' : ''}
            </span>
          </button>
        )
      })}
    </div>
  )
}

// ============================================================================
// Custom / Gateway form (bring-your-own endpoint)
// ============================================================================

interface AddProviderFormProps {
  gatewayProviders: ProviderInfo[]
  onSave: (input: CreateProviderInput) => Promise<void>
  onCancel: () => void
  saving: boolean
}

function buildGatewayDefaultForm(providers: ProviderInfo[]): AddProviderForm {
  const first = providers[0]
  if (!first) return DEFAULT_FORM
  return {
    provider: first.provider,
    displayName: first.displayName,
    model: first.defaultModel ?? first.models[0]?.model ?? '',
    apiKey: '',
    baseUrl: '',
  }
}

function AddProviderFormPanel({
  gatewayProviders,
  onSave,
  onCancel,
  saving,
}: AddProviderFormProps) {
  const providerOptions = gatewayProviders
  const [form, setForm] = useState<AddProviderForm>(() => buildGatewayDefaultForm(providerOptions))
  const [submitAttempted, setSubmitAttempted] = useState(false)

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
      provider,
      displayName: info?.displayName ?? '',
      model: info?.defaultModel ?? info?.models[0]?.model ?? '',
      apiKey: '',
      baseUrl: '',
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
          Custom / Gateway setup path
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
                placeholder={selectedProvider?.defaultModel ?? 'e.g. gpt-4o-mini…'}
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
                placeholder="e.g. gpt-4o-mini…"
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
          <p
            id={apiKeyHelpId}
            className="mb-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            Paste the secret key from your provider account. It is hidden after saving.
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

        {/* Base URL */}
        <div className="sm:col-span-2">
          <label htmlFor={baseUrlInputId} className={uiStyles.label}>
            Base URL {needsBaseUrl && <span className="text-red-500">*</span>}
          </label>
          <p
            id={baseUrlHelpId}
            className="mb-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            Paste the HTTPS endpoint for your OpenAI-compatible service or gateway. Leave blank to
            use the gateway default.
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

      <div className="flex items-center justify-between gap-2">
        <p
          id={formStatusId}
          data-testid="provider-form-status"
          className="text-ui-caption text-secondary-light dark:text-secondary-dark"
        >
          {readiness.title}
        </p>
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
            {saving ? 'Saving…' : 'Save Provider'}
          </button>
        </div>
      </div>
    </form>
  )
}

// ============================================================================
// Add Provider panel (catalog ⇄ custom)
// ============================================================================

interface AddProviderPanelProps {
  vendors: CatalogVendor[]
  gatewayProviders: ProviderInfo[]
  configuredProviders: LlmProviderConfig[]
  onSave: (input: CreateProviderInput) => Promise<void>
  onClose: () => void
  saving: boolean
}

function AddProviderPanel({
  vendors,
  gatewayProviders,
  configuredProviders,
  onSave,
  onClose,
  saving,
}: AddProviderPanelProps) {
  const [bucket, setBucket] = useState<AddBucket>('catalog')
  const [selectedVendorKey, setSelectedVendorKey] = useState<string | null>(null)
  const selectedVendor = vendors.find((vendor) => vendor.key === selectedVendorKey) ?? null

  return (
    <div className="border-t border-black/[0.06] dark:border-white/[0.08]">
      <div className="flex flex-col gap-3 px-4 pt-4 sm:flex-row sm:items-center sm:justify-between">
        <div
          role="group"
          aria-label="Add provider method"
          className="inline-flex rounded-full bg-black/[0.04] p-0.5 dark:bg-white/[0.06]"
        >
          <button
            type="button"
            aria-pressed={bucket === 'catalog'}
            onClick={() => setBucket('catalog')}
            className={cn(
              'inline-flex h-8 items-center gap-1.5 rounded-full px-3 text-ui-caption font-semibold transition-colors',
              bucket === 'catalog'
                ? 'bg-white text-foreground-light shadow-sm dark:bg-white/[0.12] dark:text-foreground-dark'
                : 'text-secondary-light dark:text-secondary-dark'
            )}
          >
            <Plus size={13} strokeWidth={2.25} aria-hidden="true" />
            Built-in catalog
          </button>
          <button
            type="button"
            aria-pressed={bucket === 'custom'}
            onClick={() => setBucket('custom')}
            className={cn(
              'inline-flex h-8 items-center gap-1.5 rounded-full px-3 text-ui-caption font-semibold transition-colors',
              bucket === 'custom'
                ? 'bg-white text-foreground-light shadow-sm dark:bg-white/[0.12] dark:text-foreground-dark'
                : 'text-secondary-light dark:text-secondary-dark'
            )}
          >
            <SlidersHorizontal size={13} strokeWidth={2.25} aria-hidden="true" />
            Custom / Gateway
          </button>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="self-start text-ui-caption font-medium text-secondary-light hover:text-foreground-light dark:text-secondary-dark dark:hover:text-foreground-dark"
        >
          Close
        </button>
      </div>

      {bucket === 'catalog' ? (
        selectedVendor ? (
          <CatalogConfigPanel
            vendor={selectedVendor}
            onSave={onSave}
            onCancel={() => setSelectedVendorKey(null)}
            saving={saving}
          />
        ) : (
          <>
            <p className="px-4 pt-3 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Pick a vendor for a minimal setup — base URL and model are filled in for you. Need a
              private endpoint? Switch to Custom / Gateway.
            </p>
            <CatalogGrid
              vendors={vendors}
              configuredProviders={configuredProviders}
              selectedVendorKey={selectedVendorKey}
              onSelect={setSelectedVendorKey}
            />
          </>
        )
      ) : (
        <AddProviderFormPanel
          gatewayProviders={gatewayProviders}
          onSave={onSave}
          onCancel={onClose}
          saving={saving}
        />
      )}
    </div>
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
  const { vendors, gateways } = useMemo(() => {
    const source = supportedProviders.length > 0 ? supportedProviders : FALLBACK_SUPPORTED_PROVIDERS
    return deriveCatalog(source)
  }, [supportedProviders])
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
    // Load supported providers for the catalog. The form falls back to the
    // hardcoded list if the request fails.
    void getSettingsApi()
      .getSupportedProviders()
      .then(setSupportedProviders)
      .catch(() => {
        // Non-critical: catalog falls back to FALLBACK_SUPPORTED_PROVIDERS.
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

        {/* Add panel: built-in catalog ⇄ custom gateway */}
        {showForm && (
          <AddProviderPanel
            vendors={vendors}
            gatewayProviders={gateways}
            configuredProviders={providers}
            onSave={handleSave}
            onClose={() => setShowForm(false)}
            saving={saving}
          />
        )}
      </div>
    </div>
  )
}
