import { useEffect, useCallback, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Activity, AlertTriangle, ArrowRight, CheckCircle2, RefreshCw } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { formatRelativeTime } from '@app/shared/lib/time'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSettingsStore } from '@app/shared/model/settings.store'
import { getAgentApi } from '@app/shared/api/legacy'
import { orchestrationApi, type ParticipantSummary } from '@app/shared/api/orchestration'
import type { CliAuthProxyStatusEntry } from '@app/entities/agent'
import type { RuntimeSettings, RuntimeType, CliTool } from '@app/shared/api/legacy/settingsApi'
import { runtimeErrorMessage, runtimeSettingsErrorMessage } from './runtimeErrorMessages'

// ============================================================================
// Setting Row
// ============================================================================

interface SettingRowProps {
  label: string
  description?: string
  children: React.ReactNode
}

interface RuntimeChecklistItem {
  id: string
  title: string
  detail: string
  ready: boolean
  action?: 'refresh' | 'connect'
  actionLabel?: string
  provider?: string
}

function SettingRow({ label, description, children }: SettingRowProps) {
  return (
    <div
      className={cn(
        'flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between sm:gap-4',
        uiStyles.row
      )}
    >
      <div className="min-w-0">
        <span className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
          {label}
        </span>
        {description && (
          <p className="mt-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {description}
          </p>
        )}
      </div>
      <div className="w-full sm:w-auto sm:shrink-0">{children}</div>
    </div>
  )
}

// ============================================================================
// RuntimeSection
// ============================================================================

