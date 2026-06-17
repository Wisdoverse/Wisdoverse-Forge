import { useEffect, useState } from 'react'
import {
  CheckCircle2,
  ExternalLink,
  GitPullRequest,
  Loader2,
  RefreshCw,
  ShieldAlert,
  XCircle,
} from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import {
  orchestrationApi,
  type SelfFixReview,
  type SelfFixReviewStatus,
  type TaskSummary,
} from '@app/shared/api/orchestration'
import { useBoardStore } from '@app/shared/model/board.store'

interface ReviewSnapshotPanelProps {
  task: TaskSummary
}

const STATUS_LABEL: Record<SelfFixReviewStatus, string> = {
  in_review: 'In review',
  approved: 'Approved',
  changes_requested: 'Changes requested',
  merged: 'Merged',
  sensitive_blocked: 'Sensitive — blocked',
}

/**
 * Self-fix draft-PR review surface (plan milestone 9). Fetches the server-side
 * review snapshot and lets an operator approve→merge. Approve is enabled ONLY
 * when `checksGreen && !sensitive && reviewStatus !== 'merged'`; both gating
 * facts are computed server-side, so disabling here is defense-in-depth, not the
 * security boundary. Errors are surfaced in a banner — never swallowed.
 */
export function ReviewSnapshotPanel({ task }: ReviewSnapshotPanelProps) {
  const upsertTask = useBoardStore((state) => state.upsertTask)
  const [review, setReview] = useState<SelfFixReview | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [approving, setApproving] = useState(false)

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError(null)
    orchestrationApi
      .getSelfFixReview(task.id)
      .then((snapshot) => {
        if (!cancelled) setReview(snapshot)
      })
      .catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : 'Failed to load review')
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [task.id])

  // Realtime sync: another operator's approve→merge (and, once the loop wires
  // it, a PR-open) is broadcast as an `orchestration:task_update` frame →
  // board upsert → this task prop. Reflect the new review status in the loaded
  // snapshot WITHOUT a refetch (the full snapshot still self-heals via Refresh
  // or remount). The functional update reads no other state, so it never loops.
  useEffect(() => {
    setReview((current) =>
      current && task.reviewStatus && current.reviewStatus !== task.reviewStatus
        ? { ...current, reviewStatus: task.reviewStatus }
        : current
    )
  }, [task.reviewStatus])

  async function refresh() {
    setLoading(true)
    setError(null)
    try {
      setReview(await orchestrationApi.getSelfFixReview(task.id))
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load review')
    } finally {
      setLoading(false)
    }
  }

  async function approve() {
    if (!review) return
    setApproving(true)
    setError(null)
    try {
      const reviewStatus = await orchestrationApi.approveSelfFix(task.id)
      setReview({ ...review, reviewStatus })
      upsertTask({ ...task, reviewStatus })
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Approve failed')
    } finally {
      setApproving(false)
    }
  }

  const merged = review?.reviewStatus === 'merged'
  const approveDisabled = !review || !review.checksGreen || review.sensitive || merged || approving

  return (
    <div className="py-3 space-y-3" data-testid="review-snapshot-panel">
      <div className="flex items-center justify-between">
        <span className="text-[10px] font-medium uppercase text-secondary-light dark:text-secondary-dark">
          Self-fix review
        </span>
        <button
          onClick={refresh}
          disabled={loading}
          className={cn(
            'flex items-center gap-1 text-[10px] text-secondary-light dark:text-secondary-dark',
            'hover:text-foreground-light dark:hover:text-foreground-dark transition-colors disabled:opacity-50'
          )}
          aria-label="Refresh review snapshot"
        >
          <RefreshCw size={11} className={loading ? 'animate-spin' : undefined} />
          Refresh
        </button>
      </div>

      {error && (
        <div className="px-3 py-2 rounded-lg bg-apple-red/10 text-apple-red text-xs" role="alert">
          {error}
        </div>
      )}

      {loading && !review ? (
        <div className="flex items-center gap-2 px-3 py-6 text-xs text-secondary-light dark:text-secondary-dark">
          <Loader2 size={14} className="animate-spin" />
          Loading review…
        </div>
      ) : review ? (
        <div className="rounded-lg border border-black/[0.06] bg-white p-3 dark:border-white/[0.08] dark:bg-white/[0.04] space-y-3">
          {/* PR linkage */}
          {review.prNumber ? (
            <a
              href={review.prUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center gap-2 text-xs font-medium text-apple-blue hover:underline"
            >
              <GitPullRequest size={14} />
              Pull request #{review.prNumber}
              <ExternalLink size={11} />
            </a>
          ) : (
            <p className="text-xs text-secondary-light dark:text-secondary-dark">
              No pull request has been opened for this task yet.
            </p>
          )}

          {/* Status + CI + sensitive rows */}
          <div className="space-y-1.5 text-xs">
            {review.reviewStatus && (
              <Row label="Status" value={STATUS_LABEL[review.reviewStatus]} />
            )}
            <div className="flex items-center gap-1.5">
              {review.checksGreen ? (
                <CheckCircle2 size={13} className="text-apple-green" />
              ) : (
                <XCircle size={13} className="text-secondary-light dark:text-secondary-dark" />
              )}
              <span className="text-foreground-light dark:text-foreground-dark">
                {review.checksGreen ? 'CI checks passing' : 'CI checks not confirmed green'}
              </span>
            </div>
            {review.sensitive && (
              <div className="flex items-start gap-1.5 text-apple-red">
                <ShieldAlert size={13} className="mt-px shrink-0" />
                <span>
                  Touches a sensitive path — in-platform merge is blocked. A maintainer must review
                  and merge it manually.
                </span>
              </div>
            )}
          </div>

          {review.diffUrl && (
            <a
              href={review.diffUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1 text-[11px] text-secondary-light dark:text-secondary-dark hover:text-apple-blue transition-colors"
            >
              Review file diff
              <ExternalLink size={10} />
            </a>
          )}

          {/* Approve action */}
          <div className="pt-1">
            <button
              onClick={approve}
              disabled={approveDisabled}
              data-testid="review-approve"
              className={cn(
                'w-full flex items-center justify-center gap-1.5 rounded-lg px-3 py-2 text-xs font-medium transition-colors',
                approveDisabled
                  ? 'bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark cursor-not-allowed'
                  : 'bg-apple-blue text-white hover:bg-apple-blue/90'
              )}
            >
              {approving && <Loader2 size={13} className="animate-spin" />}
              {merged ? 'Merged' : approving ? 'Approving…' : 'Approve & merge'}
            </button>
            {!merged && !review.checksGreen && !review.sensitive && (
              <p className="mt-1.5 text-[10px] text-secondary-light dark:text-secondary-dark">
                Approve unlocks once CI is confirmed green. Use Refresh after checks finish.
              </p>
            )}
          </div>
        </div>
      ) : null}
    </div>
  )
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-secondary-light dark:text-secondary-dark">{label}</span>
      <span className="text-foreground-light dark:text-foreground-dark">{value}</span>
    </div>
  )
}
