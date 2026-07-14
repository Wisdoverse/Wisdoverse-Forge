const RAW_NETWORK_ERRORS = [/^Network error$/i, /^Failed to fetch$/i]
const RAW_STATUS_ERRORS = [/^API\s+\d{3}/i, /^HTTP\s+\d{3}/i, /^Server error\s*\(\d{3}\)$/i]
const GENERIC_BODY_TEXT = /^(Unauthorized|Forbidden|Not Found|Internal Server Error)$/i
const SAVE_GUIDANCE_RETRY = 'choose Save guidance again'
const CREATE_NETWORK_MESSAGE = `Check your connection, then ${SAVE_GUIDANCE_RETRY}. Forge could not connect while saving it.`
const CREATE_PERMISSION_MESSAGE = `Ask an owner or admin to update your Saved guidance access for this team space, then ${SAVE_GUIDANCE_RETRY}.`
const CREATE_NOT_FOUND_MESSAGE = `Open Saved guidance again, then ${SAVE_GUIDANCE_RETRY}.`
const CREATE_CONFLICT_MESSAGE = `Open Saved guidance to check for a similar item, then change the name or matching words and ${SAVE_GUIDANCE_RETRY}.`
const CREATE_RATE_LIMIT_MESSAGE = `Wait a moment, then ${SAVE_GUIDANCE_RETRY}. Forge is busy with saved guidance right now.`
const CREATE_SERVICE_MESSAGE = `Open Saved guidance again, then ${SAVE_GUIDANCE_RETRY}. If it still fails, ask your team to confirm Saved guidance is available for this team space.`
const CREATE_DEFAULT_MESSAGE = `Check the guidance name, matching words, and reusable guidance, then ${SAVE_GUIDANCE_RETRY}.`

const USER_FACING_STARTS = [
  'The instruction could not be created',
  'Forge could not',
  'Saved instructions could not',
  'Sign in again',
  'You do not have permission',
  'Instruction setup',
  'An instruction with this name',
  'Check the instruction name',
  'Check the name',
  'Check your connection',
  'Ask an owner or admin',
  'Open Saved instructions',
  'Open Saved guidance',
  'Wait a moment',
  'Open Saved instructions',
  'Open Saved guidance',
  'Check the required fields',
  'Check the guidance name',
]

export function createSkillErrorMessage(error?: unknown): string {
  const detail = rawDetail(error)
  const existingGuidance = detail ? existingSkillGuidance(detail) : null
  if (existingGuidance) return existingGuidance

  const status = statusFromDetail(detail)
  const safeDetail = status === null || status === 422 ? safeDetailFromRaw(detail) : null

  if (!status) {
    if (isServiceDetail(detail)) return CREATE_SERVICE_MESSAGE
    if (safeDetail) {
      return validationMessage(safeDetail)
    }
    return CREATE_NETWORK_MESSAGE
  }

  if (status === 401) {
    return `Sign in again, then ${SAVE_GUIDANCE_RETRY}.`
  }
  if (status === 403) {
    return CREATE_PERMISSION_MESSAGE
  }
  if (status === 404) {
    return CREATE_NOT_FOUND_MESSAGE
  }
  if (status === 409) {
    return CREATE_CONFLICT_MESSAGE
  }
  if (status === 422) {
    return validationMessage(safeDetail)
  }
  if (status === 429) {
    return CREATE_RATE_LIMIT_MESSAGE
  }
  if (status >= 500) {
    return CREATE_SERVICE_MESSAGE
  }

  return CREATE_DEFAULT_MESSAGE
}

function rawDetail(error: unknown): string | null {
  if (typeof error === 'string' && error.trim()) return error.trim()
  if (error instanceof Error && error.message.trim()) return error.message.trim()
  if (error && typeof error === 'object') {
    const serverError = (error as { serverError?: unknown }).serverError
    if (typeof serverError === 'string' && serverError.trim()) return serverError.trim()

    const message = (error as { message?: unknown }).message
    if (typeof message === 'string' && message.trim()) return message.trim()

    const detail = (error as { detail?: unknown }).detail
    if (typeof detail === 'string' && detail.trim()) return detail.trim()

    const errorValue = (error as { error?: unknown }).error
    if (typeof errorValue === 'string' && errorValue.trim()) return errorValue.trim()

    const reason = (error as { reason?: unknown }).reason
    if (typeof reason === 'string' && reason.trim()) return reason.trim()
  }
  return null
}

function existingSkillGuidance(detail: string): string | null {
  if (RAW_STATUS_ERRORS.some((pattern) => pattern.test(detail))) return null
  if (RAW_NETWORK_ERRORS.some((pattern) => pattern.test(detail))) return null
  if (!USER_FACING_STARTS.some((start) => detail.startsWith(start))) return null
  return normalizeExistingSkillGuidance(stripInternalErrorSuffix(detail))
}

