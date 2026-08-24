import { useEffect, useState } from 'react'
import { AlertTriangle, Brain } from 'lucide-react'
import { orchestrationApi } from '@app/shared/api/orchestration'

interface SafetyEvent {
  event_name: string
  properties: { taskId?: string }
  created_at: string
}

interface SafetyStats {
  warnings: number
  failures: number
  warnedBeforeFailure: number
  trims: number
}

const EMPTY_STATS: SafetyStats = { warnings: 0, failures: 0, warnedBeforeFailure: 0, trims: 0 }

function eventsWithTaskId(events: unknown[]): SafetyEvent[] {
  return (events as SafetyEvent[]).filter(
    (event) => typeof event?.properties?.taskId === 'string' && event.properties.taskId.length > 0
  )
}

/**
 * Context-safety loop: how often the budget warning fired, how many tasks still
 * failed from a full context window, and of those how many had been warned.
 * Best-effort and derived from the same analytics events as the skills stats.
 */
export function ContextSafetyInsight() {
  const [stats, setStats] = useState<SafetyStats>(EMPTY_STATS)
  const [loaded, setLoaded] = useState(false)

  useEffect(() => {
    let cancelled = false
    Promise.all([
      orchestrationApi.listAnalyticsEvents('context_budget_warning', 500),
      orchestrationApi.listAnalyticsEvents('context_overflow_failure', 500),
      orchestrationApi.listAnalyticsEvents('context_trim_applied', 500),
    ])
      .then(([warnings, failures, trims]) => {
        if (cancelled) return
        const warningEvents = eventsWithTaskId(warnings)
        const failureEvents = eventsWithTaskId(failures)
        const trimEvents = eventsWithTaskId(trims)
        // Earliest warning per task: a warning counts as "before the failure"
        // only when it was shown for the same task and no later than the failure.
        const firstWarning = new Map<string, string>()
        for (const event of warningEvents) {
          const taskId = event.properties.taskId as string
          const existing = firstWarning.get(taskId)
          if (!existing || event.created_at < existing) firstWarning.set(taskId, event.created_at)
        }
        // Count distinct tasks — a reloaded board can re-emit a best-effort
        // event but must not double-count one task in the safety metric.
        const warnedTaskIds = new Set<string>()
        const earliestFailure = new Map<string, string>()
        for (const event of failureEvents) {
          const taskId = event.properties.taskId as string
          const warningAt = firstWarning.get(taskId)
          if (warningAt && warningAt <= event.created_at) warnedTaskIds.add(taskId)
          const existing = earliestFailure.get(taskId)
          if (!existing || event.created_at < existing) earliestFailure.set(taskId, event.created_at)
        }
        setStats({
          warnings: new Set(warningEvents.map((e) => e.properties.taskId as string)).size,
          failures: earliestFailure.size,
          warnedBeforeFailure: warnedTaskIds.size,
          trims: new Set(trimEvents.map((e) => e.properties.taskId as string)).size,
        })
        setLoaded(true)
      })
      .catch(() => {
        if (!cancelled) {
          setStats(EMPTY_STATS)
          setLoaded(true)
        }
      })
    return () => {
      cancelled = true
    }
  }, [])

  const percent =
    stats.failures > 0 ? Math.round((stats.warnedBeforeFailure / stats.failures) * 100) : null

  return (
    <section data-testid="context-safety-insight" className="mt-6">
      <div className="flex items-center gap-2">
        <Brain size={16} strokeWidth={2.2} className="text-apple-blue" aria-hidden="true" />
        <h2 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
          Context safety
        </h2>
      </div>
      <p className="mt-2 rounded-card border border-black/[0.08] bg-white px-3 py-2.5 text-ui-body text-secondary-light dark:border-white/[0.1] dark:bg-surface-dark dark:text-secondary-dark">
        {!loaded || (stats.warnings === 0 && stats.failures === 0 && stats.trims === 0)
          ? 'Signals appear after a task is prepared with a context warning, is trimmed to fit its budget, or fails because the agent ran out of context window.'
          : `${stats.warnings} context warning${stats.warnings === 1 ? '' : 's'} shown before task runs · ${stats.trims} trim${stats.trims === 1 ? '' : 's'} applied · ${stats.failures} task${stats.failures === 1 ? '' : 's'} ran out of context window`}
        {percent !== null && stats.failures > 0 && (
          <span className="mt-1 block">
            {percent}% of those failures had a context warning beforehand
            {stats.warnedBeforeFailure > 0
              ? ' — the warning did not keep the agent inside its budget.'
              : ' — warnings appear to be working; keep trimming before runs.'}
          </span>
        )}
      </p>
      {stats.failures > 0 && percent !== null && percent > 40 && (
        <p className="mt-2 flex items-start gap-1 text-ui-caption font-medium text-apple-red">
          <AlertTriangle size={14} strokeWidth={2} className="mt-0.5 shrink-0" aria-hidden="true" />
          <span>
            Ask the team to remove context before runs: fewer saved notes and a shorter brief lower
            the chance of an overflow failure.
          </span>
        </p>
      )}
    </section>
  )
}
