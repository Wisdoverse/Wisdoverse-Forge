import { useState } from 'react'
import { cn } from '@app/shared/lib/utils'
import type {
  AppliedContextItem,
  ContextFeedbackLabel,
  ContextFeedbackOutcome,
} from '@shared/types/context'

const FEEDBACK_OPTIONS: {
  label: ContextFeedbackLabel
  text: string
  description: string
  confirmation: string
}[] = [
  {
    label: 'useful',
    text: 'Useful',
    description: 'Keep recommending context like this.',
    confirmation: 'future runs will prefer context like this.',
  },
  {
    label: 'stale',
    text: 'Outdated',
    description: 'The information is old and should be checked before reuse.',
    confirmation: 'future runs will treat this item as needing review.',
  },
  {
    label: 'wrong',
    text: 'Incorrect',
    description: 'The information is wrong for this task.',
    confirmation: 'future runs will avoid trusting this item.',
  },
  {
    label: 'too_sensitive',
    text: 'Too sensitive',
    description: 'This should not be shared broadly.',
    confirmation: 'future runs will handle this item more carefully.',
  },
  {
    label: 'do_not_use_again',
    text: 'Do not use again',
    description: 'Stop selecting this item for future runs.',
    confirmation: 'future runs will avoid this item.',
  },
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
  const selectedOption = selected
    ? FEEDBACK_OPTIONS.find((option) => option.label === selected)
    : null

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
      setError(err instanceof Error ? err.message : 'Could not save feedback. Try again.')
    } finally {
      setPending(null)
    }
  }

  return (
    <div className="space-y-1.5">
      <div>
        <p className="text-[10px] font-medium text-foreground-light dark:text-foreground-dark">
          Was this context helpful?
        </p>
        <p className="mt-0.5 text-[10px] leading-relaxed text-secondary-light dark:text-secondary-dark">
          Your answer helps future runs choose safer, more useful context.
        </p>
      </div>
      <div className="flex flex-wrap gap-1" aria-label={`Feedback for ${item.title}`}>
        {FEEDBACK_OPTIONS.map((option) => (
          <button
            key={option.label}
            type="button"
            disabled={pending !== null}
            onClick={() => record(option.label)}
            title={option.description}
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
      {selectedOption && !pending && (
        <p className="text-[10px] leading-relaxed text-secondary-light dark:text-secondary-dark">
          Saved: {selectedOption.confirmation}
        </p>
      )}
      {error && <p className="text-ui-caption text-apple-red">{error}</p>}
    </div>
  )
}
