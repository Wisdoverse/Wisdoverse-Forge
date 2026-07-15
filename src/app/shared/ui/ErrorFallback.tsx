import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'

type ErrorFallbackProps = {
  title?: string
  detail?: string
  onReload: () => void
  testId?: string
}

/**
 * F069: shared recovery UI shown when the app (or a route) throws an
 * unrecoverable render error, instead of a blank white screen. Offers a reload
 * and a link back to the Tasks board.
 */
export function ErrorFallback({
  title = 'Something went wrong',
  detail = 'The page hit an unexpected error. Reloading usually fixes it — your work on the server is unaffected.',
  onReload,
  testId = 'error-fallback',
}: ErrorFallbackProps) {
  return (
    <section
      role="alert"
      aria-live="assertive"
      data-testid={testId}
      className="flex h-full min-h-[320px] items-center justify-center px-6 py-10 text-left"
    >
      <div className={cn(uiStyles.cardPadded, 'w-full max-w-md p-6')}>
        <p className="m-0 text-ui-title font-semibold text-primary-light dark:text-primary-dark">
          {title}
        </p>
        <p className="mt-2 text-ui-body leading-6 text-secondary-light dark:text-secondary-dark">
          {detail}
        </p>
        <div className="mt-4 flex flex-wrap gap-3">
          <button
            type="button"
            onClick={onReload}
            data-testid="error-fallback-reload"
            className={uiStyles.primaryButton}
          >
            Reload
          </button>
          <a href="/tasks" data-testid="error-fallback-home" className={uiStyles.secondaryButton}>
            Go to Tasks
          </a>
        </div>
      </div>
    </section>
  )
}
