#!/usr/bin/env node

import { spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import {
  classifyPullRequest,
  renderSummary,
  summarizePullRequests,
} from './lib/pr-status-summary.js'

const CACHE_VERSION = 1
const DEFAULT_CACHE_TTL_SECONDS = 900

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
  const snapshot = readPullRequestSnapshot(options)
  const pullRequests = snapshot.pullRequests
  const summary = summarizePullRequests(pullRequests)

  if (snapshot.cacheHit) {
    console.error(formatCacheNotice(snapshot, options))
  }

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
    cacheFile: '',
    cacheTtlSeconds: DEFAULT_CACHE_TTL_SECONDS,
    failOnAction: false,
    inputPath: '',
    json: false,
    limit: 120,
    noCache: false,
    refresh: false,
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
    } else if (arg === '--refresh') {
      options.refresh = true
    } else if (arg === '--no-cache') {
      options.noCache = true
    } else if (arg === '--limit') {
      options.limit = parsePositiveInt(readValue(args, index, arg), options.limit)
      index += 1
    } else if (arg === '--cache-ttl-seconds') {
      options.cacheTtlSeconds = parseNonNegativeInt(
        readValue(args, index, arg),
        options.cacheTtlSeconds
      )
      index += 1
    } else if (arg === '--cache-file') {
      options.cacheFile = readValue(args, index, arg)
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

function readPullRequestSnapshot(options, now = Date.now()) {
  if (options.inputPath) {
    return {
      pullRequests: readPullRequestsFromFile(options.inputPath),
      source: options.inputPath,
      cacheHit: false,
    }
  }

  const cachePath = resolveCachePath(options.cacheFile)
  if (!options.noCache && !options.refresh) {
    const cached = readFreshCache(cachePath, options, now)
    if (cached) return cached
  }

  const pullRequests = readPullRequestsFromGitHub(options)
  if (!options.noCache) {
    writeCache(cachePath, options, pullRequests, now)
  }

  return {
    pullRequests,
    source: 'GitHub',
    cacheHit: false,
  }
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

function readFreshCache(cachePath, options, now) {
  if (!existsSync(cachePath)) return null

  try {
    const cache = JSON.parse(readFileSync(cachePath, 'utf8'))
    if (!isUsableCacheEntry(cache, options, now)) return null
    return {
      pullRequests: cache.pullRequests,
      source: cachePath,
      cacheHit: true,
      cacheAgeSeconds: Math.max(0, Math.floor((now - cache.fetchedAt) / 1000)),
    }
  } catch {
    return null
  }
}

function writeCache(cachePath, options, pullRequests, now) {
  try {
    mkdirSync(dirname(cachePath), { recursive: true })
    writeFileSync(
      cachePath,
      `${JSON.stringify(
        {
          version: CACHE_VERSION,
          fetchedAt: now,
          query: cacheQuery(options),
          pullRequests,
        },
        null,
        2
      )}\n`
    )
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    console.error(`[pr-summary] cache write skipped: ${message}`)
  }
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

function parseNonNegativeInt(value, fallback) {
  const parsed = Number.parseInt(value, 10)
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : fallback
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

function resolveCachePath(cacheFile) {
  if (cacheFile) return cacheFile

  const result = spawnSync('git', ['rev-parse', '--git-path', 'pr-status-summary-cache.json'], {
    encoding: 'utf8',
  })
  if (!result.error && result.status === 0) {
    const gitPath = result.stdout.trim()
    if (gitPath) return gitPath
  }

  return join(process.cwd(), '.pr-status-summary-cache.json')
}

function isUsableCacheEntry(cache, options, now = Date.now()) {
  if (!cache || cache.version !== CACHE_VERSION || !Array.isArray(cache.pullRequests)) return false
  if (!cache.fetchedAt || !Number.isFinite(cache.fetchedAt)) return false
  if (options.cacheTtlSeconds <= 0) return false
  if (now - cache.fetchedAt > options.cacheTtlSeconds * 1000) return false
  return JSON.stringify(cache.query) === JSON.stringify(cacheQuery(options))
}

function cacheQuery(options) {
  return {
    fields: GH_FIELDS,
    limit: options.limit,
    state: options.state,
  }
}

function formatCacheNotice(snapshot, options) {
  return `[pr-summary] using cached GitHub snapshot from ${formatAge(
    snapshot.cacheAgeSeconds
  )}; pass --refresh to query GitHub now or --cache-ttl-seconds ${options.cacheTtlSeconds} to change reuse time`
}

function formatAge(seconds) {
  if (!Number.isFinite(seconds) || seconds <= 0) return 'just now'
  if (seconds < 60) return `${seconds}s ago`
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  return `${hours}h ago`
}

function printHelp() {
  console.info(`Usage: node scripts/pr-status-summary.mjs [options]

Summarize GitHub PRs into low-token ACTION / WAIT / DONE buckets.

Options:
  --limit <n>         Number of PRs to read from GitHub. Default: 120
  --state <state>    open, closed, or all. Default: open
  --refresh          Ignore the local cache and query GitHub now
  --no-cache         Query GitHub and do not read or write the local cache
  --cache-ttl-seconds <n>
                     Reuse a local GitHub snapshot for this many seconds. Default: 900
  --cache-file <path>
                     Use a custom cache file. Default: Git temp path
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

export {
  CACHE_VERSION,
  cacheQuery,
  classifyPullRequest,
  isUsableCacheEntry,
  parseArgs,
  readPullRequestSnapshot,
}
