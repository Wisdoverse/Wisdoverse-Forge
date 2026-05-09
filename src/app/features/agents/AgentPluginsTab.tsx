import { useEffect, useState } from 'react'
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
  description: string
  /** Effective enabled state (override if present, else plugin default). */
  enabled: boolean
  /** Whether the row is currently in flight (toggle disabled). */
  saving: boolean
}

function authHeaders(): Record<string, string> {
  const token = typeof window !== 'undefined' ? localStorage.getItem('af:auth:access') : null
  return token ? { Authorization: `Bearer ${token}` } : {}
}

function effectiveEnabled(row: AgentPluginRow): boolean {
  return row.enabled ?? row.pluginEnabled
}

interface AgentPluginsTabProps {
  agentId: string
}

export function AgentPluginsTab({ agentId }: AgentPluginsTabProps) {
  const [plugins, setPlugins] = useState<PluginItem[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

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
              description: row.description ?? '',
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
    <div className="flex flex-col gap-3">
      {plugins.map((plugin) => (
        <div
          key={plugin.id}
          data-testid={`plugin-row-${plugin.id}`}
          className={cn(
            'flex items-center justify-between gap-4',
            'rounded-card border border-black/[0.08] bg-white px-4 py-3 dark:border-white/[0.1] dark:bg-[#2a2a2c]'
          )}
        >
          <div className="flex flex-col gap-0.5 min-w-0 flex-1">
            <span className="truncate text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
              {plugin.name}
            </span>
            <span className="truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
              {plugin.description}
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
      ))}
    </div>
  )
}
