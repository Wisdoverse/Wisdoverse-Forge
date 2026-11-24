import assert from 'node:assert/strict'
import test from 'node:test'
import {
  NOT_IMPLEMENTED_HEADING,
  opsViolations,
  promqlNames,
  runbookViolations,
  yamlExprs,
} from '../check-metrics-contract.mjs'

const emitted = new Set(['http_requests_total', 'http_request_duration_seconds'])
const check = (markdown) => runbookViolations('runbook.md', markdown, emitted)

test('promqlNames skips functions, label matchers, label lists, strings and ranges', () => {
  const expr =
    'histogram_quantile(0.95, sum by (le, cli_tool) (rate(http_request_duration_seconds_bucket{path=~"/a_b"}[5m]))) ' +
    '/ on() group_left() machine_memory_bytes # trailing_comment'
  assert.deepEqual(promqlNames(expr), [
    'http_request_duration_seconds_bucket',
    'machine_memory_bytes',
  ])
})

test('runbook: an unknown metric fails with file and line', () => {
  const violations = check(
    '# Title\n\n| SLI | `nats_messages_delivered_total / http_requests_total` |\n'
  )
  assert.equal(violations.length, 1)
  assert.match(violations[0], /runbook\.md:3: nats_messages_delivered_total is never emitted/)
})

test('runbook: an emitted metric and its histogram suffixes pass', () => {
  const markdown = [
    '`http_requests_total{status="500"}`',
    '```promql',
    'sum(rate(http_request_duration_seconds_bucket[5m])) / http_request_duration_seconds_count',
    '```',
  ].join('\n')
  assert.deepEqual(check(markdown), [])
})

test('runbook: prose, shell blocks, uppercase and listed non-metric tokens are ignored', () => {
  const markdown = [
    'plain words like task_runs_completed_total outside code',
    '`SELF_FIX_MAX_MERGE_ATTEMPTS` and `request_id`',
    '```bash',
    'grep some_command_flag file',
    '```',
  ].join('\n')
  assert.deepEqual(check(markdown), [])
})

test('runbook: a never-emitted name passes only inside the Not Implemented section', () => {
  const markdown = [
    `## ${NOT_IMPLEMENTED_HEADING}`,
    '| Workflow start | `temporal_workflow_start_seconds` |',
    '### Detail',
    '`ws_outbound_buffer_depth`',
    '## Next Section',
    '`ws_outbound_buffer_depth`',
  ].join('\n')
  const violations = check(markdown)
  assert.equal(violations.length, 1)
  assert.match(violations[0], /runbook\.md:6: ws_outbound_buffer_depth is never emitted/)
})

test('runbook: an emitted name listed as not implemented fails', () => {
  const violations = check(`## ${NOT_IMPLEMENTED_HEADING}\n\n\`http_requests_total\`\n`)
  assert.equal(violations.length, 1)
  assert.match(
    violations[0],
    /runbook\.md:3: http_requests_total is listed under "Not Implemented Indicators"/
  )
})

test('ops: unknown metric fails with line, external metric passes', () => {
  const yaml = [
    'groups:',
    '  - rules:',
    '      - alert: A',
    '        expr: |',
    '          sqlx_connection_pool_in_use / probe_success',
    '      - alert: B',
    '        expr: http_requests_total > 0',
  ].join('\n')
  const violations = opsViolations('alerts.yml', yamlExprs(yaml), emitted)
  assert.equal(violations.length, 1)
  assert.match(violations[0], /alerts\.yml:5: sqlx_connection_pool_in_use is never emitted/)
})