export function RuntimeSection() {
  const { t } = useTranslation()
  const {
    runtimeSettings,
    runtimeLoading,
    runtimeError,
    loadRuntimeSettings,
    updateRuntimeSettings,
  } = useSettingsStore()
  const [saving, setSaving] = useState(false)
  const [cliStatuses, setCliStatuses] = useState<CliAuthProxyStatusEntry[]>([])
  const [cliStatusLoading, setCliStatusLoading] = useState(false)
  const [cliStatusError, setCliStatusError] = useState<string | null>(null)
  const [openingProvider, setOpeningProvider] = useState<string | null>(null)
  const [participants, setParticipants] = useState<ParticipantSummary[]>([])
  const [participantsError, setParticipantsError] = useState<string | null>(null)

  const loadCliAuthStatus = useCallback(async () => {
    setCliStatusLoading(true)
    setCliStatusError(null)
    try {
      const response = await getAgentApi().getCliAuthProxyStatus()
      setCliStatuses(response.ok ? response.statuses : [])
      if (!response.ok) setCliStatusError(runtimeErrorMessage('loadCliSignIn', response))
    } catch (err) {
      setCliStatuses([])
      setCliStatusError(runtimeErrorMessage('loadCliSignIn', err))
    } finally {
      setCliStatusLoading(false)
    }
  }, [])

  const loadParticipants = useCallback(async () => {
    setParticipantsError(null)
    try {
      setParticipants(await orchestrationApi.getParticipants('all'))
    } catch (err) {
      setParticipants([])
      setParticipantsError(runtimeErrorMessage('loadAgentSignals', err))
    }
  }, [])

  const refreshRuntimeSignals = useCallback(async () => {
    await Promise.allSettled([loadCliAuthStatus(), loadParticipants()])
  }, [loadCliAuthStatus, loadParticipants])

  useEffect(() => {
    void loadRuntimeSettings()
    void refreshRuntimeSignals()
  }, [loadRuntimeSettings, refreshRuntimeSignals])

  async function handleRuntimeChange(value: RuntimeType) {
    if (!runtimeSettings) return
    setSaving(true)
    await updateRuntimeSettings({ defaultRuntime: value })
    setSaving(false)
  }

  async function handleCliToolChange(value: CliTool) {
    if (!runtimeSettings) return
    setSaving(true)
    await updateRuntimeSettings({ defaultCliTool: value })
    setSaving(false)
  }

  const runtimeLabel = (rt: RuntimeType | string): string =>
    t(`settings.runtime.runtimeLabels.${rt}`, { defaultValue: fallbackRuntimeLabel(rt) })
  const cliToolLabel = (tool: CliTool | string): string =>
    t(`settings.runtime.cliToolLabels.${tool}`, { defaultValue: fallbackCliToolLabel(tool) })
  const runtimeReady = Boolean(
    runtimeSettings &&
    runtimeSettings.availableRuntimes.length > 0 &&
    runtimeSettings.availableCliTools.length > 0
  )
  const cliToolDetails = runtimeSettings?.cliToolDetails ?? []
  const reportedVersionCount = cliToolDetails.filter((detail) => detail.version).length
  const connectedCredentialCount = cliStatuses.filter((status) => status.connected).length
  const disconnectedCredentials = cliStatuses.filter((status) => !status.connected)
  const latestHeartbeat = latestParticipantHeartbeat(participants)
  const checklistItems = runtimeLaunchChecklistItems(
    runtimeSettings,
    cliStatuses,
    cliStatusError,
    participantsError,
    latestHeartbeat,
    runtimeLabel,
    cliToolLabel
  )
  const checklistReadyCount = checklistItems.filter((item) => item.ready).length
  const nextChecklistItem = checklistItems.find((item) => !item.ready) ?? null

  async function connectCliProvider(provider: string) {
    setOpeningProvider(provider)
    setCliStatusError(null)
    try {
      const result = await getAgentApi().startCliAuthProxyLogin(provider)
      if (!result.ok || !result.url) {
        setCliStatusError(runtimeErrorMessage('startCliSignIn', result))
        return
      }
      window.open(result.url, '_blank', 'noopener,noreferrer')
    } catch (err) {
      setCliStatusError(runtimeErrorMessage('startCliSignIn', err))
    } finally {
      setOpeningProvider(null)
    }
  }

  function handleChecklistAction(item: RuntimeChecklistItem) {
    if (item.action === 'refresh') {
      void refreshRuntimeSignals()
      return
    }
    if (item.action === 'connect' && item.provider) {
      void connectCliProvider(item.provider)
    }
  }

  return (
    <div>
      {/* Section header */}
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>{t('settings.runtime.title')}</h2>
          <p className={uiStyles.sectionDescription}>{t('settings.runtime.description')}</p>
        </div>
        {saving && (
          <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
            {t('settings.runtime.saving')}
          </span>
        )}
      </div>

      {/* Error */}
      {runtimeError && (
        <div role="alert" aria-live="polite" className={uiStyles.error}>
          {runtimeSettingsErrorMessage(runtimeError)}
        </div>
      )}
      {cliStatusError && (
        <div role="alert" aria-live="polite" className={uiStyles.error}>
          {cliStatusError}
        </div>
      )}
      {participantsError && (
        <div role="alert" aria-live="polite" className={uiStyles.error}>
          {participantsError}
        </div>
      )}

      <section
        data-testid="runtime-readiness"
        className="mb-4 rounded-lg border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-[#2a2a2c]"
      >
        <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              {runtimeReady ? (
                <CheckCircle2 size={17} strokeWidth={2.25} className="text-apple-green" />
              ) : (
                <AlertTriangle size={17} strokeWidth={2.25} className="text-apple-orange" />
              )}
              <h3 className={uiStyles.sectionTitle}>
                {runtimeReady ? 'Agent work setup is ready' : 'Agent work setup needs attention'}
              </h3>
            </div>
            <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
              {runtimeSettings
                ? runtimeReadinessSummary(
                    runtimeSettings,
                    connectedCredentialCount,
                    participants.length
                  )
                : 'Agent Work Setup has not loaded yet.'}
            </p>
          </div>
          <button
            type="button"
            onClick={() => void refreshRuntimeSignals()}
            disabled={cliStatusLoading}
            className={cn(uiStyles.secondaryButton, 'shrink-0')}
          >
            <RefreshCw
              size={14}
              strokeWidth={2}
              className={cn(cliStatusLoading && 'animate-spin')}
              aria-hidden="true"
            />
            <span>{cliStatusLoading ? 'Checking setup' : 'Check setup'}</span>
          </button>
        </div>

        <div className="mt-4 grid gap-3 md:grid-cols-4">
          <RuntimeReadinessMetric
            label="Default agent location"
            value={
              runtimeSettings
                ? runtimeLabel(runtimeSettings.defaultRuntime)
                : 'Load setup to choose a location'
            }
            ready={Boolean(
              runtimeSettings?.availableRuntimes.includes(runtimeSettings.defaultRuntime)
            )}
          />
          <RuntimeReadinessMetric
            label="Work tool setup"
            value={
              cliToolDetails.length > 0
                ? `${reportedVersionCount}/${cliToolDetails.length} work tools ready`
                : 'Check setup after tools finish.'
            }
            ready={cliToolDetails.length > 0 && reportedVersionCount === cliToolDetails.length}
          />
          <RuntimeReadinessMetric
            label="Last agent online"
            value={
              latestHeartbeat
                ? formatRelativeTime(latestHeartbeat)
                : 'Start an agent, then check again.'
            }
            ready={Boolean(latestHeartbeat)}
          />
          <RuntimeReadinessMetric
            label="Work tool sign-ins"
            value={
              cliStatuses.length > 0
                ? `${connectedCredentialCount}/${cliStatuses.length} signed in`
                : 'No sign-ins needed'
            }
            ready={cliStatuses.length === 0 || disconnectedCredentials.length === 0}
          />
        </div>

        {cliStatuses.length > 0 && (
          <div className="mt-4 space-y-2" data-testid="runtime-credential-statuses">
            {cliStatuses.map((status) => (
              <CredentialStatusRow
                key={status.provider}
                status={status}
                opening={openingProvider === status.provider}
                cliToolLabel={cliToolLabel}
                onConnect={() => void connectCliProvider(status.provider)}
              />
            ))}
          </div>
        )}

        {cliToolDetails.length > 0 && (
          <div className="mt-4 space-y-2" data-testid="runtime-cli-versions">
            <div className="flex items-center justify-between gap-2">
              <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                Work tool setup
              </p>
              <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                {reportedVersionCount} work tool{reportedVersionCount === 1 ? '' : 's'} ready
              </p>
            </div>
            {cliToolDetails.map((detail) => (
              <div
                key={detail.cliTool}
                className="grid gap-2 rounded-lg bg-black/[0.03] px-3 py-2 text-ui-caption dark:bg-white/[0.04] sm:grid-cols-[minmax(0,0.8fr)_minmax(0,1.2fr)_auto]"
              >
                <div className="min-w-0">
                  <span className="block font-medium text-foreground-light dark:text-foreground-dark">
                    {cliToolLabel(detail.cliTool)}
                  </span>
                  <span className="block truncate text-secondary-light dark:text-secondary-dark">
                    {detail.version ?? 'Needs attention'}
                  </span>
                </div>
                <span
                  className="min-w-0 truncate text-secondary-light dark:text-secondary-dark"
                  title={detail.image}
                >
                  {detail.imagePresent ? 'Installed and ready' : 'Setup needed'}
                </span>
                <span
                  className={cn(
                    'h-fit rounded-full px-2 py-0.5 text-ui-caption font-medium',
                    detail.imagePresent
                      ? 'bg-apple-green/10 text-apple-green'
                      : 'bg-apple-gray-5 text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark'
                  )}
                >
                  {versionSourceLabel(detail.versionSource, detail.imagePresent)}
                </span>
              </div>
            ))}
          </div>
        )}

        <RuntimeNextStepPanel
          item={nextChecklistItem}
          allReady={checklistItems.length > 0 && checklistReadyCount === checklistItems.length}
          busy={
            nextChecklistItem?.action === 'refresh'
              ? cliStatusLoading
              : openingProvider === nextChecklistItem?.provider
          }
          onAction={() => {
            if (nextChecklistItem) handleChecklistAction(nextChecklistItem)
          }}
        />

        <div
          data-testid="runtime-launch-checklist"
          className="mt-4 rounded-lg border border-black/[0.06] bg-black/[0.02] p-3 dark:border-white/[0.08] dark:bg-white/[0.03]"
        >
          <div className="flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between">
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <Activity
                  size={14}
                  strokeWidth={2}
                  className="text-apple-blue"
                  aria-hidden="true"
                />
                <h4 className="text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
                  Before assigning work
                </h4>
              </div>
              <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                Make these ready before giving agents tasks that need project files, commands, or
                live work access.
              </p>
            </div>
            <span className="shrink-0 rounded-full bg-white px-2 py-0.5 text-ui-caption font-medium tabular-nums text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
              {checklistReadyCount}/{checklistItems.length} ready
            </span>
          </div>
          <div className="mt-3 grid gap-2 lg:grid-cols-2">
            {checklistItems.map((item) => (
              <RuntimeChecklistRow
                key={item.id}
                item={item}
                busy={
                  item.action === 'refresh' ? cliStatusLoading : openingProvider === item.provider
                }
                onAction={() => handleChecklistAction(item)}
              />
            ))}
          </div>
        </div>
      </section>

      {/* Settings card */}
      <div className={uiStyles.card}>
        {runtimeLoading && !runtimeSettings ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            {t('settings.runtime.loading')}
          </div>
        ) : !runtimeSettings ? (
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              {t('settings.runtime.couldNotLoad')}
            </p>
          </div>
        ) : (
          <>
            {/* Default agent location */}
            <SettingRow
              label={t('settings.runtime.defaultRuntimeLabel')}
              description={t('settings.runtime.defaultRuntimeDescription')}
            >
              <select
                value={runtimeSettings.defaultRuntime}
                onChange={(e) => handleRuntimeChange(e.target.value as RuntimeType)}
                disabled={saving}
                className={cn(uiStyles.select, 'disabled:opacity-50')}
              >
                {runtimeSettings.availableRuntimes.map((rt) => (
                  <option key={rt} value={rt}>
                    {runtimeLabel(rt)}
                  </option>
                ))}
              </select>
            </SettingRow>

            {/* Default local work tool */}
            <SettingRow
              label={t('settings.runtime.defaultContainerCliLabel')}
              description={t('settings.runtime.defaultContainerCliDescription')}
            >
              <select
                value={runtimeSettings.defaultCliTool}
                onChange={(e) => handleCliToolChange(e.target.value as CliTool)}
                disabled={saving}
                className={cn(uiStyles.select, 'disabled:opacity-50')}
              >
                {runtimeSettings.availableCliTools.map((tool) => (
                  <option key={tool} value={tool}>
                    {cliToolLabel(tool)}
                  </option>
                ))}
              </select>
            </SettingRow>

            {/* Read-only: available agent locations */}
            <SettingRow
              label={t('settings.runtime.availableRuntimesLabel')}
              description={t('settings.runtime.availableRuntimesDescription')}
            >
              <div className="flex flex-wrap justify-end gap-1.5">
                {runtimeSettings.availableRuntimes.map((rt) => (
                  <span key={rt} className={uiStyles.badge}>
                    {runtimeLabel(rt)}
                  </span>
                ))}
              </div>
            </SettingRow>

            {/* Read-only: available work tools */}
            <SettingRow
              label={t('settings.runtime.availableContainerClisLabel')}
              description={t('settings.runtime.availableContainerClisDescription')}
            >
              <div className="flex flex-wrap justify-end gap-1.5">
                {runtimeSettings.availableCliTools.map((tool) => (
                  <span key={tool} className={uiStyles.badge}>
                    {cliToolLabel(tool)}
                  </span>
                ))}
              </div>
            </SettingRow>
          </>
        )}
      </div>
    </div>
  )
}

