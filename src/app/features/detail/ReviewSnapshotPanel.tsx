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
import { reviewSnapshotErrorMessage } from './model/reviewSnapshotErrorMessage'

interface ReviewSnapshotPanelProps {
  task: TaskSummary
}

const STATUS_LABEL: Record<SelfFixReviewStatus, string> = {
  in_review: 'Waiting for review',
  approved: 'Ready to finish',
  changes_requested: 'Needs changes',
  merged: 'Finished',
  sensitive_blocked: 'Needs owner or admin review',
}

/**
 * Pull request review surface (plan milestone 9). Fetches the server-side
 * review state and lets an operator approve and merge. Approve is enabled ONLY
 * when a pull request exists and `checksGreen && !sensitive &&
 * reviewStatus !== 'merged'`; the merge decision still lives server-side, so
 * disabling here is defense-in-depth, not the security boundary. Errors are
 * surfaced in a banner — never swallowed.
 */
export function ReviewSnapshotPanel({ task }: ReviewSnapshotPanelProps) {
  const upsertTask = useBoardStore((state) => state.upsertTask)
  const [review, setReview] = useState<SelfFixReview | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [approving, setApproving] = useState(false)

  // Load the pull request review on mount AND whenever the task's review status
  // changes. A server-side transition (another operator's approve→merge, and
  // once the loop wires it, a PR-open) is broadcast as an
  // `orchestration:task_update` frame → board upsert → this task prop. Keying
  // the fetch on `task.reviewStatus` re-pulls the full snapshot on that real
  // transition, so `checksGreen`/`sensitive`/`prNumber` stay consistent with
  // the new status instead of going stale. This is NOT a fetch-on-every-render
  // loop: status transitions are rare and the effect ignores all other task
  // field changes. While refetching, the prior snapshot stays on screen (the
  // loading screen only shows when there is no snapshot yet).
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
        if (!cancelled) setError(reviewSnapshotErrorMessage('load', err))
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [task.id, task.reviewStatus])

  async function refresh() {
    setLoading(true)
    setError(null)
    try {
      setReview(await orchestrationApi.getSelfFixReview(task.id))
    } catch (err) {
      setError(reviewSnapshotErrorMessage('load', err))
    } finally {
      setLoading(false)
    }
  }

  async function approve() {
    if (!review?.prNumber || !review.prUrl) return
    setApproving(true)
    setError(null)
    try {
      const reviewStatus = await orchestrationApi.approveSelfFix(task.id)
      setReview({ ...review, reviewStatus })
      upsertTask({ ...task, reviewStatus })
    } catch (err) {
      setError(reviewSnapshotErrorMessage('approve', err))
    } finally {
      setApproving(false)
    }
  }

  const merged = review?.reviewStatus === 'merged'
  const hasPullRequest = Boolean(review?.prNumber && review.prUrl)
  const approveDisabled =
    !review || !hasPullRequest || !review.checksGreen || review.sensitive || merged || approving

  return (
    <div className="py-3 space-y-3" data-testid="review-snapshot-panel">
      <div className="flex items-center justify-between">
        <span className="text-[10px] font-medium uppercase text-secondary-light dark:text-secondary-dark">
          Fix review
        </span>
        <button
          onClick={refresh}
          disabled={loading}
          className={cn(
            'flex items-center gap-1 text-[10px] text-secondary-light dark:text-secondary-dark',
            'hover:text-foreground-light dark:hover:text-foreground-dark transition-colors disabled:opacity-50'
          )}
          aria-label="Refresh fix review"
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
          {hasPullRequest ? (
            <a
              href={review.prUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center gap-2 text-xs font-medium text-apple-blue hover:underline"
            >
              <GitPullRequest size={14} />
              Review page #{review.prNumber}
              <ExternalLink size={11} />
            </a>
          ) : (
            <p className="text-xs text-secondary-light dark:text-secondary-dark">
              The agent is still preparing the review page for this fix. Refresh after it appears.
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
                {review.checksGreen ? 'Automated checks passed' : 'Automated checks still running'}
              </span>
            </div>
            {review.sensitive && (
              <div className="flex items-start gap-1.5 text-apple-red">
                <ShieldAlert size={13} className="mt-px shrink-0" />
                <span>
                  This fix changes sensitive project areas. Ask an owner or admin to review and
                  finish it manually.
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
              Review the changes
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
              {merged ? 'Finished' : approving ? 'Finishing…' : 'Finish this fix'}
            </button>
            {!merged && !hasPullRequest && (
              <p className="mt-1.5 text-[10px] text-secondary-light dark:text-secondary-dark">
                You can finish after the agent opens the review page. Use Refresh after it appears.
              </p>
            )}
            {!merged && hasPullRequest && !review.checksGreen && !review.sensitive && (
              <p className="mt-1.5 text-[10px] text-secondary-light dark:text-secondary-dark">
                You can finish after automated checks pass. Use Refresh after they finish.
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
