import { useEffect, useMemo, useState } from 'react'
import { AlertTriangle, Pin, PinOff, RefreshCw, X } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { formatRelativeTime } from '@app/shared/lib/time'
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

  if (!isOpen) return null

  async function confirm() {
    await onConfirm({ pinnedIds: [...pinnedIds], removedIds })
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
        aria-label="Close context review"
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
          'border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-[#2c2c2e]'
        )}
      >
        <div className="flex items-start justify-between gap-3 border-b border-black/[0.06] px-4 py-3 dark:border-white/[0.08]">
          <div className="min-w-0">
            <h2
              id="context-preview-title"
              className="text-ui-title font-semibold text-foreground-light dark:text-foreground-dark"
            >
              Review saved notes before sending
            </h2>
            <p
              className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
              data-testid="context-fit-summary"
            >
              {selectedSummary} · {budget}
            </p>
            <p className="mt-1 max-w-xl text-ui-caption text-secondary-light dark:text-secondary-dark">
              These are the saved notes and skill instructions the agent will see next. Remove
              anything that does not belong.
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            disabled={publishing}
            aria-label="Close context review"
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-secondary-light transition-colors hover:bg-black/[0.04] hover:text-foreground-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:opacity-50 dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark"
          >
            <X size={15} strokeWidth={2} aria-hidden="true" />
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
          {loading ? (
            <div className="flex items-center gap-2 py-8 text-ui-body text-secondary-light dark:text-secondary-dark">
              <RefreshCw size={14} strokeWidth={2} className="animate-spin" aria-hidden="true" />
              Loading context review…
            </div>
          ) : preview ? (
            <div className="space-y-4">
              {error && (
                <div
                  role="alert"
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
                empty="Nothing will be shared yet."
                selectedIds={selectedIds}
                pinnedIds={pinnedIds}
                onToggleSelected={toggleSelected}
                onTogglePinned={togglePinned}
              />
              <PreviewSection
                title="Optional matches"
                helper="These may help, but they stay out unless you choose them."
                items={preview.suggestedItems}
                empty="No extra matches were found."
                selectedIds={selectedIds}
                pinnedIds={pinnedIds}
                onToggleSelected={toggleSelected}
                onTogglePinned={togglePinned}
              />
              <PreviewSection
                title="Pinned for later"
                helper="Pinned items are kept easy to reuse for this task."
                items={preview.previouslyPinned}
                empty="Nothing is pinned yet."
                selectedIds={selectedIds}
                pinnedIds={pinnedIds}
                onToggleSelected={toggleSelected}
                onTogglePinned={togglePinned}
              />
            </div>
          ) : (
            <div className="py-8 text-ui-body text-secondary-light dark:text-secondary-dark">
              No context review is available yet.
            </div>
          )}
        </div>

        <div className="flex flex-col gap-2 border-t border-black/[0.06] px-4 py-3 dark:border-white/[0.08] sm:flex-row sm:justify-end">
          <button
            type="button"
            onClick={onClose}
            disabled={publishing}
            className="rounded-full bg-apple-gray-5 px-4 py-2 text-ui-button font-medium transition-colors hover:bg-apple-gray-4 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:opacity-50 dark:bg-white/[0.06] dark:hover:bg-white/[0.1]"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => void confirm()}
            disabled={!preview || loading || publishing}
            className="rounded-full bg-apple-blue px-4 py-2 text-ui-button font-medium text-white transition-colors hover:bg-apple-blue-focus focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:cursor-not-allowed disabled:opacity-60"
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
      <MetaCell label="Agent will use" value={formatCodeLabel(cli)} />
      <MetaCell label="Work location" value={runtimeLabel(runtime)} />
      <MetaCell label="Limits applied" value={degradationSummary(preview.degradation)} />
    </div>
  )
}

function MetaCell({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-card bg-apple-gray-6 px-3 py-2 dark:bg-white/[0.04]">
      <div className="text-ui-caption text-secondary-light dark:text-secondary-dark">{label}</div>
      <div className="mt-1 truncate font-medium text-foreground-light dark:text-foreground-dark">
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
            selected ? `Remove ${item.title} from context` : `Include ${item.title} in context`
          }
          className="mt-1 h-4 w-4 shrink-0 accent-apple-blue focus:ring-apple-blue"
        />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <h4 className="min-w-0 text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
              {item.title}
            </h4>
            <Badge>{itemKindLabel(item.itemKind)}</Badge>
            {item.scopeKind && <Badge>{scopeKindLabel(item.scopeKind)}</Badge>}
            {item.sensitivity && <Badge>{sensitivityLabel(item.sensitivity)}</Badge>}
          </div>
          <p className="mt-1 text-ui-body leading-relaxed text-secondary-light dark:text-secondary-dark">
            {item.why}
          </p>
          <div className="mt-2 flex flex-wrap gap-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
            <span>Needs about {item.estimatedTokens} context units</span>
            {item.lastUsedAt && <span>Used {formatRelativeTime(item.lastUsedAt)}</span>}
            {item.lastVerifiedAt && <span>Verified {formatRelativeTime(item.lastVerifiedAt)}</span>}
          </div>
        </div>
        <button
          type="button"
          onClick={() => onTogglePinned(item.id)}
          aria-label={pinned ? `Stop pinning ${item.title}` : `Keep ${item.title} pinned`}
          className={cn(
            'flex h-8 w-8 shrink-0 items-center justify-center rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
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
  return (
    <span className="rounded-full bg-apple-gray-6 px-1.5 py-0.5 text-ui-caption font-medium text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
      {children}
    </span>
  )
}

function budgetLabel(capability?: Record<string, unknown>): string {
  const tokens = capability?.max_context_tokens
  return typeof tokens === 'number'
    ? `Fits in this agent's context (${tokens.toLocaleString()} context units available)`
    : "Checking this agent's context room"
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
      return 'Managed workspace'
    case 'host':
    case 'cli':
    case 'host-cli':
      return 'This computer'
    case 'provider':
    case 'api':
      return 'Chat-only AI service'
    case '':
      return 'Work location not listed'
    default:
      return 'Work location needs review'
  }
}

function degradationSummary(reasons: string[]): string {
  if (reasons.length === 0) return 'No limits applied'
  return reasons.map(degradationLabel).join(', ')
}

function degradationLabel(reason: string): string {
  switch (reason) {
    case 'budget_truncated':
      return 'Some notes will be left out because this agent has limited context room'
    case 'runtime_capability_fallback':
      return 'Using safe defaults because agent setup details were incomplete'
    case 'no_subagents':
      return 'Subagent-specific context will be skipped'
    default:
      return 'Some context limits need review'
  }
}

function itemKindLabel(kind: string): string {
  switch (kind) {
    case 'memory':
      return 'Saved note'
    case 'skill':
      return 'Skill instruction'
    default:
      return 'Context item needs review'
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
      return 'Sharing setting needs review'
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
      return 'Safety label needs review'
  }
}

function formatCodeLabel(value: string): string {
  return value
    .split(/[_-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ')
}
