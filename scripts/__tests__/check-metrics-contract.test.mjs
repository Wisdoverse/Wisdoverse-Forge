import assert from 'node:assert/strict'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import test from 'node:test'
import {
  checkRepository,
  collectRecorded,
  jsonExprs,
  NOT_IMPLEMENTED_HEADING,
  opsViolations,
  promqlTokens,
  runbookViolations,
  rustViews,
  scanRustSource,
  seriesProblem,
  stripTestItems,
  yamlExprs,
} from '../check-metrics-contract.mjs'

const catalog = {
  types: new Map([
    ['http_requests_total', new Set(['counter'])],
    ['queue_depth', new Set(['gauge'])],
    ['http_request_duration_seconds', new Set(['histogram'])],
    ['publish_seconds', new Set(['histogram'])],
  ]),
  bucketed: new Set(['http_request_duration_seconds']),
}
const names = (expr) => promqlTokens(expr).map((t) => t.name)
const recorded = (src) => [...scanRustSource(stripTestItems(rustViews(src))).types.keys()]
const ops = (yaml, defined) => opsViolations('rules.yml', yamlExprs(yaml).exprs, catalog, defined)
const runbook = (markdown) => runbookViolations('runbook.md', markdown, catalog)

// --- PromQL tokenizer ---------------------------------------------------------

test('tokenizer: an operator directly before a name does not hide it', () => {
  assert.deepEqual(names('http_requests_total/missing_metric'), [
    'http_requests_total',
    'missing_metric',
  ])
})

test('tokenizer: names without underscore, with uppercase or with colons are references', () => {
  assert.deepEqual(names('up + MyMetric - job:requests:rate5m'), [
    'up',
    'MyMetric',
    'job:requests:rate5m',
  ])
})

test('tokenizer: skips functions, aggregations, keywords, labels, strings, numbers and variables', () => {
  const expr = [
    'histogram_quantile(0.95, sum by (le,',
    '  cli_tool) (rate(a_bucket{path=~"/x_y", le!="1"}[5m] offset 1h)))',
    '/ on(instance) group_left(node_name) b_total * 1e3 > bool 0x1F',
    'and topk(5, c_total) unless count without (x_y) (d_total) or vector(0) # e_total',
    '+ rate(f_total[$__rate_interval]) + g_total{path=~"$path"} + "h_total" + Inf',
  ].join('\n')
  assert.deepEqual(names(expr), ['a_bucket', 'b_total', 'c_total', 'd_total', 'f_total', 'g_total'])
})

test('tokenizer: keeps the label selector of a reference', () => {
  assert.deepEqual(promqlTokens('x_seconds{quantile="0.95"} > 1')[0].selector, '{quantile="0.95"}')
})

// --- series rules -------------------------------------------------------------

test('series: counters and gauges take no histogram suffix', () => {
  assert.equal(seriesProblem('http_requests_total', catalog), null)
  assert.match(
    seriesProblem('http_requests_total_count', catalog),
    /counter, which takes no _count suffix/
  )
  assert.match(seriesProblem('queue_depth_sum', catalog), /gauge, which takes no _sum suffix/)
})

test('series: a histogram with buckets has _bucket/_sum/_count, not the base name', () => {
  for (const suffix of ['_bucket', '_sum', '_count']) {
    assert.equal(seriesProblem(`http_request_duration_seconds${suffix}`, catalog), null)
  }
  assert.match(seriesProblem('http_request_duration_seconds', catalog), /histogram with buckets/)
  assert.equal(seriesProblem('http_request_duration_seconds', catalog, { family: true }), null)
})

test('series: a histogram without buckets is a summary', () => {
  assert.match(
    seriesProblem('publish_seconds_bucket', catalog),
    /no buckets are configured for publish_seconds/
  )
  assert.equal(seriesProblem('publish_seconds_count', catalog), null)
  assert.equal(seriesProblem('publish_seconds', catalog, { selector: '{quantile="0.95"}' }), null)
  assert.match(
    seriesProblem('publish_seconds', catalog, { selector: '{quantile="0.42"}' }),
    /quantile="0.42"/
  )
})

