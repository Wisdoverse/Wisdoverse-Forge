import type { AgentInfo, AgentRuntimeKind } from './types'

export const isHostCliAgent = (a: Pick<AgentInfo, 'runtimeKind'>): boolean =>
  a.runtimeKind === 'cli'

export const isContainerAgent = (a: Pick<AgentInfo, 'runtimeKind'>): boolean =>
  a.runtimeKind === 'container'

export const isApiAgent = (a: Pick<AgentInfo, 'runtimeKind'>): boolean => a.runtimeKind === 'api'

/** OpenAI model-name prefixes whose families accept image input. Allowlist that
 * mirrors the server `agentforge_llm::vision` OPENAI_VISION_PREFIXES — base
 * `gpt-4`/`gpt-4-0613`, `gpt-3.5*`, and `text-*` are text-only and correctly fall
 * through to false. */
const OPENAI_VISION_PREFIXES = ['gpt-4o', 'gpt-4-turbo', 'gpt-4.1', 'gpt-5']

/**
 * Whether a (provider, model) pair accepts image input. Mirrors the server's
 * `agentforge_llm::vision::model_supports_image` so the UI never advertises an
 * upload path the backend's model-aware gate would reject afterward: anthropic and
 * google (the canonical Gemini key) are vision for every model; openai is vision
 * only for the allowlisted families; any other provider is conservatively false.
 */
export const modelSupportsImage = (
  provider: string | null | undefined,
  model: string | null | undefined
): boolean => {
  // Mirror the server gate's normalization exactly (`vision.rs` trims +
  // lowercases BOTH): a model stored as `GPT-4O` or ` gpt-4o ` must match the
  // allowlist, else the UI hides an upload the backend would accept.
  const p = (provider ?? '').trim().toLowerCase()
  const m = (model ?? '').trim().toLowerCase()
  switch (p) {
    case 'anthropic':
      return true
    // "google" is canonical; "gemini" accepted defensively for drift.
    case 'google':
    case 'gemini':
      return true
    case 'openai':
      return OPENAI_VISION_PREFIXES.some((prefix) => m.startsWith(prefix))
    default:
      return false
  }
}

/**
 * Whether the quick-message composer should offer image upload for this agent.
 * Container CLI agents receive images via tasks (workspace files), not quick
 * messages — the quick-message path is provider/API only — so this is limited to
 * provider/API (chat) agents whose (provider, model) is vision-capable. Gating on
 * the model too (not just the provider) keeps the affordance in lock-step with the
 * server's model-aware gate, so a text-only model on a vision provider (e.g.
 * `gpt-4`/`gpt-3.5-turbo` on openai) is not offered an upload that fails on send.
 * The server still enforces the real boundary; this only gates the UI affordance.
 */
export const isImageCapable = (a: Pick<AgentInfo, 'cliTool' | 'provider' | 'model'>): boolean =>
  !a.cliTool && modelSupportsImage(a.provider, a.model)

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

/** Plain-language descriptions for tooltips and compact help. */
const RUNTIME_KIND_DESCRIPTIONS: Record<AgentRuntimeKind, string> = {
  container:
    'Works with shared project files. It can change files, run checks, and save what it checked.',
  cli: 'Uses files and tools on this connected computer. Use it when work should stay there.',
  api: 'Answers in chat through a connected AI service. It cannot take Tasks, change code, use computer apps, or open project files on its own.',
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

/** Return a plain-language description for a runtime kind. */
export function runtimeKindDescription(kind: AgentRuntimeKind | string | undefined): string {
  switch (kind?.trim().toLowerCase()) {
    case 'container':
      return RUNTIME_KIND_DESCRIPTIONS.container
    case 'cli':
      return RUNTIME_KIND_DESCRIPTIONS.cli
    case 'api':
      return RUNTIME_KIND_DESCRIPTIONS.api
    default:
      return 'Check this agent before sending work.'
  }
}
