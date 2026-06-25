import type { AgentInfo, AgentRuntimeKind } from './types'

export const isHostCliAgent = (a: Pick<AgentInfo, 'runtimeKind'>): boolean =>
  a.runtimeKind === 'cli'

export const isContainerAgent = (a: Pick<AgentInfo, 'runtimeKind'>): boolean =>
  a.runtimeKind === 'container'

export const isApiAgent = (a: Pick<AgentInfo, 'runtimeKind'>): boolean => a.runtimeKind === 'api'

/** Providers whose models accept image input (vision). */
const VISION_PROVIDERS = new Set(['anthropic', 'openai', 'google'])

/**
 * Whether the quick-message composer should offer image upload for this agent.
 * Container CLI agents receive images via tasks (workspace files), not quick
 * messages — the quick-message path is provider/API only — so this is limited to
 * provider/API (chat) agents whose provider is vision-capable. The server
 * enforces the real boundary; this only gates the UI affordance.
 */
export const isImageCapable = (a: Pick<AgentInfo, 'cliTool' | 'provider'>): boolean =>
  !a.cliTool && VISION_PROVIDERS.has((a.provider ?? '').toLowerCase())

/** Container CLI tools that can read image input. Mirrors the server-side
 * `CliToolKind::supports_image_input` (claude/codex/gemini, NOT opencode). */
const VISION_CLI_TOOLS = new Set<string>(['claude', 'codex', 'gemini'])

/**
 * Whether a *task* may carry instruction images for this assignee, judged from
 * the CLI tools it reports in `capabilities` (a participant reports its tool,
 * e.g. `['claude']`). Task images are materialized into a Container CLI agent's
 * `/workspace`, so only a vision-capable CLI (claude/codex/gemini) qualifies;
 * Provider+Prompt/API agents report no CLI tool and opencode has no vision, so
 * both are excluded here. A Host CLI agent reports the same tool as a container
 * one and so can still pass this UI check, but the server dispatch gate rejects
 * it (its `/workspace` is off-host) — see `task_image_materializer`. The server
 * always enforces the real boundary; this only gates the UI affordance.
 */
export const isTaskImageCapable = (capabilities: readonly string[] | undefined): boolean =>
  !!capabilities?.some((tool) => VISION_CLI_TOOLS.has(tool.toLowerCase()))

/**
 * Canonical user-facing labels for each runtime kind. These labels are plain
 * language; protocol slugs still stay in sync with server-side RuntimeKind.
 */
export const RUNTIME_KIND_LABELS: Record<AgentRuntimeKind, string> = {
  container: 'Project files',
  cli: 'This computer',
  api: 'Simple chat agent',
}

/** Short labels suited to compact table badges. */
export const RUNTIME_KIND_SHORT_LABELS: Record<AgentRuntimeKind, string> = {
  container: 'Project files',
  cli: 'This computer',
  api: 'Questions only',
}

/** Canonical runtime kinds in display order. */
export const RUNTIME_KINDS: readonly AgentRuntimeKind[] = ['container', 'cli', 'api'] as const

/**
 * Return the canonical full label for a runtime kind. Unexpected server input
 * gets a plain check label instead of exposing protocol slugs to operators.
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
      return 'Check where it works'
    default:
      return 'Check work location'
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
      return 'Check location'
    default:
      return 'Check location'
  }
}
