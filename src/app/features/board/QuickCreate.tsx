import { useState, useRef, useEffect } from 'react'
import { cn } from '@app/shared/lib/utils'

interface QuickCreateProps {
  columnId: string
  onSubmit: (title: string, columnId: string) => void
}

export function QuickCreate({ columnId, onSubmit }: QuickCreateProps) {
  const [isOpen, setIsOpen] = useState(false)
  const [title, setTitle] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)
  const submittedRef = useRef(false)
  const trimmedTitle = title.trim()

  useEffect(() => {
    if (isOpen) {
      submittedRef.current = false
      inputRef.current?.focus()
    }
  }, [isOpen])

  function handleSubmit() {
    if (submittedRef.current) return
    if (!trimmedTitle) return
    submittedRef.current = true
    onSubmit(trimmedTitle, columnId)
    setTitle('')
    setIsOpen(false)
  }

  function handleCancel() {
    submittedRef.current = true
    setTitle('')
    setIsOpen(false)
  }

  if (!isOpen) {
    return (
      <button
        type="button"
        onClick={() => setIsOpen(true)}
        className="w-full rounded-full px-3 py-2 text-left text-ui-caption font-medium text-secondary-light transition-colors hover:bg-black/[0.04] hover:text-foreground-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark"
      >
        + Add Task
      </button>
    )
  }

  return (
    <div className="space-y-2 px-1">
      <input
        ref={inputRef}
        aria-label="Task title"
        name={`${columnId}-quick-task-title`}
        autoComplete="off"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault()
            handleSubmit()
          }
          if (e.key === 'Escape') handleCancel()
        }}
        placeholder="Task title…"
        className={cn(
          'h-10 w-full rounded-full border border-black/[0.08] px-4 text-ui-body outline-none',
          'bg-white dark:border-white/[0.1] dark:bg-[#2c2c2e]',
          'placeholder:text-secondary-light dark:placeholder:text-secondary-dark'
        )}
      />
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={handleSubmit}
          disabled={!trimmedTitle}
          className={cn(
            'inline-flex h-8 flex-1 items-center justify-center rounded-full px-3 text-ui-button font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
            trimmedTitle
              ? 'bg-apple-blue text-white hover:bg-apple-blue-focus'
              : 'cursor-not-allowed bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark'
          )}
        >
          Add Task
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
