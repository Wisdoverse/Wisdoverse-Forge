#!/usr/bin/env node

import fs from 'node:fs'
import { pathToFileURL } from 'node:url'

const SECTION_TITLE = 'Beginner UX / Operator Path'
const MIN_EXPLANATION_LENGTH = 12

const REQUIRED_FIELDS = [
  'Shortest safe path',
  'Prerequisites shown before action',
  'Success looks like',
  'Error or recovery path',
  'Destructive or permission impact',
  'CLI platforms covered, if applicable',
]

const PLACEHOLDER_VALUES = new Set(['', 'n/a', 'na', 'none', 'todo', 'tbd'])

function stripComments(value) {
  // Remove HTML comments, including an unterminated trailing opener (the HTML
  // spec treats an unterminated comment as running to end-of-input). Loop until
  // the string stops changing so a removal that exposes a fresh opener is also
  // stripped — a single pass can leave a dangling marker, which is the
  // CodeQL js/incomplete-multi-character-sanitization concern.
  let previous
  do {
    previous = value
    value = value.replace(/<!--[\s\S]*?(?:-->|$)/g, '')
  } while (value !== previous)
  return value
}

function normalize(value) {
  return String(value ?? '').trim()
}

function lower(value) {
  return normalize(value).toLowerCase()
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

function extractSection(body) {
  const cleanBody = stripComments(body)
  const sectionPattern = new RegExp(`^##\\s+${escapeRegExp(SECTION_TITLE)}\\s*$`, 'im')
  const match = sectionPattern.exec(cleanBody)
  if (!match) return null

  const start = match.index + match[0].length
  const rest = cleanBody.slice(start)
  const nextSection = /^##\s+/m.exec(rest)
  return (nextSection ? rest.slice(0, nextSection.index) : rest).trim()
}

function extractFieldValue(section, field) {
  const lines = section.split('\n')
  const fieldPattern = new RegExp(`^\\s*-\\s*${escapeRegExp(field)}\\s*:\\s*(.*)$`, 'i')
  const nextKnownFieldPattern = new RegExp(
    `^\\s*-\\s*(?:${REQUIRED_FIELDS.map(escapeRegExp).join('|')})\\s*:`,
    'i'
  )

  const startIndex = lines.findIndex((line) => fieldPattern.test(line))
  if (startIndex === -1) return null

  const firstLine = lines[startIndex]?.match(fieldPattern)?.[1] ?? ''
  const valueLines = [firstLine]
  for (let index = startIndex + 1; index < lines.length; index += 1) {
    const line = lines[index] ?? ''
    if (/^#{1,6}\s+/.test(line) || nextKnownFieldPattern.test(line)) break
    if (/^\s*-\s+\S/.test(line)) break
    valueLines.push(line)
  }

  return valueLines.join('\n').trim()
}

function hasUsefulValue(value) {
  const cleaned = lower(value).replace(/\.$/, '')
  return !PLACEHOLDER_VALUES.has(cleaned) && cleaned.length >= MIN_EXPLANATION_LENGTH
}

function hasNonUserFacingExplanation(section) {
  const match = section.match(/\bnot user-facing\b\s*[:\-–—]?\s*([^\n]+)/i)
  return Boolean(match && hasUsefulValue(match[1]))
}

function shouldSkipPullRequest(pullRequest, actor) {
  const login = lower(actor ?? pullRequest?.user?.login)
  const headRef = lower(pullRequest?.head?.ref)
  if (login.startsWith('dependabot')) return true
  if (headRef.startsWith('dependabot/')) return true
  return false
}

export function checkPullRequestBody(pullRequest, options = {}) {
  if (!pullRequest) {
    return {
      ok: true,
      skipped: true,
      message: 'No pull_request payload found; skipping beginner UX body check.',
      errors: [],
    }
  }

  if (shouldSkipPullRequest(pullRequest, options.actor)) {
    return {
      ok: true,
      skipped: true,
      message: 'Automation dependency PR skipped.',
      errors: [],
    }
  }

  const body = normalize(pullRequest.body)
  const section = extractSection(body)
  if (!section) {
    return {
      ok: false,
      skipped: false,
      errors: [`Missing "## ${SECTION_TITLE}" section.`],
    }
  }

  if (hasNonUserFacingExplanation(section)) {
    return {
      ok: true,
      skipped: false,
      message: 'PR declares a non-user-facing change with an explanation.',
      errors: [],
    }
  }

  const errors = []
  for (const field of REQUIRED_FIELDS) {
    const value = extractFieldValue(section, field)
    if (value == null) {
      errors.push(`Missing field: ${field}`)
    } else if (!hasUsefulValue(value)) {
      errors.push(`Field needs a concrete value: ${field}`)
    }
  }

  return {
    ok: errors.length === 0,
    skipped: false,
    message:
      errors.length === 0
        ? 'Beginner UX / Operator Path section is complete.'
        : 'Beginner UX / Operator Path section is incomplete.',
    errors,
  }
}

function parseArgs(argv) {
  const eventIndex = argv.indexOf('--event')
  return {
    eventPath: eventIndex >= 0 ? argv[eventIndex + 1] : process.env.GITHUB_EVENT_PATH || undefined,
  }
}

export function runBeginnerUxCheck(options = {}) {
  const stdout = options.stdout || process.stdout
  const stderr = options.stderr || process.stderr
  const eventPath = options.eventPath

  if (!eventPath) {
    stderr.write('[pr-beginner-ux] ERROR: provide --event or GITHUB_EVENT_PATH\n')
    return 2
  }

  const payload = JSON.parse(fs.readFileSync(eventPath, 'utf8'))
  const result = checkPullRequestBody(payload.pull_request, {
    actor: payload.sender?.login,
  })

  if (result.ok) {
    stdout.write(`[pr-beginner-ux] ${result.message}\n`)
    return 0
  }

  stderr.write('[pr-beginner-ux] Beginner UX / Operator Path is required.\n')
  for (const error of result.errors) {
    stderr.write(`- ${error}\n`)
  }
  return 1
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exit(runBeginnerUxCheck(parseArgs(process.argv.slice(2))))
}
