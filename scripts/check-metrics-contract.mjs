#!/usr/bin/env node
// Metrics contract lint (#891 F077/F076).
//
// Prometheus alert rules (ops/prometheus/*.yml) and Grafana dashboards
// (ops/grafana/dashboards/*.json) must only reference metric series — and label
// keys — the Rust code actually emits. A reference to a never-emitted series, or
// a stale label key (the old `http_requests_total{code=~"5.."}` / `{route=...}`
// selectors), makes the alert un-fireable and the panel render empty: silent
// loss of incident-response signal (the dead-dashboard class, cf. #464/#465).
// This lint fails CI on either drift.
//
// References are read ONLY from PromQL — YAML `expr:` blocks and Grafana `expr`/
// `query` fields — so prose in `summary:`/`runbook:`/comments is never mistaken
// for a metric. The EMITTED set is every owned metric name (string literal under
// rust/, where the metrics macros and register_* sites pass the name). External
// series (process_*, container_*, machine_*, probe_*) are out of scope.

import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

const OWNED_PREFIXES = ['agentforge_', 'http_', 'af_', 'agents_']
const ownedAlt = OWNED_PREFIXES.map((p) => `${p}[a-z0-9_]+`).join('|')
const METRIC_TOKEN = new RegExp(`\\b(${ownedAlt})\\b`, 'g')

// Label keys the code never emits — using one as a selector/grouping silently
// matches zero series. The HTTP metrics use {method, path, status, le}.
const FORBIDDEN_LABEL = /\b(code|route)\b\s*(=~?|[,)}])/

function isOwned(name) {
  return OWNED_PREFIXES.some((prefix) => name.startsWith(prefix))
}

function walk(dir, filter, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'node_modules' || entry.name === 'target' || entry.name.startsWith('.')) continue
    const full = path.join(dir, entry.name)
    if (entry.isDirectory()) walk(full, filter, out)
    else if (filter(full)) out.push(full)
  }
  return out
}

// EMITTED: every owned metric-name string literal under rust/.
function collectEmitted() {
  const emitted = new Set()
  const rustDir = path.join(repoRoot, 'rust')
  const literal = new RegExp(`"(${ownedAlt})"`, 'g')
  for (const file of walk(rustDir, (f) => f.endsWith('.rs'))) {
    for (const match of fs.readFileSync(file, 'utf8').matchAll(literal)) emitted.add(match[1])
  }
  return emitted
}

const leadingSpaces = (line) => line.length - line.trimStart().length

// Collect the PromQL expression strings from a YAML alert file: `expr:` inline
// values and `expr: |` block scalars (their more-indented continuation lines).
function yamlExprs(raw) {
  const lines = raw.split('\n')
  const out = []
  for (let i = 0; i < lines.length; i += 1) {
    const m = lines[i].match(/^(\s*)expr:\s*(.*)$/)
    if (!m) continue
    const indent = m[1].length
    const inline = m[2].trim()
    if (inline === '' || inline.startsWith('|') || inline.startsWith('>')) {
      for (let j = i + 1; j < lines.length; j += 1) {
        if (lines[j].trim() === '' || leadingSpaces(lines[j]) > indent) out.push(lines[j])
        else break
      }
    } else {
      out.push(inline)
    }
  }
  return out
}

// Collect PromQL strings from a Grafana dashboard: every `expr` (panel targets)
// and `query` (template variables, e.g. `label_values(metric, label)`).
function jsonExprs(raw) {
  const out = []
  const visit = (node) => {
    if (Array.isArray(node)) node.forEach(visit)
    else if (node && typeof node === 'object') {
      for (const [key, value] of Object.entries(node)) {
        if ((key === 'expr' || key === 'query') && typeof value === 'string') out.push(value)
        else visit(value)
      }
    }
  }
  visit(JSON.parse(raw))
  return out
}

const emitted = collectEmitted()
const refs = new Map() // metric name -> Set(file)
const labelViolations = []

const opsDirs = [path.join(repoRoot, 'ops', 'prometheus'), path.join(repoRoot, 'ops', 'grafana', 'dashboards')]
for (const dir of opsDirs) {
  if (!fs.existsSync(dir)) continue
  for (const file of walk(dir, (f) => /\.(ya?ml|json)$/.test(f))) {
    const raw = fs.readFileSync(file, 'utf8')
    const rel = path.relative(repoRoot, file)
    const exprs = file.endsWith('.json') ? jsonExprs(raw) : yamlExprs(raw)
    for (const expr of exprs) {
      for (const match of expr.matchAll(METRIC_TOKEN)) {
        const name = match[1].replace(/_bucket$|_sum$|_count$/, '')
        if (!isOwned(name)) continue
        if (!refs.has(name)) refs.set(name, new Set())
        refs.get(name).add(rel)
      }
      if (FORBIDDEN_LABEL.test(expr)) {
        labelViolations.push(`  ${rel}: '${expr.trim()}' uses a never-emitted label key (code/route); the HTTP metrics use status/path`)
      }
    }
  }
}

const violations = []
for (const [name, files] of [...refs].sort()) {
  if (!emitted.has(name)) {
    violations.push(`  ${name} — referenced in ${[...files].join(', ')} but never emitted by the Rust code`)
  }
}
violations.push(...labelViolations)

if (violations.length > 0) {
  process.stderr.write(
    'ERROR: metrics contract violated — alert/dashboard references that do not match emitted series/labels.\n' +
      'Fix the metric name/label to match what the code emits, or stop emitting the reference.\n\n' +
      'Violations:\n' +
      violations.join('\n') +
      '\n',
  )
  process.exit(1)
}

process.stdout.write(
  `[metrics-contract] ${refs.size} owned metric references checked against ${emitted.size} emitted series; names + labels resolve.\n`,
)
