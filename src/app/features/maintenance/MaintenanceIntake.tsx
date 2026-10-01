import { useEffect, useRef, useState, type FormEvent } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { useNavigationStore } from '@app/entities/navigation'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
import { waitingPlaceDisplayName } from '@app/entities/navigation/agent-group'
import { MaintenanceError, submitMaintenanceRequest } from '@app/entities/maintenance'
import type { MaintenanceRequestInput, MaintenanceSubmission } from '@shared/types/maintenance'
import { useAuth } from '@app/shared/model/auth.context'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { maintenanceGuidance } from './guidance'

export function MaintenanceIntake() {
  const { user, isAuthenticated, isLoading } = useAuth()
  const selectedOrgId = useNavigationStore((s) => s.selectedOrgId)
  const projectId = useNavigationStore((s) => s.selectedProjectId)
  const groups = useNavigationStore((s) => s.agentGroups)
  const groupId = useBoardStore((s) => s.selectedGroupId)
  const group = groups.find((g) => g.id === groupId && g.projectId === projectId)
  if (isLoading || !isAuthenticated || !user?.isAdmin) return null
  // Remount before painting across account, organization or destination changes.
  return (
    <IntakeForm
      key={`${user.id}:${user.orgId ?? ''}:${selectedOrgId ?? ''}:${projectId ?? ''}:${group?.id ?? ''}`}
      groupId={group?.id ?? null}
      groupName={group ? waitingPlaceDisplayName(group.name) : null}
    />
  )
}

