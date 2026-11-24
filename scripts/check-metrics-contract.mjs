#!/usr/bin/env node
// Metrics contract lint (#891 F077/F076, #1329).
//
// Prometheus rules (ops/prometheus/**/*.yml), Grafana dashboards
// (ops/grafana/**/*.json) and the SLO runbook (docs/runbooks/observability-slo.md)
// must only name series the Rust services really export, and alert/dashboard
// selectors must only use label keys the code emits. A reference to a
// never-exported series makes an alert un-fireable, a panel render empty, and a
// runbook promise an indicator nobody measures: silent loss of incident-response
// signal (the dead-dashboard class, cf. #464/#465). This lint fails CI on drift.
//
// RECORDED: the literal name of every recording macro (`counter!`, `gauge!`,
// `histogram!`) in production Rust code, with its type. `describe_*!`, comments,
// files under `tests/` and test-only items are ignored. A histogram is exported
// with `_bucket`/`_sum`/`_count` only when the exporter configures buckets for it
// (`set_buckets_for_metric(Matcher::Full(..))`); otherwise it is a summary: the
// base name with a `quantile` label plus `_sum`/`_count`. Counters and gauges
// take no suffix.
//
// REFERENCES: every metric identifier in PromQL (rule `expr`, dashboard
// `expr`/`query`, runbook ```promql blocks). Runbook inline code spans and other
// non-shell fences use a narrower heuristic: lowercase snake_case tokens with at
// least one underscore. Series from other exporters must be in EXTERNAL_METRICS.
// The runbook may name an unrecorded metric only in its "Not Implemented
// Indicators" section, and a name listed there must NOT be recorded.

import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { isScalar, LineCounter, parseDocument, visit } from 'yaml'

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

export const RUNBOOK = 'docs/runbooks/observability-slo.md'
export const NOT_IMPLEMENTED_HEADING = 'Not Implemented Indicators'
// Rule files shipped outside ops/ (loaded by SRE per docs/runbooks/orchestration.md).
export const EXTRA_RULE_FILES = ['docker/orchestration-alerts.yml']

// Series that come from outside this codebase. None of their exporters ships
// with this repository; the runbook says so next to each dependent alert.
export const EXTERNAL_METRICS = new Set([
  // Blackbox exporter probe result (HealthCheckFailing).
  'probe_success',
  // cAdvisor machine memory (APIMemoryHigh denominator).
  'machine_memory_bytes',
  // A Docker exporter that publishes this exact name; cAdvisor and kube-state-metrics
  // do not (ContainerRestartLoop, overview dashboard).
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

// metrics-exporter-prometheus 0.18 default summary quantiles. A `set_quantiles`
// call in production code fails the check instead of silently changing these.
export const DEFAULT_QUANTILES = [0, 0.5, 0.9, 0.95, 0.99, 0.999, 1]

// Label keys the code never emits — using one as a selector/grouping silently
// matches zero series. The HTTP metrics use {method, path, status, le}.
const FORBIDDEN_LABEL = /\b(code|route)\b\s*(=~?|[,)}])/

const SHELL_FENCE = /^(bash|sh|shell|console|zsh|powershell|ps1|text)$/
const SNAKE_CASE = /^[a-z][a-z0-9]*(?:_[a-z0-9]+)+$/
// PromQL keywords, binary operators and aggregation operators (an aggregation may
// be followed by `by (...)` before its `(`, so it is not always a call).
const KEYWORDS = new Set([
  ...['and', 'or', 'unless', 'by', 'without', 'on', 'ignoring', 'group_left', 'group_right'],
  ...['offset', 'bool', 'atan2', 'inf', 'nan'],
  ...['sum', 'min', 'max', 'avg', 'group', 'stddev', 'stdvar', 'count', 'count_values'],
  ...['bottomk', 'topk', 'quantile', 'limitk', 'limit_ratio'],
])
const GROUPING = new Set(['by', 'without', 'on', 'ignoring', 'group_left', 'group_right'])

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

