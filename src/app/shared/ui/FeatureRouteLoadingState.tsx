type FeatureRouteLoadingStateProps = {
  title: string
  detail: string
  testId?: string
}

export function FeatureRouteLoadingState({
  title,
  detail,
  testId = 'feature-route-loading-state',
}: FeatureRouteLoadingStateProps) {
  return (
    <section
      role="status"
      aria-live="polite"
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
      </div>
    </section>
  )
}
