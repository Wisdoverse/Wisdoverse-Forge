#!/usr/bin/env node
// Metrics contract lint (#891 F077/F076, #1329).
//
// Prometheus alert rules (ops/prometheus/*.yml), Grafana dashboards
// (ops/grafana/**/*.json) and the SLO runbook (docs/runbooks/observability-slo.md)
// must only name metric series the Rust code actually emits, and alert/dashboard
// selectors must only use label keys the code emits. A reference to a
// never-emitted series makes an alert un-fireable, a panel render empty, and a
// runbook promise an indicator nobody measures: silent loss of incident-response
// signal (the dead-dashboard class, cf. #464/#465). This lint fails CI on drift.
//
// Ops references are read ONLY from PromQL (YAML `expr:` blocks, Grafana
// `expr`/`query` fields). Runbook references are read ONLY from code spans and
// non-shell fenced code blocks. Within those, a metric name is a lowercase
// snake_case token with at least one underscore that is not a function call,
// label matcher, label list, string or range. The EMITTED set is the literal
// name passed to every metrics macro under rust/. Series from other exporters
// must be listed in EXTERNAL_METRICS.
//
// The runbook may name a never-emitted metric only in its
// "Not Implemented Indicators" section, and a name listed there must NOT be
// emitted (so the section cannot go stale).

import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

export const RUNBOOK = 'docs/runbooks/observability-slo.md'
export const NOT_IMPLEMENTED_HEADING = 'Not Implemented Indicators'

// Series that come from outside this codebase. Each needs its source.
export const EXTERNAL_METRICS = new Set([
  // Blackbox exporter probe result (HealthCheckFailing).
  'probe_success',
  // cAdvisor machine memory (APIMemoryHigh denominator).
  'machine_memory_bytes',
  // Container restart counter from cAdvisor / kube-state, per the alert comment
  // (ContainerRestartLoop, overview dashboard).
  'container_restart_count',
  // Standard process collector name. The Rust services do NOT register one
  // (metrics-exporter-prometheus 0.18 has no process collector), so this series
  // exists only if the scrape adds another exporter (APIMemoryHigh, overview).
  'process_resident_memory_bytes',
])

// Runbook code spans that are snake_case but not metric names (crates, log
// fields, functions, config values). Add a token here only if it is not a metric.
export const RUNBOOK_NON_METRIC_TOKENS = new Set([
  'metrics_exporter_prometheus', // Rust crate name
  'rule_files', // Prometheus configuration key
  'start_workflow', // Temporal client call
  'changes_requested', // self-fix review status value
  'request_id', // log field
  'org_id', // log field
  'user_id', // log field
  'cli_auth_proxy', // Rust module with redaction tests
  'auth_callout', // Rust module with redaction tests
])

// Label keys the code never emits — using one as a selector/grouping silently
// matches zero series. The HTTP metrics use {method, path, status, le}.
const FORBIDDEN_LABEL = /\b(code|route)\b\s*(=~?|[,)}])/

const SHELL_FENCE = /^(bash|sh|shell|console|zsh|powershell|ps1|text)$/

function walk(dir, filter, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'node_modules' || entry.name === 'target' || entry.name.startsWith('.'))
      continue
    const full = path.join(dir, entry.name)
    if (entry.isDirectory()) walk(full, filter, out)
    else if (filter(full)) out.push(full)
  }
  return out
}

