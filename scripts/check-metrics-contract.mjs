#!/usr/bin/env node
// Metrics contract lint (#891 F077/F076).
//
// Prometheus alert rules (ops/prometheus/*.yml) and Grafana dashboards
// (ops/grafana/dashboards/*.json) must only reference metric series the Rust
// code actually emits. A reference to a never-emitted series makes the alert
// un-fireable and the panel render empty — silent loss of incident-response
// signal (the dead-dashboard class). This lint fails CI on any such drift.
//
// Contract: the EMITTED set is every `agentforge_*` / `http_*` / `af_*` metric
// name that appears as a string literal anywhere under rust/ (the metrics
// macros — counter!/gauge!/histogram!/describe_* — and the register_* sites all
// pass the name as such a literal). The REFERENCED set is every same-prefixed
// token in the ops alert/dashboard files. Anything referenced but not emitted
// is a violation. External/standard series (process_*, container_*, machine_*,
// probe_*, up, …) are out of scope and not checked.

import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

// Our metric names are conventionally prefixed. A bare-word metric token.
const OWNED_PREFIXES = ['agentforge_', 'http_', 'af_']
const METRIC_TOKEN = /\b(agentforge_[a-z0-9_]+|http_[a-z0-9_]+|af_[a-z0-9_]+)\b/g

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
  for (const file of walk(rustDir, (f) => f.endsWith('.rs'))) {
    const src = fs.readFileSync(file, 'utf8')
    // String literals only — avoids picking up label values or comments' prose.
    for (const match of src.matchAll(/"(agentforge_[a-z0-9_]+|http_[a-z0-9_]+|af_[a-z0-9_]+)"/g)) {
      emitted.add(match[1])
    }
  }
  return emitted
}

// REFERENCED: owned metric tokens in the ops alert/dashboard files.
function collectReferenced() {
  const refs = new Map() // name -> Set(relPath)
  const opsDirs = [path.join(repoRoot, 'ops', 'prometheus'), path.join(repoRoot, 'ops', 'grafana', 'dashboards')]
  for (const dir of opsDirs) {
    if (!fs.existsSync(dir)) continue
    for (const file of walk(dir, (f) => f.endsWith('.yml') || f.endsWith('.yaml') || f.endsWith('.json'))) {
      const raw = fs.readFileSync(file, 'utf8')
      // Strip YAML comments so prose / file-path mentions (e.g.
      // `http_metrics.rs` in a doc comment) are not mistaken for metric
      // references. JSON has no comments. A metric token immediately followed by
      // `.` is a filename, not a series, so it is excluded too.
      const isYaml = file.endsWith('.yml') || file.endsWith('.yaml')
      const text = isYaml
        ? raw
            .split('\n')
            .map((line) => line.replace(/#.*$/, ''))
            .join('\n')
        : raw
      const rel = path.relative(repoRoot, file)
      for (const match of text.matchAll(METRIC_TOKEN)) {
        // A trailing `.` (file extension) means this is a path, not a series.
        if (text[match.index + match[0].length] === '.') continue
        const name = match[1].replace(/_bucket$|_sum$|_count$/, '') // histogram-derived suffixes
        if (!isOwned(name)) continue
        if (!refs.has(name)) refs.set(name, new Set())
        refs.get(name).add(rel)
      }
    }
  }
  return refs
}

const emitted = collectEmitted()
const referenced = collectReferenced()

const violations = []
for (const [name, files] of [...referenced].sort()) {
  if (!emitted.has(name)) {
    violations.push(`  ${name} — referenced in ${[...files].join(', ')} but never emitted by the Rust code`)
  }
}

if (violations.length > 0) {
  process.stderr.write(
    'ERROR: metrics contract violated — alert/dashboard references to never-emitted series.\n' +
      'Fix the metric name/label to match what the code emits, or stop emitting the reference.\n\n' +
      'Violations:\n' +
      violations.join('\n') +
      '\n',
  )
  process.exit(1)
}

process.stdout.write(`[metrics-contract] ${referenced.size} owned metric references checked against ${emitted.size} emitted series; all resolve.\n`)
