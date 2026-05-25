#!/usr/bin/env node

import { spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import {
  classifyPullRequest,
  renderSummary,
  summarizePullRequests,
} from './lib/pr-status-summary.js'

const GH_FIELDS = [
  'autoMergeRequest',
  'headRefName',
  'isDraft',
  'mergeStateStatus',
  'number',
  'reviewDecision',
  'state',
  'statusCheckRollup',
  'title',
  'url',
].join(',')

function main() {
  const options = parseArgs(process.argv.slice(2))
  const pullRequests = options.inputPath
    ? readPullRequestsFromFile(options.inputPath)
    : readPullRequestsFromGitHub(options)
  const summary = summarizePullRequests(pullRequests)

  if (options.json) {
    console.info(JSON.stringify(summary, null, 2))
  } else {
    console.info(renderSummary(summary, { showWait: options.showWait }).trimEnd())
  }

  if (options.failOnAction && summary.action.length > 0) {
    process.exit(1)
  }
}

function parseArgs(args) {
  const options = {
    failOnAction: false,
    inputPath: '',
    json: false,
    limit: 120,
    showWait: false,
    state: 'open',
  }

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index]
    if (arg === '--show-wait') {
      options.showWait = true
    } else if (arg === '--json') {
      options.json = true
    } else if (arg === '--fail-on-action') {
      options.failOnAction = true
    } else if (arg === '--limit') {
      options.limit = parsePositiveInt(readValue(args, index, arg), options.limit)
      index += 1
    } else if (arg === '--state') {
      options.state = parseState(readValue(args, index, arg))
      index += 1
    } else if (arg === '--input') {
      options.inputPath = readValue(args, index, arg)
      index += 1
    } else if (arg === '--help' || arg === '-h') {
      printHelp()
      process.exit(0)
    } else {
      throwUsageError(`unknown option: ${arg}`)
    }
  }

  return options
}

function readPullRequestsFromGitHub(options) {
  const result = spawnSync(
    'gh',
    ['pr', 'list', '--state', options.state, '--limit', String(options.limit), '--json', GH_FIELDS],
    { encoding: 'utf8' }
  )

  if (result.error) {
    throwUsageError(`unable to run gh: ${result.error.message}`)
  }
  if (result.status !== 0) {
    throwUsageError(result.stderr.trim() || 'gh pr list failed')
  }

  return parsePullRequestJson(result.stdout, 'gh pr list')
}

function readPullRequestsFromFile(inputPath) {
  return parsePullRequestJson(readFileSync(inputPath, 'utf8'), inputPath)
}

function parsePullRequestJson(payload, source) {
  try {
    const parsed = JSON.parse(payload)
    if (!Array.isArray(parsed)) {
      throw new Error('expected a JSON array')
    }
    return parsed
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    throwUsageError(`invalid PR JSON from ${source}: ${message}`)
  }
}

function parsePositiveInt(value, fallback) {
  const parsed = Number.parseInt(value, 10)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback
}

function parseState(value) {
  if (value === 'open' || value === 'closed' || value === 'all') return value
  throwUsageError('--state must be open, closed, or all')
}

function readValue(args, index, flag) {
  const value = args[index + 1]
  if (!value || value.startsWith('--')) {
    throwUsageError(`${flag} requires a value`)
  }
  return value
}

function printHelp() {
  console.info(`Usage: node scripts/pr-status-summary.mjs [options]

Summarize GitHub PRs into low-token ACTION / WAIT / DONE buckets.

Options:
  --limit <n>         Number of PRs to read from GitHub. Default: 120
  --state <state>    open, closed, or all. Default: open
  --show-wait        Print each WAIT item instead of only the wait count
  --json             Print structured summary JSON
  --input <file>     Read gh-style PR JSON from a file instead of calling GitHub
  --fail-on-action   Exit 1 when any PR needs action
  -h, --help         Show this help
`)
}

function throwUsageError(message) {
  console.error(`[pr-summary] ERROR: ${message}`)
  process.exit(2)
}

if (process.argv[1]?.endsWith('pr-status-summary.mjs')) {
  main()
}

export { classifyPullRequest, parseArgs }
