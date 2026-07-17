import { Suspense, lazy } from 'react'

// Lazy at the barrel — react-markdown must stay out of the main chunk
// (bundle gate hard-fails >5% growth of index-*.js). Same pattern as
// src/app/widgets/views/index.ts.
const LazyMarkdownDoc = lazy(() =>
  import('./MarkdownDoc').then((m) => ({ default: m.MarkdownDoc }))
)

export function MarkdownContent({ text, className }: { text: string; className?: string }) {
  return (
    <Suspense
      fallback={
        <p className="whitespace-pre-wrap text-ui-body leading-relaxed text-foreground-light dark:text-foreground-dark">
          {text}
        </p>
      }
    >
      <LazyMarkdownDoc text={text} className={className} />
    </Suspense>
  )
}
