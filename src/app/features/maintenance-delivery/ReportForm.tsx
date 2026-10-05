import { useId, useState, type FormEvent } from 'react'
import type { MaintenanceDelivery, ReportedCheck } from '@shared/types/maintenance-delivery'
import { createVerification } from '@app/entities/maintenance'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { Feedback, TextField } from './fields'
import { useSubmission } from './useSubmission'

export function ReportForm({ data, onSaved }: { data: MaintenanceDelivery; onSaved: () => void }) {
  const checkId = useId()
  const [scope, setScope] = useState(''),
    [criteria, setCriteria] = useState(''),
    [environment, setEnvironment] = useState(''),
    [summary, setSummary] = useState(''),
    [unverified, setUnverified] = useState(''),
    [missing, setMissing] = useState(''),
    [cliVersion, setCliVersion] = useState(''),
    [comparison, setComparison] = useState('')
  const [checks, setChecks] = useState<ReportedCheck[]>([])
  const submission = useSubmission(onSaved)
  function change(index: number, patch: Partial<ReportedCheck>) {
    setChecks((items) => items.map((item, i) => (i === index ? { ...item, ...patch } : item)))
  }
  function save(event: FormEvent) {
    event.preventDefault()
    const input = {
      expectedVersion: data.taskVersion,
      expectedRevision: data.currentRevision,
      runId: data.latestRunId,
      scope,
      criteria,
      environmentNotes: environment,
      changeSummary: summary,
      checks,
      unverified: unverified
        .split('\n')
        .map((s) => s.trim())
        .filter(Boolean),
      noArtifactReason: missing.trim() || null,
      cliVersion: cliVersion.trim() || null,
      comparisonKey: comparison.trim() || null,
    }
    void submission.submit(input, (requestKey, signal) =>
      createVerification(data.trace.taskId, { ...input, requestKey }, signal)
    )
  }
  return (
    <details className={uiStyles.cardPadded}>
      <summary className="cursor-pointer font-medium">Create verification report</summary>
      <p className={uiStyles.sectionDescription}>
        Review the task brief and exact change first. A run must be finished before recording its
        evidence. These entries are your reported results; GitHub observations and human verdicts
        stay separate. Do not include credentials.
      </p>
      <form onSubmit={save} className="mt-4 space-y-3">
        <fieldset disabled={submission.busy} className="space-y-3">
          <TextField label="Allowed scope" value={scope} onChange={setScope} />
          <TextField label="Acceptance criteria" value={criteria} onChange={setCriteria} />
          <TextField
            label="Environment and constraints"
            value={environment}
            onChange={setEnvironment}
          />
          <TextField
            label="Changes and known failures"
            value={summary}
            onChange={setSummary}
            max={8000}
          />
          <TextField
            label="Unverified areas (one per line, up to 20)"
            value={unverified}
            onChange={setUnverified}
            required={false}
            max={20000}
          />
          {!data.trace.recordedPrHeadSha && (
            <TextField
              label="Why no change artifact is available"
              value={missing}
              onChange={setMissing}
              max={2000}
            />
          )}
          <div className="space-y-3">
            <h3 className="font-medium">Reported checks</h3>
            <p className={uiStyles.sectionDescription}>
              Record what was run, its result and evidence. Saving a command here does not execute
              it.
            </p>
            {checks.map((check, index) => (
              <fieldset
                key={index}
                className="space-y-2 rounded-card border border-black/[0.08] p-3 dark:border-white/[0.1]"
              >
                <legend>Check {index + 1}</legend>
                <TextField
                  label={`Check ${index + 1} name`}
                  value={check.name}
                  onChange={(name) => change(index, { name })}
                  max={256}
                />
                <TextField
                  label={`Check ${index + 1} command`}
                  value={check.command}
                  onChange={(command) => change(index, { command })}
                  max={1000}
                />
                <div className="block">
                  <label htmlFor={`${checkId}-${index}`} className={uiStyles.label}>
                    Result
                  </label>
                  <select
                    id={`${checkId}-${index}`}
                    className={uiStyles.select}
                    value={check.status}
                    onChange={(e) =>
                      change(index, { status: e.target.value as ReportedCheck['status'] })
                    }
                  >
                    <option value="not_run">Not run</option>
                    <option value="passed">Passed</option>
                    <option value="failed">Failed</option>
                  </select>
                </div>
                <TextField
                  label={`Check ${index + 1} evidence or omission reason`}
                  value={check.evidence}
                  onChange={(evidence) => change(index, { evidence })}
                  max={2000}
                />
                <button
                  type="button"
                  className={uiStyles.subtleButton}
                  onClick={() => setChecks((items) => items.filter((_, i) => i !== index))}
                >
                  Remove check {index + 1}
                </button>
              </fieldset>
            ))}
            <button
              type="button"
              className={uiStyles.secondaryButton}
              disabled={checks.length >= 20}
              onClick={() =>
                setChecks((items) => [
                  ...items,
                  { name: '', command: '', status: 'not_run', evidence: '' },
                ])
              }
            >
              Add reported check
            </button>
          </div>
          <details>
            <summary className="cursor-pointer">Optional Container CLI comparison</summary>
            <div className="mt-3 space-y-3">
              <TextField
                label="Reported Container CLI version"
                value={cliVersion}
                onChange={setCliVersion}
                required={false}
                max={256}
              />
              <TextField
                label="Comparison reference"
                value={comparison}
                onChange={setComparison}
                required={false}
                max={64}
              >
                Use the same reference, brief, criteria, scope and environment for equivalent tasks.
                Runtime and image evidence are captured from the recorded run.
              </TextField>
            </div>
          </details>
          <button className={uiStyles.primaryButton} type="submit">
            {submission.busy ? 'Saving report…' : 'Save verification report'}
          </button>
        </fieldset>
        <Feedback message={submission.message} />
      </form>
    </details>
  )
}
