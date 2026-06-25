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
      <div className="w-full max-w-md rounded-lg border border-surface-border-light bg-white p-6 shadow-sm dark:border-surface-border-dark dark:bg-surface-dark">
        <p className="m-0 text-base font-semibold text-primary-light dark:text-primary-dark">
          {title}
        </p>
        <p className="mt-2 text-sm leading-6 text-secondary-light dark:text-secondary-dark">
          {detail}
        </p>
        <div className="mt-4 flex flex-wrap gap-3">
          <button
            type="button"
            onClick={onReload}
            data-testid="error-fallback-reload"
            className="inline-flex items-center rounded-full bg-apple-blue px-4 py-2 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus active:scale-95"
          >
            Reload
          </button>
          <a
            href="/tasks"
            data-testid="error-fallback-home"
            className="inline-flex items-center rounded-full border border-black/[0.06] bg-black/[0.025] px-4 py-2 text-ui-button font-medium text-primary-light dark:border-white/[0.08] dark:bg-white/[0.04] dark:text-primary-dark"
          >
            Go to Tasks
          </a>
        </div>
      </div>
    </section>
  )
}
