const RAW_NETWORK_ERRORS = [/^Network error$/i, /^Failed to fetch$/i]
const RAW_STATUS_ERRORS = [/^API\s+\d{3}/i, /^HTTP\s+\d{3}/i, /^Server error\s*\(\d{3}\)$/i]
const GENERIC_BODY_TEXT = /^(Unauthorized|Forbidden|Not Found|Internal Server Error)$/i

const USER_FACING_STARTS = [
  'The instruction could not be created',
  'Forge could not',
  'Saved instructions could not',
  'Sign in again',
  'You do not have permission',
  'Instruction setup',
  'An instruction with this name',
  'Check the instruction name',
]

export function createSkillErrorMessage(error?: unknown): string {
  const detail = rawDetail(error)
  const existingGuidance = detail ? existingSkillGuidance(detail) : null
  if (existingGuidance) return existingGuidance

  const status = statusFromDetail(detail)
  const safeDetail = status === null || status === 422 ? safeDetailFromRaw(detail) : null

  if (!status) {
    if (safeDetail) {
      return validationMessage(safeDetail)
    }
    return 'Forge could not connect while creating this instruction. Check your connection, then try again.'
  }

  if (status === 401) {
    return 'Sign in again, then create the instruction.'
  }
  if (status === 403) {
    return 'You do not have permission to create workspace instructions. Ask an owner or admin to let you create saved instructions.'
  }
  if (status === 404) {
    return 'Saved instructions could not be opened from this page. Refresh Saved instructions, then try again.'
  }
  if (status === 409) {
    return 'An instruction with this name or trigger may already exist. Review the existing instructions, then try again.'
  }
  if (status === 422) {
    return validationMessage(safeDetail)
  }
  if (status === 429) {
    return 'Instruction setup is busy. Wait a moment, then create the instruction.'
  }
  if (status >= 500) {
    return 'Forge could not create the instruction right now. Refresh Saved instructions, then try again. If it still fails, ask an owner or admin to check instruction setup.'
  }

  return 'The instruction could not be created. Review the fields and try again.'
}

function rawDetail(error: unknown): string | null {
  if (typeof error === 'string' && error.trim()) return error.trim()
  if (error instanceof Error && error.message.trim()) return error.message.trim()
  if (error && typeof error === 'object') {
    const message = (error as { message?: unknown }).message
    if (typeof message === 'string' && message.trim()) return message.trim()

    const errorValue = (error as { error?: unknown }).error
    if (typeof errorValue === 'string' && errorValue.trim()) return errorValue.trim()
  }
  return null
}

function existingSkillGuidance(detail: string): string | null {
  if (RAW_STATUS_ERRORS.some((pattern) => pattern.test(detail))) return null
  if (RAW_NETWORK_ERRORS.some((pattern) => pattern.test(detail))) return null
  if (!USER_FACING_STARTS.some((start) => detail.startsWith(start))) return null
  return stripInternalErrorSuffix(detail)
}

function statusFromDetail(detail: string | null): number | null {
  const match = detail?.match(/\b(?:API|HTTP|Server error\s*\()? ?(\d{3})\b/i)
  return match ? Number(match[1]) : null
}

function safeDetailFromRaw(detail: string | null): string | null {
  if (!detail) return null
  if (RAW_NETWORK_ERRORS.some((pattern) => pattern.test(detail))) return null

  const statusBody = detail.match(/^(?:API|HTTP)\s+\d{3}:?\s*(.*)$/i)
  if (statusBody) return safeBodyDetail(statusBody[1])

  if (RAW_STATUS_ERRORS.some((pattern) => pattern.test(detail))) return null
  return trimDetail(detail)
}

function safeBodyDetail(body: string): string | null {
  const trimmed = body.trim()
  if (!trimmed || GENERIC_BODY_TEXT.test(trimmed)) return null

  const parsed = parseJsonBody(trimmed)
  const payloadDetail = parsed ? firstPayloadString(parsed) : null
  return trimDetail(payloadDetail ?? trimmed)
}

function parseJsonBody(body: string): unknown | null {
  try {
    return JSON.parse(body)
  } catch {
    return null
  }
}

function firstPayloadString(value: unknown): string | null {
  if (typeof value === 'string' && value.trim()) return value.trim()
  if (Array.isArray(value)) {
    for (const item of value) {
      const found = firstPayloadString(item)
      if (found) return found
    }
    return null
  }
  if (!value || typeof value !== 'object') return null

  const record = value as Record<string, unknown>
  for (const key of ['message', 'error', 'detail', 'reason']) {
    const found = firstPayloadString(record[key])
    if (found) return found
  }
  return null
}

function trimDetail(detail: string | null): string | null {
  const trimmed = detail?.trim()
  if (!trimmed || RAW_NETWORK_ERRORS.some((pattern) => pattern.test(trimmed))) return null
  if (GENERIC_BODY_TEXT.test(trimmed)) return null
  return trimmed.length > 180 ? `${trimmed.slice(0, 177)}...` : trimmed
}

function validationMessage(detail: string | null): string {
  const normalized = detail?.toLowerCase() ?? ''
  if (normalized.includes('trigger')) {
    return 'Check the matching words, then try again.'
  }
  if (normalized.includes('name')) {
    return 'Enter an instruction name, then try again.'
  }
  if (normalized.includes('content') || normalized.includes('instruction')) {
    return 'Enter the saved instructions, then try again.'
  }
  return 'Check the instruction name, matching words, and instructions, then try again.'
}

function stripInternalErrorSuffix(detail: string): string {
  return detail
    .replace(/\s+Code:\s*\d{3}\.?/gi, '')
    .replace(/\s+Details?:\s*(Unauthorized|Forbidden|Not Found|Internal Server Error)\.?$/i, '')
    .replace(/\s+Details?:\s*$/i, '')
    .trim()
}
