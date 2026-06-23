import { useEffect, useId, useMemo, useRef, useState } from 'react'
import { CheckCircle2, Circle, RotateCcw, Search, SlidersHorizontal, Wrench } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { BeginnerLoadingState } from '@app/shared/ui/BeginnerLoadingState'
import { agentPluginErrorMessage } from './model/pluginErrorMessage'

interface AgentPluginRow {
  pluginId: string
  name: string
  version: string
  description?: string
  pluginEnabled: boolean
  /** Per-agent override; null/undefined means "follow plugin default". */
  enabled: boolean | null
  config?: Record<string, unknown> | null
}

interface PluginItem {
  id: string
  name: string
  version: string
  description: string
  /** Default plugin state before the agent-level override is applied. */
  defaultEnabled: boolean
  /** Whether this agent has an explicit override for the plugin. */
  hasOverride: boolean
  /** Effective enabled state (override if present, else plugin default). */
  enabled: boolean
  /** Whether the row is currently in flight (toggle disabled). */
  saving: boolean
}

type PluginFilter = 'all' | 'enabled' | 'disabled' | 'overridden'

interface PluginSummary {
  total: number
  enabled: number
  disabled: number
  overridden: number
}

interface EmptyStateCopy {
  title: string
  detail: string
  steps?: string[]
  success?: string
}

const PLUGIN_FILTERS: { value: PluginFilter; label: string; ariaLabel: string }[] = [
  { value: 'all', label: 'All', ariaLabel: 'Show all tools for this agent' },
  { value: 'enabled', label: 'Can use', ariaLabel: 'Show tools this agent can use' },
  { value: 'disabled', label: 'Turned off', ariaLabel: 'Show tools turned off for this agent' },
  {
    value: 'overridden',
    label: 'Changed here',
    ariaLabel: 'Show tools changed only for this agent',
  },
]

function authHeaders(): Record<string, string> {
  const token = typeof window !== 'undefined' ? localStorage.getItem('af:auth:access') : null
  return token ? { Authorization: `Bearer ${token}` } : {}
}

function effectiveEnabled(row: AgentPluginRow): boolean {
  return row.enabled ?? row.pluginEnabled
}

function summarizePlugins(plugins: PluginItem[]): PluginSummary {
  return plugins.reduce(
    (summary, plugin) => {
      summary.total += 1
      if (plugin.enabled) summary.enabled += 1
      else summary.disabled += 1
      if (plugin.hasOverride) summary.overridden += 1
      return summary
    },
    { total: 0, enabled: 0, disabled: 0, overridden: 0 }
  )
}

function countPluginsByFilter(summary: PluginSummary, filter: PluginFilter): number {
  switch (filter) {
    case 'enabled':
      return summary.enabled
    case 'disabled':
      return summary.disabled
    case 'overridden':
      return summary.overridden
    default:
      return summary.total
  }
}

function agentPluginEmptyState(): EmptyStateCopy {
  return {
    title: 'Ask an owner or admin to add tools',
    detail:
      'Tools give agents extra abilities. After tools are added, return here to choose which ones this agent can use.',
    steps: [
      'Open Settings.',
      'Ask an owner or admin to add one tool for this team.',
      'Come back here after tools are added.',
    ],
    success: 'Success looks like a tool listed with Can use now or Turned off for this agent.',
  }
}

function agentPluginFilterEmptyState(filter: PluginFilter, query: string): EmptyStateCopy {
  const hasSearch = query.trim().length > 0
  const hasFilter = filter !== 'all'

  if (hasSearch && hasFilter) {
    return {
      title: 'Search and filter are hiding tools',
      detail: 'Use Show all tools before assuming this agent has no matching tool.',
    }
  }

  if (hasSearch) {
    return {
      title: 'Search is hiding tools',
      detail: 'Use Show all tools to return to the full list.',
    }
  }

  return {
    title: 'Filter is hiding tools',
    detail: 'Use Show all tools to return to the full list.',
  }
}