function RuntimeNextStepPanel({
  item,
  allReady,
  busy,
  onAction,
}: {
  item: RuntimeChecklistItem | null
  allReady: boolean
  busy: boolean
  onAction: () => void
}) {
  const busyLabel = item?.action === 'refresh' ? 'Checking' : 'Opening'

  return (
    <section
      data-testid="runtime-next-step"
      className={cn(
        'mt-4 rounded-lg border px-4 py-3',
        allReady
          ? 'border-apple-green/20 bg-apple-green/5'
          : 'border-apple-blue/20 bg-apple-blue/[0.04]'
      )}
    >
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            {allReady ? (
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
              {allReady ? 'Ready' : 'Next step'}
            </p>
          </div>
          <h3 className="mt-1 text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            {allReady ? 'Ready to give agents work' : item?.title}
          </h3>
          <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
            {allReady
              ? 'The agent location, work tools, sign-ins, and online status are ready.'
              : item?.detail}
          </p>
          <p className="mt-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Success:{' '}
            {allReady
              ? 'Open Agents, create or select an agent, then assign work from Tasks.'
              : 'This item changes to Ready.'}
          </p>
        </div>
        {!allReady && item?.action && item.actionLabel && (
          <button
            type="button"
            onClick={onAction}
            disabled={busy}
            className={cn(uiStyles.secondaryButton, 'w-full sm:w-auto sm:shrink-0')}
          >
            <span>{busy ? busyLabel : item.actionLabel}</span>
            <ArrowRight size={13} strokeWidth={2} aria-hidden="true" />
          </button>
        )}
      </div>
    </section>
  )
}

function RuntimeChecklistRow({
  item,
  busy,
  onAction,
}: {
  item: RuntimeChecklistItem
  busy: boolean
  onAction: () => void
}) {
  const busyLabel = item.action === 'refresh' ? 'Checking' : 'Opening'

  return (
    <div
      className={cn(
        'flex min-w-0 flex-col gap-3 rounded-lg border px-3 py-2 sm:flex-row sm:items-center sm:justify-between',
        item.ready
          ? 'border-apple-green/15 bg-apple-green/5'
          : 'border-apple-orange/20 bg-apple-orange/10'
      )}
    >
      <div className="flex min-w-0 items-start gap-2">
        {item.ready ? (
          <CheckCircle2
            size={15}
            strokeWidth={2.25}
            className="mt-0.5 shrink-0 text-apple-green"
            aria-hidden="true"
          />
        ) : (
          <AlertTriangle
            size={15}
            strokeWidth={2.25}
            className="mt-0.5 shrink-0 text-apple-orange"
            aria-hidden="true"
          />
        )}
        <div className="min-w-0">
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <p className="font-medium text-ui-caption text-foreground-light dark:text-foreground-dark">
              {item.title}
            </p>
            <span
              className={cn(
                'rounded-full px-2 py-0.5 text-[11px] font-semibold',
                item.ready
                  ? 'bg-apple-green/10 text-apple-green'
                  : 'bg-apple-orange/15 text-apple-orange'
              )}
            >
              {item.ready ? 'Ready' : 'Needs setup'}
            </span>
          </div>
          <p className="mt-1 text-ui-caption leading-relaxed text-secondary-light dark:text-secondary-dark">
            {item.detail}
          </p>
        </div>
      </div>
      {item.action && item.actionLabel && (
        <button
          type="button"
          onClick={onAction}
          disabled={busy}
          className={cn(uiStyles.secondaryButton, 'h-8 w-full px-2.5 sm:w-auto sm:shrink-0')}
        >
          <span>{busy ? busyLabel : item.actionLabel}</span>
          <ArrowRight size={13} strokeWidth={2} aria-hidden="true" />
        </button>
      )}
    </div>
  )
}

function RuntimeReadinessMetric({
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
      <p className="mt-1 line-clamp-2 text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
        {value}
      </p>
    </div>
  )
}

function CredentialStatusRow({
  status,
  opening,
  cliToolLabel,
  onConnect,
}: {
  status: CliAuthProxyStatusEntry
  opening: boolean
  cliToolLabel: (tool: string) => string
  onConnect: () => void
}) {
  const detail = status.connected
    ? status.lastRefresh
      ? `Last checked ${formatRelativeTime(status.lastRefresh)}`
      : 'Work tool signed in'
    : status.revokeReason || status.revokedAt
      ? 'Sign in again before starting agents that use this tool'
      : 'No work tool sign-in saved'

  return (
    <div className="flex flex-col gap-2 rounded-lg bg-black/[0.03] px-3 py-2 dark:bg-white/[0.04] sm:flex-row sm:items-center sm:justify-between">
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          <span
            className={cn(
              'h-2 w-2 rounded-full',
              status.connected ? 'bg-apple-green' : 'bg-apple-orange'
            )}
          />
          <span className="truncate text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
            {status.displayName}
          </span>
          <span className="shrink-0 rounded-full bg-black/[0.05] px-2 py-0.5 text-ui-caption text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
            {cliToolLabel(status.cliTool)}
          </span>
        </div>
        <p className="mt-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {detail}
        </p>
      </div>
      {!status.connected && (
        <button
          type="button"
          onClick={onConnect}
          disabled={opening}
          className={cn(uiStyles.secondaryButton, 'shrink-0')}
        >
          {opening ? 'Opening' : 'Sign in'}
        </button>
      )}
    </div>
  )
}

function runtimeLaunchChecklistItems(
  runtimeSettings: RuntimeSettings | null,
  cliStatuses: CliAuthProxyStatusEntry[],
  cliStatusError: string | null,
  participantsError: string | null,
  latestHeartbeat: string | null,
  runtimeLabel: (rt: RuntimeType) => string,
  cliToolLabel: (tool: CliTool) => string
): RuntimeChecklistItem[] {
  const items: RuntimeChecklistItem[] = []
  if (!runtimeSettings) {
    return [
      {
        id: 'runtime-api',
        title: 'Agent work setup status',
        detail:
          'Agent Work Setup has not loaded yet. Check setup. If it still does not load, ask an owner or admin to check Agent Work Setup.',
        ready: false,
        action: 'refresh',
        actionLabel: 'Check again',
      },
    ]
  }

  const defaultRuntimeReady =
    runtimeSettings.availableRuntimes.length > 0 &&
    runtimeSettings.availableCliTools.length > 0 &&
    runtimeSettings.availableRuntimes.includes(runtimeSettings.defaultRuntime) &&
    runtimeSettings.availableCliTools.includes(runtimeSettings.defaultCliTool)
  items.push({
    id: 'defaults',
    title: 'Default agent location and work tool',
    detail: defaultRuntimeReady
      ? `${runtimeLabel(runtimeSettings.defaultRuntime)} with ${cliToolLabel(
          runtimeSettings.defaultCliTool
        )} is selected for new agents.`
      : 'Choose where new agents run and which tool, such as Claude or Codex, they use.',
    ready: defaultRuntimeReady,
  })

  const missingImages = runtimeSettings.cliToolDetails.filter((detail) => !detail.imagePresent)
  const reportedVersionCount = runtimeSettings.cliToolDetails.filter(
    (detail) => detail.version
  ).length
  const imageInventoryReady =
    runtimeSettings.availableCliTools.length > 0 &&
    runtimeSettings.cliToolDetails.length > 0 &&
    missingImages.length === 0 &&
    reportedVersionCount === runtimeSettings.cliToolDetails.length
  let imageDetail = `${reportedVersionCount}/${runtimeSettings.cliToolDetails.length} work tools are ready.`
  if (runtimeSettings.availableCliTools.length === 0) {
    imageDetail =
      'Enable at least one tool before giving agents tasks that need project files, commands, or live work access.'
  } else if (runtimeSettings.cliToolDetails.length === 0) {
    imageDetail = 'No work tool setup status yet. Check again after the tools finish setting up.'
  } else if (missingImages.length > 0) {
    imageDetail = `${missingImages.length} tool${
      missingImages.length === 1 ? '' : 's'
    } need setup. Ask an owner to finish setting up the tools, then check again.`
  } else if (reportedVersionCount !== runtimeSettings.cliToolDetails.length) {
    imageDetail = `${reportedVersionCount}/${runtimeSettings.cliToolDetails.length} work tools are ready. Ask an owner to finish setting up the tools that still need attention.`
  }
  items.push({
    id: 'images',
    title: 'Work tools ready',
    detail: imageDetail,
    ready: imageInventoryReady,
    action: imageInventoryReady ? undefined : 'refresh',
    actionLabel: imageInventoryReady ? undefined : 'Check again',
  })

  const connectedCredentialCount = cliStatuses.filter((status) => status.connected).length
  const disconnectedCredential = cliStatuses.find((status) => !status.connected)
  const credentialReady = !cliStatusError && (!disconnectedCredential || cliStatuses.length === 0)
  items.push({
    id: 'credentials',
    title: 'Work tool sign-ins',
    detail: cliStatusError
      ? 'Work tool sign-ins could not be checked. Check setup. If they still cannot be checked, ask an owner or admin to check work tool sign-ins.'
      : cliStatuses.length === 0
        ? 'No work tool sign-ins are required.'
        : disconnectedCredential
          ? `${connectedCredentialCount}/${cliStatuses.length} work tool sign-ins ready. Sign in to ${disconnectedCredential.displayName} before starting agents that use this tool.`
          : `${connectedCredentialCount}/${cliStatuses.length} work tool sign-ins ready.`,
    ready: credentialReady,
    action: cliStatusError ? 'refresh' : disconnectedCredential ? 'connect' : undefined,
    actionLabel: cliStatusError
      ? 'Check again'
      : disconnectedCredential
        ? `Sign in to ${disconnectedCredential.displayName}`
        : undefined,
    provider: disconnectedCredential?.provider,
  })

  items.push({
    id: 'heartbeats',
    title: 'Agent online status',
    detail: participantsError
      ? 'Agent online status could not be checked. Check setup. If it still cannot be checked, ask an owner or admin to check Agent Work Setup.'
      : latestHeartbeat
        ? `An agent was online ${formatRelativeTime(latestHeartbeat)}.`
        : 'No agent has been seen online yet. Start or wake an agent, then check again.',
    ready: !participantsError && Boolean(latestHeartbeat),
    action: participantsError || !latestHeartbeat ? 'refresh' : undefined,
    actionLabel: participantsError || !latestHeartbeat ? 'Check again' : undefined,
  })

  return items
}

function versionSourceLabel(source: string, imagePresent: boolean): string {
  if (source === 'docker-label') return 'ready'
  if (source === 'image-tag') return imagePresent ? 'ready' : 'needs attention'
  return 'needs attention'
}

function runtimeReadinessSummary(
  runtimeSettings: RuntimeSettings,
  connectedCredentialCount: number,
  onlineAgentCount: number
): string {
  const locations = countPhrase(runtimeSettings.availableRuntimes.length, 'agent location')
  const tools = countPhrase(runtimeSettings.availableCliTools.length, 'work tool')
  const signIns =
    connectedCredentialCount === 0
      ? 'Sign in to a work tool before starting agents that need one'
      : `${countPhrase(connectedCredentialCount, 'work tool sign-in')} ${
          connectedCredentialCount === 1 ? 'is' : 'are'
        } connected`
  const onlineAgents =
    onlineAgentCount === 0
      ? 'no agents are online yet'
      : `${countPhrase(onlineAgentCount, 'agent')} ${onlineAgentCount === 1 ? 'is' : 'are'} online`

  return `Setup has ${locations} and ${tools} like Claude or Codex. ${signIns}, and ${onlineAgents}.`
}

function countPhrase(count: number, singular: string): string {
  return `${count} ${singular}${count === 1 ? '' : 's'}`
}

function fallbackRuntimeLabel(runtime: string): string {
  switch (runtime.trim().toLowerCase()) {
    case 'cli':
      return 'This computer'
    case 'api':
      return 'Chat-only AI service'
    case 'container':
      return 'Managed workspace'
    default:
      return runtime.trim() ? 'Agent location needs review' : 'Agent location not listed'
  }
}

function fallbackCliToolLabel(tool: string): string {
  switch (tool.trim().toLowerCase()) {
    case 'claude':
      return 'Claude'
    case 'codex':
      return 'Codex'
    case 'gemini':
      return 'Gemini'
    case 'opencode':
      return 'OpenCode'
    default:
      return tool.trim() ? 'Work tool needs review' : 'Work tool not listed'
  }
}

function latestParticipantHeartbeat(participants: ParticipantSummary[]): string | null {
  let latest: string | null = null
  let latestMs = Number.NEGATIVE_INFINITY
  for (const participant of participants) {
    if (!participant.lastHeartbeatAt) continue
    const value = Date.parse(participant.lastHeartbeatAt)
    if (!Number.isFinite(value) || value <= latestMs) continue
    latest = participant.lastHeartbeatAt
    latestMs = value
  }
  return latest
}
