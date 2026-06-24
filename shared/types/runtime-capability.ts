export type CliToolKind = 'claude' | 'codex' | 'gemini' | 'opencode'

export type RuntimeKind = 'container' | 'cli' | 'api'

export interface RuntimeCapability {
  cli_tool?: CliToolKind | null
  provider_name?: string | null
  runtime_kind: RuntimeKind
  max_context_tokens: number
  supports_skills_mount: boolean
  supports_hooks: boolean
  supports_subagents: boolean
  supports_mcp_bridge: boolean
  supports_terminal: boolean
}
