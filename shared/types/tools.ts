/**
 * Tool Mapping & CLI Tool Configuration
 *
 * Station types, tool-to-station mapping, and CLI tool metadata.
 */

import type { ToolName } from './events.js'

// ============================================================================
// Station Types & Tool Mapping
// ============================================================================

/** Station/location in the 3D workshop */
export type StationType =
  | 'center' // Default idle position
  | 'bookshelf' // Read
  | 'desk' // Write
  | 'workbench' // Edit
  | 'terminal' // Bash
  | 'scanner' // Grep/Glob
  | 'antenna' // WebFetch/WebSearch
  | 'portal' // Task (spawning subagents)
  | 'taskboard' // TodoWrite

/** Map tools to stations */
export const TOOL_STATION_MAP: Record<ToolName, StationType> = {
  Read: 'bookshelf',
  Write: 'desk',
  Edit: 'workbench',
  Bash: 'terminal',
  Grep: 'scanner',
  Glob: 'scanner',
  WebFetch: 'antenna',
  WebSearch: 'antenna',
  Task: 'portal',
  TodoWrite: 'taskboard',
  AskUserQuestion: 'center',
  NotebookEdit: 'desk',
}

/** Get station for a tool (handles unknown/MCP tools) */
export function getStationForTool(tool: string): StationType {
  return TOOL_STATION_MAP[tool as ToolName] ?? 'center'
}

// ============================================================================
// CLI Tool Configuration
// ============================================================================

/** Supported CLI tools for CLI runtime mode */
export type CliTool = 'claude' | 'opencode' | 'codex' | 'gemini'

/** CLI tool metadata for display and configuration */
export interface CliToolInfo {
  /** Display name */
  name: string
  /** Short description */
  description: string
  /** CLI command to run */
  command: string
  /** Path to settings file (~ will be expanded) */
  settingsPath: string
  /** Config directory root (~ will be expanded, e.g. ~/.claude) */
  configDir: string
  /** Hook compatibility level */
  hookCompatibility: 'native' | 'adapter' | 'notify'
}

/** Information about each supported CLI tool */
export const CLI_TOOL_INFO: Record<CliTool, CliToolInfo> = {
  claude: {
    name: 'Claude Code',
    description: 'Anthropic Claude Code CLI',
    command: 'claude',
    settingsPath: '~/.claude/settings.json',
    configDir: '~/.claude',
    hookCompatibility: 'native',
  },
  opencode: {
    name: 'OpenCode',
    description: 'OpenCode CLI (SST)',
    command: 'opencode',
    settingsPath: '~/.config/opencode/opencode.json',
    configDir: '~/.opencode',
    hookCompatibility: 'notify',
  },
  codex: {
    name: 'Codex',
    description: 'OpenAI Codex CLI',
    command: 'codex',
    settingsPath: '~/.codex/config.toml',
    configDir: '~/.codex',
    hookCompatibility: 'native',
  },
  gemini: {
    name: 'Gemini CLI',
    description: 'Google Gemini CLI',
    command: 'gemini',
    settingsPath: '~/.gemini/settings.json',
    configDir: '~/.gemini',
    hookCompatibility: 'native',
  },
}

/** LLM provider key used by the Rust LLM gateway. */
export type LlmProviderKey =
  | 'anthropic'
  | 'openai'
  | 'google'
  | 'ollama'
  | 'groq'
  | 'deepseek'
  // Mainstream China-region vendors; `*_coding` keys are the vendors'
  // subscription Coding Plan products on Anthropic-compatible endpoints.
  | 'zhipu'
  | 'zhipu_coding'
  | 'minimax'
  | 'minimax_coding'
  | 'moonshot'
  | 'moonshot_coding'
  | 'dashscope'
  | 'dashscope_coding'
  | 'hunyuan'
  | 'xiaomi'
  | 'xiaomi_coding'
  | 'xai'
  | 'openrouter'
  | 'together'
  | 'fireworks'
  | 'litellm'
  | 'openai_compatible'

/** Maps CLI tool to its primary LLM provider */
export const CLI_TOOL_PROVIDER_MAP: Record<CliTool, LlmProviderKey> = {
  claude: 'anthropic',
  gemini: 'google',
  codex: 'openai',
  opencode: 'anthropic',
}

/** Maps CLI tool to the environment variable name used for its API key */
export const CLI_TOOL_API_KEY_ENV: Record<CliTool, string> = {
  claude: 'ANTHROPIC_API_KEY',
  gemini: 'GEMINI_API_KEY',
  codex: 'OPENAI_API_KEY',
  opencode: 'ANTHROPIC_API_KEY',
}

/** Container path to each CLI tool's credential/config directory */
export const CLI_TOOL_CREDS_DIR: Record<CliTool, string> = {
  claude: '/home/agent/.claude',
  gemini: '/home/agent/.gemini',
  codex: '/home/agent/.codex',
  opencode: '/home/agent/.local/share/opencode',
}

/** Message format for CLI credential sync via NATS */
export interface CredentialSyncMessage {
  agentId: string
  orgId: string
  cliTool: CliTool
  /** Map of filename → file content (e.g., {"credentials.json": "..."}) */
  files: Record<string, string>
}
