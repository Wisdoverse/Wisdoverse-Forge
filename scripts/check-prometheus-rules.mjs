#!/usr/bin/env node
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const dashboard = JSON.parse(
  fs.readFileSync(path.join(root, 'ops/grafana/dashboards/agentforge-overview.json'), 'utf8')
)
const panels = {
  __API_5XX_30D__: ['Observed API 5xx (30-day lookback)', 0],
  __API_5XX_5M__: ['5xx error rate (% of requests)', 0],
  __API_REQUEST_RATE__: ['Request rate (req/s)', 0],
  __API_LATENCY_P95__: ['Latency p50 / p95 / p99', 1],
}
const fixtureFiles = ['api-availability.test.yml', 'api-overview.test.yml']
let fixtures = fixtureFiles.map((file) =>
  fs.readFileSync(path.join(root, 'ops/prometheus/tests', file), 'utf8')
)
for (const [marker, [title, index]] of Object.entries(panels)) {
  const expression = dashboard.panels.find((panel) => panel.title === title)?.targets[index]?.expr
  assert.equal(typeof expression, 'string', `Missing query: ${title}`)
  assert.ok(
    fixtures.some((fixture) => fixture.includes(marker)),
    `Missing fixture: ${title}`
  )
  fixtures = fixtures.map((fixture) =>
    fixture.replaceAll(marker, JSON.stringify(expression.replaceAll('$path', '.*')))
  )
}
assert.ok(
  fixtures.every((fixture) => !/__API_[A-Z0-9_]+__/.test(fixture)),
  'Unresolved query fixture'
)

const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'forge-prometheus-rules-'))
try {
  fixtures.forEach((fixture, index) =>
    fs.writeFileSync(path.join(temporary, fixtureFiles[index]), fixture)
  )
  const user =
    typeof process.getuid === 'function'
      ? ['--user', `${process.getuid()}:${process.getgid()}`]
      : []
  const options = [
    'run',
    '--rm',
    '--network=none',
    '--read-only',
    '--cap-drop=ALL',
    '--security-opt=no-new-privileges',
    '--pids-limit=128',
    '--cpus=1',
    '--memory=512m',
    '--tmpfs=/tmp:rw,noexec,nosuid,nodev,size=256m,mode=1777',
    ...user,
    '--volume',
    `${root}:/workspace:ro`,
    '--volume',
    `${temporary}:/fixtures:ro`,
    '--workdir=/workspace',
    '--entrypoint=/bin/promtool',
    'prom/prometheus@sha256:efd719c99d83b060d9daefdcf00360461adf279f45ef5391f8d111892118753e',
  ]
  for (const args of [
    ['check', 'rules', 'ops/prometheus/alerts.yml', 'ops/prometheus/agents-runtime.yml'],
    ['test', 'rules', ...fixtureFiles.map((file) => `/fixtures/${file}`)],
  ]) {
    const result = spawnSync('docker', [...options, ...args], { stdio: 'inherit' })
    assert.equal(result.status, 0, result.error?.message ?? 'Prometheus rule checks failed')
  }
} finally {
  fs.rmSync(temporary, { recursive: true, force: true })
}