function filterPlugins(plugins: PluginItem[], filter: PluginFilter, query: string): PluginItem[] {
  const normalized = query.trim().toLowerCase()
  return plugins.filter((plugin) => {
    const matchesFilter =
      filter === 'all' ||
      (filter === 'enabled' && plugin.enabled) ||
      (filter === 'disabled' && !plugin.enabled) ||
      (filter === 'overridden' && plugin.hasOverride)
    if (!matchesFilter) return false
    if (!normalized) return true
    return [plugin.name, plugin.description].join(' ').toLowerCase().includes(normalized)
  })
}

export function pluginSettingNote(
  plugin: Pick<PluginItem, 'defaultEnabled' | 'hasOverride'>
): string {
  const teamSetting = plugin.defaultEnabled
    ? 'normally available for agents'
    : 'normally off for agents'
  return plugin.hasOverride
    ? `Changed for this agent - ${teamSetting}`
    : `Using team setting - ${teamSetting}`
}

function toolDescription(plugin: Pick<PluginItem, 'description'>): string {
  const description = plugin.description.trim()
  return (
    description ||
    'Tool summary is missing. Keep the team setting until an owner explains what this tool lets the agent do.'
  )
}

interface AgentPluginsTabProps {
  agentId: string
  onBackToAgents?: () => void
}

