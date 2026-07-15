import { ChevronDown, ChevronRight, X } from 'lucide-react'
import { useId, type ReactNode } from 'react'
import { cn } from '@app/shared/lib/utils'

export interface GuideDisclosureProps {
  icon: ReactNode
  title: string
  expanded: boolean
  onToggle: () => void
  onDismiss?: () => void
  children: ReactNode
  className?: string
}

export function GuideDisclosure({
  icon,
  title,
  expanded,
  onToggle,
  onDismiss,
  children,
  className,
}: GuideDisclosureProps) {
  const bodyId = useId()

  return (
    <section
      className={cn(
        'rounded-card border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-white/[0.04]',
        className
      )}
    >
      <div className="flex min-h-8 items-center">
        <button
          type="button"
          aria-expanded={expanded}
          aria-controls={bodyId}
          onClick={onToggle}
          className="flex min-h-8 min-w-0 flex-1 items-center gap-2 px-3 text-left text-ui-body font-medium text-foreground-light transition-colors hover:bg-black/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.06]"
        >
          <span
            className="flex h-4 w-4 shrink-0 items-center justify-center text-secondary-light dark:text-secondary-dark [&>svg]:h-4 [&>svg]:w-4"
            aria-hidden="true"
          >
            {icon}
          </span>
          <span className="min-w-0 flex-1 truncate">{title}</span>
          {expanded ? (
            <ChevronDown size={16} strokeWidth={2} aria-hidden="true" />
          ) : (
            <ChevronRight size={16} strokeWidth={2} aria-hidden="true" />
          )}
        </button>
        {onDismiss ? (
          <button
            type="button"
            onClick={onDismiss}
            aria-label={`Dismiss ${title}`}
            title={`Dismiss ${title}`}
            className="mr-1 flex h-8 w-8 shrink-0 items-center justify-center rounded-button text-secondary-light transition-colors hover:bg-black/[0.04] hover:text-foreground-light dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark"
          >
            <X size={14} strokeWidth={2} aria-hidden="true" />
          </button>
        ) : null}
      </div>
      {expanded ? (
        <div
          id={bodyId}
          className="border-t border-black/[0.08] px-3 py-3 text-ui-body text-secondary-light dark:border-white/[0.1] dark:text-secondary-dark"
        >
          {children}
        </div>
      ) : null}
    </section>
  )
}
