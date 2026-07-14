import { LoaderCircle } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'

interface BeginnerLoadingStateProps {
  title: string
  detail: string
  nextStep: string
  success?: string
  testId?: string
  className?: string
  framed?: boolean
  compact?: boolean
}

export function BeginnerLoadingState({
  title,
  detail,
  nextStep,
  success,
  testId = 'beginner-loading-state',
  className,
  framed = true,
  compact = false,
}: BeginnerLoadingStateProps) {
  return (
    <div
      role="status"
      aria-label={title}
      aria-live="polite"
      data-testid={testId}
      className={cn(
        'flex flex-col items-center justify-center gap-3 px-6 text-center text-secondary-light dark:text-secondary-dark',
        compact ? 'min-h-40 py-4' : 'min-h-64',
        framed && 'rounded-md border border-black/[0.08] dark:border-white/[0.1]',
        className
      )}
    >
      <LoaderCircle
        size={24}
        strokeWidth={2}
        className="animate-spin text-secondary-light dark:text-secondary-dark"
        aria-hidden="true"
      />
      <div className="max-w-sm space-y-1">
        <p className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
          {title}
        </p>
        <p className="text-ui-body text-secondary-light dark:text-secondary-dark">{detail}</p>
        <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">{nextStep}</p>
        {success ? (
          <p className="text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
            {success}
          </p>
        ) : null}
      </div>
    </div>
  )
}
