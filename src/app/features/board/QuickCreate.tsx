import { useState, useRef, useEffect } from 'react'
import { Check, Plus, X } from 'lucide-react'
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
        className="inline-flex w-full items-center gap-2 rounded-full px-3 py-2 text-left text-ui-caption font-medium text-secondary-light transition-colors hover:bg-black/[0.04] hover:text-foreground-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark"
      >
        <Plus size={14} strokeWidth={2.2} aria-hidden="true" />
        <span>Add quick task</span>
      </button>
    )
  }

  return (
    <form
      data-testid="quick-create-editor"
      className="rounded-card border border-black/[0.08] bg-white p-2 shadow-sm dark:border-white/[0.1] dark:bg-[#2c2c2e]"
      onSubmit={(event) => {
        event.preventDefault()
        handleSubmit()
      }}
    >
      <input
        ref={inputRef}
        aria-label="Quick task outcome"
        name={`${columnId}-quick-task-title`}
        autoComplete="off"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') handleSubmit()
          if (e.key === 'Escape') {
            handleCancel()
          }
        }}
        placeholder="e.g. Fix login error"
        className={cn(
          'h-10 w-full rounded-lg border border-black/[0.08] px-3 text-ui-body outline-none',
          'bg-white dark:border-white/[0.1] dark:bg-[#2c2c2e]',
          'placeholder:text-secondary-light dark:placeholder:text-secondary-dark',
          'focus:ring-2 focus:ring-apple-blue-focus'
        )}
      />
      <p className="mt-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
        Write one clear outcome. You can add details, assignee, and context after the card is
        created.
      </p>
      <div className="mt-2 flex items-center justify-end gap-2">
        <button
          type="button"
          onClick={handleCancel}
          className="inline-flex h-8 items-center gap-1.5 rounded-lg px-2.5 text-ui-caption font-medium text-secondary-light transition-colors hover:bg-black/[0.04] hover:text-foreground-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark"
        >
          <X size={13} strokeWidth={2.2} aria-hidden="true" />
          <span>Cancel</span>
        </button>
        <button
          type="submit"
          disabled={!trimmedTitle}
          className={cn(
            'inline-flex h-8 items-center gap-1.5 rounded-lg px-2.5 text-ui-caption font-medium',
            'bg-apple-blue text-white transition-colors hover:bg-apple-blue/90',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
            'disabled:cursor-not-allowed disabled:opacity-50'
          )}
        >
          <Check size={13} strokeWidth={2.2} aria-hidden="true" />
          <span>Create</span>
        </button>
      </div>
    </form>
  )
}
