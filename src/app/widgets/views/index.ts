import { lazy } from 'react'

// Lazy at the barrel: Workshop3D (three.js) and Timeline must stay separate
// dynamic chunks. Static re-exports here would merge both into the one chunk
// any dynamic import of this barrel produces.
export const Workshop3DView = lazy(() =>
  import('./Workshop3DView').then((m) => ({ default: m.Workshop3DView }))
)
export const TimelineView = lazy(() =>
  import('./TimelineView').then((m) => ({ default: m.TimelineView }))
)
