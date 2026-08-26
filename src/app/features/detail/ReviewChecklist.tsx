import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { CheckCircle2, ClipboardCheck } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import {
  orchestrationApi,
  type ReviewGateStatus,
  type TaskReviewCheck,
} from '@app/shared/api/orchestration'

const REVIEW_CHECK_ITEMS = [
  { key: 'result_matches_brief', labelKey: 'reviewChecklist.resultMatchesBrief' },
  { key: 'artifacts_checked', labelKey: 'reviewChecklist.artifactsChecked' },
  { key: 'no_secrets', labelKey: 'reviewChecklist.noSecrets' },
  { key: 'reusable_saved', labelKey: 'reviewChecklist.reusableSaved' },
] as const

/** Human review checklist for a finished task — review evidence, per user. */
export function ReviewChecklist({ taskId }: { taskId: string }) {
  const { t } = useTranslation()
  const [checks, setChecks] = useState<Record<string, boolean>>({})
  const [gates, setGates] = useState<ReviewGateStatus | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [savingKey, setSavingKey] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    orchestrationApi
      .listTaskReviewChecks(taskId)
      .then((rows: TaskReviewCheck[]) => {
        if (cancelled) return
        setChecks(Object.fromEntries(rows.map((row) => [row.checkKey, row.done])))
        setError(null)
      })
      .catch(() => {
        if (!cancelled) setError(t('reviewChecklist.loadError'))
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    orchestrationApi
      .fetchTaskReviewGates(taskId)
      .then((status) => {
        if (!cancelled) setGates(status)
      })
      .catch(() => {
        if (!cancelled) setGates(null)
      })
    return () => {
      cancelled = true
    }
  }, [taskId, t])

  async function toggle(checkKey: string, done: boolean) {
    if (savingKey) return
    setSavingKey(checkKey)
    setError(null)
    const previous = checks[checkKey] ?? false
    setChecks((current) => ({ ...current, [checkKey]: done }))
    try {
      await orchestrationApi.setTaskReviewCheck(taskId, checkKey, done)
    } catch {
      setChecks((current) => ({ ...current, [checkKey]: previous }))
      setError(t('reviewChecklist.saveError'))
    } finally {
      setSavingKey(null)
    }
  }

  const completed = REVIEW_CHECK_ITEMS.filter((item) => checks[item.key]).length
  const allComplete = completed === REVIEW_CHECK_ITEMS.length
  const busy = loading || Boolean(savingKey)
  const requiredKeys = gates?.requiredKeys ?? []
  const missingKeys = gates?.missing ?? []
  const gatesConfigured = (requiredKeys.length ?? 0) > 0

  return (
    <section
      data-testid="task-review-checklist"
      className="mt-5 rounded-card border border-black/[0.08] bg-white px-4 py-3 dark:border-white/[0.1] dark:bg-surface-dark"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <ClipboardCheck
              size={16}
              strokeWidth={2.2}
              className="text-apple-blue"
              aria-hidden="true"
            />
            <h2 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
              {t('reviewChecklist.title')}
            </h2>
          </div>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {allComplete
              ? t('reviewChecklist.allDone')
              : t('reviewChecklist.progress', {
                  done: completed,
                  total: REVIEW_CHECK_ITEMS.length,
                })}
          </p>
        </div>
        {allComplete && (
          <CheckCircle2
            size={18}
            strokeWidth={2.25}
            className="shrink-0 text-apple-green"
            aria-hidden="true"
          />
        )}
      </div>

      {gatesConfigured && !(gates?.satisfied ?? false) && (
        <p
          role="status"
          data-testid="review-gates-warning"
          className="mt-2 rounded-card border border-apple-orange/25 bg-apple-orange/10 px-2 py-1.5 text-ui-caption text-apple-orange"
        >
          {t('reviewChecklist.gatesWarning', { missing: missingKeys.length })}
        </p>
      )}
      {gatesConfigured && (gates?.satisfied ?? false) && (
        <p
          role="status"
          data-testid="review-gates-satisfied"
          className="mt-2 rounded-card border border-apple-green/25 bg-apple-green/10 px-2 py-1.5 text-ui-caption text-apple-green"
        >
          {t('reviewChecklist.gatesSatisfied')}
        </p>
      )}

      <div className="mt-3 flex flex-col gap-2">
        {REVIEW_CHECK_ITEMS.map((item) => {
          const done = checks[item.key] ?? false
          return (
            <label
              key={item.key}
              data-testid={`review-check-${item.key}`}
              className={cn(
                'flex cursor-pointer items-start gap-2.5 rounded-button px-2 py-1.5 text-ui-body transition-colors',
                done
                  ? 'bg-apple-green/[0.08] text-foreground-light dark:text-foreground-dark'
                  : 'text-foreground-light dark:text-foreground-dark hover:bg-black/[0.03] dark:hover:bg-white/[0.04]'
              )}
            >
              <input
                type="checkbox"
                checked={done}
                disabled={busy}
                onChange={(event) => void toggle(item.key, event.target.checked)}
                className="mt-0.5 h-4 w-4 rounded border border-black/[0.15] accent-apple-blue"
              />
              <span className={cn(done && 'line-through opacity-70')}>
                {t(item.labelKey)}
                {requiredKeys.includes(item.key) && (
                  <span className="ml-1.5 rounded bg-apple-orange/15 px-1 py-0.5 text-ui-caption font-medium text-apple-orange">
                    Required
                  </span>
                )}
              </span>
            </label>
          )
        })}
      </div>

      {error && (
        <p
          role="alert"
          aria-live="polite"
          className="mt-2 text-ui-caption font-medium text-apple-red"
        >
          {error}
        </p>
      )}
    </section>
  )
}
