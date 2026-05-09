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

  useEffect(() => {
    if (isOpen) {
      submittedRef.current = false
      inputRef.current?.focus()
    }
  }, [isOpen])

  function handleSubmit() {
    if (submittedRef.current) return
    const trimmed = title.trim()
    if (!trimmed) return
    submittedRef.current = true
    onSubmit(trimmed, columnId)
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
    <div className="px-1">
      <input
        ref={inputRef}
        aria-label="Task title"
        name={`${columnId}-quick-task-title`}
        autoComplete="off"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') handleSubmit()
          if (e.key === 'Escape') {
            submittedRef.current = true
            setIsOpen(false)
            setTitle('')
          }
        }}
        onBlur={handleSubmit}
        placeholder="Task title…"
        className={cn(
          'h-10 w-full rounded-full border border-black/[0.08] px-4 text-ui-body outline-none',
          'bg-white dark:border-white/[0.1] dark:bg-[#2c2c2e]',
          'placeholder:text-secondary-light dark:placeholder:text-secondary-dark'
        )}
      />
    </div>
  )
}
