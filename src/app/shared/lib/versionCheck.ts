/**
 * Minimal semantic-version comparison for self-hosted update checks.
 * Handles `v1.2.3` and `1.2.3` prefixes; tolerates missing parts and
 * non-numeric suffixes so an unreachable or malformed registry never breaks
 * the About page.
 */

export interface ParsedVersion {
  major: number
  minor: number
  patch: number
}

export function parseVersion(value: unknown): ParsedVersion | null {
  if (typeof value !== 'string') return null
  const match = /v?(\d+)\.(\d+)\.(\d+)/.exec(value.trim())
  if (!match) return null
  return { major: Number(match[1]), minor: Number(match[2]), patch: Number(match[3]) }
}

/** True when `candidate` is strictly newer than `current`. */
export function isNewerVersion(candidate: unknown, current: unknown): boolean {
  const next = parseVersion(candidate)
  const now = parseVersion(current)
  if (!next || !now) return false
  if (next.major !== now.major) return next.major > now.major
  if (next.minor !== now.minor) return next.minor > now.minor
  return next.patch > now.patch
}
