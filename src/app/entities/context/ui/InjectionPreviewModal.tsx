import { useEffect, useMemo, useRef, useState } from 'react'
import { AlertTriangle, Pin, PinOff, RefreshCw, X } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { formatRelativeTime } from '@app/shared/lib/time'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { trackProductEvent } from '@app/shared/api/orchestration'
import { suggestContextTrims } from '../model/contextTrim'
import type { ContextPreviewItem, ContextPreviewResponse } from '@shared/types/context'

interface InjectionPreviewModalProps {
  isOpen: boolean
  preview: ContextPreviewResponse | null
  loading?: boolean
  publishing?: boolean
  error?: string | null
  onClose: () => void
  onConfirm: (selection: { pinnedIds: string[]; removedIds: string[] }) => void | Promise<void>
}

export function InjectionPreviewModal({
  isOpen,
  preview,
  loading = false,
  publishing = false,
  error = null,
  onClose,
  onConfirm,
}: InjectionPreviewModalProps) {
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const [pinnedIds, setPinnedIds] = useState<Set<string>>(new Set())

  useEffect(() => {
    if (!preview) {
      setSelectedIds(new Set())
      setPinnedIds(new Set())
      return
    }
    setSelectedIds(new Set(preview.items.map((item) => item.id)))
    setPinnedIds(new Set(preview.previouslyPinned.map((item) => item.id)))
  }, [preview])

  useEffect(() => {
    if (!isOpen) return
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape' && !publishing) onClose()
    }
    document.addEventListener('keydown', onKeyDown)
    return () => document.removeEventListener('keydown', onKeyDown)
  }, [isOpen, onClose, publishing])

  const defaultIds = useMemo(() => new Set(preview?.items.map((item) => item.id) ?? []), [preview])
  const removedIds = useMemo(
    () => [...defaultIds].filter((id) => !selectedIds.has(id)),
    [defaultIds, selectedIds]
  )
  const selectedCount = selectedIds.size
  const budget = budgetLabel(preview?.capability)
  const selectedSummary = `${selectedCount} item${selectedCount === 1 ? '' : 's'} selected`
  const selectedTokens = useMemo(() => {
    const items = preview?.items ?? []
    return items
      .filter((item) => selectedIds.has(item.id))
      .reduce(
        (sum, item) => sum + (Number.isFinite(item.estimatedTokens) ? item.estimatedTokens : 0),
        0
      )
  }, [preview, selectedIds])
  const tokenBudget =
    typeof preview?.capability?.max_context_tokens === 'number'
      ? (preview.capability.max_context_tokens as number)
      : null
  const budgetRatio = tokenBudget && tokenBudget > 0 ? selectedTokens / tokenBudget : null
  const budgetWarning =
    budgetRatio === null
      ? null
      : budgetRatio > 1
        ? 'This exceeds the agent context budget: remove items until it fits.'
        : budgetRatio > 0.8
          ? 'This uses most of the agent context budget: the agent may lose earlier work or skip items.'
          : null

  // One-click compaction suggestion: remove the least-recently-used items
  // (pinned items are protected) until the selection fits the budget.
  const trimSuggestion = useMemo(() => {
    if (!preview || tokenBudget === null) return []
    return suggestContextTrims(preview.items, selectedIds, tokenBudget, pinnedIds)
  }, [preview, selectedIds, tokenBudget, pinnedIds])

  // Best-effort measurement: one warning event per open session, showing which
  // tasks were already at risk before (or despite) a run — the raw signal for
  // context-safety remediation.
  const warnedRef = useRef(false)
  useEffect(() => {
    if (!isOpen) {
      warnedRef.current = false
      return
    }
    if (!budgetWarning || warnedRef.current) return
    warnedRef.current = true
    void trackProductEvent('context_budget_warning', {
      taskId: preview?.taskId ?? '',
      ratio: budgetRatio === null ? undefined : Math.round(budgetRatio * 100) / 100,
      overLimit: budgetRatio === null ? undefined : budgetRatio > 1,
    })
  }, [isOpen, budgetWarning, budgetRatio, preview?.taskId])

  if (!isOpen) return null

  async function confirm() {
    await onConfirm({ pinnedIds: [...pinnedIds], removedIds })
  }

  function applyTrim() {
    if (trimSuggestion.length === 0) return
    setSelectedIds((current) => {
      const next = new Set(current)
      for (const id of trimSuggestion) next.delete(id)
      return next
    })
    void trackProductEvent('context_trim_applied', {
      taskId: preview?.taskId ?? '',
      removed: trimSuggestion.length,
      ratioBefore: budgetRatio === null ? undefined : Math.round(budgetRatio * 100) / 100,
    })
  }

  function toggleSelected(id: string) {
    setSelectedIds((current) => {
      const next = new Set(current)
      if (next.has(id)) {
        next.delete(id)
        setPinnedIds((pins) => {
          const without = new Set(pins)
          without.delete(id)
          return without
        })
      } else {
        next.add(id)
      }
      return next
    })
  }

  function togglePinned(id: string) {
    setSelectedIds((current) => new Set(current).add(id))
    setPinnedIds((current) => {
      const next = new Set(current)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center px-3 py-4">
      <button
        type="button"
        className="absolute inset-0 bg-black/40 backdrop-blur-sm"
        aria-label="Close context items check"
        onClick={() => {
          if (!publishing) onClose()
        }}
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="context-preview-title"
        className={cn(
          'relative flex max-h-[92vh] w-full max-w-3xl flex-col overflow-hidden rounded-panel',
          'border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-surface-dark'
        )}
      >
        <div className="flex items-start justify-between gap-3 border-b border-black/[0.06] px-4 py-3 dark:border-white/[0.08]">
          <div className="min-w-0">
            <h2
              id="context-preview-title"
              className="text-ui-title font-semibold text-foreground-light dark:text-foreground-dark"
            >
              Check context items before sending
            </h2>
            <p
              className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
              data-testid="context-fit-summary"
            >
              {selectedSummary} ·{' '}
              {selectedTokens > 0
                ? `${selectedTokens.toLocaleString()} tokens`
                : 'no sizable items'}{' '}
              · {budget}
            </p>
            {trimSuggestion.length > 0 && (
              <button
                type="button"
                data-testid="context-trim-button"
                onClick={applyTrim}
                className="mt-2 inline-flex items-center gap-1 rounded-button bg-apple-blue px-3 py-1.5 text-ui-button font-medium text-white transition-colors hover:bg-apple-blue/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
              >
                Trim to fit (remove {trimSuggestion.length} item{trimSuggestion.length === 1 ? '' : 's'})
              </button>
            )}
            {budgetWarning && (
              <p
                role="status"
                data-testid="context-budget-warning"
                className={cn(
                  'mt-2 rounded-card border px-3 py-2 text-ui-caption font-medium',
                  budgetRatio !== null && budgetRatio > 1
                    ? 'border-apple-red/30 bg-apple-red/10 text-apple-red'
                    : 'border-apple-orange/30 bg-apple-orange/10 text-apple-orange'
                )}
              >
                {budgetWarning}
              </p>
            )}
            <p className="mt-1 max-w-xl text-ui-caption text-secondary-light dark:text-secondary-dark">
              These are the saved notes and guidance the agent will see next. Remove anything that
              does not belong.
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            disabled={publishing}
            aria-label="Close context items check"
            className={cn(uiStyles.subtleButton, 'w-8 shrink-0 px-0')}
          >
            <X size={15} strokeWidth={2} aria-hidden="true" />
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
          {loading ? (
            <div className="flex items-center gap-2 py-8 text-ui-body text-secondary-light dark:text-secondary-dark">
              <RefreshCw size={14} strokeWidth={2} className="animate-spin" aria-hidden="true" />
              Checking context items…
            </div>
          ) : preview ? (
            <div className="space-y-4">
              {error && (
                <div
                  role="alert"
                  aria-live="polite"
                  className="flex gap-2 rounded-card bg-apple-red/10 px-3 py-2 text-ui-body text-apple-red"
                >
                  <AlertTriangle
                    size={14}
                    strokeWidth={2}
                    className="mt-0.5 shrink-0"
                    aria-hidden="true"
                  />
                  <span>{error}</span>
                </div>
              )}

              <PreviewMeta preview={preview} />

              {preview.warnings.length > 0 && (
                <div className="space-y-1 rounded-card bg-apple-red/10 px-3 py-2 text-ui-body text-apple-red">
                  {preview.warnings.map((warning) => (
                    <div key={warning} className="flex gap-2">
                      <AlertTriangle
                        size={14}
                        strokeWidth={2}
                        className="mt-0.5 shrink-0"
                        aria-hidden="true"
                      />
                      <span>{warning}</span>
                    </div>
                  ))}
                </div>
              )}

              <PreviewSection
                title="Will be included"
                helper="Checked items will be shared with the agent when you send the task."
                items={preview.items}
                empty="Add a context item below, or send without notes if none fit."
                selectedIds={selectedIds}
                pinnedIds={pinnedIds}
                onToggleSelected={toggleSelected}
                onTogglePinned={togglePinned}
              />
              <PreviewSection
                title="More context items you can include"
                helper="These are not shared unless you add them."
                items={preview.suggestedItems}
                empty="More context items appear here after tasks save helpful notes or guidance."
                selectedIds={selectedIds}
                pinnedIds={pinnedIds}
                onToggleSelected={toggleSelected}
                onTogglePinned={togglePinned}
              />
              <PreviewSection
                title="Kept easy to reuse"
                helper="These context items stay easy to reuse for this task."
                items={preview.previouslyPinned}
                empty="Choose the pin button on a context item to keep it easy to reuse."
                selectedIds={selectedIds}
                pinnedIds={pinnedIds}
                onToggleSelected={toggleSelected}
                onTogglePinned={togglePinned}
              />
            </div>
          ) : (
            <div className="space-y-3 py-8 text-ui-body text-secondary-light dark:text-secondary-dark">
              {error && (
                <div
                  role="alert"
                  aria-live="polite"
                  className="flex gap-2 rounded-card bg-apple-red/10 px-3 py-2 text-ui-body text-apple-red"
                >
                  <AlertTriangle
                    size={14}
                    strokeWidth={2}
                    className="mt-0.5 shrink-0"
                    aria-hidden="true"
                  />
                  <span>{error}</span>
                </div>
              )}
              <p>
                Context items check is not ready yet. Close this window, choose an available agent,
                then try sending again.
              </p>
            </div>
          )}
        </div>

        <div className="flex flex-col gap-2 border-t border-black/[0.06] px-4 py-3 dark:border-white/[0.08] sm:flex-row sm:justify-end">
          <button
            type="button"
            onClick={onClose}
            disabled={publishing}
            className={uiStyles.secondaryButton}
          >
            Back to task
          </button>
          <button
            type="button"
            onClick={() => void confirm()}
            disabled={!preview || loading || publishing}
            className={uiStyles.primaryButton}
          >
            {publishing ? 'Sending…' : 'Send task with selected notes'}
          </button>
        </div>
      </div>
    </div>
  )
}

function PreviewMeta({ preview }: { preview: ContextPreviewResponse }) {
  const cli =
    stringValue(preview.capability.cli_tool) ??
    stringValue(preview.capability.provider_name) ??
    'the selected agent'
  const runtime = stringValue(preview.capability.runtime_kind)
  return (
    <div className="grid gap-2 text-ui-caption sm:grid-cols-3">
      <MetaCell label="Agent will use" value={formatCodeLabel(cli)} chip />
      <MetaCell label="Work location" value={runtimeLabel(runtime)} chip />
      <MetaCell label="Note limits" value={degradationSummary(preview.degradation)} />
    </div>
  )
}

function MetaCell({
  label,
  value,
  chip = false,
}: {
  label: string
  value: string
  chip?: boolean
}) {
  return (
    <div className="rounded-card bg-apple-gray-6 px-3 py-2 dark:bg-white/[0.04]">
      <div className="text-ui-caption text-secondary-light dark:text-secondary-dark">{label}</div>
      <div
        className={cn(
          'mt-1 truncate font-medium text-foreground-light dark:text-foreground-dark',
          chip && uiStyles.chip
        )}
      >
        {value}
      </div>
    </div>
  )
}

interface PreviewSectionProps {
  title: string
  helper: string
  empty: string
  items: ContextPreviewItem[]
  selectedIds: Set<string>
  pinnedIds: Set<string>
  onToggleSelected: (id: string) => void
  onTogglePinned: (id: string) => void
}

function PreviewSection({
  title,
  helper,
  empty,
  items,
  selectedIds,
  pinnedIds,
  onToggleSelected,
  onTogglePinned,
}: PreviewSectionProps) {
  return (
    <section className="space-y-2">
      <div className="flex items-center justify-between">
        <div className="min-w-0">
          <h3 className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
            {title}
          </h3>
          <p className="mt-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {helper}
          </p>
        </div>
        <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
          {items.length}
        </span>
      </div>
      {items.length === 0 ? (
        <div className="rounded-card bg-apple-gray-6 px-3 py-3 text-ui-body text-secondary-light dark:bg-white/[0.04] dark:text-secondary-dark">
          {empty}
        </div>
      ) : (
        <div className="space-y-2">
          {items.map((item) => (
            <PreviewItemRow
              key={item.id}
              item={item}
              selected={selectedIds.has(item.id)}
              pinned={pinnedIds.has(item.id)}
              onToggleSelected={onToggleSelected}
              onTogglePinned={onTogglePinned}
            />
          ))}
        </div>
      )}
    </section>
  )
}

function PreviewItemRow({
  item,
  selected,
  pinned,
  onToggleSelected,
  onTogglePinned,
}: {
  item: ContextPreviewItem
  selected: boolean
  pinned: boolean
  onToggleSelected: (id: string) => void
  onTogglePinned: (id: string) => void
}) {
  return (
    <div className="rounded-card border border-black/[0.06] p-3 dark:border-white/[0.08]">
      <div className="flex items-start gap-3">
        <input
          type="checkbox"
          checked={selected}
          onChange={() => onToggleSelected(item.id)}
          aria-label={
            selected ? `Remove ${item.title} from this task` : `Include ${item.title} for the agent`
          }
          className="mt-1 h-4 w-4 shrink-0 accent-apple-blue focus:ring-apple-blue"
        />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <h4 className="min-w-0 text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
              {item.title}
            </h4>
            <Badge>{itemKindLabel(item.itemKind)}</Badge>
            {item.scopeKind && (
              <span className={uiStyles.chip}>{scopeKindLabel(item.scopeKind)}</span>
            )}
            {item.sensitivity && <Badge>{sensitivityLabel(item.sensitivity)}</Badge>}
          </div>
          <p className="mt-1 text-ui-body leading-relaxed text-secondary-light dark:text-secondary-dark">
            {item.why}
          </p>
          <div className="mt-2 flex flex-wrap gap-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
            <span>{noteSizeLabel(item.estimatedTokens)}</span>
            {item.lastUsedAt && <span>Used {formatRelativeTime(item.lastUsedAt)}</span>}
            {item.lastVerifiedAt && <span>Verified {formatRelativeTime(item.lastVerifiedAt)}</span>}
          </div>
        </div>
        <button
          type="button"
          onClick={() => onTogglePinned(item.id)}
          aria-label={
            pinned ? `Stop keeping ${item.title} easy to reuse` : `Keep ${item.title} easy to reuse`
          }
          className={cn(
            'flex h-8 w-8 shrink-0 items-center justify-center rounded-button transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
            pinned
              ? 'bg-apple-blue/10 text-apple-blue hover:bg-apple-blue/20'
              : 'text-secondary-light hover:bg-black/[0.04] hover:text-foreground-light dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark'
          )}
        >
          {pinned ? (
            <PinOff size={14} strokeWidth={2} aria-hidden="true" />
          ) : (
            <Pin size={14} strokeWidth={2} aria-hidden="true" />
          )}
        </button>
      </div>
    </div>
  )
}

function Badge({ children }: { children: string }) {
  return <span className={uiStyles.badge}>{children}</span>
}

function budgetLabel(capability?: Record<string, unknown>): string {
  const tokens = capability?.max_context_tokens
  if (typeof tokens !== 'number') return "Checking this agent's note space"
  if (tokens >= 3000) return 'Plenty of room for saved notes'
  if (tokens >= 1000) return 'Enough room for a few saved notes'
  return 'Limited room for saved notes'
}

function noteSizeLabel(estimatedTokens: number): string {
  if (estimatedTokens >= 1000) return 'Large context item'
  if (estimatedTokens >= 300) return 'Medium context item'
  return 'Small context item'
}

function stringValue(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}

function runtimeLabel(runtime: string | null): string {
  switch (runtime?.toLowerCase() ?? '') {
    case 'container':
    case 'container-cli':
      return 'Project files'
    case 'host':
    case 'cli':
    case 'host-cli':
      return 'This computer'
    case 'provider':
    case 'api':
      return 'Simple chat agent'
    case '':
      return 'Check where this ran'
    default:
      return 'Check work location'
  }
}

function degradationSummary(reasons: string[]): string {
  if (reasons.length === 0) return 'No note limits right now'
  return reasons.map(degradationLabel).join(', ')
}

function degradationLabel(reason: string): string {
  switch (reason) {
    case 'budget_truncated':
      return 'Some notes will be left out because this agent has limited note space'
    case 'runtime_capability_fallback':
      return "Using safe defaults because this agent's work details were incomplete"
    case 'no_subagents':
      return 'Notes meant only for helper agents will be skipped'
    default:
      return 'Check note limits'
  }
}

function itemKindLabel(kind: string): string {
  switch (kind) {
    case 'memory':
      return 'Saved note'
    case 'skill':
      return 'Skill'
    default:
      return 'Check context item'
  }
}

function scopeKindLabel(scope: string): string {
  switch (scope) {
    case 'org':
      return 'Team space'
    case 'user':
      return 'Only me'
    case 'team':
      return 'Team'
    case 'project':
      return 'Project'
    default:
      return 'Check sharing setting'
  }
}

function sensitivityLabel(sensitivity: string): string {
  switch (sensitivity) {
    case 'public':
      return 'Public'
    case 'internal':
      return 'Internal'
    case 'confidential':
      return 'Confidential'
    case 'secret_detected':
      return 'Possible secret'
    default:
      return 'Check safety label'
  }
}

function formatCodeLabel(value: string): string {
  return value
    .split(/[_-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ')
}
