import { useEffect, useCallback, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Activity, AlertTriangle, ArrowRight, CheckCircle2, RefreshCw } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { formatRelativeTime } from '@app/shared/lib/time'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSettingsStore } from '@app/shared/model/settings.store'
import { getAgentApi } from '@app/shared/api/legacy'
import { orchestrationApi, type ParticipantSummary } from '@app/shared/api/orchestration'
import type { CliAuthProxyStatusEntry } from '@app/shared/api/legacy/AgentAPI'
import type { RuntimeSettings, RuntimeType, CliTool } from '@app/shared/api/legacy/settingsApi'

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
    <div className={cn('flex items-center justify-between gap-4 px-4 py-3', uiStyles.row)}>
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
      <div className="shrink-0">{children}</div>
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
      if (!response.ok) setCliStatusError('Could not load Container CLI credential status')
    } catch (err) {
      setCliStatuses([])
      setCliStatusError(
        err instanceof Error ? err.message : 'Could not load Container CLI credential status'
      )
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
      setParticipantsError(err instanceof Error ? err.message : 'Could not load agent heartbeats')
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

  const runtimeLabel = (rt: RuntimeType): string =>
    t(`settings.runtime.runtimeLabels.${rt}`, { defaultValue: rt })
  const cliToolLabel = (tool: CliTool): string =>
    t(`settings.runtime.cliToolLabels.${tool}`, { defaultValue: tool })
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

  async function connectCliProvider(provider: string) {
    setOpeningProvider(provider)
    setCliStatusError(null)
    try {
      const result = await getAgentApi().startCliAuthProxyLogin(provider)
      if (!result.ok || !result.url) {
        setCliStatusError(result.error ?? 'Could not start Container CLI authorization')
        return
      }
      window.open(result.url, '_blank', 'noopener,noreferrer')
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
      {runtimeError && <div className={uiStyles.error}>{runtimeError}</div>}
      {cliStatusError && <div className={uiStyles.error}>{cliStatusError}</div>}
      {participantsError && <div className={uiStyles.error}>{participantsError}</div>}

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
                {runtimeReady ? 'Runtime ready for agent work' : 'Runtime needs attention'}
              </h3>
            </div>
            <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
              {runtimeSettings
                ? `${runtimeSettings.availableRuntimes.length} runtime option${runtimeSettings.availableRuntimes.length === 1 ? '' : 's'}, ${runtimeSettings.availableCliTools.length} Container CLI option${runtimeSettings.availableCliTools.length === 1 ? '' : 's'}, ${connectedCredentialCount} connected CLI credential${connectedCredentialCount === 1 ? '' : 's'}, ${participants.length} agent heartbeat source${participants.length === 1 ? '' : 's'}.`
                : 'The API has not returned runtime settings yet.'}
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
            <span>{cliStatusLoading ? 'Refreshing' : 'Refresh status'}</span>
          </button>
        </div>

        <div className="mt-4 grid gap-3 md:grid-cols-4">
          <RuntimeReadinessMetric
            label="Default runtime"
            value={runtimeSettings ? runtimeLabel(runtimeSettings.defaultRuntime) : 'Unknown'}
            ready={Boolean(
              runtimeSettings?.availableRuntimes.includes(runtimeSettings.defaultRuntime)
            )}
          />
          <RuntimeReadinessMetric
            label="CLI versions"
            value={
              cliToolDetails.length > 0
                ? `${reportedVersionCount}/${cliToolDetails.length} reported`
                : 'No CLI image metadata'
            }
            ready={cliToolDetails.length > 0 && reportedVersionCount === cliToolDetails.length}
          />
          <RuntimeReadinessMetric
            label="Last heartbeat"
            value={latestHeartbeat ? formatRelativeTime(latestHeartbeat) : 'No agent heartbeat'}
            ready={Boolean(latestHeartbeat)}
          />
          <RuntimeReadinessMetric
            label="Credential state"
            value={
              cliStatuses.length > 0
                ? `${connectedCredentialCount}/${cliStatuses.length} connected`
                : 'No CLI OAuth providers'
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
                onConnect={() => void connectCliProvider(status.provider)}
              />
            ))}
          </div>
        )}

        {cliToolDetails.length > 0 && (
          <div className="mt-4 space-y-2" data-testid="runtime-cli-versions">
            <div className="flex items-center justify-between gap-2">
              <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                Container CLI images
              </p>
              <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                {reportedVersionCount} version{reportedVersionCount === 1 ? '' : 's'} reported
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
                    {detail.version ?? 'Version not reported'}
                  </span>
                </div>
                <span className="min-w-0 truncate font-mono text-secondary-light dark:text-secondary-dark">
                  {detail.image}
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
                  Launch checklist
                </h4>
              </div>
              <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                Resolve warnings here before assigning Container CLI work.
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
            {/* Default Runtime */}
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

            {/* Default Container CLI */}
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

            {/* Read-only: Available Runtimes */}
            <SettingRow
              label={t('settings.runtime.availableRuntimesLabel')}
              description={t('settings.runtime.availableRuntimesDescription')}
            >
              <div className="flex flex-wrap justify-end gap-1.5">
                {runtimeSettings.availableRuntimes.map((rt) => (
                  <span key={rt} className={uiStyles.badge}>
                    {rt}
                  </span>
                ))}
              </div>
            </SettingRow>

            {/* Read-only: Available Container CLIs */}
            <SettingRow
              label={t('settings.runtime.availableContainerClisLabel')}
              description={t('settings.runtime.availableContainerClisDescription')}
            >
              <div className="flex flex-wrap justify-end gap-1.5">
                {runtimeSettings.availableCliTools.map((tool) => (
                  <span key={tool} className={uiStyles.badge}>
                    {tool}
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

function RuntimeChecklistRow({
  item,
  busy,
  onAction,
}: {
  item: RuntimeChecklistItem
  busy: boolean
  onAction: () => void
}) {
  const busyLabel = item.action === 'refresh' ? 'Refreshing' : 'Opening'

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
              {item.ready ? 'Ready' : 'Action'}
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
          className={cn(uiStyles.secondaryButton, 'h-8 shrink-0 px-2.5')}
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
  onConnect,
}: {
  status: CliAuthProxyStatusEntry
  opening: boolean
  onConnect: () => void
}) {
  const detail = status.connected
    ? status.lastRefresh
      ? `Last refreshed ${formatRelativeTime(status.lastRefresh)}`
      : 'Stored credential is usable'
    : status.revokeReason || status.revokedAt
      ? 'Reconnect before starting new Container CLI agents'
      : 'No stored OAuth credential'

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
            {status.cliTool}
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
          {opening ? 'Opening' : 'Connect'}
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
        title: 'Runtime API',
        detail: 'Runtime settings have not loaded yet. Refresh after the Rust API is reachable.',
        ready: false,
        action: 'refresh',
        actionLabel: 'Refresh Status',
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
    title: 'Execution defaults',
    detail: defaultRuntimeReady
      ? `${runtimeLabel(runtimeSettings.defaultRuntime)} with ${cliToolLabel(
          runtimeSettings.defaultCliTool
        )} is selectable for new agents.`
      : 'Choose defaults that are present in the available runtime and Container CLI lists.',
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
  let imageDetail = `${reportedVersionCount}/${runtimeSettings.cliToolDetails.length} Container CLI versions reported.`
  if (runtimeSettings.availableCliTools.length === 0) {
    imageDetail = 'Build or enable Container CLI images so CLI-backed agents can start.'
  } else if (runtimeSettings.cliToolDetails.length === 0) {
    imageDetail = 'No Container CLI image metadata has been reported yet.'
  } else if (missingImages.length > 0) {
    imageDetail = `${missingImages.length} Container CLI image${
      missingImages.length === 1 ? '' : 's'
    } could not be inspected. Run make update-agents or make build-agent-all, then refresh.`
  } else if (reportedVersionCount !== runtimeSettings.cliToolDetails.length) {
    imageDetail = `${reportedVersionCount}/${runtimeSettings.cliToolDetails.length} Container CLI versions reported. Rebuild missing images with CLI_VERSION labels.`
  }
  items.push({
    id: 'images',
    title: 'CLI image inventory',
    detail: imageDetail,
    ready: imageInventoryReady,
    action: imageInventoryReady ? undefined : 'refresh',
    actionLabel: imageInventoryReady ? undefined : 'Refresh Status',
  })

  const connectedCredentialCount = cliStatuses.filter((status) => status.connected).length
  const disconnectedCredential = cliStatuses.find((status) => !status.connected)
  const credentialReady = !cliStatusError && (!disconnectedCredential || cliStatuses.length === 0)
  items.push({
    id: 'credentials',
    title: 'CLI credentials',
    detail: cliStatusError
      ? 'Credential status could not be checked. Refresh after the API is healthy.'
      : cliStatuses.length === 0
        ? 'No CLI OAuth providers require connection.'
        : disconnectedCredential
          ? `${connectedCredentialCount}/${cliStatuses.length} credentials connected. Reconnect ${disconnectedCredential.displayName} before starting new CLI agents.`
          : `${connectedCredentialCount}/${cliStatuses.length} credentials connected.`,
    ready: credentialReady,
    action: cliStatusError ? 'refresh' : disconnectedCredential ? 'connect' : undefined,
    actionLabel: cliStatusError
      ? 'Refresh Status'
      : disconnectedCredential
        ? `Connect ${disconnectedCredential.displayName}`
        : undefined,
    provider: disconnectedCredential?.provider,
  })

  items.push({
    id: 'heartbeats',
    title: 'Agent heartbeat',
    detail: participantsError
      ? 'Agent heartbeat status could not be checked. Refresh after orchestration is healthy.'
      : latestHeartbeat
        ? `Latest agent heartbeat ${formatRelativeTime(latestHeartbeat)}.`
        : 'No agent heartbeat has been observed yet. Start or wake an agent runtime, then refresh.',
    ready: !participantsError && Boolean(latestHeartbeat),
    action: participantsError || !latestHeartbeat ? 'refresh' : undefined,
    actionLabel: participantsError || !latestHeartbeat ? 'Refresh Status' : undefined,
  })

  return items
}

function versionSourceLabel(source: string, imagePresent: boolean): string {
  if (source === 'docker-label') return 'image found'
  if (source === 'image-tag') return imagePresent ? 'tag fallback' : 'not inspected'
  return 'not reported'
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
