import { useState } from 'react'
import { cn } from '@app/shared/lib/utils'
import type {
  AppliedContextItem,
  ContextFeedbackLabel,
  ContextFeedbackOutcome,
} from '@shared/types/context'
import { feedbackErrorMessage } from '../model/feedbackErrorMessage'

const FEEDBACK_OPTIONS: {
  label: ContextFeedbackLabel
  text: string
  description: string
  confirmation: string
}[] = [
  {
    label: 'useful',
    text: 'Useful',
    description: 'Keep showing saved items like this.',
    confirmation: 'future tasks will prefer saved items like this.',
  },
  {
    label: 'stale',
    text: 'Outdated',
    description: 'The information is old and should be checked before reuse.',
    confirmation: 'future tasks will ask you to check this item before using it.',
  },
  {
    label: 'wrong',
    text: 'Incorrect',
    description: 'The information is wrong for this task.',
    confirmation: 'future tasks will avoid trusting this item.',
  },
  {
    label: 'too_sensitive',
    text: 'Too sensitive',
    description: 'This should not be shared broadly.',
    confirmation: 'future tasks will handle this item more carefully.',
  },
  {
    label: 'do_not_use_again',
    text: 'Do not use again',
    description: 'Stop selecting this item for future tasks.',
    confirmation: 'future tasks will avoid this item.',
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
      setError(feedbackErrorMessage(err))
    } finally {
      setPending(null)
    }
  }

  return (
    <div className="space-y-1.5">
      <div>
        <p className="text-[10px] font-medium text-foreground-light dark:text-foreground-dark">
          Was this saved item helpful?
        </p>
        <p className="mt-0.5 text-[10px] leading-relaxed text-secondary-light dark:text-secondary-dark">
          Your answer helps future tasks choose safer, more useful saved items.
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
      {error && (
        <p role="alert" className="text-ui-caption text-apple-red">
          {error}
        </p>
      )}
    </div>
  )
}
