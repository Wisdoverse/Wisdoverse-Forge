import type { AgentInfo, AgentRuntimeKind } from './types'

export const isHostCliAgent = (a: Pick<AgentInfo, 'runtimeKind'>): boolean =>
  a.runtimeKind === 'cli'

export const isContainerAgent = (a: Pick<AgentInfo, 'runtimeKind'>): boolean =>
  a.runtimeKind === 'container'

export const isApiAgent = (a: Pick<AgentInfo, 'runtimeKind'>): boolean => a.runtimeKind === 'api'

/**
 * Canonical user-facing labels for each runtime kind. These labels are plain
 * language; protocol slugs still stay in sync with server-side RuntimeKind.
 */
export const RUNTIME_KIND_LABELS: Record<AgentRuntimeKind, string> = {
  container: 'Managed workspace',
  cli: 'This computer',
  api: 'Chat-only AI service',
}

/** Short labels suited to compact table badges. */
export const RUNTIME_KIND_SHORT_LABELS: Record<AgentRuntimeKind, string> = {
  container: 'Managed',
  cli: 'This computer',
  api: 'Chat-only',
}

/** Canonical runtime kinds in display order. */
export const RUNTIME_KINDS: readonly AgentRuntimeKind[] = ['container', 'cli', 'api'] as const

/**
 * Return the canonical full label for a runtime kind. Unexpected server input
 * gets a plain review label instead of exposing protocol slugs to operators.
 */
export function runtimeKindLabel(kind: AgentRuntimeKind | string | undefined): string {
  switch (kind?.trim().toLowerCase()) {
    case 'container':
      return RUNTIME_KIND_LABELS.container
    case 'cli':
      return RUNTIME_KIND_LABELS.cli
    case 'api':
      return RUNTIME_KIND_LABELS.api
    case undefined:
    case '':
      return 'Work type not reported'
    default:
      return 'Work type needs review'
  }
}

/** Return the compact badge label for a runtime kind. */
export function runtimeKindShortLabel(kind: AgentRuntimeKind | string | undefined): string {
  switch (kind?.trim().toLowerCase()) {
    case 'container':
      return RUNTIME_KIND_SHORT_LABELS.container
    case 'cli':
      return RUNTIME_KIND_SHORT_LABELS.cli
    case 'api':
      return RUNTIME_KIND_SHORT_LABELS.api
    case undefined:
    case '':
      return 'Not reported'
    default:
      return 'Needs review'
  }
}