test('series: unknown fails, external and recording-rule names pass', () => {
  assert.match(seriesProblem('missing_metric', catalog), /never recorded/)
  assert.equal(seriesProblem('probe_success', catalog), null)
  assert.equal(
    seriesProblem('job:requests:rate5m', catalog, { defined: new Set(['job:requests:rate5m']) }),
    null
  )
})

// --- YAML and JSON extraction -------------------------------------------------

test('yaml: quoted, folded and literal expressions are all checked, with the rule line', () => {
  const yaml = [
    'groups:',
    '  - rules:',
    "      - expr: 'single_missing > 0'",
    '      - expr: "double_missing > 0"',
    '      - expr: >',
    '          folded_missing',
    '          > 0',
    '      - expr: |',
    '          sum by (',
    '            cli_tool',
    '          ) (literal_missing) / http_requests_total',
  ].join('\n')
  const violations = ops(yaml)
  assert.deepEqual(
    violations.map((v) =>
      v
        .match(/rules\.yml:(\d+): (\w+)/)
        .slice(1)
        .join(' ')
    ),
    ['3 single_missing', '4 double_missing', '5 folded_missing', '8 literal_missing']
  )
})

test('yaml: a recording rule defines its name', () => {
  const yaml =
    'groups:\n  - rules:\n      - record: job:x:rate5m\n        expr: rate(http_requests_total[5m])\n'
  const { exprs, records } = yamlExprs(yaml)
  assert.deepEqual(records, ['job:x:rate5m'])
  assert.deepEqual(
    opsViolations('r.yml', [{ line: 1, text: 'job:x:rate5m > 1' }], catalog, new Set(records)),
    []
  )
  assert.equal(exprs.length, 1)
})

test('json: datasource variables are skipped and label_values keeps only its selector', () => {
  const dashboard = JSON.stringify({
    templating: {
      list: [
        { type: 'datasource', query: 'prometheus' },
        { type: 'query', query: 'label_values(http_requests_total{a="b"}, path)' },
        { type: 'query', query: 'label_values(path)' },
      ],
    },
    panels: [{ targets: [{ expr: 'rate(missing_total[5m])' }] }],
  })
  const exprs = jsonExprs(dashboard)
  assert.deepEqual(
    exprs.map((e) => e.text),
    ['http_requests_total{a="b"}', '', 'rate(missing_total[5m])']
  )
  const violations = opsViolations('d.json', exprs, catalog)
  assert.equal(violations.length, 1)
  assert.match(violations[0], /d\.json:1: missing_total is never recorded/)
})

// --- Rust emitters ------------------------------------------------------------

test('emitters: only recording macros in production code count', () => {
  const src = [
    'fn register() {',
    '    metrics::describe_counter!("described_only_total", "help");',
    '    // metrics::counter!("line_comment_total").increment(1);',
    '    /* metrics::counter!("block_comment_total") /* nested */ */',
    '    let url = "http://example.invalid/{"; let brace = \'{\';',
    '    let raw = r#"counter!("raw_string_total") }"#;',
    '    metrics::counter!("real_total").increment(1);',
    '    metrics::gauge!(',
    '        "real_gauge"',
    '    ).set(1.0);',
    '}',
    '#[cfg(test)]',
    'mod tests {',
    '    fn helper() { metrics::counter!("cfg_test_module_total").increment(1); }',
    '}',
    '#[tokio::test(flavor = "multi_thread")]',
    'async fn t() { metrics::histogram!("test_fn_seconds").record(1.0); }',
    'fn after() { metrics::histogram!("after_seconds").record(1.0); }',
  ].join('\n')
  assert.deepEqual(recorded(src), ['real_total', 'real_gauge', 'after_seconds'])
})

