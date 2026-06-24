/**
 * SLO gate utility functions.
 *
 * Pure functions for computing latency percentiles, success rates,
 * error budget, and readiness status. The production CI pipeline uses
 * check-deploy-slo.sh which reimplements this logic natively and adds
 * WebSocket SLO and error-budget gates.
 * Returns Infinity for empty percentile samples so gates fail-closed.
 */

function clampPercentile(percentile) {
  if (!Number.isFinite(percentile)) return 95
  if (percentile < 1) return 1
  if (percentile > 100) return 100
  return Math.round(percentile)
}

export function computePercentileMs(samplesSeconds, percentile = 95) {
  if (!Array.isArray(samplesSeconds) || samplesSeconds.length === 0) {
    return Infinity
  }

  const cleaned = samplesSeconds
    .map((value) => Number(value))
    .filter((value) => Number.isFinite(value) && value >= 0)
    .sort((a, b) => a - b)

  if (cleaned.length === 0) {
    return Infinity
  }

  const p = clampPercentile(percentile)
  const rank = Math.ceil((p / 100) * cleaned.length)
  const index = Math.min(Math.max(rank - 1, 0), cleaned.length - 1)

  return Math.round(cleaned[index] * 1000)
}

export function computeSuccessRatePercent(successCount, totalCount) {
  const success = Number(successCount)
  const total = Number(totalCount)
  if (!Number.isFinite(success) || !Number.isFinite(total) || total <= 0 || success < 0) {
    return 0
  }
  return Math.floor((success / total) * 100)
}

export function computeErrorBudgetPercent(successRatePercent) {
  const successRate = Number(successRatePercent)
  if (!Number.isFinite(successRate)) {
    return 100
  }
  if (successRate <= 0) {
    return 100
  }
  if (successRate >= 100) {
    return 0
  }
  return 100 - Math.floor(successRate)
}

export function parseReadinessStatus(payload) {
  if (typeof payload !== 'string' || payload.trim() === '') {
    return 'unknown'
  }
  try {
    const parsed = JSON.parse(payload)
    if (parsed && typeof parsed.status === 'string') {
      return parsed.status
    }
    return 'unknown'
  } catch {
    return 'unknown'
  }
}
