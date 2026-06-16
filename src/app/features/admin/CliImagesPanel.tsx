import { useEffect, useMemo, useRef, useState } from 'react'
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

const MIN_STATUS_REFRESH_MS = 60_000
const DEFAULT_STATUS_REFRESH_MS = 5 * 60_000

// ============================================================================
// Presentation helpers
// ============================================================================

/** Plain-language label per update state. */
function stateLabel(state: CliImageToolState): string {
  switch (state) {
    case 'up_to_date':
      return 'Up to date'
    case 'update_available':
      return 'Update available'
    case 'updated':
      return 'Just updated'
    case 'failed':
      return 'Choose Check now'
    case 'pending':
      return 'Run first check'
  }
}

function stateTone(state: CliImageToolState): string {
  if (state === 'failed') return 'bg-apple-red/10 text-apple-red'
  if (state === 'update_available') return 'bg-apple-orange/10 text-apple-orange'
  if (state === 'up_to_date' || state === 'updated') return 'bg-apple-blue/10 text-apple-blue'
  return 'bg-black/[0.05] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark'
}

function stateDot(state: CliImageToolState): string {
  if (state === 'failed') return 'bg-apple-red'
  if (state === 'update_available') return 'bg-apple-orange'
  if (state === 'up_to_date' || state === 'updated') return 'bg-apple-blue'
  return 'bg-gray-400'
}

function currentToolStatus(tool: CliImageTool): string {
  if (!tool.localDigest) return 'Choose Check now to prepare the first tool'
  return 'Ready for new agents'
}

function latestToolStatus(tool: CliImageTool): string {
  if (!tool.remoteDigest) return 'Choose Check now to check for updates'
  if (tool.state === 'update_available') return 'Update ready for new agents'
  if (tool.state === 'failed') return 'Current tool kept until a check succeeds'
  return 'No update needed'
}

function versionMarker(version: string | null): string {
  return version ? `v${version}` : 'Choose Check now to find it'
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

function statusRefreshMs(pollIntervalSecs?: number): number {
  if (!pollIntervalSecs || pollIntervalSecs <= 0) return DEFAULT_STATUS_REFRESH_MS
  return Math.max(pollIntervalSecs * 1000, MIN_STATUS_REFRESH_MS)
}

function statusRefreshLabel(ms: number): string {
  const minutes = Math.max(1, Math.round(ms / 60_000))
  if (minutes < 60) return `${minutes} minute${minutes === 1 ? '' : 's'}`
  const hours = Math.round(minutes / 60)
  return `${hours} hour${hours === 1 ? '' : 's'}`
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
    return 'Another tool update is already running. Wait a minute, then choose Check now.'
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
    return 'The tool updater reported an access setup problem. Ask an owner or admin to check tool package access, then choose Check now.'
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
    return 'Old package cleanup could not finish. Ask an owner or admin to check disk space, then choose Check now.'
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
    return 'Old package cleanup could not finish. Ask an owner or admin to check tool package cleanup, then choose Check now.'
  }

  return 'The tool updater reported a problem. Choose Check now again, then ask an owner or admin to check tool update setup if it still fails.'
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
  // Only offer a roll when there is something to roll. Local-build tools
  // (claude) are not rollable — the backend rejects them with 422; restart
  // those agents individually from the Agents view instead.
  if (tool.agentsWithContainer === 0 || tool.updateMode === 'local_build') return null

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
      Restart agents on latest tool
    </button>
  )
}

interface BuildControl {
  /** Auto-build flag from deployment config (zero-click builds). */
  autoBuildOn: boolean
  onBuild: () => void
}

/**
 * One-click local build for a `local_build` tool (claude). Prominent when an
 * update is waiting; a disabled progress label while the server builds; and a
 * quiet "Build latest" when nothing has been checked yet or the last attempt
 * failed — the build endpoint looks up npm itself, so it works even while
 * automatic checks are off.
 */