const lineAt = (text, index) => text.slice(0, index).split('\n').length
const blank = (text) => text.replace(/[^\n]/g, ' ')

// ---------------------------------------------------------------------------
// Rust sources
// ---------------------------------------------------------------------------

// Two same-length views of a Rust file: `code` with comments blanked, and
// `skeleton` with comments plus string/char literal contents blanked (so braces
// and attributes can be matched without being fooled by literals).
export function rustViews(src) {
  const code = []
  const skeleton = []
  const push = (text, skel = text) => {
    code.push(text)
    skeleton.push(skel)
  }
  const rawString = /b?r(#*)"/y
  const charLiteral = /'(?:\\(?:u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.)|[^\\'\n])'/y
  let i = 0
  while (i < src.length) {
    const c = src[i]
    const next = src[i + 1]
    if (c === '/' && next === '/') {
      const end = src.indexOf('\n', i) < 0 ? src.length : src.indexOf('\n', i)
      push(blank(src.slice(i, end)))
      i = end
      continue
    }
    if (c === '/' && next === '*') {
      let depth = 1
      let j = i + 2
      while (j < src.length && depth > 0) {
        const pair = src.slice(j, j + 2)
        if (pair === '/*' || pair === '*/') {
          depth += pair === '/*' ? 1 : -1
          j += 2
        } else j += 1
      }
      push(blank(src.slice(i, j)))
      i = j
      continue
    }
    if ((c === 'r' || c === 'b') && !/[\w]/.test(src[i - 1] ?? '')) {
      rawString.lastIndex = i
      const raw = rawString.exec(src)
      if (raw) {
        const close = `"${raw[1]}`
        const end = src.indexOf(close, rawString.lastIndex)
        const stop = end < 0 ? src.length : end + close.length
        const body = src.slice(rawString.lastIndex, end < 0 ? src.length : end)
        push(src.slice(i, stop), raw[0] + blank(body) + (end < 0 ? '' : close))
        i = stop
        continue
      }
    }
    if (c === '"') {
      let j = i + 1
      while (j < src.length && src[j] !== '"') j += src[j] === '\\' ? 2 : 1
      const literal = src.slice(i, j + 1)
      push(literal, `"${blank(literal.slice(1, -1))}"`)
      i = j + 1
      continue
    }
    if (c === "'") {
      charLiteral.lastIndex = i
      const literal = charLiteral.exec(src)
      if (literal) {
        push(literal[0], blank(literal[0]))
        i += literal[0].length
        continue
      }
    }
    push(c)
    i += 1
  }
  return { code: code.join(''), skeleton: skeleton.join('') }
}

const TEST_ATTRIBUTE =
  /#\[\s*(?:cfg\s*\(\s*test\s*\)|test|tokio::test|sqlx::test)\s*(?:\([^\]]*\))?\s*\]/g

// Blank test-only items out of `code`. An item is everything from a test
// attribute (`#[cfg(test)]`, `#[test]`, `#[tokio::test]`, `#[sqlx::test]`) to
// the first `;` outside ()/[] or the `}` matching the item's first `{`, found by
// brace counting on the skeleton. Approximation: `cfg(any(test, ..))`,
// `cfg(all(test, ..))` and macro-generated test items are not recognized; an
// out-of-line `#[cfg(test)] mod name;` is returned so its file can be skipped
// (`#[path]` overrides are not followed).
export function stripTestItems({ code, skeleton }) {
  if (/^\s*#!\[\s*cfg\s*\(\s*test\s*\)\s*\]/.test(skeleton))
    return { code: blank(code), skeleton: blank(skeleton), testModules: [] }
  let out = code
  let skel = skeleton
  const testModules = []
  for (const match of skeleton.matchAll(TEST_ATTRIBUTE)) {
    let j = match.index + match[0].length
    const more = /\s*#\[[^\]]*\]/y
    for (more.lastIndex = j; more.exec(skeleton); more.lastIndex = j) j = more.lastIndex
    let depth = 0
    let end = skeleton.length
    for (let k = j; k < skeleton.length; k += 1) {
      const ch = skeleton[k]
      if (ch === '(' || ch === '[') depth += 1
      else if (ch === ')' || ch === ']') depth -= 1
      else if (ch === ';' && depth === 0) {
        end = k + 1
        const outOfLine = /^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_]\w*)\s*;$/.exec(
          skeleton.slice(j, end)
        )
        if (outOfLine) testModules.push(outOfLine[1])
        break
      } else if (ch === '{' && depth === 0) {
        let braces = 0
        for (let m = k; m < skeleton.length; m += 1) {
          if (skeleton[m] === '{') braces += 1
          else if (skeleton[m] === '}' && --braces === 0) {
            end = m + 1
            break
          }
        }
        break
      }
    }
    out = out.slice(0, match.index) + blank(out.slice(match.index, end)) + out.slice(end)
    skel = skel.slice(0, match.index) + blank(skel.slice(match.index, end)) + skel.slice(end)
  }
  return { code: out, skeleton: skel, testModules }
}

