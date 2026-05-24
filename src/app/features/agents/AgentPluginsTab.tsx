import { useEffect, useMemo, useState } from 'react'
import { CheckCircle2, Circle, RotateCcw, Search, SlidersHorizontal, Wrench } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'

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

const FILTER_LABELS: Record<PluginFilter, string> = {
  all: 'All',
  enabled: 'Enabled',
  disabled: 'Disabled',
  overridden: 'Overrides',
}

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
    return [plugin.name, plugin.description, plugin.id, plugin.version]
      .join(' ')
      .toLowerCase()
      .includes(normalized)
  })
}

interface AgentPluginsTabProps {
  agentId: string
}

export function AgentPluginsTab({ agentId }: AgentPluginsTabProps) {
  const [plugins, setPlugins] = useState<PluginItem[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState<PluginFilter>('all')

  const summary = useMemo(() => summarizePlugins(plugins), [plugins])
  const visiblePlugins = useMemo(
    () => filterPlugins(plugins, filter, query),
    [plugins, filter, query]
  )

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError(null)

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
        if (!cancelled) setError(err instanceof Error ? err.message : 'Failed to load plugins')
      } finally {
        if (!cancelled) setLoading(false)
      }
    }

    void load()
    return () => {
      cancelled = true
    }
  }, [agentId])

  async function toggle(plugin: PluginItem) {
    const next = !plugin.enabled
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
    } catch {
      // Revert on failure so the UI stays consistent with the server.
      setPlugins((prev) =>
        prev.map((p) => (p.id === plugin.id ? { ...p, enabled: !next, saving: false } : p))
      )
    }
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center py-8">
        <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
          Loading plugins…
        </p>
      </div>
    )
  }

  if (error) {
    return (
      <div className="flex items-center justify-center py-8">
        <p className="text-ui-body text-apple-red">{error}</p>
      </div>
    )
  }

  if (plugins.length === 0) {
    return (
      <div className="flex items-center justify-center py-8">
        <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
          No plugins available
        </p>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-4">
      <section data-testid="agent-plugin-readiness" className="space-y-4">
        <div className="grid grid-cols-2 gap-2 lg:grid-cols-4">
          <PluginMetric label="Enabled" value={summary.enabled} tone="success" />
          <PluginMetric label="Disabled" value={summary.disabled} tone="muted" />
          <PluginMetric label="Overrides" value={summary.overridden} tone="attention" />
          <PluginMetric label="Total" value={summary.total} tone="default" />
        </div>

        <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
          <label className="relative min-w-0 flex-1">
            <Search
              size={15}
              strokeWidth={2}
              className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-secondary-light dark:text-secondary-dark"
              aria-hidden="true"
            />
            <input
              data-testid="agent-plugin-search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search plugins"
              className="h-9 w-full rounded-lg border border-black/[0.08] bg-white pl-9 pr-3 text-ui-body text-foreground-light outline-none transition-colors placeholder:text-secondary-light/75 focus:border-apple-blue/45 focus:ring-2 focus:ring-apple-blue/15 dark:border-white/[0.1] dark:bg-[#2a2a2c] dark:text-foreground-dark dark:placeholder:text-secondary-dark/75"
            />
          </label>

          <div
            data-testid="agent-plugin-filter"
            role="group"
            aria-label="Plugin filter"
            className="inline-flex h-9 items-center gap-1 rounded-lg border border-black/[0.08] bg-black/[0.025] p-1 dark:border-white/[0.1] dark:bg-white/[0.04]"
          >
            {(Object.keys(FILTER_LABELS) as PluginFilter[]).map((option) => (
              <button
                key={option}
                type="button"
                onClick={() => setFilter(option)}
                className={cn(
                  'inline-flex h-7 items-center gap-1 rounded-md px-2 text-ui-caption font-medium transition-colors',
                  filter === option
                    ? 'bg-white text-foreground-light shadow-sm dark:bg-white/[0.12] dark:text-foreground-dark'
                    : 'text-secondary-light hover:text-foreground-light dark:text-secondary-dark dark:hover:text-foreground-dark'
                )}
              >
                {FILTER_LABELS[option]}
                <span className="font-mono text-[10px]">
                  {countPluginsByFilter(summary, option)}
                </span>
              </button>
            ))}
          </div>
        </div>

        <div className="flex items-center gap-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
          <SlidersHorizontal size={14} strokeWidth={2} aria-hidden="true" />
          <span>
            Showing {visiblePlugins.length} of {summary.total} plugins
          </span>
        </div>
      </section>

      {visiblePlugins.length === 0 ? (
        <div
          data-testid="agent-plugin-filter-empty"
          className="flex flex-col items-center justify-center rounded-card border border-dashed border-black/[0.1] bg-black/[0.02] px-4 py-8 text-center dark:border-white/[0.12] dark:bg-white/[0.03]"
        >
          <Wrench
            size={18}
            strokeWidth={2}
            className="text-secondary-light dark:text-secondary-dark"
          />
          <p className="mt-2 text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
            No plugins match the current view
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
            Clear filters
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
                {plugin.description || 'No description provided'}
              </span>
              <span className="text-[10px] font-mono uppercase tracking-normal text-secondary-light/80 dark:text-secondary-dark/80">
                {plugin.hasOverride
                  ? `Agent override · default ${plugin.defaultEnabled ? 'enabled' : 'disabled'}`
                  : `Default ${plugin.defaultEnabled ? 'enabled' : 'disabled'}`}
              </span>
            </div>

            <button
              type="button"
              role="switch"
              aria-checked={plugin.enabled}
              aria-label={`Toggle ${plugin.name}`}
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
  label,
  value,
  tone,
}: {
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
      data-testid={`agent-plugin-metric-${label.toLowerCase()}`}
      className="rounded-card border border-black/[0.08] bg-white px-3 py-2 dark:border-white/[0.1] dark:bg-[#2a2a2c]"
    >
      <p className="text-[10px] font-medium uppercase tracking-normal text-secondary-light dark:text-secondary-dark">
        {label}
      </p>
      <p className={cn('mt-1 text-ui-title font-semibold', toneClass)}>{value}</p>
    </div>
  )
}

function PluginStatusPill({ plugin }: { plugin: PluginItem }) {
  const Icon = plugin.enabled ? CheckCircle2 : Circle
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
      {plugin.enabled ? 'Enabled' : 'Disabled'}
    </span>
  )
}
