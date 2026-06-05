import { useEffect, useState } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import {
  useAdminStore,
  type CliImagePruneStatus,
  type CliImageRollReport,
  type CliImageTool,
  type CliImageToolState,
} from '@app/shared/model/admin.store'
import { CLI_IMAGE_RECOVERY, cliImageStatusErrorMessage } from './adminErrorCopy'

// ============================================================================
// Presentation helpers
// ============================================================================

/** Plain-language label per update state. */
function stateLabel(state: CliImageToolState): string {
  switch (state) {
    case 'up_to_date':
      return 'Up to date'
    case 'updated':
      return 'Just updated'
    case 'failed':
      return 'Check failed'
    case 'pending':
      return 'Not checked yet'
  }
}

function stateTone(state: CliImageToolState): string {
  if (state === 'failed') return 'bg-apple-red/10 text-apple-red'
  if (state === 'up_to_date' || state === 'updated') return 'bg-apple-blue/10 text-apple-blue'
  return 'bg-black/[0.05] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark'
}

function stateDot(state: CliImageToolState): string {
  if (state === 'failed') return 'bg-apple-red'
  if (state === 'up_to_date' || state === 'updated') return 'bg-apple-blue'
  return 'bg-gray-400'
}

/** `sha256:abcdef…` → `abcdef…` truncated for display. */
function shortDigest(digest: string | null): string {
  if (!digest) return '—'
  const bare = digest.includes(':') ? (digest.split(':').pop() ?? digest) : digest
  return bare.length > 12 ? `${bare.slice(0, 12)}…` : bare
}

