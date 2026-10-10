import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import {
  computePercentileMs,
  computeSuccessRatePercent,
  computeErrorBudgetPercent,
  parseReadinessStatus,
} from '../../../scripts/lib/slo-gate.js'
import {
  summarizeAppShellSamples,
  validateAppShellStorageState,
} from '../../../scripts/measure-app-shell.mjs'

const appShellScript = fileURLToPath(
  new URL('../../../scripts/measure-app-shell.mjs', import.meta.url)
)

describe('slo gate utility', () => {
  it('shows app-shell measurement help without authentication configuration', () => {
    const result = spawnSync(process.execPath, [appShellScript, '--help'], {
      encoding: 'utf8',
      env: { ...process.env, BASE_URL: '', E2E_STORAGE_STATE: '' },
    })

    expect(result.status).toBe(0)
    expect(result.stdout).toContain('BASE_URL:')
    expect(result.stdout).toContain('E2E_STORAGE_STATE:')
    expect(result.stdout).toContain('Success: 20 valid samples')
    expect(result.stdout).toContain('Recovery: docs/runbooks/frontend-quality.md')
  })

  it('rejects unsupported options before reading authentication state', () => {
    const result = spawnSync(process.execPath, [appShellScript, '--unsupported'], {
      encoding: 'utf8',
      env: {
        ...process.env,
        BASE_URL: 'https://staging.example.com',
        E2E_STORAGE_STATE: '/nonexistent/forge-app-shell-test-storage-state-sentinel',
      },
    })

    expect(result.status).toBe(1)
    expect(JSON.parse(result.stdout)).toMatchObject({
      status: 'NOT_RUN',
      attempted: 0,
      issue: 'configuration_invalid',
    })
  })

  it('computes P95 in milliseconds with ceiling rank', () => {
    const samplesSeconds = [0.1, 0.2, 0.3, 0.4, 0.5]
    // rank = ceil(0.95 * 5) = 5 => 0.5s
    expect(computePercentileMs(samplesSeconds, 95)).toBe(500)
  })

  it('returns Infinity for empty percentile input (fail-closed)', () => {
    expect(computePercentileMs([], 95)).toBe(Infinity)
  })

  it('uses the 15th ordered sample for a complete 20-sample p75 cohort', () => {
    const samples = Array.from({ length: 20 }, (_, index) => (20 - index) * 100)
    expect(summarizeAppShellSamples(samples)).toMatchObject({
      status: 'PASS',
      p75Ms: 1500,
      complete: true,
      failed: 0,
    })
  })

  it('requires p75 strictly below 3500 milliseconds', () => {
    expect(summarizeAppShellSamples(Array(20).fill(3499)).status).toBe('PASS')
    expect(summarizeAppShellSamples(Array(20).fill(3500)).status).toBe('FAIL')
  })

  it.each(
    [
      [],
      Array(19).fill(100),
      Array(21).fill(100),
      [...Array(19).fill(100), null],
      [...Array(19).fill(100), NaN],
      [...Array(19).fill(100), 0],
    ].map((samples) => [samples])
  )('rejects empty, incomplete, or invalid cohorts: %j', (samples) => {
    expect(summarizeAppShellSamples(samples)).toMatchObject({
      status: 'FAIL',
      p75Ms: null,
      complete: false,
    })
  })

  it('requires authentication storage and cookie domains to match the target origin', () => {
    const state = {
      origins: [
        {
          origin: 'https://staging.example.com',
          localStorage: [{ name: 'af:auth:access', value: 'fixture-only' }],
        },
      ],
      cookies: [{ domain: 'staging.example.com' }],
    }
    expect(() => validateAppShellStorageState(state, 'https://staging.example.com')).not.toThrow()
    expect(() => validateAppShellStorageState(state, 'https://other.example.com')).toThrow()
    expect(() =>
      validateAppShellStorageState(
        { ...state, cookies: [{ domain: 'other.example.com' }] },
        'https://staging.example.com'
      )
    ).toThrow()
    expect(() =>
      validateAppShellStorageState({ ...state, origins: [] }, 'https://staging.example.com')
    ).toThrow()
    expect(() =>
      validateAppShellStorageState(
        { ...state, origins: [{ origin: 'https://staging.example.com', localStorage: [] }] },
        'https://staging.example.com'
      )
    ).toThrow()
  })

  it('computes integer success rate percentage', () => {
    expect(computeSuccessRatePercent(19, 20)).toBe(95)
    expect(computeSuccessRatePercent(0, 0)).toBe(0)
  })

  it('computes error budget consumption from success rate', () => {
    expect(computeErrorBudgetPercent(95)).toBe(5)
    expect(computeErrorBudgetPercent(100)).toBe(0)
    expect(computeErrorBudgetPercent(0)).toBe(100)
  })

  it('parses readiness payload status safely', () => {
    expect(parseReadinessStatus('{"status":"ready"}')).toBe('ready')
    expect(parseReadinessStatus('{"status":"not_ready"}')).toBe('not_ready')
    expect(parseReadinessStatus('not-json')).toBe('unknown')
  })
})
