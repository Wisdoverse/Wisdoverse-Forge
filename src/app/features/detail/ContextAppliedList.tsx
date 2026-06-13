import { useState } from 'react'
import type { ReactNode } from 'react'
import { Brain, Workflow } from 'lucide-react'
import { FeedbackControls } from '@app/entities/context/ui/FeedbackControls'
import { formatRelativeTime } from '@app/shared/lib/time'
import { cn } from '@app/shared/lib/utils'
import type {
  AppliedContextItem,
  ContextCandidateKind,
  ContextFeedbackLabel,
  ContextFeedbackOutcome,
  MemoryContent,
} from '@shared/types/context'

interface ContextAppliedListProps {
  title: string
  kind: ContextCandidateKind
  items: AppliedContextItem[]
  onReadMemoryContent: (memoryId: string) => Promise<MemoryContent>
  onRecordFeedback: (
    item: AppliedContextItem,
    label: ContextFeedbackLabel
  ) => Promise<ContextFeedbackOutcome>
}

export function ContextAppliedList({
  title,
  kind,
  items,
  onReadMemoryContent,
  onRecordFeedback,
}: ContextAppliedListProps) {
  if (items.length === 0) return null

  return (
    <section className="space-y-2" data-testid={`context-applied-${kind}`}>
      <div>
        <h3 className="text-xs font-semibold text-foreground-light dark:text-foreground-dark">
          {title}
        </h3>
        <p className="mt-0.5 text-[11px] text-secondary-light dark:text-secondary-dark">
          {appliedContextDescription(kind)}
        </p>
      </div>
      <div className="space-y-2">
        {items.map((item) => (
          <AppliedContextCard
            key={item.injectionId}
            item={item}
            onReadMemoryContent={onReadMemoryContent}
            onRecordFeedback={(label) => onRecordFeedback(item, label)}
          />
        ))}
      </div>
    </section>
  )
}

function appliedContextDescription(kind: ContextCandidateKind): string {
  if (kind === 'skill') {
    return 'These saved instructions helped the agent before it worked on this task.'
  }
  return 'These saved notes helped the agent before it worked on this task.'
}

interface AppliedContextCardProps {
  item: AppliedContextItem
  onReadMemoryContent: (memoryId: string) => Promise<MemoryContent>
  onRecordFeedback: (label: ContextFeedbackLabel) => Promise<ContextFeedbackOutcome>
}

function AppliedContextCard({
  item,
  onReadMemoryContent,
  onRecordFeedback,
}: AppliedContextCardProps) {
  const [expandedContent, setExpandedContent] = useState<string | null>(null)
  const [loadingContent, setLoadingContent] = useState(false)
  const [contentError, setContentError] = useState<string | null>(null)
  const Icon = item.itemKind === 'skill' ? Workflow : Brain
  const content = expandedContent ?? item.contentPreview
  const showMoreLabel = loadingContent ? 'Loading full saved note…' : 'Show full saved note'

  async function showMore() {
    if (!item.contentTruncated || item.itemKind !== 'memory') {
      setExpandedContent(item.contentPreview)
      return
    }
    setLoadingContent(true)
    setContentError(null)
    try {
      const result = await onReadMemoryContent(item.itemId)
      setExpandedContent(result.content)
    } catch {
      setContentError(
        'The full saved note could not load. Choose Show full saved note again before relying on it.'
      )
    } finally {
      setLoadingContent(false)
    }
  }

  return (
    <article
      className={cn(
        'rounded-lg border border-black/[0.05] dark:border-white/[0.06]',
        'bg-apple-gray-6/70 dark:bg-white/[0.035] p-3 space-y-2',
        item.revoked && 'border-apple-red/30 bg-apple-red/[0.04]'
      )}
    >
      <div className="flex items-start gap-2">
        <div className="mt-0.5 w-6 h-6 rounded-md bg-white dark:bg-white/[0.06] flex items-center justify-center text-apple-blue shrink-0">
          <Icon size={14} strokeWidth={2} aria-hidden="true" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5 flex-wrap">
            <h4 className="text-xs font-semibold text-foreground-light dark:text-foreground-dark break-words">
              {item.title}
            </h4>
            {item.revoked && <Badge tone="red">Revoked</Badge>}
            {item.scopeKind && <Badge>{scopeKindLabel(item.scopeKind)}</Badge>}
            {item.sensitivity && <Badge tone="orange">{sensitivityLabel(item.sensitivity)}</Badge>}
          </div>
          <p className="mt-1 text-[11px] leading-relaxed text-secondary-light dark:text-secondary-dark whitespace-pre-wrap break-words">
            {content}
          </p>
          {item.contentTruncated && expandedContent === null && (
            <button
              type="button"
              onClick={showMore}
              disabled={loadingContent}
              aria-label={`${showMoreLabel} for ${item.title}`}
              title={
                loadingContent
                  ? 'Loading the full saved note text.'
                  : 'Open the full saved note text.'
              }
              className="mt-1 text-[10px] font-medium text-apple-blue hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:cursor-wait disabled:opacity-60"
            >
              {showMoreLabel}
            </button>
          )}
          {contentError && (
            <p role="alert" className="mt-1 text-[10px] text-apple-red">
              {contentError}
            </p>
          )}
        </div>
      </div>

      <div className="grid grid-cols-2 gap-2 text-[10px] text-secondary-light dark:text-secondary-dark">
        <span>Used {formatRelativeTime(item.appliedAt)}</span>
        <span>Last used {formatRelativeTime(item.lastUsedAt ?? item.appliedAt)}</span>
        {item.sourceTaskId && <span className="truncate">Saved from an earlier task</span>}
        {item.adapter && <span className="truncate">Prepared before the agent worked</span>}
      </div>

      {item.degradationReason && (
        <p className="text-[10px] text-apple-orange">
          This saved item was shortened before the agent used it. Review the full item before
          relying on it.
        </p>
      )}

      <FeedbackControls item={item} onRecord={onRecordFeedback} />
    </article>
  )
}

function scopeKindLabel(scopeKind: string): string {
  switch (scopeKind) {
    case 'org':
      return 'Team space'
    case 'team':
      return 'Team'
    case 'project':
      return 'Project'
    case 'user':
      return 'Only you'
    default:
      return 'Sharing setting needs review'
  }
}

function sensitivityLabel(sensitivity: string): string {
  switch (sensitivity) {
    case 'public':
      return 'Shareable'
    case 'internal':
      return 'Internal only'
    case 'confidential':
      return 'Confidential'
    case 'secret_detected':
      return 'May contain secrets'
    default:
      return 'Safety label needs review'
  }
}

function Badge({
  children,
  tone = 'gray',
}: {
  children: ReactNode
  tone?: 'gray' | 'orange' | 'red'
}) {
  return (
    <span
      className={cn(
        'text-[10px] font-medium px-1.5 py-0.5 rounded-badge',
        tone === 'orange' && 'bg-apple-orange/10 text-apple-orange',
        tone === 'red' && 'bg-apple-red/10 text-apple-red',
        tone === 'gray' &&
          'bg-white dark:bg-white/[0.06] text-secondary-light dark:text-secondary-dark'
      )}
    >
      {children}
    </span>
  )
}
