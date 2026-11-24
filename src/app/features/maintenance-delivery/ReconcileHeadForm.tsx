import { useEffect, useId, useRef, useState } from 'react'
import type { MaintenanceDelivery } from '@shared/types/maintenance-delivery'
import { reconcileMaintenanceHead } from '@app/entities/maintenance'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { Feedback } from './fields'
import { useSubmission } from './useSubmission'

const short = (sha: string) => sha.slice(0, 7)

/** The pull request's new head, when an administrator may adopt it now. */
export function adoptableHead(data: MaintenanceDelivery): { stored: string; next: string } | null {
  const observed = data.trace.producedState
  const stored = data.trace.recordedPrHeadSha
  const status = data.recovery.reviewStatus
  const snapshot = observed.snapshot
  if (!observed.headChanged || snapshot?.state !== 'open') return null
  if (!stored || status === 'approved' || status === 'merged') return null
  return { stored, next: snapshot.headSha }
}

/**
 * Adopt the pull request's new head after an outside push. Two steps: the
 * action, then an in-page confirmation showing both revisions.
 */
export function ReconcileHeadForm({
  data,
  onReconciled,
}: {
  data: MaintenanceDelivery
  onReconciled: () => void
}) {
  const heads = adoptableHead(data)
  const [confirming, setConfirming] = useState(false)
  const submission = useSubmission(onReconciled)
  const consequenceId = useId()
  const confirmRef = useRef<HTMLButtonElement>(null)
  // Move focus to the confirmation when it appears.
  useEffect(() => {
    if (confirming) confirmRef.current?.focus()
  }, [confirming])
  if (!heads) return null
  function adopt() {
    if (!heads) return
    const input = { expectedStoredHead: heads.stored, expectedNewHead: heads.next }
    void submission.submit(input, (_key, signal) =>
      reconcileMaintenanceHead(data.trace.taskId, input, signal)
    )
  }
  return (
    <div
      className={`${uiStyles.cardPadded} space-y-3`}
      data-testid="maintenance-reconcile-head"
      role="group"
      aria-labelledby={`${consequenceId}-title`}
    >
      <h3 id={`${consequenceId}-title`} className="font-medium">
        The pull request has a newer revision
      </h3>
      <p className="break-all font-mono text-ui-caption">
        Recorded revision {short(heads.stored)} · newer revision on GitHub {short(heads.next)}
      </p>
      <p id={consequenceId} className={uiStyles.sectionDescription}>
        Earlier reports and verdicts no longer apply; a new report is needed before acceptance.
      </p>
      {confirming ? (
        <div className="flex flex-wrap gap-2">
          <button
            ref={confirmRef}
            type="button"
            className={uiStyles.primaryButton}
            disabled={submission.busy}
            aria-describedby={consequenceId}
            onClick={adopt}
          >
            {submission.busy
              ? 'Using the new revision…'
              : `Confirm: use revision ${short(heads.next)}`}
          </button>
          <button
            type="button"
            className={uiStyles.secondaryButton}
            disabled={submission.busy}
            onClick={() => setConfirming(false)}
          >
            Cancel
          </button>
        </div>
      ) : (
        <button
          type="button"
          className={uiStyles.secondaryButton}
          aria-describedby={consequenceId}
          onClick={() => setConfirming(true)}
        >
          Use the new revision
        </button>
      )}
      <Feedback message={submission.message} />
    </div>
  )
}
