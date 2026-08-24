import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate } from '@tanstack/react-router'
import { AlertTriangle, ArrowRight, CheckCircle2, RefreshCw } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSettingsStore } from '@app/entities/settings'
import { useAdminStore } from '@app/entities/admin'
import {
  orchestrationApi,
  type ParticipantSummary,
  type TaskStats,
} from '@app/shared/api/orchestration'
import { listAuthProviders, type AuthProviderInfo } from '@app/shared/api/auth'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
import { BeginnerLoadingState } from '@app/shared/ui/BeginnerLoadingState'
import { summarizeOperations, type OperationsSummary } from './model/triage'

export function OperationsView() {
  const { t } = useTranslation()
  const runtimeSettings = useSettingsStore((s) => s.runtimeSettings)
  const runtimeLoading = useSettingsStore((s) => s.runtimeLoading)
  const loadRuntimeSettings = useSettingsStore((s) => s.loadRuntimeSettings)
  const providers = useSettingsStore((s) => s.providers)
  const loadProviders = useSettingsStore((s) => s.loadProviders)
  const { health, healthLoading, loadHealth } = useAdminStore()
  const selectedGroupId = useBoardStore((s) => s.selectedGroupId)
  const [participants, setParticipants] = useState<ParticipantSummary[]>([])
  const [ssoProviders, setSsoProviders] = useState<AuthProviderInfo[]>([])
  const [stats, setStats] = useState<TaskStats | null>(null)
  const [refreshing, setRefreshing] = useState(false)
  const routerNavigate = useNavigate()
  const navigate = (path: string) => void routerNavigate({ to: path })

  const refresh = useCallback(async () => {
    setRefreshing(true)
    await Promise.allSettled([
      loadRuntimeSettings(),
      loadProviders(),
      loadHealth(),
      orchestrationApi
        .getParticipants('all')
        .then(setParticipants)
        .catch(() => setParticipants([])),
      selectedGroupId
        ? orchestrationApi
            .getStats(selectedGroupId)
            .then(setStats)
            .catch(() => setStats(null))
        : Promise.resolve(),
      listAuthProviders().then(setSsoProviders),
    ])
    setRefreshing(false)
  }, [loadHealth, loadProviders, loadRuntimeSettings, selectedGroupId])

  useEffect(() => {
    if (!selectedGroupId) {
      setStats(null)
      return
    }
    let cancelled = false
    orchestrationApi
      .getStats(selectedGroupId)
      .then((value) => {
        if (!cancelled) setStats(value)
      })
      .catch(() => {
        if (!cancelled) setStats(null)
      })
    return () => {
      cancelled = true
    }
  }, [selectedGroupId])

  useEffect(() => {
    // Mount-only refresh; the Refresh button re-runs the same work.
    void refresh()
  }, [])

  const runtimeReady = Boolean(
    runtimeSettings &&
    runtimeSettings.availableRuntimes.length > 0 &&
    runtimeSettings.availableCliTools.length > 0
  )
  const verifiedProvider = providers.find((p) => p.isEnabled && p.lastTestStatus === 'passed')
  const availableAgents = participants.filter((p) => p.status === 'available').length
  const queueBacklog = stats?.byState.backlog ?? 0
  const queueWorking = stats?.byState.working ?? 0
  const healthChecks = useMemo(() => {
    const checks = health?.checks
    if (!checks) return undefined
    const out: Record<string, boolean> = {}
    for (const [name, value] of Object.entries(checks)) {
      out[name] =
        typeof value === 'object' && value !== null ? value.status === 'up' : value === true
    }
    return out
  }, [health])

  const summary: OperationsSummary = useMemo(
    () =>
      summarizeOperations({
        runtimeReady,
        providerVerified: Boolean(verifiedProvider),
        providerCount: providers.length,
        availableAgents,
        queueBacklog,
        queueWorking,
        queueCompleted: stats?.byState.completed ?? 0,
        healthChecks,
        healthStatus: health?.status,
      }),
    [
      availableAgents,
      health,
      providers.length,
      queueBacklog,
      queueWorking,
      runtimeReady,
      stats,
      verifiedProvider,
    ]
  )

  const loading = runtimeLoading || healthLoading || refreshing
  const attentionCount = summary.attention.length

  return (
    <div
      data-testid="page-operations"
      className="min-h-full overflow-y-auto bg-background-light px-4 py-5 dark:bg-background-dark sm:px-6"
    >
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-ui-doc-title text-foreground-light dark:text-foreground-dark">
            {t('operations.title')}
          </h2>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {t('operations.subtitle')}
          </p>
        </div>
        <button
          type="button"
          data-testid="operations-refresh"
          onClick={() => void refresh()}
          disabled={refreshing}
          className={uiStyles.secondaryButton}
        >
          <RefreshCw
            size={14}
            strokeWidth={2.25}
            className={cn(refreshing && 'animate-spin')}
            aria-hidden="true"
          />
          {t('operations.refresh')}
        </button>
      </header>

      <div
        data-testid="operations-status"
        className={cn(
          'mt-4 flex items-center gap-2 rounded-card border px-4 py-3',
          attentionCount === 0
            ? 'border-apple-green/25 bg-apple-green/[0.07]'
            : 'border-apple-orange/30 bg-apple-orange/[0.08]'
        )}
      >
        {attentionCount === 0 ? (
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
        <span className="text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
          {attentionCount === 0
            ? t('operations.allClear')
            : t('operations.attention', { count: attentionCount })}
        </span>
      </div>

      {loading && (
        <div className="mt-4">
          <BeginnerLoadingState
            compact
            framed={false}
            title={t('operations.loadingTitle')}
            detail={t('operations.loadingDetail')}
            nextStep={t('operations.loadingNext')}
            success={t('operations.loadingSuccess')}
          />
        </div>
      )}

      <section className="mt-4 grid gap-3 md:grid-cols-2">
        <OpsCard
          id="runtime"
          title={t('operations.cards.runtime.title')}
          ok={runtimeReady}
          okText={t('operations.cards.runtime.ready')}
          failText={t('operations.cards.runtime.notReady')}
          path="/settings/runtime"
          onNavigate={navigate}
        />
        <OpsCard
          id="providers"
          title={t('operations.cards.providers.title')}
          ok={Boolean(verifiedProvider)}
          okText={t('operations.cards.providers.ready')}
          failText={
            providers.length > 0
              ? t('operations.cards.providers.needsTest')
              : t('operations.cards.providers.none')
          }
          path="/settings/providers"
          onNavigate={navigate}
        />
        <OpsCard
          id="agents"
          title={t('operations.cards.agents.title')}
          ok={availableAgents > 0}
          okText={t('operations.cards.agents.ready', { count: availableAgents })}
          failText={t('operations.cards.agents.none')}
          path="/agents"
          onNavigate={navigate}
        />
        <OpsCard
          id="queue"
          title={t('operations.cards.queue.title')}
          ok={!(queueBacklog > 0 && queueWorking === 0)}
          okText={t('operations.cards.queue.ready')}
          failText={t('operations.cards.queue.stalled')}
          path="/tasks"
          onNavigate={navigate}
        />
        <OpsCard
          id="health"
          title={t('operations.cards.health.title')}
          ok={summary.healthDegradedDetails.length === 0}
          okText={t('operations.cards.health.ok')}
          failText={t('operations.cards.health.degraded', {
            details: summary.healthDegradedDetails.join(', '),
          })}
          path="/admin"
          onNavigate={navigate}
        />
      </section>

      <section
        data-testid="operations-sso"
        className="mt-4 rounded-card border border-black/[0.08] bg-white px-4 py-3 dark:border-white/[0.1] dark:bg-surface-dark"
      >
        <p className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
          {t('operations.sso.title')}
        </p>
        <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
          {ssoProviders.length > 0
            ? t('operations.sso.configured', { provider: ssoProviders[0].displayName })
            : t('operations.sso.off')}
        </p>
      </section>
    </div>
  )
}

function OpsCard({
  id,
  title,
  ok,
  okText,
  failText,
  path,
  onNavigate,
}: {
  id: string
  title: string
  ok: boolean
  okText: string
  failText: string
  path: string
  onNavigate: (path: string) => void
}) {
  return (
    <article
      data-testid={`operations-card-${id}`}
      className={cn(
        'rounded-card border bg-white px-4 py-3 dark:bg-surface-dark',
        ok ? 'border-black/[0.08] dark:border-white/[0.1]' : 'border-apple-orange/30'
      )}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            {title}
          </p>
          <p
            className={cn(
              'mt-1 text-ui-body',
              ok ? 'text-secondary-light dark:text-secondary-dark' : 'font-medium text-apple-orange'
            )}
          >
            {ok ? okText : failText}
          </p>
          {!ok && (
            <button
              type="button"
              onClick={() => onNavigate(path)}
              className="mt-2 inline-flex items-center gap-1.5 text-ui-caption font-semibold text-apple-blue underline-offset-2 hover:underline"
            >
              {actionLabel(path)}
              <ArrowRight size={13} strokeWidth={2.25} aria-hidden="true" />
            </button>
          )}
        </div>
        <span
          aria-hidden="true"
          className={cn(
            'mt-1 h-2 w-2 shrink-0 rounded-full',
            ok ? 'bg-apple-green' : 'bg-apple-orange'
          )}
        />
      </div>
    </article>
  )
}

function actionLabel(path: string): string {
  switch (path) {
    case '/settings/runtime':
      return 'Set work locations in Runtime settings'
    case '/settings/providers':
      return 'Add and verify an AI service'
    case '/agents':
      return 'Start an agent from Agents'
    case '/tasks':
      return 'Check queue progress in Tasks'
    case '/admin':
      return 'Check system health in Admin'
    default:
      return 'Open the next step'
  }
}
