import assert from 'node:assert/strict'
import { constants } from 'node:fs'
import { open } from 'node:fs/promises'
import { arch, cpus, platform } from 'node:os'
import { resolve } from 'node:path'
import { fileURLToPath, URL } from 'node:url'
import { computePercentileMs } from './lib/slo-gate.js'

const SAMPLE_COUNT = 20
const MARKER = 'forge:app-shell-commit'
const NETWORK = { latency: 40, downloadThroughput: 1_250_000, uploadThroughput: 125_000 }

export function summarizeAppShellSamples(samplesMs) {
  const valid = samplesMs.filter((sample) => Number.isFinite(sample) && sample > 0)
  const complete = samplesMs.length === SAMPLE_COUNT && valid.length === SAMPLE_COUNT
  const p75Ms = complete
    ? computePercentileMs(
        valid.map((sample) => sample / 1000),
        75
      )
    : null
  return {
    status: complete && p75Ms < 3500 ? 'PASS' : 'FAIL',
    expected: SAMPLE_COUNT,
    attempted: samplesMs.length,
    failed: samplesMs.length - valid.length,
    complete,
    samplesMs,
    p75Ms,
    thresholdMs: 3500,
  }
}

export function validateAppShellStorageState(state, origin) {
  const hostname = new URL(origin).hostname
  assert(Array.isArray(state.origins) && state.origins.length === 1)
  assert(state.origins[0].origin === origin)
  assert(Array.isArray(state.origins[0].localStorage))
  assert(
    state.origins[0].localStorage.some(
      (entry) => entry.name === 'af:auth:access' && typeof entry.value === 'string' && entry.value
    )
  )
  assert(Array.isArray(state.cookies))
  assert(state.cookies.every((cookie) => cookie.domain.replace(/^\./, '') === hostname))
}

async function main() {
  if (process.argv.length === 3 && process.argv[2] === '--help') {
    process.stdout.write(
      'Usage: npm run measure:app-shell\n' +
        'Prerequisites: Node.js 24.15+, repository dependencies, Chromium, and a production frontend.\n' +
        'BASE_URL: the frontend origin.\n' +
        'E2E_STORAGE_STATE: private, valid authentication state for that origin.\n' +
        'CHROMIUM_PATH: optional Chromium executable.\n' +
        'Success: 20 valid samples, zero failures, and p75 below 3500 ms.\n' +
        'Recovery: docs/runbooks/frontend-quality.md.\n'
    )
    return
  }
  const samplesMs = []
  const report = {
    measuredAt: new Date().toISOString(),
    metric: 'verified_app_shell_ready',
    route: '/tasks',
    start: 'Performance.timeOrigin',
    end: 'max(shell_commit, successful_auth_response_end)',
    environment: { platform: platform(), architecture: arch(), cpuModel: cpus()[0]?.model },
    profile: {
      viewport: { width: 1440, height: 900 },
      ...NETWORK,
      cpuSlowdown: 4,
      cache: 'disabled',
      serviceWorkers: 'blocked',
    },
  }
  let browser
  let issue = null
  let stage = 'configuration_invalid'
  try {
    assert(process.argv.length === 2)
    assert(process.env.BASE_URL && process.env.E2E_STORAGE_STATE)
    const target = new URL(process.env.BASE_URL)
    assert(['http:', 'https:'].includes(target.protocol))
    assert(
      !target.username &&
        !target.password &&
        !target.search &&
        !target.hash &&
        target.pathname === '/'
    )
    stage = 'auth_state_invalid'
    const authFile = await open(
      process.env.E2E_STORAGE_STATE,
      constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0)
    )
    let storageState
    try {
      const stat = await authFile.stat()
      assert(stat.isFile())
      assert(process.platform === 'win32' || (stat.mode & 0o077) === 0)
      storageState = JSON.parse(await authFile.readFile('utf8'))
      validateAppShellStorageState(storageState, target.origin)
    } finally {
      await authFile.close()
    }
    stage = 'browser_unavailable'
    const { chromium } = await import('@playwright/test')
    browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH || undefined })
    report.browserVersion = browser.version()
    target.pathname = '/tasks'
    for (let index = 0; index < SAMPLE_COUNT; index += 1) {
      samplesMs.push(null)
      stage = 'browser_configuration_failed'
      const context = await browser.newContext({
        storageState,
        viewport: report.profile.viewport,
        serviceWorkers: 'block',
      })
      try {
        const page = await context.newPage()
        let pageFailed = false
        page.on('pageerror', () => {
          pageFailed = true
        })
        page.on('crash', () => {
          pageFailed = true
        })
        const cdp = await context.newCDPSession(page)
        await cdp.send('Network.enable')
        await cdp.send('Network.setCacheDisabled', { cacheDisabled: true })
        await cdp.send('Network.setBypassServiceWorker', { bypass: true })
        await cdp.send('Network.emulateNetworkConditionsByRule', {
          offline: false,
          matchedNetworkConditions: [{ urlPattern: '', ...NETWORK }],
        })
        await cdp.send('Network.overrideNetworkState', { offline: false, ...NETWORK })
        await cdp.send('Emulation.setCPUThrottlingRate', { rate: 4 })
        const authenticated = page
          .waitForResponse(
            (response) => {
              const url = new URL(response.url())
              return (
                url.origin === target.origin &&
                url.pathname === '/api/v1/me' &&
                response.request().method() === 'GET'
              )
            },
            { timeout: 30_000 }
          )
          .then(async (response) =>
            response.ok() && (await response.json()).ok === true ? response.url() : null
          )
          .catch(() => null)
        stage = 'navigation_failed'
        const response = await page.goto(target.href, {
          waitUntil: 'domcontentloaded',
          timeout: 30_000,
        })
        assert(response?.ok() && !response.request().redirectedFrom())
        stage = 'authenticated_shell_unavailable'
        await page.waitForFunction(
          (marker) =>
            window.performance.getEntriesByName(marker).length > 0 ||
            window.location.pathname === '/login',
          MARKER,
          { timeout: 30_000 }
        )
        assert(page.url() === target.href)
        const authUrl = await authenticated
        assert(authUrl)
        assert(await page.locator('#forge-workspace').isVisible())
        const timing = await page.evaluate(
          ({ marker, authUrl }) => ({
            commitMs: window.performance.getEntriesByName(marker)[0]?.startTime,
            authenticationMs: window.performance.getEntriesByName(authUrl, 'resource').at(-1)
              ?.responseEnd,
          }),
          { marker: MARKER, authUrl }
        )
        assert(!pageFailed && Number.isFinite(timing.commitMs) && timing.commitMs > 0)
        assert(Number.isFinite(timing.authenticationMs) && timing.authenticationMs > 0)
        samplesMs[index] = Math.max(timing.commitMs, timing.authenticationMs)
      } finally {
        await context.close()
      }
    }
  } catch {
    issue = stage
  } finally {
    await browser?.close().catch(() => {
      issue = 'browser_cleanup_failed'
    })
  }
  const result = { ...report, ...summarizeAppShellSamples(samplesMs), issue }
  if (issue) result.status = samplesMs.length ? 'FAIL' : 'NOT_RUN'
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`)
  process.exitCode = result.status === 'PASS' ? 0 : 1
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main()