/** Unix seconds → coarse "x ago" relative to now. */
function relativeTime(unix: number | null): string {
  if (!unix) return 'never'
  const seconds = Math.max(0, Math.floor(Date.now() / 1000) - unix)
  if (seconds < 60) return 'just now'
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)}h ago`
  return `${Math.floor(seconds / 86_400)}d ago`
}

function toolLabel(tool: string): string {
  switch (tool) {
    case 'claude':
      return 'Claude'
    case 'codex':
      return 'Codex'
    case 'gemini':
      return 'Gemini'
    case 'opencode':
      return 'OpenCode'
    default:
      return tool
        .split(/[-_\s]+/)
        .filter(Boolean)
        .map((part) => part.charAt(0).toUpperCase() + part.slice(1).toLowerCase())
        .join(' ')
  }
}

type CliImageIssueContext = 'check' | 'restart' | 'cleanup'

function cliImageIssueNote(error: string, context: CliImageIssueContext): string {
  const detail = error.toLowerCase()

  if (
    detail.includes('already in progress') ||
    detail.includes('busy') ||
    detail.includes('too many') ||
    detail.includes('rate limit')
  ) {
    if (context === 'restart') {
      return 'Another restart is already running. Wait for it to finish, then check this page again.'
    }
    return 'The tool update service is busy. Wait a minute, then choose Check now.'
  }
  if (
    detail.includes('password') ||
    detail.includes('token') ||
    detail.includes('secret') ||
    detail.includes('credential') ||
    detail.includes('unauthorized') ||
    detail.includes('forbidden') ||
    detail.includes('permission') ||
    detail.includes('auth')
  ) {
    return 'The tool updater reported an access setup problem. Ask an owner to check tool package access, then choose Check now.'
  }
  if (
    detail.includes('connection') ||
    detail.includes('refused') ||
    detail.includes('unreachable') ||
    detail.includes('timeout') ||
    detail.includes('timed out') ||
    detail.includes('registry')
  ) {
    return 'The updater could not reach the tool package source. Check network access, then choose Check now.'
  }
  if (
    detail.includes('space') ||
    detail.includes('disk') ||
    detail.includes('overlay') ||
    detail.includes('cleanup') ||
    detail.includes('prune')
  ) {
    return 'Old package cleanup could not finish. Ask an owner to check disk space, then choose Check now.'
  }
  if (
    context === 'restart' ||
    detail.includes('stop') ||
    detail.includes('start') ||
    detail.includes('restart') ||
    detail.includes('respawn') ||
    detail.includes('container')
  ) {
    return 'Some agents could not restart cleanly. Open Agents, check affected agents, then restart them one at a time.'
  }
  if (context === 'cleanup') {
    return 'Old package cleanup could not finish. Ask an owner to check tool package cleanup, then choose Check now.'
  }

  return 'The tool updater reported a problem. Choose Check now again, then ask an owner to check the tool updater if it still fails.'
}

function StateBadge({ state, label }: { state: CliImageToolState; label?: string }) {
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-ui-caption font-medium',
        stateTone(state)
      )}
    >
      <span className={cn('w-2 h-2 rounded-full flex-shrink-0', stateDot(state))} />
      {label ?? stateLabel(state)}
    </span>
  )
}

// ============================================================================
// Tool row
// ============================================================================

interface RollControl {
  confirming: boolean
  rolling: boolean
  onRequest: () => void
  onConfirm: () => void
  onCancel: () => void
}

function RollButton({ tool, control }: { tool: CliImageTool; control: RollControl }) {
  // Only offer a roll when there is something to roll.
  if (tool.agentsWithContainer === 0) return null

  if (control.rolling) {
    return (
      <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
        Restarting agents…
      </span>
    )
  }
  if (control.confirming) {
    return (
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={control.onConfirm}
          className="rounded-full bg-apple-red/10 px-3 py-1 text-ui-caption font-medium text-apple-red"
        >
          Restart {tool.agentsWithContainer} agents now
        </button>
        <button
          type="button"
          onClick={control.onCancel}
          className="text-ui-caption text-secondary-light dark:text-secondary-dark"
        >
          Cancel
        </button>
      </div>
    )
  }
  return (
    <button
      type="button"
      onClick={control.onRequest}
      className="rounded-full border border-black/[0.1] px-3 py-1 text-ui-caption font-medium text-foreground-light dark:border-white/[0.12] dark:text-foreground-dark"
    >
      Restart on latest tool
    </button>
  )
}

function ToolRow({
  tool,
  enabled,
  roll,
}: {
  tool: CliImageTool
  enabled: boolean
  roll: RollControl
}) {
  // `pending` means no check has recorded a result. Distinguish the common
  // cause — auto-update is off, so nothing will ever check — from "enabled but
  // the first tick hasn't run yet", so an operator can't read gray "pending" as
  // "verified fine".
  const pendingOff = tool.state === 'pending' && !enabled
  const badgeLabel = pendingOff ? 'Not checked — updates off' : undefined

  return (
    <div className={cn('grid gap-3 px-4 py-3 sm:grid-cols-[1fr_auto]', uiStyles.row)}>
      <div className="flex min-w-0 gap-3">
        <span className={cn('mt-1.5 w-2 h-2 rounded-full flex-shrink-0', stateDot(tool.state))} />
        <div className="min-w-0">
          <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
            {toolLabel(tool.tool)}
          </p>
          <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
            {tool.agentsWithContainer === 1
              ? '1 agent is currently using this tool'
              : `${tool.agentsWithContainer} agents are currently using this tool`}
          </p>
          {tool.state === 'pending' ? (
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              {enabled
                ? 'No result yet — the first check has not finished.'
                : 'This tool has never been checked because automatic updates are off.'}
            </p>
          ) : (
            <div className="mt-1 grid gap-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
              {/* The locally-pulled image the NEXT agent will start from — not
                  necessarily what already-running agents booted from. */}
              <span className="font-mono">
                package for new agents: {shortDigest(tool.localDigest)}
              </span>
              <span className="font-mono">
                latest available package: {shortDigest(tool.remoteDigest)}
              </span>
              <span>last checked {relativeTime(tool.lastCheckedUnix)}</span>
            </div>
          )}
          {tool.state === 'failed' && tool.lastError && (
            <div className="mt-2 rounded-card border border-apple-red/20 bg-apple-red/[0.04] px-3 py-2">
              <p className="text-ui-caption text-foreground-light dark:text-foreground-dark">
                Last check failed. New agents keep the current tool package until the next check
                succeeds.
              </p>
              <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                Support note: {cliImageIssueNote(tool.lastError, 'check')}
              </p>
            </div>
          )}
        </div>
      </div>
      <div className="flex flex-col items-end gap-2 shrink-0 ml-4">
        <StateBadge state={tool.state} label={badgeLabel} />
        <RollButton tool={tool} control={roll} />
      </div>
    </div>
  )
}

// ============================================================================
// Config banner
// ============================================================================

function ConfigBanner({ enabled, intervalSecs }: { enabled: boolean; intervalSecs: number }) {
  const intervalLabel =
    intervalSecs < 120 ? `${intervalSecs}s` : `${Math.round(intervalSecs / 60)} min`

  return (
    <div
      className={cn(
        'mb-6 flex items-start gap-3 rounded-card border px-4 py-3',
        enabled
          ? 'border-apple-blue/20 bg-apple-blue/10'
          : 'border-black/[0.08] bg-black/[0.03] dark:border-white/[0.08] dark:bg-white/[0.03]'
      )}
    >
      <span
        className={cn('mt-1 w-2.5 h-2.5 rounded-full', enabled ? 'bg-apple-blue' : 'bg-[#86868b]')}
      />
      <div className="min-w-0">
        <p
          className={cn(
            'text-ui-body font-medium',
            enabled ? 'text-apple-blue' : 'text-secondary-light dark:text-secondary-dark'
          )}
        >
          {enabled ? 'Automatic updates are on' : 'Automatic updates are off'}
        </p>
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {enabled
            ? `This service checks for newer agent tool packages about every ${intervalLabel} and downloads them so new agents start on the latest tool version. Running agents are never interrupted.`
            : 'New agents keep using the tool package that was last downloaded. Ask an owner or admin to turn on automatic tool updates in Admin settings so updates are checked and downloaded automatically.'}
        </p>
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          Tool package source is managed in Admin settings.
        </p>
      </div>
    </div>
  )
}

// ============================================================================
// CliImagesPanel
// ============================================================================

export function CliImagesPanel() {
  const {
    cliImages,
    cliImagesLoading,
    cliImagesError,
    loadCliImages,
    rollCliImage,
    cliImageRollingTool,
    cliImageRollResult,
    cliImageRollError,
  } = useAdminStore()
  const [confirmTool, setConfirmTool] = useState<string | null>(null)

  useEffect(() => {
    void loadCliImages()
    const interval = setInterval(() => void loadCliImages(), 30_000)
    return () => clearInterval(interval)
  }, [loadCliImages])

  const rollControlFor = (tool: string): RollControl => ({
    confirming: confirmTool === tool,
    rolling: cliImageRollingTool === tool,
    onRequest: () => setConfirmTool(tool),
    onCancel: () => setConfirmTool(null),
    onConfirm: () => {
      setConfirmTool(null)
      void rollCliImage(tool)
    },
  })

  return (
    <div>
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>Agent tool updates</h2>
          <p className={uiStyles.sectionDescription}>
            Shows whether each agent tool package is up to date. New agents use the latest checked
            package. Refreshes every 30 seconds.
          </p>
        </div>
        <button
          type="button"
          onClick={() => void loadCliImages()}
          disabled={cliImagesLoading}
          className={uiStyles.secondaryButton}
        >
          {cliImagesLoading ? 'Checking...' : 'Check now'}
        </button>
      </div>

      {cliImagesError && !cliImages && (
        <div className={uiStyles.error}>
          <p>{cliImageStatusErrorMessage(cliImagesError)}</p>
          <p className="mt-1 text-ui-caption">{CLI_IMAGE_RECOVERY}</p>
        </div>
      )}

      {cliImagesLoading && !cliImages && (
        <div className="flex items-center justify-center py-12">
          <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
            Checking agent tool update status...
          </p>
        </div>
      )}

      {/* A failed background refresh after a first success leaves stale data on
          screen. Surface that so the operator never reads minutes-old digests
          as current. */}
      {cliImagesError && cliImages && (
        <div className={cn(uiStyles.error, 'mb-4')}>
          <p>
            The status below may be out of date. Do not restart agents from this table until Check
            now succeeds.
          </p>
          <p className="mt-1 text-ui-caption">{CLI_IMAGE_RECOVERY}</p>
        </div>
      )}

      {cliImages && (
        <>
          <ConfigBanner
            enabled={cliImages.autoUpdateEnabled}
            intervalSecs={cliImages.pollIntervalSecs}
          />

          <div className={cn(uiStyles.card)}>
            {cliImages.tools.length === 0 ? (
              <p className="px-4 py-6 text-ui-body text-secondary-light dark:text-secondary-dark">
                No agent tools are configured for update checks.
              </p>
            ) : (
              cliImages.tools.map((tool) => (
                <ToolRow
                  key={tool.tool}
                  tool={tool}
                  enabled={cliImages.autoUpdateEnabled}
                  roll={rollControlFor(tool.tool)}
                />
              ))
            )}
          </div>

          <RollResultBlock result={cliImageRollResult} error={cliImageRollError} />

          <PruneSummaryBlock prune={cliImages.prune} />

          <p className="mt-4 text-ui-caption text-secondary-light dark:text-secondary-dark">
            “Agents currently using this tool” is a rough hint of how many agents may restart. It
            does not confirm which exact package each one started from.
          </p>
        </>
      )}
    </div>
  )
}

function RollResultBlock({
  result,
  error,
}: {
  result: CliImageRollReport | null
  error: string | null
}) {
  if (error) {
    return (
      <div className={cn(uiStyles.error, 'mt-4')}>
        The restart could not be started.
        <span className="mt-1 block text-ui-caption">{cliImageIssueNote(error, 'restart')}</span>
      </div>
    )
  }
  if (!result) return null
  const nowStopped = result.results.filter((r) => !r.ok && r.stopped)
  const stillRunning = result.results.filter((r) => !r.ok && !r.stopped)
  const firstError =
    nowStopped.find((r) => r.error)?.error ?? stillRunning.find((r) => r.error)?.error
  return (
    <div className="mt-4 rounded-card border border-black/[0.06] bg-black/[0.02] px-4 py-3 dark:border-white/[0.08] dark:bg-white/[0.03]">
      <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
        Last restart: {toolLabel(result.tool)}
      </p>
      <p className="mt-1 text-ui-caption tabular-nums text-secondary-light dark:text-secondary-dark">
        {result.succeeded} of {result.total} agents restarted
        {result.failed > 0 ? ` · ${result.failed} failed` : ''}
        {result.skippedBusy > 0 ? ` · ${result.skippedBusy} skipped (busy)` : ''}
      </p>
      {result.skippedBusy > 0 && (
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          Busy agents were left running to avoid interrupting their work. Restart again once they
          are idle.
        </p>
      )}
      {(nowStopped.length > 0 || stillRunning.length > 0) && (
        <div className="mt-2 rounded-card border border-apple-red/20 bg-apple-red/[0.04] px-3 py-2">
          {nowStopped.length > 0 && (
            <p className="text-ui-caption text-foreground-light dark:text-foreground-dark">
              {nowStopped.length} {nowStopped.length === 1 ? 'agent' : 'agents'} did not restart and{' '}
              {nowStopped.length === 1 ? 'is' : 'are'} now stopped — restart from the Agents view.
            </p>
          )}
          {stillRunning.length > 0 && (
            <p className="text-ui-caption text-foreground-light dark:text-foreground-dark">
              {stillRunning.length} {stillRunning.length === 1 ? 'agent' : 'agents'} could not be
              stopped cleanly — {stillRunning.length === 1 ? 'it' : 'they'} may still be running on
              the previous tool version, or may have stopped without restarting. Check the Agents
              view and restart if needed.
            </p>
          )}
          {firstError && (
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Support note: {cliImageIssueNote(firstError, 'restart')}
            </p>
          )}
        </div>
      )}
    </div>
  )
}

function PruneSummaryBlock({ prune }: { prune: CliImagePruneStatus }) {
  // Three distinct states: off-by-config; configured on but no sweep has run
  // (commonly auto-update off); configured on and ran.
  const neverRan = prune.enabled && prune.lastRunUnix === null
  const hasErrors = prune.enabled && prune.errors > 0

  return (
    <div className="mt-4 rounded-card border border-black/[0.06] bg-black/[0.02] px-4 py-3 dark:border-white/[0.08] dark:bg-white/[0.03]">
      <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
        Old tool package cleanup
      </p>
      {!prune.enabled && (
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          Off. Old tool packages are kept until removed manually. Ask an owner or admin to turn on
          automatic cleanup for old tool packages in Admin settings to reclaim disk automatically.
        </p>
      )}
      {neverRan && (
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          On, but no cleanup has run yet. Cleanup runs as part of the update check — ask an owner or
          admin to confirm automatic tool updates are on, or wait for the first check to finish.
        </p>
      )}
      {prune.enabled && prune.lastRunUnix !== null && (
        <>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Old agent tool packages are removed automatically after each check, freeing disk. Only
            unused packages for these tools are removed — never a package an agent is using.
          </p>
          <p className="mt-1 text-ui-caption tabular-nums text-secondary-light dark:text-secondary-dark">
            Last sweep: {prune.removed} removed · {prune.skippedInUse} still in use ·{' '}
            {prune.scanned} scanned
            {prune.errors > 0 ? ` · ${prune.errors} errors` : ''} · checked{' '}
            {relativeTime(prune.lastRunUnix)}
          </p>
        </>
      )}
      {hasErrors && prune.lastError && (
        <div className="mt-2 rounded-card border border-apple-red/20 bg-apple-red/[0.04] px-3 py-2">
          <p className="text-ui-caption text-foreground-light dark:text-foreground-dark">
            The last cleanup hit {prune.errors} {prune.errors === 1 ? 'error' : 'errors'}.
          </p>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Support note: {cliImageIssueNote(prune.lastError, 'cleanup')}
          </p>
        </div>
      )}
    </div>
  )
}