test('emitters: bucket configuration is read; unsupported configuration fails', () => {
  const scan = (src, rel) => scanRustSource(rustViews(src), rel)
  const ok = scan(
    'PrometheusBuilder::new().set_buckets_for_metric(Matcher::Full("a_seconds".to_owned()), &B)?'
  )
  assert.deepEqual([...ok.bucketed], ['a_seconds'])
  assert.deepEqual(ok.problems, [])
  const bad = scan(
    'b.set_buckets_for_metric(Matcher::Prefix("a".into()), &B)?\n.set_buckets(&B)?\n.set_quantiles(&Q)?',
    'main.rs'
  )
  assert.equal(bad.problems.length, 3)
  assert.match(bad.problems[0], /main\.rs:1: set_buckets_for_metric without Matcher::Full/)
})

test('emitters: tests/ directories and out-of-line #[cfg(test)] module files are ignored', (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'metrics-contract-'))
  t.after(() => fs.rmSync(root, { recursive: true, force: true }))
  const write = (file, content) => {
    fs.mkdirSync(path.dirname(path.join(root, file)), { recursive: true })
    fs.writeFileSync(path.join(root, file), content)
  }
  write(
    'rust/crates/a/src/lib.rs',
    '#[cfg(test)]\nmod router_tests;\nfn f() { counter!("prod_total"); }\n'
  )
  write('rust/crates/a/src/router_tests.rs', 'fn g() { counter!("out_of_line_test_total"); }\n')
  write('rust/crates/a/tests/it.rs', 'fn h() { counter!("integration_test_total"); }\n')
  assert.deepEqual([...collectRecorded(root).types.keys()], ['prod_total'])
})

// --- runbook ------------------------------------------------------------------

test('runbook: an unknown metric fails with file and line', () => {
  const violations = runbook('# Title\n\n| SLI | `missing_metric_total / http_requests_total` |\n')
  assert.equal(violations.length, 1)
  assert.match(violations[0], /runbook\.md:3: missing_metric_total is never recorded/)
})

test('runbook: recorded names, histogram suffixes and family mentions pass', () => {
  const markdown = [
    '`http_requests_total{status="500"}` and `http_request_duration_seconds`',
    '```promql',
    'sum(rate(http_request_duration_seconds_bucket[5m])) / http_request_duration_seconds_count',
    '```',
  ].join('\n')
  assert.deepEqual(runbook(markdown), [])
})

test('runbook: promql fences use the full tokenizer with the token line', () => {
  const markdown = [
    'text',
    '```promql',
    'sum by (',
    '  le',
    ') (rate(http_requests_total[5m]))',
    '/ up',
    '```',
  ].join('\n')
  const violations = runbook(markdown)
  assert.equal(violations.length, 1)
  assert.match(violations[0], /runbook\.md:6: up is never recorded/)
})

test('runbook: prose, shell blocks, uppercase and listed non-metric tokens are ignored', () => {
  const markdown = [
    'plain words like task_runs_completed_total outside code',
    '`SELF_FIX_MAX_MERGE_ATTEMPTS` and `request_id`',
    '```bash',
    'grep some_command_flag file',
    '```',
  ].join('\n')
  assert.deepEqual(runbook(markdown), [])
})

test('runbook: an unrecorded name passes only inside the Not Implemented section', () => {
  const markdown = [
    `## ${NOT_IMPLEMENTED_HEADING}`,
    '| Workflow start | `temporal_workflow_start_seconds` |',
    '### Detail',
    '`ws_outbound_buffer_depth`',
    '## Next Section',
    '`ws_outbound_buffer_depth`',
  ].join('\n')
  const violations = runbook(markdown)
  assert.equal(violations.length, 1)
  assert.match(violations[0], /runbook\.md:6: ws_outbound_buffer_depth is never recorded/)
})

test('runbook: a recorded name listed as not implemented fails', () => {
  const violations = runbook(`## ${NOT_IMPLEMENTED_HEADING}\n\n\`http_requests_total\`\n`)
  assert.equal(violations.length, 1)
  assert.match(
    violations[0],
    /runbook\.md:3: http_requests_total is listed under "Not Implemented Indicators"/
  )
})

// --- the real repository ------------------------------------------------------

test('the repository satisfies the metrics contract', () => {
  assert.deepEqual(checkRepository().violations, [])
})
