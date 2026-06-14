#!/usr/bin/env node

import fs from 'node:fs'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

const ROOTS = ['src/app']
const EXTENSIONS = new Set(['.ts', '.tsx'])

const EMPTY_STATE_PATTERNS = [
  /\bNo [A-Za-z][^.!?\n]{0,80} (?:yet|found|available|to show)\b/,
  /\bNo (?:active|recent) [A-Za-z][^.!?\n]{0,80}\b/,
  /\bNothing [^.!?\n]{0,80}\b/,
]

const NEXT_ACTION_PATTERN =
  /\b(Add|Ask|Check|Choose|Clear|Close|Connect|Create|Enter|Fix|Invite|Keep|Open|Reconnect|Refresh|Review|Retry|Run|Save|Select|Send|Sign in|Start|Try|Use|Wait)\b/i

const RAW_USER_VISIBLE_PATTERNS = [
  /\bAn error occurred\b/,
  /\bError occurred\b/,
  /\bConnection failed\b/,
  /\bFailed to fetch\b/,
  /\bInternal Server Error\b/,
  /\bNetwork error\b/,
  /\bOperation not permitted\b/i,
  /\bServer error\s*\(\d{3}\)/,
  /\bStack trace\b/i,
  /\bUnhandled exception\b/i,
  /\bSQL error\b/i,
  /\bUnknown error\b/,
  /\bdatabase unavailable\b/i,
]

const RECOVERABLE_ERROR_PATTERNS = [
  /\b(?:could not|did not|was not|were not)\b/i,
  /\bfailed to\b/i,
  /\b(?:are|is|was|were) not (?:created|deleted|loaded|saved|started|updated)\b/i,
]

const BEGINNER_JARGON_PATTERNS = [
  /\blocal agents?\b/i,
  /\bmanaged local agent\b/i,
  /\bmanaged workspace agents?\b/i,
  /\bclaude,\s*codex,\s*gemini,?\s*or\s*opencode\b/,
  /claude、codex、gemini\s*或\s*opencode/,
  /\bHost CLI\b/i,
  /\bPlatform CLI\b/i,
  /\bForge CLI\b/i,
]

const PLACEHOLDER_COPY_PATTERNS = [/\bUnknown\b/, /\bunknown\b/, /\bN\/A\b/, /\bTBD\b/]

const PLACEHOLDER_STRING_LITERAL_PATTERN = /(['"`])[^'"`]*(?:Unknown|unknown|N\/A|TBD)[^'"`]*\1/

const NON_UI_PATH_PARTS = [
  '/api/',
  '/lib/',
  '/model/',
  '/models/',
  '/store/',
  '/stores/',
  '/types/',
]

const NON_UI_FILE_PATTERNS = [
  /\.test\.[jt]sx?$/,
  /\.spec\.[jt]sx?$/,
  /ErrorMessage\.ts$/,
  /ErrorMessages\.ts$/,
  /errors\.ts$/,
  /\.store\.ts$/,
]

const USER_VISIBLE_ERROR_FILE_PATTERNS = [
  /ErrorMessages?\.ts$/,
  /errors\.ts$/,
  /\/model\/agents\.store\.ts$/,
  /\/model\/navigation\.store\.ts$/,
  /\/model\/settings\.store\.ts$/,
  /\/model\/billing\.store\.ts$/,
  /\/model\/admin\.store\.ts$/,
  /\/model\/skills\.store\.ts$/,
  /\/model\/analytics\.store\.ts$/,
]

const USER_VISIBLE_ERROR_FRAGMENT_FILE_PATTERNS = [
  /ErrorCopy\.ts$/,
  /ErrorMessages?\.ts$/,
  /errors\.ts$/,
]

function toPosix(value) {
  return value.split(path.sep).join('/')
}

function walk(dir, files) {
  if (!fs.existsSync(dir)) return
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name)
    if (entry.isDirectory()) {
      if (entry.name === 'node_modules' || entry.name === 'dist' || entry.name === 'coverage') {
        continue
      }
      walk(full, files)
      continue
    }

    if (!EXTENSIONS.has(path.extname(entry.name))) continue
    files.push(full)
  }
}

function isUiCopyFile(relFile) {
  if (relFile === 'src/app/shared/api/legacy/AgentAPI.ts') return true
  if (USER_VISIBLE_ERROR_FILE_PATTERNS.some((pattern) => pattern.test(relFile))) return true
  if (NON_UI_FILE_PATTERNS.some((pattern) => pattern.test(relFile))) return false
  if (NON_UI_PATH_PARTS.some((part) => relFile.includes(part))) return false
  return true
}

function isLikelyEmptyStateContext(lines, index, line) {
  if (/\bempty\s*[:=]/i.test(line)) return true
  if (/^\s*no[A-Z][A-Za-z0-9_]*\s*:/.test(line)) return true

  const start = Math.max(0, index - 20)
  const end = Math.min(lines.length, index + 4)
  const context = lines.slice(start, end).join('\n')
  return (
    /EmptyState\b/.test(context) ||
    /\bempty[-_\s]?state\b/i.test(context) ||
    /ProfileSummaryRow\b/.test(context)
  )
}

function hasEmptyStateCopy(lines, index) {
  const line = lines[index] ?? ''
  return (
    isLikelyEmptyStateContext(lines, index, line) &&
    EMPTY_STATE_PATTERNS.some((pattern) => pattern.test(line))
  )
}

function hasNextAction(lines, index) {
  const start = Math.max(0, index - 2)
  const end = Math.min(lines.length, index + 9)
  return lines.slice(start, end).some((line) => NEXT_ACTION_PATTERN.test(line))
}

function isLikelyGuardOrParserLine(line) {
  return (
    line.includes('includes(') ||
    line.includes('match(') ||
    line.includes('.test(') ||
    line.includes('.replace(/') ||
    line.includes('= /') ||
    line.includes('new Error(') ||
    line.includes('new TypeError(') ||
    line.includes('RAW_') ||
    line.includes('console.') ||
    line.includes('===') ||
    line.includes('!==') ||
    line.includes('throw ') ||
    line.trim().startsWith('/') ||
    line.trim().startsWith('//') ||
    line.trim().startsWith('*')
  )
}

function hasRawUserVisibleCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return RAW_USER_VISIBLE_PATTERNS.some((pattern) => pattern.test(line))
}

function hasBeginnerJargon(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return BEGINNER_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function looksLikeUserVisibleCopyLine(line) {
  if (/<[^>]*>[^<]*(?:Unknown|unknown|N\/A|TBD)[^<]*<\/[^>]+>/.test(line)) return true
  if (/\b(?:aria-label|title|placeholder)\s*=/.test(line)) return true
  if (
    /\b[A-Za-z][A-Za-z0-9_]*(?:Label|Title|Description|Message|Detail|Tooltip|Placeholder|Help|Hint|Text|Copy)?\s*:\s*['"`]/.test(
      line
    )
  ) {
    return true
  }
  if (
    /\b(?:label|title|description|message|detail|tooltip|placeholder|help|hint|text|copy)\s*=\s*['"`]/i.test(
      line
    )
  ) {
    return true
  }
  return false
}

function hasPlaceholderCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  if (
    !/<[^>]*>[^<]*(?:Unknown|unknown|N\/A|TBD)[^<]*<\/[^>]+>/.test(line) &&
    !PLACEHOLDER_STRING_LITERAL_PATTERN.test(line)
  ) {
    return false
  }
  if (!looksLikeUserVisibleCopyLine(line)) return false
  return PLACEHOLDER_COPY_PATTERNS.some((pattern) => pattern.test(line))
}

function hasRecoverableErrorCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return RECOVERABLE_ERROR_PATTERNS.some((pattern) => pattern.test(line))
}

function scanFile(file, relFile) {
  const lines = fs.readFileSync(file, 'utf8').split('\n')
  const findings = []

  lines.forEach((line, index) => {
    const location = `${relFile}:${index + 1}`
    if (hasEmptyStateCopy(lines, index) && !hasNextAction(lines, index)) {
      findings.push({
        type: 'empty-state-next-action',
        location,
        message: 'Empty state copy must include a nearby next action for first-time operators.',
        sample: line.trim(),
      })
    }

    const rawUserVisibleCopy = hasRawUserVisibleCopy(line)
    if (rawUserVisibleCopy) {
      findings.push({
        type: 'raw-error-copy',
        location,
        message: 'User-visible copy must not expose raw transport or backend failure wording.',
        sample: line.trim(),
      })
    }

    if (hasBeginnerJargon(line)) {
      findings.push({
        type: 'beginner-jargon-copy',
        location,
        message: 'User-visible copy must use beginner-facing agent location wording.',
        sample: line.trim(),
      })
    }

    if (hasPlaceholderCopy(line)) {
      findings.push({
        type: 'placeholder-copy',
        location,
        message:
          'User-visible copy must explain missing information instead of showing placeholder labels.',
        sample: line.trim(),
      })
    }

    if (
      !rawUserVisibleCopy &&
      !USER_VISIBLE_ERROR_FRAGMENT_FILE_PATTERNS.some((pattern) => pattern.test(relFile)) &&
      hasRecoverableErrorCopy(line) &&
      !hasNextAction(lines, index)
    ) {
      findings.push({
        type: 'error-next-action',
        location,
        message: 'User-visible failure copy must include a nearby next action for beginners.',
        sample: line.trim(),
      })
    }
  })

  return findings
}

export function checkBeginnerUxCopy(options = {}) {
  const cwd = options.cwd || process.cwd()
  const files = []
  for (const root of ROOTS) {
    walk(path.join(cwd, root), files)
  }

  const findings = []
  for (const file of files) {
    const relFile = toPosix(path.relative(cwd, file))
    if (!isUiCopyFile(relFile)) continue
    findings.push(...scanFile(file, relFile))
  }

  return {
    ok: findings.length === 0,
    findings,
  }
}

export function runBeginnerUxCopyCheck(options = {}) {
  const stdout = options.stdout || process.stdout
  const stderr = options.stderr || process.stderr
  const result = checkBeginnerUxCopy({ cwd: options.cwd || process.cwd() })

  if (result.ok) {
    stdout.write('[beginner-ux-copy] UI copy guard passed.\n')
    return 0
  }

  stderr.write('[beginner-ux-copy] Beginner UX copy guard failed.\n')
  for (const finding of result.findings) {
    stderr.write(`[${finding.type}] ${finding.location}: ${finding.message}\n`)
    stderr.write(`  -> ${finding.sample}\n`)
  }
  return 1
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exit(runBeginnerUxCopyCheck())
}
