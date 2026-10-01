import { useState, type FormEvent } from 'react'
import type { MaintenanceDelivery } from '@shared/types/maintenance-delivery'
import { recordMaintenanceHandoff } from '@app/entities/maintenance'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { Feedback, TextField } from './fields'
import { useSubmission } from './useSubmission'

export function HandoffForm({ data, onSaved }: { data: MaintenanceDelivery; onSaved: () => void }) {
  const [reason, setReason] = useState(''),
    [nextStep, setNextStep] = useState('')
  const submission = useSubmission(onSaved)
  const active = ['working', 'queued'].includes(data.recovery.taskState)
  function save(event: FormEvent) {
    event.preventDefault()
    const input = {
      expectedVersion: data.taskVersion,
      expectedRevision: data.currentRevision,
      reason,
      nextStep,
    }
    void submission.submit(input, (requestKey, signal) =>
      recordMaintenanceHandoff(data.trace.taskId, { ...input, requestKey }, signal)
    )
  }
  return (
    <details className={uiStyles.cardPadded}>
      <summary className="cursor-pointer font-medium">
        Record handoff for a person to continue
      </summary>
      <p className={uiStyles.sectionDescription}>
        Save the blocker and next action with the last recorded revision, run, reports and recovery
        state. This creates a handoff record. Use task controls separately to stop, retry or assign
        work. Container CLI session resumption depends on that CLI.
      </p>
      {active && (
        <p className={`${uiStyles.note} mt-3`}>
          Work is queued or active. Stop it using task controls, then refresh evidence before
          recording a handoff.
        </p>
      )}
      <form className="mt-4 space-y-3" onSubmit={save}>
        <fieldset disabled={submission.busy || active} className="space-y-3">
          <TextField label="Handoff blocker or reason" value={reason} onChange={setReason} />
          <TextField
            label="Next action for the person continuing"
            value={nextStep}
            onChange={setNextStep}
          />
          <button type="submit" className={uiStyles.primaryButton}>
            {submission.busy ? 'Saving handoff…' : 'Save handoff'}
          </button>
        </fieldset>
        <Feedback message={submission.message} />
      </form>
    </details>
  )
}
