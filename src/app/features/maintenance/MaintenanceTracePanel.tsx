import { useEffect, useState, type ReactNode } from 'react'
import type { MaintenanceObservation, MaintenanceTrace } from '@shared/types/maintenance'
import { getMaintenanceTrace, MaintenanceError } from '@app/entities/maintenance'
import { useAuth } from '@app/shared/model/auth.context'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { maintenanceGuidance } from './guidance'

export function MaintenanceTracePanel({ taskId }: { taskId: string }) {
  const { user, isAuthenticated, isLoading } = useAuth()
  if (isLoading || !isAuthenticated || !user?.isAdmin) return null
  return <TracePanel key={`${user.id}:${user.orgId ?? ''}:${taskId}`} taskId={taskId} />
}

type TraceState =
  | { status: 'loading' }
  | { status: 'error'; message: string }
  | { status: 'ready'; trace: MaintenanceTrace | null }
function TracePanel({ taskId }: { taskId: string }) {
  const [state, setState] = useState<TraceState>({ status: 'loading' })
  const [version, setVersion] = useState(0)
  useEffect(() => {
    const controller = new AbortController()
    setState({ status: 'loading' })
    void getMaintenanceTrace(taskId, controller.signal).then(
      (trace) => {
        if (!controller.signal.aborted) setState({ status: 'ready', trace })
      },
      (error: unknown) => {
        if (!controller.signal.aborted)
          setState({
            status: 'error',
            message:
              maintenanceGuidance[error instanceof MaintenanceError ? error.reason : 'failed'],
          })
      }
    )
    return () => controller.abort()
  }, [taskId, version])
  if (state.status === 'ready' && state.trace === null) return null
  const trace = state.status === 'ready' ? state.trace : null
  return (
    <section
      className="mt-6 space-y-3 break-words rounded-card border border-black/[0.08] p-4 dark:border-white/[0.1]"
      data-testid="maintenance-trace"
      aria-labelledby="maintenance-trace-title"
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 id="maintenance-trace-title" className={uiStyles.sectionTitle}>
          Maintenance source and result
        </h2>
        <button
          type="button"
          disabled={state.status === 'loading'}
          className={uiStyles.secondaryButton}
          onClick={() => {
            setState({ status: 'loading' })
            setVersion((n) => n + 1)
          }}
        >
          {state.status === 'loading' ? 'Checking source and result…' : 'Refresh source and result'}
        </button>
      </div>
      {state.status === 'loading' && (
        <p role="status" className="text-ui-body">
          Loading stored history and checking GitHub. This takes up to 30 seconds.
        </p>
      )}
      {state.status === 'error' && (
        <p role="alert" aria-live="polite" className={uiStyles.error}>
          {state.message}
        </p>
      )}
      {trace && (
        <>
          <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
            Stored versions record this task’s history. Current PR observations show what GitHub
            reported at the stated time. Execution state and PR state do not establish checks or
            human acceptance; review those separately before using a change.
          </p>
          <dl className="space-y-2 text-ui-body">
            <Row label="Approved repository">
              <a
                className="text-apple-blue underline"
                href={`https://github.com/${trace.repository}`}
                target="_blank"
                rel="noopener noreferrer"
              >
                {trace.repository}
              </a>
            </Row>
            <Row label="Source">
              {trace.source.kind === 'request' || !trace.source.url ? (
                `Request ${trace.source.reference}`
              ) : (
                <ExternalPr url={trace.source.url}>Source PR #{trace.source.reference}</ExternalPr>
              )}
            </Row>
            <Row label="Submitted">
              <Timestamp value={trace.createdAt} />
            </Row>
            <Row label="Request ID">
              <code className="break-all">{trace.requestId}</code>
            </Row>
            <Row label="Task state">{trace.taskState}</Row>
            <Row label="Execution attempt">{trace.attempt}</Row>
            <Row label="Default branch at submission">{trace.defaultBranch}</Row>
            <Row label="Starting version at submission">
              <Revision value={trace.startingSha} />
            </Row>
            {trace.source.submittedHeadSha && (
              <Row label="Source PR version at submission">
                <Revision value={trace.source.submittedHeadSha} />
              </Row>
            )}
            <Row label="Recorded rebuild base">
              <Revision value={trace.rebuildBaseSha} />
            </Row>
            <Row label="Recorded produced PR version">
              <Revision value={trace.recordedPrHeadSha} />
            </Row>
            <Row label="Produced change">
              {trace.producedPrUrl ? (
                <ExternalPr url={trace.producedPrUrl}>Open produced PR</ExternalPr>
              ) : (
                'No produced PR recorded'
              )}
            </Row>
          </dl>
          <h3 className={uiStyles.groupLabel}>Execution history</h3>
          {trace.executions.length === 0 ? (
            <p className="text-ui-body">
              No execution recorded. Review and assign the task when ready.
            </p>
          ) : (
            <ol className="space-y-3 text-ui-body">
              {trace.executions.map((run) => (
                <li
                  key={run.runId}
                  className="rounded-card bg-black/[0.025] p-3 dark:bg-white/[0.04]"
                >
                  <dl className="space-y-1">
                    <Row label="Run ID">
                      <code className="break-all">{run.runId}</code>
                    </Row>
                    <Row label="Agent ID">
                      <code className="break-all">{run.agentId}</code>
                    </Row>
                    <Row label="Execution state">{run.state}</Row>
                    <Row label="Started">
                      <Timestamp value={run.startedAt} />
                    </Row>
                    <Row label="Finished">
                      {run.finishedAt ? <Timestamp value={run.finishedAt} /> : 'No finish recorded'}
                    </Row>
                  </dl>
                </li>
              ))}
            </ol>
          )}
          <Observation title="Current source PR" value={trace.sourceState} />
          <Observation title="Current produced PR" value={trace.producedState} />
        </>
      )}
    </section>
  )
}

