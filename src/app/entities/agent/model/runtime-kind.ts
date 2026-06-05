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
 * Return the canonical full label for a runtime kind, falling back to the raw
 * value for any unexpected server input so the UI never renders `undefined`.
 */
export function runtimeKindLabel(kind: AgentRuntimeKind | undefined): string {
  if (!kind) return 'Unknown'
  return RUNTIME_KIND_LABELS[kind] ?? kind
}

/** Return the compact badge label for a runtime kind. */
export function runtimeKindShortLabel(kind: AgentRuntimeKind | undefined): string {
  if (!kind) return 'Unknown'
  return RUNTIME_KIND_SHORT_LABELS[kind] ?? kind
}
