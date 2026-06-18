import { useEffect, useMemo, useState, type FormEvent } from 'react'
import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  Plus,
  Power,
  Search,
  SlidersHorizontal,
  Trash2,
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
type ProviderNextAction = 'add-provider' | 'show-needs-test' | 'show-disabled'
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

interface ProviderFilterEmptyState {
  title: string
  detail: string
}

/**
 * A built-in catalog vendor. The `xxx` (standard setup) and `xxx_coding`
 * provider keys are collapsed into one card so operators pick the vendor once,
 * then choose the service plan and address region instead of scrolling a
 * 25-entry dropdown.
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
  { id: 'needs-test', label: 'Needs check' },
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
  { label: 'Choose AI account', value: 'Pick the service your team already pays for or runs.' },
  {
    label: 'Paste the service access key',
    value: 'Open that account, copy the service access key, and paste it here.',
  },
  {
    label: 'Save, then check connection',
    value: 'After saving, choose Check connection. Ready means agents can use this service.',
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
        'Open your AI service account, copy the service access key, and paste it here. Some services call this an API key. Forge hides it after saving.',
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
      title: 'Turn on or replace an AI service',
      detail:
        'All saved AI services are disabled. Show the disabled list, turn on one service if this account should still be used, then click Check. Add a new service only if none of these accounts should be used.',
      success: 'At least 1 enabled AI service is checked and marked Ready.',
      ready: false,
      action: 'show-disabled',
      actionLabel: 'Show disabled services',
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
  onSetEnabled: (id: string, isEnabled: boolean) => Promise<LlmProviderConfig | null>
  onDelete: (id: string) => Promise<boolean>
}

function ProviderCard({ providerConfig, onTest, onSetEnabled, onDelete }: ProviderCardProps) {
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
  const [deleting, setDeleting] = useState(false)
  const [testing, setTesting] = useState(false)
  const [changingEnabled, setChangingEnabled] = useState(false)
  const [testResult, setTestResult] = useState<{ ok: boolean; message: string } | null>(null)
  const persistedTestResult =
    lastTestStatus === 'passed'
      ? { ok: true, message: 'Connection ready' }
      : lastTestStatus === 'failed'
        ? { ok: false, message: providerTestErrorMessage(lastTestErrorMessage, displayName) }
        : null
  const visibleTestResult = testResult ?? persistedTestResult

  async function handleDelete() {
    if (!confirming) {
      setConfirming(true)
      return
    }
    setDeleting(true)
    try {
      const removed = await onDelete(id)
      if (removed) {
        setConfirming(false)
      }
    } finally {
      setDeleting(false)
    }
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

  async function handleSetEnabled() {
    const nextEnabled = !isEnabled
    setChangingEnabled(true)
    setTestResult(null)
    try {
      await onSetEnabled(id, nextEnabled)
    } finally {
      setChangingEnabled(false)
    }
  }

  return (
    <div className={cn('px-4 py-3', uiStyles.row)}>
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
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
            onClick={handleSetEnabled}
            disabled={changingEnabled || testing || deleting}
            className={uiStyles.secondaryButton}
            aria-label={`${isEnabled ? 'Turn off' : 'Turn on'} ${displayName} AI service`}
            title={isEnabled ? 'Turn off AI service' : 'Turn on AI service'}
          >
            <Power className="h-4 w-4" aria-hidden="true" />
            <span>{changingEnabled ? 'Updating' : isEnabled ? 'Turn off' : 'Turn on'}</span>
          </button>
          <button
            type="button"
            onClick={handleTest}
            disabled={testing || !isEnabled || deleting}
            className={uiStyles.secondaryButton}
            aria-label={`Check ${displayName} AI service connection`}
            title="Check AI service connection"
          >
            <Activity className="h-4 w-4" aria-hidden="true" />
            <span>{testing ? 'Checking' : 'Check'}</span>
          </button>
          {confirming && (
            <button
              type="button"
              onClick={() => setConfirming(false)}
              disabled={deleting}
              className={uiStyles.secondaryButton}
              aria-label={`Keep ${displayName} AI service`}
            >
              Keep service
            </button>
          )}
          <button
            type="button"
            onClick={() => void handleDelete()}
            disabled={deleting}
            aria-label={
              confirming
                ? `Remove ${displayName} AI service now`
                : `Remove ${displayName} AI service`
            }
            aria-describedby={confirming ? `${id}-remove-help` : undefined}
            className={cn(
              'shrink-0 gap-1.5',
              confirming ? uiStyles.dangerConfirmButton : uiStyles.dangerButton
            )}
          >
            <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
            {deleting ? 'Removing...' : confirming ? 'Remove now' : 'Remove AI service'}
          </button>
        </div>
      </div>
      {confirming && (
        <div
          id={`${id}-remove-help`}
          role="note"
          className="mt-3 flex gap-2 rounded-lg border border-apple-red/20 bg-apple-red/10 px-3 py-2 text-ui-caption text-apple-red"
        >
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
          <span>
            Removing this service stops agents from using {displayName}. Keep it if any current
            agent still depends on this AI service.
          </span>
        </div>
      )}
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
              {allReady ? 'AI services ready for agent creation' : 'Finish AI service setup'}
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
// Segmented toggle
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
    ? 'Next: add model'
    : missingApiKey
      ? 'Next: paste the service access key'
      : 'Ready to save this service'

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
            Forge fills in the service website address and model for you. Paste the service access
            key and save. After saving, choose Check connection. Ready means simple chat agents can
            use this service.
          </p>
        </div>
        <button
          type="button"
          onClick={onCancel}
          className="text-ui-caption font-medium text-apple-blue hover:underline"
        >
          Back to service list
        </button>
      </div>

      {(hasPlanToggle || hasRegionToggle) && (
        <div className="mb-3 flex flex-wrap gap-4">
          {hasPlanToggle && (
            <SegmentedToggle<PlanVariant>
              label="Service plan"
              value={plan}
              onChange={setPlan}
              options={[
                { value: 'api', label: 'Standard' },
                { value: 'coding', label: 'Coding Plan' },
              ]}
            />
          )}
          {hasRegionToggle && (
            <SegmentedToggle<RegionVariant>
              label="Service website region"
              value={region}
              onChange={setRegion}
              options={[
                { value: 'cn', label: 'China' },
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
            Model to use
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
              Add a model before saving this AI service.
            </p>
          )}
        </div>

        {/* API Key */}
        {needsApiKey ? (
          <div>
            <label htmlFor={apiKeyInputId} className={uiStyles.label}>
              Service access key <span className="text-red-500">*</span>
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
                Paste the service access key before saving this AI service.
              </p>
            )}
          </div>
        ) : (
          <div className={uiStyles.note}>
            This AI service runs locally and needs no access key. Save to add it.
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
            {saving ? 'Saving…' : 'Save AI service'}
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
  // A vendor counts as configured when either service-plan key exists.
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
      aria-label="Known AI services"
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
              {vendor.coding ? 'Standard setup · Coding plan' : 'Standard setup'}
              {vendor.api?.globalBaseUrl || vendor.coding?.globalBaseUrl
                ? ' · China or global website address'
                : ''}
            </span>
          </button>
        )
      })}
    </div>
  )
}

// ============================================================================
// Custom service address form
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
                placeholder={selectedProvider?.defaultModel ?? 'e.g. gpt-4o-mini…'}
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
                placeholder="e.g. gpt-4o-mini…"
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
          aria-label="Choose how to add an AI service"
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
            Known AI services
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
            Custom service address
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
              Pick a known AI service. Forge fills in the service address and model for you. If your
              setup guide gives you a private address, choose Custom service address.
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
    setProviderEnabled,
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
  const filterEmptyState = providerFilterEmptyState(providerFilter, providerSearch)

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
    return deleteProvider(id)
  }

  async function handleTest(id: string) {
    const result = await getSettingsApi().testProvider(id)
    await loadProviders()
    return result
  }

  async function handleSetProviderEnabled(id: string, isEnabled: boolean) {
    const provider = await setProviderEnabled(id, isEnabled)
    if (provider && isEnabled) {
      setProviderSearch('')
      setProviderFilter('needs-test')
    }
    return provider
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
      return
    }
    if (action === 'show-disabled') {
      setProviderSearch('')
      setProviderFilter('disabled')
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
              onSetEnabled={handleSetProviderEnabled}
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
