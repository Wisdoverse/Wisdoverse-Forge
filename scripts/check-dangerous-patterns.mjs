#!/usr/bin/env node

import fs from 'node:fs'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

const ROOTS = ['scripts', 'shared', 'src']
const EXTENSIONS = new Set(['.ts', '.js'])

const ALLOWED_EVAL_FILES = new Set()
const ALLOWED_SQL_TEMPLATE_FILES = new Set()
const SUSPICIOUS_SQL_INTERPOLATION_RE =
  /\b(req|request|body|query|params|input|filter|search|sort|order|where)\b/i

function walk(dir, files) {
  if (!fs.existsSync(dir)) return
  const entries = fs.readdirSync(dir, { withFileTypes: true })
  for (const entry of entries) {
    const full = path.join(dir, entry.name)
    if (entry.isDirectory()) {
      if (entry.name === 'node_modules' || entry.name === '__tests__') continue
      walk(full, files)
      continue
    }
    const ext = path.extname(entry.name)
    if (!EXTENSIONS.has(ext)) continue
    if (entry.name.includes('.test.')) continue
    files.push(full)
  }
}

function toPosix(p) {
  return p.split(path.sep).join('/')
}

function lineNumber(text, index) {
  return text.slice(0, index).split('\n').length
}

function findMatches(content, regex) {
  const matches = []
  let match
  while ((match = regex.exec(content)) !== null) {
    matches.push({
      index: match.index,
      text: match[0],
    })
  }
  return matches
}

function writeLine(stream, message) {
  stream.write(`${message}\n`)
}

export function runDangerousPatternScan(options = {}) {
  const cwd = options.cwd || process.cwd()
  const stdout = options.stdout || process.stdout
  const stderr = options.stderr || process.stderr

  const files = []
  for (const root of ROOTS) {
    walk(path.join(cwd, root), files)
  }

  const findings = []

  for (const file of files) {
    const relFile = toPosix(path.relative(cwd, file))
    const content = fs.readFileSync(file, 'utf8')

    const evalMatches = findMatches(content, /eval\s*\(/g)
    for (const m of evalMatches) {
      const line = lineNumber(content, m.index)
      if (ALLOWED_EVAL_FILES.has(relFile) && m.text.includes('eval(')) {
        continue
      }
      findings.push({
        type: 'eval',
        message: `eval() usage detected`,
        location: `${relFile}:${line}`,
        sample: m.text,
      })
    }

    const fnMatches = findMatches(content, /new\s+Function\s*\(/g)
    for (const m of fnMatches) {
      const line = lineNumber(content, m.index)
      findings.push({
        type: 'new-function',
        message: 'new Function() usage detected',
        location: `${relFile}:${line}`,
        sample: m.text,
      })
    }

    const sqlTemplateMatches = findMatches(content, /(query|execute)\s*\(\s*`[^`]*\$\{[^`]*`/g)
    for (const m of sqlTemplateMatches) {
      const line = lineNumber(content, m.index)
      if (ALLOWED_SQL_TEMPLATE_FILES.has(relFile)) {
        continue
      }
      const interpolations = [...m.text.matchAll(/\$\{([^}]+)\}/g)]
      const hasSuspiciousInterpolation = interpolations.some((x) =>
        SUSPICIOUS_SQL_INTERPOLATION_RE.test(x[1] ?? '')
      )
      if (!hasSuspiciousInterpolation) {
        continue
      }
      findings.push({
        type: 'sql-template',
        message: 'Possible SQL injection (template literal in query)',
        location: `${relFile}:${line}`,
        sample: m.text.split('\n')[0]?.slice(0, 120) ?? m.text.slice(0, 120),
      })
    }
  }

  if (findings.length > 0) {
    writeLine(stderr, '=== Dangerous Pattern Scan ===')
    for (const item of findings) {
      writeLine(stderr, `[${item.type}] ${item.message}: ${item.location}`)
      writeLine(stderr, `  -> ${item.sample}`)
    }
    return 1
  }

  writeLine(stdout, 'No dangerous patterns found')
  return 0
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exit(runDangerousPatternScan())
}
