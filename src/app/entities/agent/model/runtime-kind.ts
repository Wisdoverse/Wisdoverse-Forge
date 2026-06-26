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

/** Heartbeat token a sidecar advertises once it understands image_paths. Mirrors
 * the Rust `agentforge_core::SIDECAR_IMAGE_INPUT_CAPABILITY`. */
const SIDECAR_IMAGE_INPUT_CAPABILITY = 'image_input'

/**
 * Whether a *task* may carry instruction images for this assignee. Task images
 * are materialized into a Container CLI agent's `/workspace`, so this requires
 * ALL of: a container runtime (`runtimeKind === 'container'` — a Host CLI agent's
 * workspace is off-host and the server rejects it); a vision-capable CLI tool in
 * `capabilities` (claude/codex/gemini; opencode and Provider+Prompt/API agents
 * are excluded); AND the live sidecar advertising the `image_input` protocol
 * token, so an older not-yet-restarted sidecar (which the server dispatch gate
 * rejects) isn't offered an upload that would fail. The server still enforces the
 * real boundary; this only gates the UI affordance.
 */
export const isTaskImageCapable = (
  agent: { runtimeKind?: AgentRuntimeKind; capabilities?: readonly string[] } | undefined
): boolean => {
  if (agent?.runtimeKind !== 'container' || !agent.capabilities) return false
  const caps = agent.capabilities.map((c) => c.toLowerCase())
  return (
    caps.some((tool) => VISION_CLI_TOOLS.has(tool)) && caps.includes(SIDECAR_IMAGE_INPUT_CAPABILITY)
  )
}

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
