import { useEffect, useRef, useState } from 'react'
import type {
  MaintenanceComparison,
  MaintenanceOutcomes,
  OutcomeQuery,
} from '@shared/types/maintenance-delivery'
import { compareMaintenanceReports, getMaintenanceOutcomes } from '@app/entities/maintenance'
import { useNavigationStore } from '@app/entities/navigation'
import { useAuth } from '@app/shared/model/auth.context'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { deliveryGuidance } from './guidance'
import { Feedback, Timestamp } from './fields'

export function MaintenanceOutcomesDashboard() {
  const { user, isAuthenticated, isLoading } = useAuth()
  const org = useNavigationStore((s) => s.selectedOrgId),
    projectId = useNavigationStore((s) => s.selectedProjectId)
  if (isLoading || !isAuthenticated || !user?.isAdmin) return null
  return (
    <Dashboard
      key={`${user.id}:${user.orgId ?? ''}:${org ?? ''}:${projectId ?? ''}`}
      projectId={projectId}
    />
  )
}
function Dashboard({ projectId }: { projectId: string | null }) {
  const [days, setDays] = useState(7),
    [refresh, setRefresh] = useState(0),
    [query, setQuery] = useState<OutcomeQuery>({ projectId })
  const [data, setData] = useState<MaintenanceOutcomes | null>(null),
    [busy, setBusy] = useState(true),
    [message, setMessage] = useState('')
  const [selected, setSelected] = useState<string[]>([]),
    [comparison, setComparison] = useState<MaintenanceComparison | null>(null),
    [comparing, setComparing] = useState(false),
    [compareMessage, setCompareMessage] = useState('')
  const comparingRequest = useRef<AbortController | null>(null)
  useEffect(() => () => comparingRequest.current?.abort(), [])
  useEffect(() => {
    const controller = new AbortController()
    setBusy(true)
    setMessage('')
    void getMaintenanceOutcomes(query, controller.signal).then(
      (next) => {
        if (!controller.signal.aborted) {
          setData((old) =>
            query.cursor && old ? { ...next, tasks: [...old.tasks, ...next.tasks] } : next
          )
          setBusy(false)
        }
      },
      (error) => {
        if (!controller.signal.aborted) {
          setMessage(deliveryGuidance(error))
          setBusy(false)
        }
      }
    )
    return () => controller.abort()
  }, [query, refresh])
  function reset(period: number) {
    comparingRequest.current?.abort()
    comparingRequest.current = null
    setComparing(false)
    setSelected([])
    setComparison(null)
    setCompareMessage('')
    setData(null)
    setBusy(true)
    const to = new Date(),
      from = new Date(to.getTime() - period * 86400000)
    setDays(period)
    setQuery({ projectId, from: from.toISOString(), to: to.toISOString() })
    setRefresh((v) => v + 1)
  }
  function choose(id: string) {
    comparingRequest.current?.abort()
    comparingRequest.current = null
    setComparing(false)
    setComparison(null)
    setCompareMessage('')
    setSelected((ids) => (ids.includes(id) ? ids.filter((v) => v !== id) : [...ids, id]))
  }
  async function compare() {
    if (comparingRequest.current) return
    const controller = new AbortController()
    comparingRequest.current = controller
    setComparing(true)
    setCompareMessage('')
    try {
      const result = await compareMaintenanceReports(selected, controller.signal)
      if (!controller.signal.aborted) setComparison(result)
    } catch (error) {
      if (!controller.signal.aborted) setCompareMessage(deliveryGuidance(error))
    } finally {
      if (!controller.signal.aborted) {
        comparingRequest.current = null
        setComparing(false)
      }
    }
  }
  const s = data?.summary
  const reduction =
    s?.baselineMinutes && s.pairedHumanMinutes !== null
      ? (((s.baselineMinutes - s.pairedHumanMinutes) / s.baselineMinutes) * 100).toFixed(1)
      : null
  return (
    <section
      className="min-w-0 space-y-4 p-4 sm:p-6"
      data-testid="maintenance-outcomes"
      aria-labelledby="maintenance-outcomes-title"
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 id="maintenance-outcomes-title" className={uiStyles.sectionTitle}>
            Maintenance outcomes and human effort
          </h2>
          <p className={uiStyles.sectionDescription}>
            {projectId ? 'Selected project' : 'All projects in this team space'} · reviewed changes,
            remaining work and recorded human time.
          </p>
        </div>
        <div className="flex gap-2">
          <label>
            <span className="sr-only">Outcome period</span>
            <select
              className={uiStyles.select}
              value={days}
              onChange={(e) => reset(Number(e.target.value))}
            >
              <option value={7}>Last 7 days</option>
              <option value={30}>Last 30 days</option>
              <option value={120}>Last 120 days</option>
            </select>
          </label>
          <button className={uiStyles.secondaryButton} disabled={busy} onClick={() => reset(days)}>
            Refresh outcomes
          </button>
        </div>
      </div>
      {busy && <p role="status">Loading maintenance outcomes…</p>}
      <Feedback message={message} />
      {data && s && (
        <>
          <p className={uiStyles.sectionDescription}>
            Submission cohort: <Timestamp value={data.from} /> to <Timestamp value={data.to} /> (end
            excluded). Observed <Timestamp value={data.observedAt} />. Verdicts apply to each task’s
            latest report and recorded revision; refresh task evidence to check the current GitHub
            head.
          </p>
          <dl className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5">
            {Object.entries({
              Submitted: s.submitted,
              Accepted: s.accepted,
              Rejected: s.rejected,
              'Needs rework': s.rework,
              Reopened: s.reopened,
              'Awaiting review': s.awaitingReview,
              'Stale verdicts': s.staleReviews,
              'Failed execution': s.failed,
              'Canceled execution': s.canceled,
            }).map(([label, value]) => (
              <div key={label} className={uiStyles.cardPadded}>
                <dt className={uiStyles.label}>{label}</dt>
                <dd className="text-ui-title font-semibold">{value}</dd>
              </div>
            ))}
          </dl>
          <div className={`${uiStyles.cardPadded} space-y-2`}>
            <h3 className="font-medium">Recorded human effort</h3>
            <p>
              {s.humanMinutes === null
                ? 'No complete totals recorded'
                : `${s.humanMinutes} minutes`}{' '}
              · {s.completeEffortTasks} / {s.submitted} tasks have all six cumulative categories
              recorded for a current report.
            </p>
            <p>
              Comparable baseline pairs: {s.pairedBaselineTasks} · recorded human minutes{' '}
              {s.pairedHumanMinutes ?? 'unknown'} · baseline minutes{' '}
              {s.baselineMinutes ?? 'unknown'}
              {reduction !== null && ` · recorded reduction ${reduction}%`}
            </p>
            <p className={uiStyles.sectionDescription}>
              Missing minutes stay unknown. Totals use the latest verdict once per task. Baseline
              pairs need equivalent work and task mix; this statistic alone does not establish time
              savings.
            </p>
          </div>
          <p className={uiStyles.note}>
            Review activity during this period, including tasks submitted earlier:{' '}
            {s.acceptedInPeriod} distinct tasks accepted; {s.reopenedInPeriod} distinct tasks
            reopened. A task can appear in both counts.
          </p>
          {!s.submitted && (
            <p className={uiStyles.note}>
              No maintenance requests in this submission period. Start a maintenance request from
              Tasks, then record its verification report and human verdict.
            </p>
          )}
          {data.tasks.length > 0 && (
            <>
              <div className="flex flex-wrap items-center justify-between gap-3">
                <h3 className="font-medium">Task records and Container CLI comparison</h3>
                <button
                  className={uiStyles.secondaryButton}
                  disabled={selected.length < 2 || comparing}
                  onClick={() => void compare()}
                >
                  {comparing
                    ? 'Comparing reports…'
                    : `Compare selected reports (${selected.length}/8)`}
                </button>
              </div>
              <p className={uiStyles.sectionDescription}>
                Select reports from equivalent tasks executed by at least two different Container
                CLIs. Recorded conditions, versions, verdicts and effort are shown together. Compare
                quality and interruption evidence before drawing conclusions.
              </p>
              <div className="overflow-x-auto">
                <table className={uiStyles.table}>
                  <thead className={uiStyles.tableHead}>
                    <tr>
                      {[
                        'Compare',
                        'Task',
                        'Execution',
                        'Human verdict',
                        'Human minutes',
                        'Elapsed minutes',
                      ].map((label) => (
                        <th key={label} className={uiStyles.tableHeaderCell} scope="col">
                          {label}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {data.tasks.map((task) => (
                      <tr key={task.taskId} className={uiStyles.row}>
                        <td className={uiStyles.tableCell}>
                          {task.report && (
                            <input
                              type="checkbox"
                              aria-label={`Compare report for ${task.title}`}
                              checked={selected.includes(task.report.id)}
                              disabled={!selected.includes(task.report.id) && selected.length >= 8}
                              onChange={() => {
                                if (task.report) choose(task.report.id)
                              }}
                            />
                          )}
                        </td>
                        <td className={uiStyles.tableCell}>
                          <a className="text-apple-blue underline" href={`/tasks/${task.taskId}`}>
                            {task.title}
                          </a>
                        </td>
                        <td className={uiStyles.tableCell}>{task.state}</td>
                        <td className={uiStyles.tableCell}>
                          {task.decision
                            ? `${task.decision.verdict}${task.reviewCurrent ? '' : ' (stale)'}`
                            : 'Awaiting review'}
                        </td>
                        <td className={uiStyles.tableCell}>
                          {task.decision?.totalMinutes ?? 'Incomplete'}
                        </td>
                        <td className={uiStyles.tableCell}>
                          {task.leadTimeMinutes ?? 'Not reviewed'}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <p className={uiStyles.sectionDescription}>
                Elapsed minutes run from task creation to its latest verdict and include waiting.
                They are separate from human effort.
              </p>
              {data.nextCursor && (
                <button
                  className={uiStyles.secondaryButton}
                  disabled={busy}
                  onClick={() =>
                    setQuery({ projectId, from: data.from, to: data.to, cursor: data.nextCursor })
                  }
                >
                  Load more task records
                </button>
              )}
            </>
          )}
          <Feedback message={compareMessage} />
          {comparison && (
            <div
              className={`${uiStyles.cardPadded} space-y-3`}
              data-testid="maintenance-comparison"
            >
              <h3 className="font-medium">
                {comparison.conditionsMatch
                  ? 'Recorded comparison conditions match'
                  : 'Comparison conditions need review'}
              </h3>
              <p>{comparison.distinctClis} distinct recorded Container CLIs</p>
              {comparison.reasons.length > 0 && (
                <ul className="list-inside list-disc">
                  {comparison.reasons.map((reason) => (
                    <li key={reason}>{reason}</li>
                  ))}
                </ul>
              )}
              {comparison.rows.map((row) => (
                <div
                  key={row.report.id}
                  className="space-y-1 border-t border-black/[0.08] pt-3 dark:border-white/[0.1]"
                >
                  <p>
                    {row.report.snapshot.runtime?.cliTool ?? 'CLI unavailable'} · reported version{' '}
                    {row.report.snapshot.cliVersion ?? 'unknown'} ·{' '}
                    {row.decision?.verdict ?? 'not reviewed'}
                  </p>
                  <p className="break-all font-mono text-ui-caption">
                    Revision {row.report.revision} · run {row.report.runId ?? 'none'}
                  </p>
                  <p>
                    Human effort {row.decision?.totalMinutes ?? 'incomplete'} minutes ·{' '}
                    {row.report.snapshot.unverified.length} listed unverified areas ·{' '}
                    {row.report.snapshot.evidence.length} captured evidence references
                  </p>
                  <p className="whitespace-pre-wrap">{row.report.snapshot.changeSummary}</p>
                </div>
              ))}
            </div>
          )}
        </>
      )}
    </section>
  )
}