function Observation({ title, value }: { title: string; value: MaintenanceObservation }) {
  return (
    <div className="space-y-2 text-ui-body">
      <h3 className={uiStyles.groupLabel}>{title}</h3>
      {value.checkedAt && (
        <p>
          Checked <Timestamp value={value.checkedAt} />
        </p>
      )}
      {value.status === 'unavailable' && (
        <p role="status" className={uiStyles.note}>
          GitHub state is unavailable. Stored history is still shown. Check repository access or
          refresh later; the PR’s current state is unconfirmed.
        </p>
      )}
      {value.status === 'not_applicable' && (
        <p>This source is a request reference and has no source PR.</p>
      )}
      {value.status === 'not_created' && <p>No produced PR recorded yet.</p>}
      {value.snapshot && (
        <dl className="space-y-1">
          <Row label="Observed PR">
            <ExternalPr url={value.snapshot.url}>PR #{value.snapshot.number}</ExternalPr>
          </Row>
          <Row label="Observed state">{value.snapshot.state}</Row>
          <Row label="Observed target branch">{value.snapshot.baseBranch}</Row>
          <Row label="Observed current version">
            <Revision value={value.snapshot.headSha} />
          </Row>
        </dl>
      )}
      {value.headChanged && (
        <p role="status" className={uiStyles.note}>
          The PR version changed since the recorded version. Review the current revision and its
          checks before accepting this change.
        </p>
      )}
    </div>
  )
}
function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid min-w-0 gap-1 sm:grid-cols-[180px_minmax(0,1fr)]">
      <dt className="text-secondary-light dark:text-secondary-dark">{label}</dt>
      <dd className="min-w-0">{children}</dd>
    </div>
  )
}
function Revision({ value }: { value: string | null }) {
  return value ? <code className="break-all">{value}</code> : <>Not recorded</>
}
function Timestamp({ value }: { value: string }) {
  return <time dateTime={value}>{new Date(value).toLocaleString()}</time>
}
function ExternalPr({ url, children }: { url: string; children: ReactNode }) {
  return (
    <a
      className="break-all text-apple-blue underline"
      href={url}
      target="_blank"
      rel="noopener noreferrer"
    >
      {children}
    </a>
  )
}