function statusFromDetail(detail: string | null): number | null {
  const normalized = detail?.toLowerCase() ?? ''
  if (normalized.includes('role required')) return 403
  if (/\b(rate limit|quota exceeded|too many requests)\b/i.test(normalized)) return 429
  const match = detail?.match(
    /\b(?:API|HTTP|status code)\s*(\d{3})\b|Server error\s*\(?\s*(\d{3})\)?/i
  )
  return match ? Number(match[1] ?? match[2]) : null
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
  for (const key of ['serverError', 'message', 'error', 'detail', 'reason']) {
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

function isServiceDetail(detail: string | null): boolean {
  return /\b(database|sql|stack trace|traceback|exception|panic|internal server error)\b/i.test(
    detail ?? ''
  )
}

function validationMessage(detail: string | null): string {
  const normalized = detail?.toLowerCase() ?? ''
  if (normalized.includes('trigger')) {
    return `Check the matching words, then ${SAVE_GUIDANCE_RETRY}.`
  }
  if (normalized.includes('name')) {
    return `Enter a guidance name, then ${SAVE_GUIDANCE_RETRY}.`
  }
  if (normalized.includes('content') || normalized.includes('instruction')) {
    return `Enter the reusable guidance, then ${SAVE_GUIDANCE_RETRY}.`
  }
  return `Check the guidance name, matching words, and reusable guidance, then ${SAVE_GUIDANCE_RETRY}.`
}

function stripInternalErrorSuffix(detail: string): string {
  return detail
    .replace(/\s+Code:\s*\d{3}\.?/gi, '')
    .replace(/\s+Details?:\s*(Unauthorized|Forbidden|Not Found|Internal Server Error)\.?$/i, '')
    .replace(/\s+Details?:\s*$/i, '')
    .trim()
}

function normalizeExistingSkillGuidance(detail: string): string {
  if (detail.startsWith('Check your connection, then save the instruction again.')) {
    return CREATE_NETWORK_MESSAGE
  }
  if (detail.startsWith('Check your connection, then save the guidance again.')) {
    return CREATE_NETWORK_MESSAGE
  }
  if (detail.startsWith('Ask an owner or admin to let you create saved instructions')) {
    return CREATE_PERMISSION_MESSAGE
  }
  if (detail.startsWith('Ask an owner or admin to let you save reusable guidance')) {
    return CREATE_PERMISSION_MESSAGE
  }
  if (
    detail.startsWith('Open Saved instructions again, then save the instruction again.') &&
    detail.includes('If it still fails')
  ) {
    return CREATE_SERVICE_MESSAGE
  }
  if (
    detail.startsWith('Open Saved guidance again, then save the guidance again.') &&
    detail.includes('If it still fails')
  ) {
    return CREATE_SERVICE_MESSAGE
  }
  if (detail.startsWith('Open Saved instructions again, then save the instruction again')) {
    return CREATE_NOT_FOUND_MESSAGE
  }
  if (detail.startsWith('Open Saved guidance again, then save the guidance again')) {
    return CREATE_NOT_FOUND_MESSAGE
  }
  if (detail.startsWith('Wait a moment, then save the instruction again.')) {
    return CREATE_RATE_LIMIT_MESSAGE
  }
  if (detail.startsWith('Wait a moment, then save the guidance again.')) {
    return CREATE_RATE_LIMIT_MESSAGE
  }
  if (detail.startsWith('Check the required fields, then save the instruction again.')) {
    return CREATE_DEFAULT_MESSAGE
  }
  if (detail.startsWith('Check the required fields, then save the guidance again.')) {
    return CREATE_DEFAULT_MESSAGE
  }
  if (detail.startsWith('Check the name, matching words, and reusable guidance')) {
    return CREATE_DEFAULT_MESSAGE
  }
  if (detail.startsWith('Check the guidance name, matching words, and reusable guidance')) {
    return CREATE_DEFAULT_MESSAGE
  }
  if (detail.startsWith('Forge could not connect while creating this instruction.')) {
    return CREATE_NETWORK_MESSAGE
  }
  if (detail.startsWith('You do not have permission to create workspace instructions.')) {
    return CREATE_PERMISSION_MESSAGE
  }
  if (detail.startsWith('Saved instructions could not be opened from this page.')) {
    return CREATE_NOT_FOUND_MESSAGE
  }
  if (detail.startsWith('An instruction with this name or trigger may already exist.')) {
    return CREATE_CONFLICT_MESSAGE
  }
  if (detail.startsWith('Instruction setup is busy.')) {
    return CREATE_RATE_LIMIT_MESSAGE
  }
  if (detail.startsWith('Forge could not create the instruction right now.')) {
    return CREATE_SERVICE_MESSAGE
  }
  if (detail.startsWith('The instruction could not be created.')) {
    return CREATE_DEFAULT_MESSAGE
  }
  return detail
    .replace(/\bsave the instruction again\b/g, SAVE_GUIDANCE_RETRY)
    .replace(/\bsave the guidance again\b/g, SAVE_GUIDANCE_RETRY)
}
