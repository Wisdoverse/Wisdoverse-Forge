import { describe, expect, it } from 'vitest'
import {
  computePercentileMs,
  computeSuccessRatePercent,
  computeErrorBudgetPercent,
  parseReadinessStatus,
} from '../../../scripts/lib/slo-gate.js'

describe('slo gate utility', () => {
  it('computes P95 in milliseconds with ceiling rank', () => {
    const samplesSeconds = [0.1, 0.2, 0.3, 0.4, 0.5]
    // rank = ceil(0.95 * 5) = 5 => 0.5s
    expect(computePercentileMs(samplesSeconds, 95)).toBe(500)
  })

  it('returns Infinity for empty percentile input (fail-closed)', () => {
    expect(computePercentileMs([], 95)).toBe(Infinity)
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
