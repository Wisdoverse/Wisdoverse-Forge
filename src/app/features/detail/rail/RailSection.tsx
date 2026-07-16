import { useId, useState, type ReactNode } from 'react'
import { ChevronDown, ChevronRight } from 'lucide-react'

interface RailSectionProps {
  title: string
  defaultOpen?: boolean
  children: ReactNode
}

export function RailSection({ title, defaultOpen = true, children }: RailSectionProps) {
  const [open, setOpen] = useState(defaultOpen)
  const titleId = useId()

  return (
    <section
      aria-labelledby={titleId}
      className="border-b border-black/[0.06] last:border-b-0 dark:border-white/[0.08]"
    >
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((current) => !current)}
        className="flex w-full items-center justify-between px-1 py-1.5 text-ui-caption font-medium text-secondary-light hover:text-foreground-light dark:text-secondary-dark dark:hover:text-foreground-dark"
      >
        <span id={titleId}>{title}</span>
        {open ? (
          <ChevronDown size={14} aria-hidden="true" />
        ) : (
          <ChevronRight size={14} aria-hidden="true" />
        )}
      </button>
      {open && <div className="mt-1 space-y-2 px-1 pb-2">{children}</div>}
    </section>
  )
}

export function RailRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-start gap-2">
      <span className="w-20 shrink-0 text-ui-caption text-secondary-light dark:text-secondary-dark">
        {label}
      </span>
      <div className="min-w-0 text-ui-body text-foreground-light dark:text-foreground-dark">
        {children}
      </div>
    </div>
  )
}