// Recording macros, histogram bucket configuration and unsupported exporter
// configuration in one production source (comments and test items removed).
export function scanRustSource({ code, skeleton }, rel = 'source.rs') {
  const types = new Map()
  const bucketed = new Set()
  const problems = []
  // Match call heads on the skeleton (so text inside string literals is never a
  // call), then read the name literal from `code` at the same offset.
  const literalAt = (index) => {
    const literal = /"([a-zA-Z_:][a-zA-Z0-9_:]*)"/y
    literal.lastIndex = index
    return literal.exec(code)?.[1]
  }
  for (const m of skeleton.matchAll(/\b(counter|gauge|histogram)!\s*\(\s*"/g)) {
    const name = literalAt(m.index + m[0].length - 1)
    if (!name) continue
    if (!types.has(name)) types.set(name, new Set())
    types.get(name).add(m[1])
  }
  for (const m of skeleton.matchAll(/\bset_buckets_for_metric\s*\(\s*(Matcher::Full\s*\(\s*")?/g)) {
    const name = m[1] && literalAt(m.index + m[0].length - 1)
    if (name) bucketed.add(name)
    else
      problems.push(
        `  ${rel}:${lineAt(code, m.index)}: set_buckets_for_metric without Matcher::Full("<name>") is not understood; extend scripts/check-metrics-contract.mjs`
      )
  }
  for (const m of skeleton.matchAll(/\.\s*(set_buckets|set_quantiles)\s*\(/g)) {
    problems.push(
      `  ${rel}:${lineAt(code, m.index)}: ${m[1]}() changes every histogram's export format; extend scripts/check-metrics-contract.mjs`
    )
  }
  return { types, bucketed, problems }
}

// Out-of-line module file candidates for `mod name;` declared in `file`.
function moduleFiles(file, name) {
  const dir = path.dirname(file)
  const base = ['mod.rs', 'lib.rs', 'main.rs'].includes(path.basename(file))
    ? dir
    : path.join(dir, path.basename(file, '.rs'))
  return [path.join(base, `${name}.rs`), path.join(base, name, 'mod.rs')]
}

// The recorded-metric catalog for a repository: { types, bucketed, problems }.
// Bucket configuration is the union over every exporter installer
// (approximation: a metric bucketed in one process is treated as bucketed in all).
export function collectRecorded(root = repoRoot) {
  const files = walk(
    path.join(root, 'rust'),
    (f) => f.endsWith('.rs') && !/[\\/]tests[\\/]/.test(path.relative(root, f))
  )
  const prepared = new Map()
  const skip = new Set()
  for (const file of files) {
    const stripped = stripTestItems(rustViews(fs.readFileSync(file, 'utf8')))
    prepared.set(file, stripped)
    for (const name of stripped.testModules) moduleFiles(file, name).forEach((f) => skip.add(f))
  }
  const catalog = { types: new Map(), bucketed: new Set(), problems: [] }
  for (const [file, views] of prepared) {
    if (skip.has(file)) continue
    const scanned = scanRustSource(views, path.relative(root, file))
    for (const [name, kinds] of scanned.types) {
      if (!catalog.types.has(name)) catalog.types.set(name, new Set())
      kinds.forEach((kind) => catalog.types.get(name).add(kind))
    }
    scanned.bucketed.forEach((name) => catalog.bucketed.add(name))
    catalog.problems.push(...scanned.problems)
  }
  return catalog
}

// ---------------------------------------------------------------------------
// PromQL
// ---------------------------------------------------------------------------

// Metric references in a PromQL expression: [{ name, index, selector }]. Skips
// strings, comments, numbers, durations, Grafana variables, keywords and
// operators, function/aggregation names (identifier followed by `(`), label
// matchers `{...}`, ranges `[...]` and the label list after by/without/on/
// ignoring/group_left/group_right.
export function promqlTokens(expr) {
  const out = []
  const n = expr.length
  const skipString = (j) => {
    const quote = expr[j]
    let k = j + 1
    while (k < n && expr[k] !== quote) k += expr[k] === '\\' && quote !== '`' ? 2 : 1
    return k + 1
  }
  const skipBalanced = (open, close, j) => {
    let depth = 0
    let k = j
    while (k < n) {
      const ch = expr[k]
      if (ch === '"' || ch === "'" || ch === '`') {
        k = skipString(k)
        continue
      }
      if (ch === open) depth += 1
      else if (ch === close && --depth === 0) return k + 1
      k += 1
    }
    return n
  }
  const sticky = (re, j) => {
    re.lastIndex = j
    return re.exec(expr)
  }
  const identifier = /[a-zA-Z_:][a-zA-Z0-9_:]*/y
  const number = /\.?[0-9][0-9a-zA-Z_.]*(?:[eE][+-]?[0-9]+)?[a-zA-Z0-9_]*/y
  const variable = /\$(?:\{[^}]*\}|\w+)/y
  let i = 0
  while (i < n) {
    const ch = expr[i]
    if (ch === '"' || ch === "'" || ch === '`') i = skipString(i)
    else if (ch === '#') i = expr.indexOf('\n', i) < 0 ? n : expr.indexOf('\n', i)
    else if (ch === '{') i = skipBalanced('{', '}', i)
    else if (ch === '[') i = skipBalanced('[', ']', i)
    else if (ch === '$') i = sticky(variable, i) ? variable.lastIndex : i + 1
    else if (/[0-9]/.test(ch) || (ch === '.' && /[0-9]/.test(expr[i + 1] ?? '')))
      i = sticky(number, i) ? number.lastIndex : i + 1
    else if (/[a-zA-Z_:]/.test(ch)) {
      const name = sticky(identifier, i)[0]
      const after = i + name.length
      let k = after
      while (/\s/.test(expr[k] ?? '')) k += 1
      const lower = name.toLowerCase()
      if (expr[k] === '(') i = GROUPING.has(lower) ? skipBalanced('(', ')', k) : after
      else if (KEYWORDS.has(lower)) i = after
      else {
        out.push({
          name,
          index: i,
          selector: expr[k] === '{' ? expr.slice(k, skipBalanced('{', '}', k)) : '',
        })
        i = after
      }
    } else i += 1
  }
  return out
}

// Why `name` (with label `selector`) is not an exported series, or null if it
// is. `family: true` accepts a histogram's base name as a family mention (prose).
export function seriesProblem(
  name,
  catalog,
  { selector = '', family = false, defined = new Set() } = {}
) {
  if (EXTERNAL_METRICS.has(name) || defined.has(name)) return null
  const { types, bucketed } = catalog
  const kinds = types.get(name)
  if (kinds) {
    if (kinds.has('counter') || kinds.has('gauge') || family) return null
    if (bucketed.has(name))
      return `${name} is a histogram with buckets; it is exported only as ${name}_bucket, _sum and _count`
    const quantile = /\bquantile\s*=\s*"([^"]*)"/.exec(selector)
    if (quantile && !DEFAULT_QUANTILES.includes(Number(quantile[1]))) {
      return `${name}{quantile="${quantile[1]}"} is not exported; the summary quantiles are ${DEFAULT_QUANTILES.join(', ')}`
    }
    return null
  }
  const suffixed = /^(.+)_(bucket|sum|count)$/.exec(name)
  const baseKinds = suffixed && types.get(suffixed[1])
  if (baseKinds?.has('histogram')) {
    if (suffixed[2] !== 'bucket' || bucketed.has(suffixed[1])) return null
    return `${name} does not exist: no buckets are configured for ${suffixed[1]}, so it is exported as a summary (quantile label, _sum, _count)`
  }
  if (baseKinds)
    return `${name} does not exist: ${suffixed[1]} is a ${[...baseKinds].join('/')}, which takes no _${suffixed[2]} suffix`
  return `${name} is never recorded by production Rust code and is not in EXTERNAL_METRICS`
}

const familyRecorded = (name, catalog) =>
  catalog.types.has(name) || catalog.types.has(name.replace(/_(bucket|sum|count)$/, ''))

// ---------------------------------------------------------------------------
// Ops files and runbook
// ---------------------------------------------------------------------------

// Rule-file `expr` strings as [{ line, text }] and recording-rule names, parsed
// with the `yaml` package so quoting and block scalars are resolved.
export function yamlExprs(raw) {
  const lineCounter = new LineCounter()
  const doc = parseDocument(raw, { lineCounter })
  if (doc.errors.length > 0) throw new Error(doc.errors[0].message)
  const exprs = []
  const records = []
  visit(doc, {
    Pair(_, pair) {
      if (!isScalar(pair.key) || !isScalar(pair.value)) return
      const line = lineCounter.linePos(pair.key.range[0]).line
      if (pair.key.value === 'expr') exprs.push({ line, text: String(pair.value.value) })
      if (pair.key.value === 'record') records.push(String(pair.value.value))
    },
  })
  return { exprs, records }
}

// Dashboard PromQL as [{ line, text }]: every `expr` and `query` string, except
// datasource-variable queries; `label_values(selector, label)` keeps only its
// selector. `line` is the first raw occurrence of the string, if found.
export function jsonExprs(raw) {
  const out = []
  const add = (text) => {
    const at = raw.indexOf(JSON.stringify(text))
    out.push({ line: at < 0 ? undefined : lineAt(raw, at), text })
  }
  const visitNode = (node) => {
    if (Array.isArray(node)) node.forEach(visitNode)
    else if (node && typeof node === 'object') {
      for (const [key, value] of Object.entries(node)) {
        if (key === 'expr' && typeof value === 'string') add(value)
        else if (key === 'query' && typeof value === 'string') {
          if (node.type === 'datasource') continue
          const labelValues = /^\s*label_values\(\s*(?:([\s\S]*),\s*)?[A-Za-z_]\w*\s*\)\s*$/.exec(
            value
          )
          add(labelValues ? (labelValues[1] ?? '') : value)
        } else visitNode(value)
      }
    }
  }
  visitNode(JSON.parse(raw))
  return out
}

// Violations for the PromQL of one ops file.
export function opsViolations(rel, exprs, catalog, defined = new Set()) {
  const out = []
  for (const { line, text } of exprs) {
    const where = line ? `${rel}:${line}` : rel
    for (const { name, selector } of promqlTokens(text)) {
      const problem = seriesProblem(name, catalog, { selector, defined })
      if (problem) out.push(`  ${where}: ${problem}`)
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
export function runbookViolations(rel, markdown, catalog) {
  const out = []
  const lines = markdown.split('\n')
  let notImplementedLevel = 0 // heading level of the open Not Implemented section, 0 if none
  const check = (name, line, { selector, family }) => {
    const where = `${rel}:${line}`
    if (RUNBOOK_NON_METRIC_TOKENS.has(name) || EXTERNAL_METRICS.has(name)) return
    if (notImplementedLevel) {
      if (familyRecorded(name, catalog))
        out.push(
          `  ${where}: ${name} is listed under "${NOT_IMPLEMENTED_HEADING}" but the Rust code records it`
        )
      return
    }
    const problem = seriesProblem(name, catalog, { selector, family })
    if (problem) {
      out.push(
        `  ${where}: ${problem}; move it to the "${NOT_IMPLEMENTED_HEADING}" section or fix the name ` +
          '(if it is not a metric, add it to RUNBOOK_NON_METRIC_TOKENS)'
      )
    }
  }
  const snakeTokens = (text) => promqlTokens(text).filter((t) => SNAKE_CASE.test(t.name))
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i]
    const fence = line.match(/^\s*(```|~~~)\s*([\w-]*)/)
    if (fence) {
      let end = i + 1
      while (end < lines.length && !lines[end].trimStart().startsWith(fence[1])) end += 1
      const lang = fence[2].toLowerCase()
      const body = lines.slice(i + 1, end).join('\n')
      if (lang === 'promql') {
        for (const t of promqlTokens(body))
          check(t.name, i + 1 + lineAt(body, t.index), { selector: t.selector, family: false })
      } else if (!SHELL_FENCE.test(lang)) {
        for (const t of snakeTokens(body))
          check(t.name, i + 1 + lineAt(body, t.index), { selector: t.selector, family: true })
      }
      i = end
      continue
    }
    const heading = line.match(/^(#{1,6})\s+(.*?)\s*#*\s*$/)
    if (heading) {
      const level = heading[1].length
      if (notImplementedLevel && level <= notImplementedLevel) notImplementedLevel = 0
      if (heading[2].toLowerCase() === NOT_IMPLEMENTED_HEADING.toLowerCase())
        notImplementedLevel = level
      continue
    }
    for (const span of line.matchAll(/`([^`]+)`/g)) {
      for (const t of snakeTokens(span[1]))
        check(t.name, i + 1, { selector: t.selector, family: true })
    }
  }
  return out
}

// Run the whole contract against a repository checkout.
export function checkRepository(root = repoRoot) {
  const catalog = collectRecorded(root)
  const violations = [...catalog.problems]
  const parsed = []
  const defined = new Set()
  // promtool unit-test fixtures (ops/prometheus/tests/) query the synthetic
  // input_series they define; the rules and dashboards they test are checked here directly.
  const opsFiles = [path.join(root, 'ops', 'prometheus'), path.join(root, 'ops', 'grafana')]
    .filter((dir) => fs.existsSync(dir))
    .flatMap((dir) =>
      walk(dir, (f) => /\.(ya?ml|json)$/.test(f) && !/[\\/]tests[\\/]/.test(path.relative(root, f)))
    )
  for (const file of [...opsFiles, ...EXTRA_RULE_FILES.map((f) => path.join(root, f))]) {
    const raw = fs.readFileSync(file, 'utf8')
    const rel = path.relative(root, file)
    if (file.endsWith('.json')) parsed.push([rel, jsonExprs(raw)])
    else {
      const { exprs, records } = yamlExprs(raw)
      records.forEach((name) => defined.add(name))
      parsed.push([rel, exprs])
    }
  }
  for (const [rel, exprs] of parsed) violations.push(...opsViolations(rel, exprs, catalog, defined))
  violations.push(
    ...runbookViolations(RUNBOOK, fs.readFileSync(path.join(root, RUNBOOK), 'utf8'), catalog)
  )
  return {
    violations: [...new Set(violations)],
    files: parsed.length + 1,
    recorded: catalog.types.size,
  }
}

function main() {
  const { violations, files, recorded } = checkRepository()
  if (violations.length > 0) {
    process.stderr.write(
      'ERROR: metrics contract violated — references that do not match exported series/labels.\n' +
        'Fix the metric name/label to match what the code exports, or stop referencing it.\n\n' +
        'Violations:\n' +
        violations.join('\n') +
        '\n'
    )
    process.exit(1)
  }
  process.stdout.write(
    `[metrics-contract] ${files} files checked against ${recorded} recorded metrics; names + labels resolve.\n`
  )
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main()
