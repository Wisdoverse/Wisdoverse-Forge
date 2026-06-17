const RAW_INTERNAL_LABEL_PATTERN = /[/\\@]|(?:^|\s)[a-z0-9]+[._-][a-z0-9._-]*(?:\s|$)/i

export function savedInstructionSourceLabel(source: string, fallback: string): string {
  const label = source
    .replace(/\bworkspace\b/gi, (match) => (match[0] === 'W' ? 'Team space' : 'team space'))
    .replace(/\bskills\b/gi, 'saved instructions')
    .replace(/\s+/g, ' ')
    .trim()

  if (!label || RAW_INTERNAL_LABEL_PATTERN.test(label)) return fallback
  return label
}

export function knownWorkToolLabel(tool: string): string | null {
  switch (tool.trim().toLowerCase()) {
    case 'claude':
      return 'Claude Code'
    case 'codex':
      return 'Codex'
    case 'gemini':
      return 'Gemini'
    case 'opencode':
      return 'OpenCode'
    default:
      return null
  }
}