// EMITTED: the metric NAME (first string argument) of every metrics-macro call
// under rust/ — `counter!/gauge!/histogram!` emission sites and the matching
// `describe_*!`. Reading only the macro sites (not every string literal) means
// unrelated fixtures or constraint names can't masquerade as emitted series.
// The name may sit on a following line, so `\s*` spans newlines. A macro whose
// name is not a literal is invisible here; it can only cause a false failure.
export function collectEmitted(root = repoRoot) {
  const emitted = new Set()
  const macro =
    /(?:counter|gauge|histogram|describe_counter|describe_gauge|describe_histogram)!\s*\(\s*"([a-z0-9_]+)"/g
  for (const file of walk(path.join(root, 'rust'), (f) => f.endsWith('.rs'))) {
    for (const match of fs.readFileSync(file, 'utf8').matchAll(macro)) emitted.add(match[1])
  }
  return emitted
}

// True if `name` is emitted as-is or is a histogram/summary series of an emitted name.
export function isEmitted(name, emitted) {
  return emitted.has(name) || emitted.has(name.replace(/_(bucket|sum|count)$/, ''))
}

// Metric-name tokens of a PromQL fragment.
export function promqlNames(expr) {
  const stripped = expr
    .replace(/"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'/g, ' ')
    .replace(/#.*$/gm, ' ')
    .replace(/\{[^}]*\}/g, ' ')
    .replace(/\[[^\]]*\]/g, ' ')
    .replace(/\b(?:by|without|on|ignoring|group_left|group_right)\s*\([^)]*\)/g, ' ')
  return [...stripped.matchAll(/(?<![\w$.:/-])([a-z][a-z0-9]*(?:_[a-z0-9]+)+)\b(?!\s*\()/g)].map(
    (m) => m[1]
  )
}

const leadingSpaces = (line) => line.length - line.trimStart().length

// PromQL from a YAML rule file as [{ line, text }]: `expr:` inline values and
// `expr: |` block scalars (their more-indented continuation lines).
export function yamlExprs(raw) {
  const lines = raw.split('\n')
  const out = []
  for (let i = 0; i < lines.length; i += 1) {
    const m = lines[i].match(/^(\s*)expr:\s*(.*)$/)
    if (!m) continue
    const indent = m[1].length
    const inline = m[2].trim()
    if (inline === '' || inline.startsWith('|') || inline.startsWith('>')) {
      for (let j = i + 1; j < lines.length; j += 1) {
        if (lines[j].trim() === '' || leadingSpaces(lines[j]) > indent)
          out.push({ line: j + 1, text: lines[j] })
        else break
      }
    } else {
      out.push({ line: i + 1, text: inline })
    }
  }
  return out
}

// PromQL from a Grafana dashboard: every `expr` (panel targets) and `query`
// (template variables, e.g. `label_values(metric, label)`). JSON has no line
// numbers after parsing, so `line` is undefined.
export function jsonExprs(raw) {
  const out = []
  const visit = (node) => {
    if (Array.isArray(node)) node.forEach(visit)
    else if (node && typeof node === 'object') {
      for (const [key, value] of Object.entries(node)) {
        if ((key === 'expr' || key === 'query') && typeof value === 'string')
          out.push({ text: value })
        else visit(value)
      }
    }
  }
  visit(JSON.parse(raw))
  return out
}

// Violations for PromQL references in one ops file.
export function opsViolations(rel, exprs, emitted) {
  const out = []
  for (const { line, text } of exprs) {
    const where = line ? `${rel}:${line}` : rel
    for (const name of promqlNames(text)) {
      if (!isEmitted(name, emitted) && !EXTERNAL_METRICS.has(name)) {
        out.push(
          `  ${where}: ${name} is never emitted by the Rust code and is not in EXTERNAL_METRICS`
        )
      }
    }
    if (FORBIDDEN_LABEL.test(text)) {
      out.push(
        `  ${where}: '${text.trim()}' uses a never-emitted label key (code/route); the HTTP metrics use status/path`
      )
    }
  }
  return out
}

// Violations for metric names in a Markdown runbook.
export function runbookViolations(rel, markdown, emitted) {
  const out = []
  let fence = null // null outside a fence, else { shell }
  let notImplementedLevel = 0 // heading level of the open Not Implemented section, 0 if none
  markdown.split('\n').forEach((line, index) => {
    const where = `${rel}:${index + 1}`
    const fenceMatch = line.match(/^\s*(```|~~~)\s*([\w-]*)/)
    if (fenceMatch) {
      fence = fence ? null : { shell: SHELL_FENCE.test(fenceMatch[2].toLowerCase()) }
      return
    }
    if (!fence) {
      const heading = line.match(/^(#{1,6})\s+(.*?)\s*#*\s*$/)
      if (heading) {
        const level = heading[1].length
        if (notImplementedLevel && level <= notImplementedLevel) notImplementedLevel = 0
        if (heading[2].toLowerCase() === NOT_IMPLEMENTED_HEADING.toLowerCase())
          notImplementedLevel = level
        return
      }
    }
    if (fence?.shell) return
    const fragments = fence ? [line] : [...line.matchAll(/`([^`]+)`/g)].map((m) => m[1])
    for (const fragment of fragments) {
      for (const name of promqlNames(fragment)) {
        if (RUNBOOK_NON_METRIC_TOKENS.has(name) || EXTERNAL_METRICS.has(name)) continue
        const emittedName = isEmitted(name, emitted)
        if (notImplementedLevel && emittedName) {
          out.push(
            `  ${where}: ${name} is listed under "${NOT_IMPLEMENTED_HEADING}" but the Rust code emits it`
          )
        } else if (!notImplementedLevel && !emittedName) {
          out.push(
            `  ${where}: ${name} is never emitted by the Rust code; ` +
              `move it to the "${NOT_IMPLEMENTED_HEADING}" section or fix the name ` +
              '(if it is not a metric, add it to RUNBOOK_NON_METRIC_TOKENS)'
          )
        }
      }
    }
  })
  return out
}

function main() {
  const emitted = collectEmitted()
  const violations = []
  let files = 0

  // Walk all of ops/grafana (not just dashboards/) so root-level dashboards like
  // orchestration-release-gate-dashboard.json are guarded too.
  for (const dir of [
    path.join(repoRoot, 'ops', 'prometheus'),
    path.join(repoRoot, 'ops', 'grafana'),
  ]) {
    if (!fs.existsSync(dir)) continue
    for (const file of walk(dir, (f) => /\.(ya?ml|json)$/.test(f))) {
      const raw = fs.readFileSync(file, 'utf8')
      const exprs = file.endsWith('.json') ? jsonExprs(raw) : yamlExprs(raw)
      violations.push(...opsViolations(path.relative(repoRoot, file), exprs, emitted))
      files += 1
    }
  }
  violations.push(
    ...runbookViolations(RUNBOOK, fs.readFileSync(path.join(repoRoot, RUNBOOK), 'utf8'), emitted)
  )
  files += 1

  if (violations.length > 0) {
    process.stderr.write(
      'ERROR: metrics contract violated — references that do not match emitted series/labels.\n' +
        'Fix the metric name/label to match what the code emits, or stop referencing it.\n\n' +
        'Violations:\n' +
        [...new Set(violations)].join('\n') +
        '\n'
    )
    process.exit(1)
  }

  process.stdout.write(
    `[metrics-contract] ${files} files checked against ${emitted.size} emitted series; names + labels resolve.\n`
  )
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main()