function BuildButton({ tool, control }: { tool: CliImageTool; control: BuildControl }) {
  if (tool.updateMode !== 'local_build') return null

  if (tool.building) {
    return (
      <button
        type="button"
        disabled
        className="rounded-full bg-apple-blue/60 px-3 py-1 text-ui-caption font-medium text-white"
      >
        Building…
      </button>
    )
  }
  if (tool.state === 'update_available') {
    return (
      <button
        type="button"
        onClick={control.onBuild}
        className="rounded-full bg-apple-blue px-3 py-1 text-ui-caption font-medium text-white"
      >
        Build {tool.remoteVersion ? `v${tool.remoteVersion}` : 'update'}
      </button>
    )
  }
  if (tool.state === 'pending' || tool.state === 'failed') {
    return (
      <button
        type="button"
        onClick={control.onBuild}
        className="rounded-full border border-black/[0.1] px-3 py-1 text-ui-caption font-medium text-foreground-light dark:border-white/[0.12] dark:text-foreground-dark"
      >
        Build latest
      </button>
    )
  }
  return null
}

function ToolRow({
  tool,
  enabled,
  roll,
  build,
}: {
  tool: CliImageTool
  enabled: boolean
  roll: RollControl
  build: BuildControl
}) {
  const localBuild = tool.updateMode === 'local_build'
  // `pending` means no check has recorded a result. Distinguish the common
  // cause — auto-update is off, so nothing will ever check — from "enabled but
  // the first tick hasn't run yet", so an operator can't read gray "pending" as
  // "verified fine".
  const pendingOff = tool.state === 'pending' && !enabled
  const badgeLabel = pendingOff ? 'Check manually or turn updates on' : undefined

  return (
    <div className={cn('grid gap-3 px-4 py-3 sm:grid-cols-[1fr_auto]', uiStyles.row)}>
      <div className="flex min-w-0 gap-3">
        <span className={cn('mt-1.5 w-2 h-2 rounded-full flex-shrink-0', stateDot(tool.state))} />
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
              {toolLabel(tool.tool)}
            </p>
            {localBuild && (
              <span className="rounded-full bg-black/[0.05] px-2 py-0.5 text-ui-caption text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
                Built here
              </span>
            )}
          </div>
          <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
            {tool.agentsWithContainer === 1
              ? '1 agent is currently using this tool'
              : `${tool.agentsWithContainer} agents are currently using this tool`}
          </p>
          {tool.state === 'pending' ? (
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              {enabled
                ? 'The first check is still running. Wait, then choose Check now if this stays unchanged.'
                : 'Automatic updates are off. Choose Check now for a manual check, or ask an owner or admin to turn updates on.'}
            </p>
          ) : localBuild ? (
            <div className="mt-1 grid gap-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
              {/* Built on this server (no public registry image), so versions —
                  not registry digests — are the meaningful comparison. */}
              <span>
                Current version: {versionMarker(tool.localVersion)}
                {tool.state === 'update_available' && tool.remoteVersion
                  ? ` -> latest available: v${tool.remoteVersion}`
                  : ''}
              </span>
              <span>last checked {relativeTime(tool.lastCheckedUnix)}</span>
            </div>
          ) : (
            <div className="mt-1 grid gap-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
              <span>New agents use: {currentToolStatus(tool)}</span>
              <span>Latest check: {latestToolStatus(tool)}</span>
              <span>last checked {relativeTime(tool.lastCheckedUnix)}</span>
            </div>
          )}
          {localBuild && tool.state === 'update_available' && !tool.building && (
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Building only affects new agents — running agents keep working.
            </p>
          )}
          {localBuild && tool.building && (
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Building on this server — usually a few minutes. You can leave this page.
            </p>
          )}
          {tool.state === 'failed' && tool.lastError && (
            <div className="mt-2 rounded-card border border-apple-red/20 bg-apple-red/[0.04] px-3 py-2">
              <p className="text-ui-caption text-foreground-light dark:text-foreground-dark">
                New agents keep the current tool package until the next check succeeds.
              </p>
              <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                What to do: {cliImageIssueNote(tool.lastError, 'check')}
              </p>
            </div>
          )}
        </div>
      </div>
      <div className="flex flex-col items-end gap-2 shrink-0 ml-4">
        <StateBadge state={tool.state} label={badgeLabel} />
        <BuildButton tool={tool} control={build} />
        {localBuild && build.autoBuildOn && (
          <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
            Builds automatically — new versions build themselves
          </span>
        )}
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
            ? `Forge checks for newer agent tool packages about every ${intervalLabel} and downloads them so new agents start on the latest tool version. Running agents are never interrupted.`
            : 'New agents keep using the tool package that was last downloaded. Ask an owner or admin to turn on automatic tool updates in Admin settings so updates are checked and downloaded automatically.'}
        </p>
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          Where updates come from is managed in Admin settings.
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
    buildClaudeImage,
    cliImageBuildError,
  } = useAdminStore()
  const [confirmTool, setConfirmTool] = useState<string | null>(null)
  const refreshMs = useMemo(
    () => statusRefreshMs(cliImages?.pollIntervalSecs),
    [cliImages?.pollIntervalSecs]
  )
  const refreshLabel = statusRefreshLabel(refreshMs)
  const firstLoadRequestedRef = useRef(false)

  useEffect(() => {
    if (!firstLoadRequestedRef.current) {
      firstLoadRequestedRef.current = true
      void loadCliImages()
    }
    const interval = setInterval(() => {
      if (document.visibilityState === 'hidden') return
      void loadCliImages()
    }, refreshMs)
    return () => clearInterval(interval)
  }, [loadCliImages, refreshMs])

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

  const buildControl: BuildControl = {
    autoBuildOn: cliImages?.claudeAutoBuildEnabled ?? false,
    onBuild: () => void buildClaudeImage(),
  }

  return (
    <div>
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>Agent tool updates</h2>
          <p className={uiStyles.sectionDescription}>
            Shows whether each agent tool is ready for new agents. New agents use the latest
            successful check. This page checks when opened, then about every {refreshLabel} while
            visible. Checks pause when this browser tab is hidden.
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
        <div role="alert" aria-live="polite" className={uiStyles.error}>
          <p>{cliImageStatusErrorMessage(cliImagesError)}</p>
          <p className="mt-1 text-ui-caption">{CLI_IMAGE_RECOVERY}</p>
        </div>
      )}

      {cliImagesLoading && !cliImages && (
        <div className="flex items-center justify-center py-12">
          <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
            Checking agent tool updates...
          </p>
        </div>
      )}

      {/* A failed background refresh after a first success leaves stale data on
          screen. Surface that so the operator never reads minutes-old digests
          as current. */}
      {cliImagesError && cliImages && (
        <div role="alert" aria-live="polite" className={cn(uiStyles.error, 'mb-4')}>
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
              <div
                data-testid="cli-images-empty-tools"
                className="px-4 py-6 text-center text-ui-body"
              >
                <p className="font-medium text-foreground-light dark:text-foreground-dark">
                  No agent tools are ready for update checks
                </p>
                <p className="mx-auto mt-1 max-w-xl text-secondary-light dark:text-secondary-dark">
                  Open Agents to add or enable a work tool, then return here and choose Check now
                  before restarting agents.
                </p>
              </div>
            ) : (
              cliImages.tools.map((tool) => (
                <ToolRow
                  key={tool.tool}
                  tool={tool}
                  enabled={cliImages.autoUpdateEnabled}
                  roll={rollControlFor(tool.tool)}
                  build={buildControl}
                />
              ))
            )}
          </div>

          {cliImageBuildError && (
            <div role="alert" aria-live="polite" className={cn(uiStyles.error, 'mt-4')}>
              Check the note below, then choose Build again. Nothing was changed.
              <span className="mt-1 block text-ui-caption">
                {cliImageIssueNote(cliImageBuildError, 'check')}
              </span>
            </div>
          )}

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
      <div role="alert" aria-live="polite" className={cn(uiStyles.error, 'mt-4')}>
        Check the note below, then restart affected agents again.
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
        {result.failed > 0 ? ` · ${result.failed} need a retry` : ''}
        {result.skippedBusy > 0 ? ` · ${result.skippedBusy} still working` : ''}
      </p>
      {result.skippedBusy > 0 && (
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          Agents still working were left running to avoid interrupting their work. Restart again
          once they show Ready.
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
              What to do: {cliImageIssueNote(firstError, 'restart')}
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
            Last cleanup: {prune.removed} old packages removed · {prune.skippedInUse} kept because
            agents use them · {prune.scanned} checked
            {prune.errors > 0 ? ` · ${prune.errors} need a check` : ''} · ran{' '}
            {relativeTime(prune.lastRunUnix)}
          </p>
        </>
      )}
      {hasErrors && prune.lastError && (
        <div className="mt-2 rounded-card border border-apple-red/20 bg-apple-red/[0.04] px-3 py-2">
          <p className="text-ui-caption text-foreground-light dark:text-foreground-dark">
            The last cleanup needs a check for {prune.errors}{' '}
            {prune.errors === 1 ? 'package' : 'packages'}.
          </p>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            What to do: {cliImageIssueNote(prune.lastError, 'cleanup')}
          </p>
        </div>
      )}
    </div>
  )
}
