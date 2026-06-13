#!/usr/bin/env node

import fs from 'node:fs'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

const ROOTS = ['src/app']
const EXTENSIONS = new Set(['.ts', '.tsx'])

const EMPTY_STATE_PATTERNS = [
  /\bNo [A-Za-z][^.!?\n]{0,80} (?:yet|found|available|to show)\b/,
  /\bNothing [^.!?\n]{0,80}\b/,
]

const NEXT_ACTION_PATTERN =
  /\b(Add|Ask|Check|Choose|Clear|Connect|Create|Keep|Open|Refresh|Review|Run|Save|Select|Send|Start|Try|Use|Wait)\b/i

const RAW_USER_VISIBLE_PATTERNS = [
  /\bFailed to fetch\b/,
  /\bInternal Server Error\b/,
  /\bStack trace\b/i,
  /\bUnhandled exception\b/i,
  /\bSQL error\b/i,
  /\bdatabase unavailable\b/i,
]

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
  if (NON_UI_FILE_PATTERNS.some((pattern) => pattern.test(relFile))) return false
  if (NON_UI_PATH_PARTS.some((part) => relFile.includes(part))) return false
  return true
}

function isLikelyEmptyStateContext(lines, index, line) {
  if (/\bempty\s*[:=]/i.test(line)) return true

  const start = Math.max(0, index - 20)
  const end = Math.min(lines.length, index + 4)
  const context = lines.slice(start, end).join('\n')
  return /EmptyState\b/.test(context) || /\bempty[-_\s]?state\b/i.test(context)
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
    line.includes('.test(') ||
    line.includes('new Error(') ||
    line.includes('throw ') ||
    line.trim().startsWith('//') ||
    line.trim().startsWith('*')
  )
}

function hasRawUserVisibleCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return RAW_USER_VISIBLE_PATTERNS.some((pattern) => pattern.test(line))
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

    if (hasRawUserVisibleCopy(line)) {
      findings.push({
        type: 'raw-error-copy',
        location,
        message: 'User-visible copy must not expose raw transport or backend failure wording.',
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
