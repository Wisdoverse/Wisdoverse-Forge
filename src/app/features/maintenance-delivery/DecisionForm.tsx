import { useId, useState, type FormEvent } from 'react'
import type {
  HumanMinutes,
  HumanVerdict,
  MaintenanceDelivery,
  VerificationReport,
} from '@shared/types/maintenance-delivery'
import { recordMaintenanceDecision } from '@app/entities/maintenance'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { Feedback, TextField } from './fields'
import { useSubmission } from './useSubmission'

const categories: (keyof HumanMinutes)[] = [
  'setup',
  'handling',
  'review',
  'recovery',
  'rework',
  'operation',
]
export function DecisionForm({
  data,
  report,
  onSaved,
}: {
  data: MaintenanceDelivery
  report: VerificationReport
  onSaved: () => void
}) {
  const verdictId = useId()
  const previous = data.decisions.find((d) => d.reportId === report.id)
  const [verdict, setVerdict] = useState<HumanVerdict>('rework'),
    [reason, setReason] = useState(''),
    [baseline, setBaseline] = useState(previous?.baselineMinutes?.toString() ?? '')
  const [minutes, setMinutes] = useState<Record<keyof HumanMinutes, string>>({
    setup: previous?.humanMinutes.setup?.toString() ?? '',
    handling: previous?.humanMinutes.handling?.toString() ?? '',
    review: previous?.humanMinutes.review?.toString() ?? '',
    recovery: previous?.humanMinutes.recovery?.toString() ?? '',
    rework: previous?.humanMinutes.rework?.toString() ?? '',
    operation: previous?.humanMinutes.operation?.toString() ?? '',
  })
  const submission = useSubmission(onSaved)
  const canAccept =
    data.currentChecks.status === 'observed' &&
    data.currentChecks.revision === data.currentRevision &&
    report.snapshot.revisionKind === 'produced' &&
    !!report.snapshot.runtime?.finishedAt
  function save(event: FormEvent) {
    event.preventDefault()
    const humanMinutes: HumanMinutes = {
      setup: null,
      handling: null,
      review: null,
      recovery: null,
      rework: null,
      operation: null,
    }
    for (const category of categories)
      humanMinutes[category] = minutes[category].trim() === '' ? null : Number(minutes[category])
    const input = {
      reportId: report.id,
      expectedVersion: data.taskVersion,
      expectedRevision: data.currentRevision,
      verdict,
      reason,
      humanMinutes,
      baselineMinutes: baseline.trim() === '' ? null : Number(baseline),
    }
    void submission.submit(input, (requestKey, signal) =>
      recordMaintenanceDecision(data.trace.taskId, { ...input, requestKey }, signal)
    )
  }
  return (
    <details className={uiStyles.cardPadded}>
      <summary className="cursor-pointer font-medium">Record human verdict and effort</summary>
      <p className={uiStyles.sectionDescription}>
        Review this report’s exact revision against its criteria. Acceptance records your judgement;
        merge still uses the repository’s separate checks and approval controls.
      </p>
      <form className="mt-4 space-y-3" onSubmit={save}>
        <fieldset disabled={submission.busy} className="space-y-3">
          <div className="block">
            <label htmlFor={verdictId} className={uiStyles.label}>
              Human verdict
            </label>
            <select
              id={verdictId}
              className={uiStyles.select}
              value={verdict}
              onChange={(e) => setVerdict(e.target.value as HumanVerdict)}
            >
              <option value="rework">Needs rework</option>
              <option value="rejected">Rejected</option>
              <option value="reopened">Reopened after review</option>
              <option value="accepted" disabled={!canAccept}>
                Accepted
              </option>
            </select>
          </div>
          {!canAccept && (
            <p className={uiStyles.note}>
              Acceptance needs a produced revision, a finished run and a fresh matching GitHub head.
              Refresh evidence after those are available.
            </p>
          )}
          <TextField
            label="Verdict reason and quality findings"
            value={reason}
            onChange={setReason}
          />
          <p className={uiStyles.sectionDescription}>
            Minutes are cumulative totals for this task, including earlier attempts. Leave an
            unknown category blank; enter 0 only when no time was spent. Later verdicts replace
            earlier totals in the summary.
          </p>
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
            {categories.map((category) => (
              <label key={category}>
                <span className={`${uiStyles.label} capitalize`}>{category} minutes</span>
                <input
                  type="number"
                  min={0}
                  max={100000}
                  step={1}
                  className={uiStyles.input}
                  value={minutes[category]}
                  onChange={(e) => setMinutes((old) => ({ ...old, [category]: e.target.value }))}
                />
              </label>
            ))}
          </div>
          <label className="block">
            <span className={uiStyles.label}>
              Comparable baseline minutes (optional, total for equivalent human work)
            </span>
            <input
              type="number"
              min={0}
              max={600000}
              step={1}
              className={uiStyles.input}
              value={baseline}
              onChange={(e) => setBaseline(e.target.value)}
            />
          </label>
          <button type="submit" className={uiStyles.primaryButton}>
            {submission.busy ? 'Saving verdict…' : 'Save human verdict'}
          </button>
        </fieldset>
        <Feedback message={submission.message} />
      </form>
    </details>
  )
}