function IntakeForm({ groupId, groupName }: { groupId: string | null; groupName: string | null }) {
  const navigate = useNavigate()
  const [open, setOpen] = useState(false)
  const [kind, setKind] = useState<'request' | 'pull_request'>('request')
  const [reference, setReference] = useState('')
  const [number, setNumber] = useState('')
  const [title, setTitle] = useState('')
  const [brief, setBrief] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [result, setResult] = useState<MaintenanceSubmission | null>(null)
  const [pending, setPending] = useState(false)
  const active = useRef<AbortController | null>(null)
  useEffect(() => () => active.current?.abort(), [])

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (active.current || result) return
    setError(null)
    const normalizedReference = reference.trim().toLowerCase()
    const prNumber = Number(number)
    const validationError = !groupId
      ? 'Choose a project and place for new tasks before submitting.'
      : !title.trim() || new TextEncoder().encode(title.trim()).length > 500
        ? 'Add a title up to 500 UTF-8 bytes. Shorten it if needed; accented letters and other scripts may use more than one byte.'
        : !brief.trim() || new TextEncoder().encode(brief.trim()).length > 16_000
          ? 'Add a brief up to 16,000 UTF-8 bytes. Describe the result, where to work and how to check it.'
          : kind === 'request' && !/^[a-z0-9][a-z0-9._:-]{0,127}$/.test(normalizedReference)
            ? 'Use a stable reference of up to 128 ASCII letters, digits, dots, underscores, colons or hyphens. Start with a letter or digit.'
            : kind === 'pull_request' &&
                (!/^[1-9]\d*$/.test(number) ||
                  !Number.isSafeInteger(prNumber) ||
                  prNumber > 2_147_483_647)
              ? 'Enter a positive PR number up to 2147483647, without a URL or decimal places.'
              : null
    if (validationError || !groupId) {
      setError(validationError)
      return
    }
    const input: MaintenanceRequestInput = {
      groupId,
      title: title.trim(),
      brief: brief.trim(),
      source:
        kind === 'request' ? { kind, reference: normalizedReference } : { kind, number: prNumber },
    }
    const controller = new AbortController()
    active.current = controller
    setPending(true)
    try {
      const saved = await submitMaintenanceRequest(input, controller.signal)
      if (!controller.signal.aborted) setResult(saved)
    } catch (failure) {
      if (!controller.signal.aborted)
        setError(
          maintenanceGuidance[failure instanceof MaintenanceError ? failure.reason : 'failed']
        )
    } finally {
      if (!controller.signal.aborted) setPending(false)
      if (active.current === controller) active.current = null
    }
  }

  return (
    <section
      className="shrink-0 border-b border-black/[0.08] px-4 py-3 dark:border-white/[0.1]"
      data-testid="maintenance-intake"
    >
      <button
        type="button"
        className={uiStyles.secondaryButton}
        aria-expanded={open}
        aria-controls="maintenance-intake-form"
        onClick={() => setOpen(!open)}
      >
        {open ? 'Hide maintenance request' : 'Maintenance request'}
      </button>
      {open && (
        <div id="maintenance-intake-form" className="mt-3 max-h-[65vh] overflow-y-auto">
          <h2 className={uiStyles.sectionTitle}>Submit maintenance work</h2>
          <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
            A Forge administrator must connect the approved repository in{' '}
            <a className="text-apple-blue underline" href="/settings/maintenance-repository">
              Maintenance repository settings
            </a>{' '}
            first. Choose a project and a place for new tasks before submitting.
          </p>
          <p className="mt-2 text-ui-body">
            Place for new tasks:{' '}
            <strong>{groupName ?? 'Choose a project and place on the task board'}</strong>
          </p>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Submitting saves one unassigned task to wait. Open the task to review and assign it when
            ready. The same source always opens the original task, retaining its original brief and
            destination, even after completion.
          </p>
          {!groupId && (
            <p className={uiStyles.note}>
              Open{' '}
              <a className="text-apple-blue underline" href="/settings/projects">
                project settings
              </a>{' '}
              to set up a project, then{' '}
              <a className="text-apple-blue underline" href="/agents">
                Agents
              </a>{' '}
              to set up a place for new tasks.
            </p>
          )}
          {result ? (
            <div role="status" className="mt-3 space-y-2">
              <p>
                {result.reused
                  ? 'This source already has a task. Its original brief and destination were kept.'
                  : 'Maintenance task saved to wait. Review its brief before assigning an agent.'}
              </p>
              <button
                type="button"
                className={uiStyles.primaryButton}
                onClick={() =>
                  void navigate({ to: '/tasks/$taskId', params: { taskId: result.taskId } })
                }
              >
                Open maintenance task
              </button>
              <button
                type="button"
                className={uiStyles.secondaryButton}
                onClick={() => {
                  setResult(null)
                  setReference('')
                  setNumber('')
                  setTitle('')
                  setBrief('')
                }}
              >
                Submit another source
              </button>
            </div>
          ) : (
            <form onSubmit={(event) => void submit(event)} className="mt-3 space-y-3">
              <fieldset disabled={pending} className="space-y-3">
                <div>
                  <label htmlFor="maintenance-source-kind" className="block text-ui-body">
                    Source type
                  </label>
                  <select
                    id="maintenance-source-kind"
                    value={kind}
                    onChange={(e) => setKind(e.target.value as typeof kind)}
                    className={`${uiStyles.select} mt-1 w-full`}
                  >
                    <option value="request">Request reference</option>
                    <option value="pull_request">GitHub PR number</option>
                  </select>
                </div>
                {kind === 'request' ? (
                  <div>
                    <label htmlFor="maintenance-reference" className="block text-ui-body">
                      Stable request reference
                    </label>
                    <input
                      id="maintenance-reference"
                      aria-describedby="maintenance-reference-help"
                      required
                      value={reference}
                      onChange={(e) => setReference(e.target.value)}
                      maxLength={128}
                      placeholder="dependency-2026-10"
                      className={`${uiStyles.input} mt-1 w-full`}
                    />
                    <span id="maintenance-reference-help" className="mt-1 block text-ui-caption">
                      Keep this reference when retrying. Letters are treated as lowercase. Use a
                      different reference for new work.
                    </span>
                  </div>
                ) : (
                  <div>
                    <label htmlFor="maintenance-pr-number" className="block text-ui-body">
                      GitHub PR number
                    </label>
                    <input
                      id="maintenance-pr-number"
                      aria-describedby="maintenance-pr-help"
                      required
                      inputMode="numeric"
                      value={number}
                      onChange={(e) => setNumber(e.target.value)}
                      placeholder="42"
                      className={`${uiStyles.input} mt-1 w-full`}
                    />
                    <span id="maintenance-pr-help" className="mt-1 block text-ui-caption">
                      Use an open PR in the approved repository targeting its default branch.
                    </span>
                  </div>
                )}
                <label className="block text-ui-body">
                  Maintenance title
                  <input
                    required
                    value={title}
                    onChange={(e) => setTitle(e.target.value)}
                    maxLength={500}
                    className={`${uiStyles.input} mt-1 w-full`}
                  />
                </label>
                <label className="block text-ui-body">
                  Maintenance brief
                  <textarea
                    required
                    value={brief}
                    onChange={(e) => setBrief(e.target.value)}
                    maxLength={16_000}
                    rows={4}
                    placeholder="What should change, where to work, and how to check the result"
                    className={`${uiStyles.input} mt-1 min-h-28 w-full resize-y py-2`}
                  />
                </label>
              </fieldset>
              {error && (
                <p role="alert" aria-live="polite" className={uiStyles.error}>
                  {error}
                </p>
              )}
              <button
                type="submit"
                disabled={!groupId || pending}
                aria-busy={pending}
                className={uiStyles.primaryButton}
              >
                {pending ? 'Saving maintenance task…' : 'Save maintenance task to wait'}
              </button>
              {pending && (
                <p role="status" className="text-ui-caption">
                  This takes up to 30 seconds. Keep the source reference if the outcome cannot be
                  confirmed.
                </p>
              )}
            </form>
          )}
        </div>
      )}
    </section>
  )
}
