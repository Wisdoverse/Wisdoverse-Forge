import { useState } from 'react'
import { cn } from '@app/shared/lib/utils'
import type {
  AppliedContextItem,
  ContextFeedbackLabel,
  ContextFeedbackOutcome,
} from '@shared/types/context'

const FEEDBACK_OPTIONS: { label: ContextFeedbackLabel; text: string }[] = [
  { label: 'useful', text: 'Useful' },
  { label: 'stale', text: 'Stale' },
  { label: 'wrong', text: 'Wrong' },
  { label: 'too_sensitive', text: 'Sensitive' },
  { label: 'do_not_use_again', text: 'Do not use' },
]

interface FeedbackControlsProps {
  item: AppliedContextItem
  onRecord: (label: ContextFeedbackLabel) => Promise<ContextFeedbackOutcome>
  onRecorded?: (label: ContextFeedbackLabel) => void
}

export function FeedbackControls({ item, onRecord, onRecorded }: FeedbackControlsProps) {
  const [selected, setSelected] = useState<ContextFeedbackLabel | null>(
    item.feedback?.label ?? null
  )
  const [pending, setPending] = useState<ContextFeedbackLabel | null>(null)
  const [error, setError] = useState<string | null>(null)

  async function record(label: ContextFeedbackLabel) {
    const previous = selected
    setSelected(label)
    setPending(label)
    setError(null)
    try {
      await onRecord(label)
      onRecorded?.(label)
    } catch (err) {
      setSelected(previous)
      setError(err instanceof Error ? err.message : 'Feedback failed')
    } finally {
      setPending(null)
    }
  }

  return (
    <div className="space-y-1.5">
      <div className="flex flex-wrap gap-1" aria-label={`Feedback for ${item.title}`}>
        {FEEDBACK_OPTIONS.map((option) => (
          <button
            key={option.label}
            type="button"
            disabled={pending !== null}
            onClick={() => record(option.label)}
            className={cn(
              'rounded-full px-2 py-1 text-ui-caption font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
              selected === option.label
                ? 'bg-apple-blue text-white'
                : 'bg-apple-gray-6 dark:bg-white/[0.05] text-secondary-light dark:text-secondary-dark hover:text-foreground-light dark:hover:text-foreground-dark',
              pending !== null && 'cursor-not-allowed opacity-60'
            )}
          >
            {pending === option.label ? 'Saving…' : option.text}
          </button>
        ))}
      </div>
      {error && <p className="text-ui-caption text-apple-red">{error}</p>}
    </div>
  )
}
