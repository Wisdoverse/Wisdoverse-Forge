import { Plus } from 'lucide-react'
import { useState, useRef, useEffect } from 'react'
import { cn } from '@app/shared/lib/utils'

interface QuickCreateProps {
  columnId: string
  onSubmit: (title: string, columnId: string) => void | boolean | Promise<void | boolean>
}

const QUICK_TASK_EXAMPLES = [
  'Review setup instructions',
  'Fix the login error',
  'Summarize the latest task result',
]

export function QuickCreate({ columnId, onSubmit }: QuickCreateProps) {
  const [isOpen, setIsOpen] = useState(false)
  const [title, setTitle] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const inputRef = useRef<HTMLInputElement>(null)
  const submittedRef = useRef(false)
  const trimmedTitle = title.trim()
  const helpId = `${columnId}-quick-task-help`
  const errorId = `${columnId}-quick-task-error`

  useEffect(() => {
    if (isOpen) {
      submittedRef.current = false
      setError(null)
      inputRef.current?.focus()
    }
  }, [isOpen])

  async function handleSubmit() {
    if (submittedRef.current) return
    if (!trimmedTitle) {
      setError('Write the task goal before saving it.')
      inputRef.current?.focus()
      return
    }
    submittedRef.current = true
    setSubmitting(true)
    try {
      const result = await onSubmit(trimmedTitle, columnId)
      if (result === false) {
        submittedRef.current = false
        setError('Check your connection, then choose Save for later again. The task was not saved.')
        inputRef.current?.focus()
        return
      }
      setTitle('')
      setError(null)
      setIsOpen(false)
    } catch {
      submittedRef.current = false
      setError('Check your connection, then choose Save for later again. The task was not saved.')
      inputRef.current?.focus()
    } finally {
      setSubmitting(false)
    }
  }

  function handleCancel() {
    submittedRef.current = true
    setTitle('')
    setError(null)
    setIsOpen(false)
  }

  function useExample(example: string) {
    setTitle(example)
    setError(null)
    inputRef.current?.focus()
  }

  if (!isOpen) {
    return (
      <button
        type="button"
        onClick={() => setIsOpen(true)}
        className="inline-flex w-full items-center gap-2 rounded-full px-3 py-2 text-left text-ui-caption font-medium text-secondary-light transition-colors hover:bg-black/[0.04] hover:text-foreground-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark"
      >
        <Plus className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
        <span>Add task idea</span>
      </button>
    )
  }

  return (
    <div className="space-y-2 px-1">
      <input
        ref={inputRef}
        aria-label="Task goal"
        name={`${columnId}-quick-task-title`}
        autoComplete="off"
        value={title}
        aria-invalid={error ? 'true' : undefined}
        aria-describedby={error ? `${helpId} ${errorId}` : helpId}
        onChange={(e) => {
          setTitle(e.target.value)
          if (error) setError(null)
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault()
            void handleSubmit()
          }
          if (e.key === 'Escape') handleCancel()
        }}
        placeholder="Example: Fix the login error"
        className={cn(
          'h-10 w-full rounded-full border border-black/[0.08] px-4 text-ui-body outline-none',
          'bg-white dark:border-white/[0.1] dark:bg-[#2c2c2e]',
          'placeholder:text-secondary-light dark:placeholder:text-secondary-dark'
        )}
      />
      <p id={helpId} className="text-ui-caption text-secondary-light dark:text-secondary-dark">
        This only saves a draft in Not sent yet. Next: open the card, add where to work and done
        when, then choose an agent.
      </p>
      <div className="rounded-lg bg-black/[0.025] px-3 py-2 dark:bg-white/[0.04]">
        <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
          Need a starting point?
        </p>
        <div role="group" aria-label="Task examples" className="mt-2 flex flex-wrap gap-1.5">
          {QUICK_TASK_EXAMPLES.map((example) => (
            <button
              key={example}
              type="button"
              onClick={() => useExample(example)}
              disabled={submitting}
              className="rounded-full border border-black/[0.08] bg-white px-2.5 py-1 text-ui-caption font-medium text-secondary-light transition-colors hover:border-apple-blue/30 hover:text-foreground-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:cursor-wait disabled:opacity-60 dark:border-white/[0.1] dark:bg-[#2c2c2e] dark:text-secondary-dark dark:hover:text-foreground-dark"
            >
              {example}
            </button>
          ))}
        </div>
      </div>
      {error && (
        <p id={errorId} role="alert" className="text-ui-caption font-medium text-apple-red">
          {error}
        </p>
      )}
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={() => void handleSubmit()}
          disabled={!trimmedTitle || submitting}
          className={cn(
            'inline-flex h-8 flex-1 items-center justify-center rounded-full px-3 text-ui-button font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
            trimmedTitle && !submitting
              ? 'bg-apple-blue text-white hover:bg-apple-blue-focus'
              : 'cursor-not-allowed bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark'
          )}
        >
          {submitting ? 'Saving...' : 'Save for later'}
        </button>
        <button
          type="button"
          onClick={handleCancel}
          className="inline-flex h-8 items-center justify-center rounded-full border border-black/[0.08] bg-white px-3 text-ui-button font-medium text-secondary-light transition-colors hover:border-apple-blue/35 hover:text-foreground-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-[#2c2c2e] dark:text-secondary-dark dark:hover:text-foreground-dark"
        >
          Cancel
        </button>
      </div>
    </div>
  )
}
