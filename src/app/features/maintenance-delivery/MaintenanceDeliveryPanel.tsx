import { useEffect, useState } from 'react'
import type { MaintenanceDelivery } from '@shared/types/maintenance-delivery'
import { getMaintenanceDelivery } from '@app/entities/maintenance'
import { useNavigationStore } from '@app/entities/navigation'
import { useAuth } from '@app/shared/model/auth.context'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { deliveryGuidance } from './guidance'
import { Timestamp } from './fields'
import { ReportForm } from './ReportForm'
import { DecisionForm } from './DecisionForm'
import { HandoffForm } from './HandoffForm'
import { CheckObservation, ReportCard } from './ReportCard'

type State =
  | { status: 'loading' }
  | { status: 'error'; message: string }
  | { status: 'ready'; data: MaintenanceDelivery | null }
export function MaintenanceDeliveryPanel({ taskId }: { taskId: string }) {
  const { user, isAuthenticated, isLoading } = useAuth()
  const org = useNavigationStore((s) => s.selectedOrgId)
  if (isLoading || !isAuthenticated || !user?.isAdmin) return null
  return <Panel key={`${user.id}:${user.orgId ?? ''}:${org ?? ''}:${taskId}`} taskId={taskId} />
}
function Panel({ taskId }: { taskId: string }) {
  const [state, setState] = useState<State>({ status: 'loading' }),
    [version, setVersion] = useState(0),
    [notice, setNotice] = useState('')
  useEffect(() => {
    const controller = new AbortController()
    setState({ status: 'loading' })
    void getMaintenanceDelivery(taskId, controller.signal).then(
      (data) => {
        if (!controller.signal.aborted) setState({ status: 'ready', data })
      },
      (error) => {
        if (!controller.signal.aborted)
          setState({ status: 'error', message: deliveryGuidance(error) })
      }
    )
    return () => controller.abort()
  }, [taskId, version])
  if (state.status === 'ready' && state.data === null) return null
  const data = state.status === 'ready' ? state.data : null
  const current = (report: MaintenanceDelivery['reports'][number]) =>
    report.revision === data?.currentRevision && report.runId === data.latestRunId
  const latest = data?.reports[0] && current(data.reports[0]) ? data.reports[0] : undefined
  function refresh() {
    setState({ status: 'loading' })
    setVersion((v) => v + 1)
  }
  function saved() {
    setNotice('Record saved. Refreshing the current evidence and history.')
    refresh()
  }
  return (
    <section
      className="mt-6 space-y-4 break-words"
      data-testid="maintenance-delivery"
      aria-labelledby="maintenance-delivery-title"
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 id="maintenance-delivery-title" className={uiStyles.sectionTitle}>
          Verification, recovery and human review
        </h2>
        <button
          type="button"
          className={uiStyles.secondaryButton}
          disabled={state.status === 'loading'}
          onClick={refresh}
        >
          Refresh evidence
        </button>
      </div>
      {notice && (
        <p role="status" className={uiStyles.note}>
          {notice}
        </p>
      )}
      {state.status === 'loading' && (
        <p role="status">
          Loading recorded evidence and checking GitHub. This takes up to 30 seconds.
        </p>
      )}
      {state.status === 'error' && (
        <p role="alert" aria-live="polite" className={uiStyles.error}>
          {state.message}
        </p>
      )}
      {data && (
        <>
          <div className={`${uiStyles.cardPadded} space-y-3`}>
            <p className="break-all font-mono text-ui-caption">
              Current recorded revision: {data.currentRevision}
            </p>
            <CheckObservation checks={data.currentChecks} label="Current GitHub observation" />
          </div>
          <div className={`${uiStyles.cardPadded} space-y-2`}>
            <h3 className="font-medium">Recovery state</h3>
            <p>
              Observed <Timestamp value={data.recovery.observedAt} />
            </p>
            <p>
              {data.recovery.taskState} · task attempt {data.recovery.attempt}
              {data.recovery.failureCode && ` · ${data.recovery.failureCode}`}
              {data.recovery.blockedReason && ` · waiting on ${data.recovery.blockedReason}`}
            </p>
            <p>
              Merge attempts: {data.recovery.mergeAttempts} / {data.recovery.mergeLimit}
            </p>
            {data.recovery.bridge && (
              <p>
                PR preparation: {data.recovery.bridge.state} · failed attempts{' '}
                {data.recovery.bridge.attempts} / {data.recovery.bridge.limit}
                {data.recovery.bridge.nextAttemptAt && (
                  <>
                    {' '}
                    · next eligible attempt <Timestamp value={data.recovery.bridge.nextAttemptAt} />
                  </>
                )}
              </p>
            )}
            <p className={uiStyles.sectionDescription}>
              {data.recovery.manualRetryAllowed
                ? 'Use Retry in task controls to request another execution after checking the blocker.'
                : 'Review the blocker and task controls before deciding whether work can continue.'}{' '}
              Stored reports and handoffs remain available after retries. A retry can create a new
              run and revision requiring a new report.
            </p>
          </div>
          <ReportForm data={data} onSaved={saved} />
          {latest ? (
            <DecisionForm data={data} report={latest} onSaved={saved} />
          ) : (
            <p className={uiStyles.note}>
              Create a report for the current recorded run and revision before recording a human
              verdict.
            </p>
          )}
          <HandoffForm data={data} onSaved={saved} />
          {!data.reports.length && (
            <p className={uiStyles.note}>
              No verification reports yet. Finish an execution and review its change, or record why
              no change artifact is available.
            </p>
          )}
          {data.reports.map((r) => (
            <ReportCard
              key={r.id}
              report={r}
              current={current(r)}
              decision={data.decisions.find((d) => d.reportId === r.id)}
            />
          ))}
          {data.handoffs.map((h) => (
            <article
              key={h.id}
              className={`${uiStyles.cardPadded} space-y-2`}
              data-testid="maintenance-handoff"
            >
              <h3 className="font-medium">Recorded handoff</h3>
              <p>
                <Timestamp value={h.createdAt} />
              </p>
              <p className="whitespace-pre-wrap">{h.reason}</p>
              <p className="whitespace-pre-wrap">Next action: {h.nextStep}</p>
              <p className="break-all font-mono text-ui-caption">
                Revision: {h.snapshot.revision} · run: {h.snapshot.runId ?? 'none recorded'}
              </p>
              <p>
                Recorded state: {h.snapshot.recovery.taskState} · attempt{' '}
                {h.snapshot.recovery.attempt}
              </p>
              <p className={uiStyles.sectionDescription}>
                Refresh task controls before continuing. This records the state at handoff time.
              </p>
            </article>
          ))}
          {(data.hasMoreReports || data.hasMoreDecisions || data.hasMoreHandoffs) && (
            <p className={uiStyles.note}>
              Showing up to 20 reports, 100 verdicts and 20 handoffs. Older reports can be retrieved
              by their report reference through the maintenance report API.
            </p>
          )}
        </>
      )}
    </section>
  )
}