export function AgentPluginsTab({ agentId, onBackToAgents }: AgentPluginsTabProps) {
  const [plugins, setPlugins] = useState<PluginItem[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const [actionErrorAttempt, setActionErrorAttempt] = useState(0)
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState<PluginFilter>('all')
  const actionErrorRef = useRef<HTMLDivElement>(null)
  const searchHelpId = useId()

  const summary = useMemo(() => summarizePlugins(plugins), [plugins])
  const visiblePlugins = useMemo(
    () => filterPlugins(plugins, filter, query),
    [plugins, filter, query]
  )
  const emptyPlugins = agentPluginEmptyState()
  const filteredEmpty = agentPluginFilterEmptyState(filter, query)

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError(null)
    setActionError(null)

    async function load() {
      try {
        const res = await fetch(`/api/v1/agents/${encodeURIComponent(agentId)}/plugins`, {
          headers: authHeaders(),
        })
        if (!res.ok) throw new Error(`HTTP ${res.status}`)
        const data = (await res.json()) as { ok: boolean; plugins: AgentPluginRow[] }
        if (!data.ok) throw new Error('server returned ok: false')
        if (!cancelled) {
          setPlugins(
            (data.plugins ?? []).map((row) => ({
              id: row.pluginId,
              name: row.name,
              version: row.version,
              description: row.description ?? '',
              defaultEnabled: row.pluginEnabled,
              hasOverride: row.enabled != null,
              enabled: effectiveEnabled(row),
              saving: false,
            }))
          )
        }
      } catch (err) {
        if (!cancelled) setError(agentPluginErrorMessage('load', err))
      } finally {
        if (!cancelled) setLoading(false)
      }
    }

    void load()
    return () => {
      cancelled = true
    }
  }, [agentId])

  // Tool toggles sit below the summary and filter controls. Bring repeated
  // failures back into view so users can see why the switch returned.
  useEffect(() => {
    if (actionError)
      actionErrorRef.current?.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
  }, [actionError, actionErrorAttempt])

  async function toggle(plugin: PluginItem) {
    const next = !plugin.enabled
    setActionError(null)
    // Optimistic update with saving guard
    setPlugins((prev) =>
      prev.map((p) => (p.id === plugin.id ? { ...p, enabled: next, saving: true } : p))
    )
    try {
      const res = await fetch(
        `/api/v1/agents/${encodeURIComponent(agentId)}/plugins/${encodeURIComponent(plugin.id)}`,
        {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json', ...authHeaders() },
          body: JSON.stringify({ enabled: next }),
        }
      )
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      setPlugins((prev) => prev.map((p) => (p.id === plugin.id ? { ...p, saving: false } : p)))
    } catch (err) {
      // Revert on failure so the UI stays consistent with the server.
      setPlugins((prev) =>
        prev.map((p) => (p.id === plugin.id ? { ...p, enabled: !next, saving: false } : p))
      )
      setActionError(agentPluginErrorMessage('save', err))
      setActionErrorAttempt((current) => current + 1)
    }
  }

  if (loading) {
    return (
      <BeginnerLoadingState
        title="Checking this agent's tools"
        detail="Forge is checking which tools this agent can use for its next task."
        nextStep="If this takes more than a moment, open Tools again or ask an owner or admin to check tool access."
        success="Success looks like available tools or an ask-an-owner step."
        testId="agent-tools-loading"
        framed={false}
        compact
      />
    )
  }

  if (error) {
    return (
      <div
        role="alert"
        aria-live="polite"
        className="flex flex-col items-center justify-center py-8 text-center"
      >
        <p className="text-ui-body font-medium text-apple-red">Open Tools again from Agents</p>
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {error}
        </p>
        {onBackToAgents ? (
          <button
            type="button"
            onClick={onBackToAgents}
            className={cn(
              'mt-4 rounded-full px-4 py-2 text-ui-button font-medium',
              'bg-apple-blue text-white transition-colors hover:bg-apple-blue/90'
            )}
          >
            Back to Agents
          </button>
        ) : null}
      </div>
    )
  }

  if (plugins.length === 0) {
    return (
      <div
        data-testid="agent-plugin-empty"
        className="flex flex-col items-center justify-center rounded-card border border-dashed border-black/[0.1] bg-black/[0.02] px-4 py-8 text-center dark:border-white/[0.12] dark:bg-white/[0.03]"
      >
        <Wrench
          size={18}
          strokeWidth={2}
          className="text-secondary-light dark:text-secondary-dark"
        />
        <p className="mt-2 text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
          {emptyPlugins.title}
        </p>
        <p className="mt-1 max-w-xl text-ui-caption text-secondary-light dark:text-secondary-dark">
          {emptyPlugins.detail}
        </p>
        {emptyPlugins.steps ? (
          <ol className="mt-4 grid max-w-xl list-decimal gap-1 pl-5 text-left text-ui-caption text-secondary-light dark:text-secondary-dark">
            {emptyPlugins.steps.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ol>
        ) : null}
        {emptyPlugins.success ? (
          <p className="mt-3 max-w-xl text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
            {emptyPlugins.success}
          </p>
        ) : null}
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-4">
      <section data-testid="agent-plugin-readiness" className="space-y-4">
        <div className="rounded-card border border-black/[0.08] bg-white px-4 py-3 dark:border-white/[0.1] dark:bg-[#2a2a2c]">
          <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
            Agent tools
          </p>
          <h3 className="mt-1 text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            What this agent can use
          </h3>
          <p className="mt-1 max-w-2xl text-ui-caption text-secondary-light dark:text-secondary-dark">
            Tools are extra abilities. Only turn on tools this agent needs for its next tasks. If
            you are not sure, keep the team setting and ask an owner before changing access.
          </p>
        </div>

        <div className="grid grid-cols-2 gap-2 lg:grid-cols-4">
          <PluginMetric
            testId="agent-plugin-metric-enabled"
            label="Can use now"
            value={summary.enabled}
            tone="success"
          />
          <PluginMetric
            testId="agent-plugin-metric-disabled"
            label="Turned off"
            value={summary.disabled}
            tone="muted"
          />
          <PluginMetric
            testId="agent-plugin-metric-overrides"
            label="Changed here"
            value={summary.overridden}
            tone="attention"
          />
          <PluginMetric
            testId="agent-plugin-metric-total"
            label="Installed tools"
            value={summary.total}
            tone="default"
          />
        </div>

        <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
          <label className="relative min-w-0 flex-1">
            <span className="sr-only">Search this agent's tools</span>
            <Search
              size={15}
              strokeWidth={2}
              className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-secondary-light dark:text-secondary-dark"
              aria-hidden="true"
            />
            <input
              data-testid="agent-plugin-search"
              aria-describedby={searchHelpId}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search by tool name or what it does"
              className="h-9 w-full rounded-lg border border-black/[0.08] bg-white pl-9 pr-3 text-ui-body text-foreground-light outline-none transition-colors placeholder:text-secondary-light/75 focus:border-apple-blue/45 focus:ring-2 focus:ring-apple-blue/15 dark:border-white/[0.1] dark:bg-[#2a2a2c] dark:text-foreground-dark dark:placeholder:text-secondary-dark/75"
            />
          </label>
          <p
            id={searchHelpId}
            className="text-ui-caption text-secondary-light dark:text-secondary-dark lg:max-w-[16rem]"
          >
            Search only filters this agent&apos;s tools. Use Show all tools to return to the full
            list.
          </p>

          <div
            data-testid="agent-plugin-filter"
            role="group"
            aria-label="Tool filter"
            className="inline-flex h-9 items-center gap-1 rounded-lg border border-black/[0.08] bg-black/[0.025] p-1 dark:border-white/[0.1] dark:bg-white/[0.04]"
          >
            {PLUGIN_FILTERS.map((option) => (
              <PluginFilterButton
                key={option.value}
                active={filter === option.value}
                label={option.label}
                ariaLabel={option.ariaLabel}
                count={countPluginsByFilter(summary, option.value)}
                onClick={() => setFilter(option.value)}
              />
            ))}
          </div>
        </div>

        <div className="flex items-center gap-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
          <SlidersHorizontal size={14} strokeWidth={2} aria-hidden="true" />
          <span>
            Showing {visiblePlugins.length} of {summary.total} tools
          </span>
          <span className="hidden sm:inline" aria-hidden="true">
            ·
          </span>
          <span className="hidden sm:inline">
            Saved changes apply to this agent&apos;s next task.
          </span>
        </div>
      </section>

      {actionError ? (
        <div
          ref={actionErrorRef}
          role="alert"
          aria-live="polite"
          className="rounded-card border border-apple-red/25 bg-apple-red/[0.06] px-4 py-3 text-ui-caption text-apple-red"
        >
          {actionError}
        </div>
      ) : null}

      {visiblePlugins.length === 0 ? (
        <div
          role="status"
          aria-live="polite"
          data-testid="agent-plugin-filter-empty"
          className="flex flex-col items-center justify-center rounded-card border border-dashed border-black/[0.1] bg-black/[0.02] px-4 py-8 text-center dark:border-white/[0.12] dark:bg-white/[0.03]"
        >
          <Wrench
            size={18}
            strokeWidth={2}
            className="text-secondary-light dark:text-secondary-dark"
          />
          <p className="mt-2 text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
            {filteredEmpty.title}
          </p>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {filteredEmpty.detail}
          </p>
          <button
            type="button"
            onClick={() => {
              setQuery('')
              setFilter('all')
            }}
            className="mt-3 inline-flex h-8 items-center gap-2 rounded-lg border border-black/[0.08] bg-white px-3 text-ui-caption font-medium text-foreground-light transition-colors hover:border-apple-blue/35 hover:text-apple-blue dark:border-white/[0.1] dark:bg-white/[0.05] dark:text-foreground-dark"
          >
            <RotateCcw size={13} strokeWidth={2} aria-hidden="true" />
            Show all tools
          </button>
        </div>
      ) : (
        visiblePlugins.map((plugin) => (
          <div
            key={plugin.id}
            data-testid={`plugin-row-${plugin.id}`}
            className={cn(
              'flex items-center justify-between gap-4',
              'rounded-card border border-black/[0.08] bg-white px-4 py-3 dark:border-white/[0.1] dark:bg-[#2a2a2c]'
            )}
          >
            <div className="min-w-0 flex flex-1 flex-col gap-1">
              <div className="flex min-w-0 flex-wrap items-center gap-2">
                <span className="truncate text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
                  {plugin.name}
                </span>
                <PluginStatusPill plugin={plugin} />
              </div>
              <span className="truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
                {toolDescription(plugin)}
              </span>
              <span className="text-[10px] font-mono uppercase tracking-normal text-secondary-light/80 dark:text-secondary-dark/80">
                {pluginSettingNote(plugin)}
              </span>
            </div>

            <button
              type="button"
              role="switch"
              aria-checked={plugin.enabled}
              aria-label={`${plugin.enabled ? 'Turn off' : 'Turn on'} ${plugin.name} for this agent`}
              onClick={() => void toggle(plugin)}
              disabled={plugin.saving}
              className={cn(
                'relative inline-flex h-6 w-11 shrink-0 cursor-pointer rounded-full border-2 border-transparent',
                'transition-colors duration-200 focus:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
                plugin.saving && 'opacity-50 cursor-wait',
                plugin.enabled ? 'bg-apple-blue' : 'bg-apple-gray-2'
              )}
            >
              <span
                className={cn(
                  'pointer-events-none inline-block h-5 w-5 rounded-full bg-white shadow-md',
                  'transform transition-transform duration-200',
                  plugin.enabled ? 'translate-x-5' : 'translate-x-0'
                )}
              />
            </button>
          </div>
        ))
      )}
    </div>
  )
}

function PluginMetric({
  testId,
  label,
  value,
  tone,
}: {
  testId: string
  label: string
  value: number
  tone: 'success' | 'muted' | 'attention' | 'default'
}) {
  const toneClass = {
    success: 'text-apple-green',
    muted: 'text-secondary-light dark:text-secondary-dark',
    attention: 'text-apple-orange',
    default: 'text-foreground-light dark:text-foreground-dark',
  }[tone]

  return (
    <div
      data-testid={testId}
      className="rounded-card border border-black/[0.08] bg-white px-3 py-2 dark:border-white/[0.1] dark:bg-[#2a2a2c]"
    >
      <p className="text-[10px] font-medium uppercase tracking-normal text-secondary-light dark:text-secondary-dark">
        {label}
      </p>
      <p className={cn('mt-1 text-ui-title font-semibold', toneClass)}>{value}</p>
    </div>
  )
}

function PluginFilterButton({
  active,
  label,
  ariaLabel,
  count,
  onClick,
}: {
  active: boolean
  label: string
  ariaLabel: string
  count: number
  onClick: () => void
}) {
  const countLabel = `${count} matching ${count === 1 ? 'tool' : 'tools'}`

  return (
    <button
      type="button"
      aria-pressed={active}
      aria-label={`${ariaLabel}, ${countLabel}`}
      onClick={onClick}
      className={cn(
        'inline-flex h-7 items-center gap-1 rounded-md px-2 text-ui-caption font-medium transition-colors',
        active
          ? 'bg-white text-foreground-light shadow-sm dark:bg-white/[0.12] dark:text-foreground-dark'
          : 'text-secondary-light hover:text-foreground-light dark:text-secondary-dark dark:hover:text-foreground-dark'
      )}
    >
      <span>{label}</span>
      <span className="font-mono text-[10px]" aria-hidden="true">
        {count}
      </span>
    </button>
  )
}

function PluginStatusPill({ plugin }: { plugin: PluginItem }) {
  const Icon = plugin.enabled ? CheckCircle2 : Circle
  const label = plugin.enabled ? 'Can use now' : 'Turned off for this agent'
  return (
    <span
      className={cn(
        'inline-flex shrink-0 items-center gap-1 rounded-md px-2 py-0.5 text-[10px] font-semibold uppercase tracking-normal',
        plugin.enabled
          ? 'bg-apple-green/[0.1] text-apple-green'
          : 'bg-black/[0.05] text-secondary-light dark:bg-white/[0.08] dark:text-secondary-dark'
      )}
    >
      <Icon size={11} strokeWidth={2.2} aria-hidden="true" />
      {label}
    </span>
  )
}
