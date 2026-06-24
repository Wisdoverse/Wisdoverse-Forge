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

export function savedInstructionAudienceLabel(source: string, fallback: string): string {
  const label = savedInstructionSourceLabel(source, fallback)
  const normalized = label.toLowerCase()

  if (normalized === 'team space saved instructions') return 'Saved for this team space'
  if (normalized === 'project saved instructions') return 'Saved for this project'
  if (normalized === 'global saved instructions') return 'Saved for everyone'
  if (normalized === 'saved instructions') return 'Saved as a saved instruction'

  return `Saved in ${label}`
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
