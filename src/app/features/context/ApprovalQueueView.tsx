import { useCallback, useEffect, useMemo, useState, type FormEvent, type ReactNode } from 'react'
import {
  AlertTriangle,
  CheckCircle2,
  Clock3,
  FileText,
  Loader2,
  RefreshCw,
  ShieldCheck,
  XCircle,
  Zap,
} from 'lucide-react'
import { orchestrationApi } from '@app/shared/api/orchestration'
import { cn } from '@app/shared/lib/utils'
import { useWebSocket } from '@app/shared/model/websocket.context'
import { useContextStore } from '@app/shared/model/context.store'
import { approvalQueueErrorMessage } from './approvalQueueErrorMessages'
import type {
  ContextCandidateKind,
  ContextCandidateState,
  ContextCandidateSummary,
  ContextScopeKind,
  ContextSensitivity,
} from '@shared/types/context'

type StateFilter = 'pending' | 'all'
type KindFilter = ContextCandidateKind | 'all'
type ScopeFilter = ContextScopeKind | 'all'
type DecisionMode = 'approve' | 'reject'

interface ApprovalFormState {
  scopeKind: ContextScopeKind
  scopeId: string
  ttlLocal: string
  sensitivity: ContextSensitivity
  reason: string
  redacted: boolean
  userAttested: boolean
  confirmExpansion: boolean
}

interface ActiveDecision {
  mode: DecisionMode
  candidate: ContextCandidateSummary
}

interface WsContextCandidateMessage {
  type: string
  candidateId?: string
}

const STATE_FILTERS: Array<{ value: StateFilter; label: string }> = [
  { value: 'pending', label: 'Waiting for review' },
  { value: 'all', label: 'All saved items' },
]

const KIND_FILTERS: Array<{ value: KindFilter; label: string }> = [
  { value: 'all', label: 'All items' },
  { value: 'memory', label: 'Saved notes' },
  { value: 'skill', label: 'Saved instructions' },
]

const SCOPE_FILTERS: Array<{ value: ScopeFilter; label: string }> = [
  { value: 'all', label: 'All reuse options' },
  { value: 'user', label: 'Only me' },
  { value: 'team', label: 'My team' },
  { value: 'project', label: 'This project' },
]

const SENSITIVITIES: Array<{ value: ContextSensitivity; label: string }> = [
  { value: 'public', label: 'Safe to share' },
  { value: 'internal', label: 'Team internal' },
  { value: 'confidential', label: 'Confidential' },
  { value: 'secret_detected', label: 'May contain secrets' },
]

const APPROVAL_PATH_STEPS = [
  'Open the item and check what agents would save.',
  'Save it only if it will help future work, then choose who can reuse it.',
  'Do not save it if it is outdated, unsafe, duplicated, or unclear.',
]

const APPROVE_CHECKLIST = [
  'The original task is complete and this saved item still helps.',
  'Only the right people can reuse it.',
  'Sensitive details are hidden before saving.',
]

const REJECT_CHECKLIST = [
  'Do not save it when the item is wrong, duplicated, unsafe, or too narrow to help later.',
  'Add a short reason so the next reviewer knows what happened.',
]

const SOURCE_MISSING_LABEL = 'Task details need to load'
const SOURCE_MISSING_DETAIL = 'Save unlocks after the original task details load.'
const SOURCE_MISSING_NEXT_STEP =
  'Refresh saved items, then save after the original task details load.'

interface ApprovalQueueEmptyState {
  title: string
  detail: string
  nextStep: string
  actionLabel?: string
}

function approvalQueueEmptyState({
  stateFilter,
  kindFilter,
  scopeFilter,
}: {
  stateFilter: StateFilter
  kindFilter: KindFilter
  scopeFilter: ScopeFilter
}): ApprovalQueueEmptyState {
  if (kindFilter !== 'all' || scopeFilter !== 'all') {
    return {
      title: 'Filters are hiding saved items',
      detail:
        'This view only shows the selected item type and reuse option. Clear filters before assuming there is nothing to check.',
      nextStep: 'Next: review everything first, then narrow the list again only if it is long.',
      actionLabel: 'Clear filters',
    }
  }

  if (stateFilter === 'all') {
    return {
      title: 'Review the first saved item to start history',
      detail:
        'Saved and not-saved notes or instructions appear here after someone reviews the first suggestion.',
      nextStep:
        'Next: switch back to Waiting for review when you only want items waiting for a decision.',
      actionLabel: 'Show pending reviews',
    }
  }

  return {
    title: 'No saved items need review',
    detail:
      'When an agent suggests a saved note or saved instruction, it will appear here before anyone can reuse it.',
    nextStep: 'Next: finish a task, then come back here if you want agents to reuse what worked.',
  }
}

export function ApprovalQueueView() {
  const { subscribe } = useWebSocket()
  const pendingCandidateCount = useContextStore((s) => s.pendingCandidateCount)
  const setPendingCandidateCount = useContextStore((s) => s.setPendingCandidateCount)
  const decrementPendingCandidateCount = useContextStore((s) => s.decrementPendingCandidateCount)
  const [stateFilter, setStateFilter] = useState<StateFilter>('pending')
  const [kindFilter, setKindFilter] = useState<KindFilter>('all')
  const [scopeFilter, setScopeFilter] = useState<ScopeFilter>('all')
  const [candidates, setCandidates] = useState<ContextCandidateSummary[]>([])
  const [loading, setLoading] = useState(false)
  const [decisionLoading, setDecisionLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [activeDecision, setActiveDecision] = useState<ActiveDecision | null>(null)

  const loadCandidates = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const [queue, pending] = await Promise.all([
        orchestrationApi.listContextCandidates({
          state: stateFilter,
          itemKind: kindFilter,
          scopeKind: scopeFilter,
          limit: 100,
        }),
        orchestrationApi.listContextCandidates({ state: 'pending', limit: 200 }),
      ])
      setCandidates(queue)
      setPendingCandidateCount(pending.length)
    } catch (err) {
      setError(approvalQueueErrorMessage('loadQueue', err))
    } finally {
      setLoading(false)
    }
  }, [kindFilter, scopeFilter, setPendingCandidateCount, stateFilter])

  useEffect(() => {
    void loadCandidates()
  }, [loadCandidates])

  useEffect(
    () =>
      subscribe((data) => {
        if (isContextCandidateMessage(data)) {
          void loadCandidates()
        }
      }),
    [loadCandidates, subscribe]
  )

  const totalLabel = useMemo(() => {
    const suffix = pendingCandidateCount === 1 ? 'pending item' : 'pending items'
    return `${pendingCandidateCount} ${suffix}`
  }, [pendingCandidateCount])

  const emptyState = useMemo(
    () => approvalQueueEmptyState({ stateFilter, kindFilter, scopeFilter }),
    [kindFilter, scopeFilter, stateFilter]
  )

  const resetFilters = useCallback(() => {
    setStateFilter('pending')
    setKindFilter('all')
    setScopeFilter('all')
  }, [])

  const handleDecisionComplete = useCallback(
    (candidateId: string, state: Extract<ContextCandidateState, 'approved' | 'rejected'>) => {
      setCandidates((items) =>
        stateFilter === 'pending'
          ? items.filter((candidate) => candidate.id !== candidateId)
          : items.map((candidate) =>
              candidate.id === candidateId ? { ...candidate, state } : candidate
            )
      )
      decrementPendingCandidateCount()
    },
    [decrementPendingCandidateCount, stateFilter]
  )

  async function submitApprove(candidate: ContextCandidateSummary, form: ApprovalFormState) {
    if (!candidate.source_available) return
    setDecisionLoading(true)
    setError(null)
    try {
      await orchestrationApi.approveContextCandidate(candidate.id, {
        scope_kind: form.scopeKind,
        scope_id: form.scopeKind === 'user' ? null : form.scopeId.trim(),
        ttl_at: toIsoDateTime(form.ttlLocal),
        sensitivity: form.sensitivity,
        reason: form.reason.trim() || null,
        redacted: form.redacted,
        user_attested: form.userAttested,
        confirm_expansion: form.scopeKind === 'user' ? false : form.confirmExpansion,
      })
      handleDecisionComplete(candidate.id, 'approved')
      setActiveDecision(null)
    } catch (err) {
      setError(approvalQueueErrorMessage('approveCandidate', err))
    } finally {
      setDecisionLoading(false)
    }
  }

  async function submitReject(candidate: ContextCandidateSummary, reason: string) {
    setDecisionLoading(true)
    setError(null)
    try {
      await orchestrationApi.rejectContextCandidate(candidate.id, {
        reason: reason.trim() || null,
      })
      handleDecisionComplete(candidate.id, 'rejected')
      setActiveDecision(null)
    } catch (err) {
      setError(approvalQueueErrorMessage('rejectCandidate', err))
    } finally {
      setDecisionLoading(false)
    }
  }

  return (
    <div
      data-testid="context-approval-page"
      className="min-h-full bg-transparent text-foreground-light dark:text-foreground-dark"
    >
      <div className="mx-auto flex min-h-full w-full max-w-7xl flex-col gap-4 p-3 sm:p-4 lg:p-5">
        <section className="flex flex-col gap-3 border-b border-black/[0.06] pb-4 dark:border-white/[0.06] lg:flex-row lg:items-end lg:justify-between">
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-ui-caption font-semibold text-apple-blue">
              <ShieldCheck size={14} strokeWidth={2} aria-hidden="true" />
              <span>Saved item review</span>
            </div>
            <h1 className="mt-1 text-ui-title font-semibold">Review what agents can save</h1>
            <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
              {totalLabel}
            </p>
          </div>
          <button
            type="button"
            onClick={() => void loadCandidates()}
            className="inline-flex h-9 items-center justify-center gap-2 rounded-full border border-black/[0.08] bg-white px-3 text-ui-button font-medium text-foreground-light transition-colors hover:bg-black/[0.03] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.08]"
            title="Refresh review list"
          >
            <RefreshCw
              size={15}
              strokeWidth={2}
              className={cn(loading && 'animate-spin')}
              aria-hidden="true"
            />
            <span>Refresh</span>
          </button>
        </section>

        <section
          data-testid="context-approval-path"
          className="rounded-card border border-black/[0.08] bg-white p-3 dark:border-white/[0.1] dark:bg-white/[0.04]"
        >
          <div className="grid gap-3 md:grid-cols-[minmax(0,0.9fr)_minmax(0,1.4fr)] md:items-start">
            <div>
              <p className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
                Review steps
              </p>
              <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                Decide which saved notes or instructions are useful and safe to save for future
                work.
              </p>
            </div>
            <ol className="list-decimal space-y-1 pl-4 text-ui-caption text-secondary-light dark:text-secondary-dark">
              {APPROVAL_PATH_STEPS.map((step) => (
                <li key={step}>{step}</li>
              ))}
            </ol>
          </div>
        </section>

        <section className="grid gap-3 md:grid-cols-[minmax(0,1fr)_auto_auto]">
          <SegmentedFilter
            label="Status"
            value={stateFilter}
            options={STATE_FILTERS}
            onChange={(value) => setStateFilter(value)}
          />
          <SelectFilter
            label="Item type"
            value={kindFilter}
            options={KIND_FILTERS}
            onChange={(value) => setKindFilter(value)}
          />
          <SelectFilter
            label="Sharing range"
            value={scopeFilter}
            options={SCOPE_FILTERS}
            onChange={(value) => setScopeFilter(value)}
          />
        </section>

        {error && (
          <div
            data-testid="context-approval-error"
            className="flex items-start gap-2 rounded-card border border-apple-red/20 bg-apple-red/10 px-3 py-2 text-ui-body text-apple-red"
          >
            <AlertTriangle
              size={16}
              strokeWidth={2}
              className="mt-0.5 flex-shrink-0"
              aria-hidden="true"
            />
            <span>{error}</span>
          </div>
        )}

        <section className="min-h-[360px]">
          {loading && candidates.length === 0 ? (
            <div className="flex h-64 items-center justify-center gap-2 text-ui-body text-secondary-light dark:text-secondary-dark">
              <Loader2 size={18} strokeWidth={2} className="animate-spin" aria-hidden="true" />
              <span>Checking saved items...</span>
            </div>
          ) : candidates.length === 0 ? (
            <div
              data-testid="context-approval-empty"
              className="flex h-64 flex-col items-center justify-center rounded-card border border-dashed border-black/[0.12] px-4 text-center dark:border-white/[0.12]"
            >
              <CheckCircle2
                size={24}
                strokeWidth={2}
                className="text-apple-blue"
                aria-hidden="true"
              />
              <p className="mt-2 text-ui-section font-medium">{emptyState.title}</p>
              <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                {emptyState.detail}
              </p>
              <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                {emptyState.nextStep}
              </p>
              {emptyState.actionLabel && (
                <button
                  type="button"
                  onClick={resetFilters}
                  className="mt-3 inline-flex h-9 items-center justify-center rounded-full bg-apple-blue px-3 text-ui-button font-semibold text-white transition-colors hover:bg-apple-blue/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
                >
                  {emptyState.actionLabel}
                </button>
              )}
            </div>
          ) : (
            <div className="grid gap-3">
              {candidates.map((candidate) => (
                <CandidateRow
                  key={candidate.id}
                  candidate={candidate}
                  onApprove={() => setActiveDecision({ mode: 'approve', candidate })}
                  onReject={() => setActiveDecision({ mode: 'reject', candidate })}
                />
              ))}
            </div>
          )}
        </section>
      </div>

      {activeDecision && (
        <DecisionPanel
          activeDecision={activeDecision}
          loading={decisionLoading}
          onClose={() => setActiveDecision(null)}
          onApprove={submitApprove}
          onReject={submitReject}
        />
      )}
    </div>
  )
}

function CandidateRow({
  candidate,
  onApprove,
  onReject,
}: {
  candidate: ContextCandidateSummary
  onApprove: () => void
  onReject: () => void
}) {
  const pending = candidate.state === 'pending'
  const title = candidateTitle(candidate)
  const preview = candidatePreview(candidate)
  const unavailable = pending && !candidate.source_available
  const Icon = candidate.item_kind === 'skill' ? Zap : FileText
  const itemKindLabel = contextItemKindLabel(candidate.item_kind)
  const reuseLabel = reuseRangeLabel(candidate.proposed_scope_kind)

  return (
    <article
      data-testid={`context-candidate-${candidate.id}`}
      className="grid gap-3 rounded-card border border-black/[0.08] bg-white/80 p-3 dark:border-white/[0.08] dark:bg-white/[0.04] md:grid-cols-[minmax(0,1fr)_auto]"
    >
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2">
          <span
            className={cn(
              'inline-flex h-6 items-center gap-1.5 rounded-full px-2 text-ui-caption font-semibold',
              candidate.item_kind === 'skill'
                ? 'bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark'
                : 'bg-apple-blue/10 text-apple-blue'
            )}
          >
            <Icon size={13} strokeWidth={2} aria-hidden="true" />
            {itemKindLabel}
          </span>
          <StatusPill state={candidate.state} />
          <span className="inline-flex h-6 items-center rounded-full bg-black/[0.05] px-2 text-ui-caption font-medium text-secondary-light dark:bg-white/[0.08] dark:text-secondary-dark">
            {reuseLabel}
          </span>
          {unavailable && (
            <span
              data-testid={`context-source-unavailable-${candidate.id}`}
              className="inline-flex h-6 items-center gap-1 rounded-full bg-apple-red/10 px-2 text-ui-caption font-semibold text-apple-red"
            >
              <AlertTriangle size={12} strokeWidth={2} aria-hidden="true" />
              {SOURCE_MISSING_LABEL}
            </span>
          )}
        </div>
        <h2 className="mt-2 truncate text-ui-section font-semibold">{title}</h2>
        {preview && (
          <p className="mt-1 line-clamp-2 text-ui-body leading-5 text-secondary-light dark:text-secondary-dark">
            {preview}
          </p>
        )}
        <div className="mt-3 flex flex-wrap items-center gap-3 text-ui-caption text-secondary-light dark:text-secondary-dark">
          <span>Who can reuse it: {reuseRangeLabel(candidate.proposed_scope_kind)}</span>
          {candidate.source_run_id && (
            <span>
              {candidate.source_available
                ? 'Original task preview available'
                : SOURCE_MISSING_DETAIL}
            </span>
          )}
          <span>Created {formatTimestamp(candidate.created_at)}</span>
        </div>
      </div>

      <div className="flex items-center gap-2 md:justify-end">
        {pending ? (
          <>
            <button
              type="button"
              data-testid={`context-approve-${candidate.id}`}
              onClick={onApprove}
              disabled={!candidate.source_available}
              className="inline-flex h-9 items-center justify-center gap-2 rounded-full bg-apple-blue px-3 text-ui-button font-semibold text-white transition-colors hover:bg-apple-blue-focus focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:cursor-not-allowed disabled:bg-black/20 disabled:text-black/45 dark:disabled:bg-white/10 dark:disabled:text-white/35"
              title={
                candidate.source_available
                  ? 'Save this item for future work'
                  : SOURCE_MISSING_NEXT_STEP
              }
            >
              <CheckCircle2 size={15} strokeWidth={2} aria-hidden="true" />
              <span>Save</span>
            </button>
            <button
              type="button"
              data-testid={`context-reject-${candidate.id}`}
              onClick={onReject}
              className="inline-flex h-9 items-center justify-center gap-2 rounded-full border border-black/[0.08] bg-white px-3 text-ui-button font-semibold text-foreground-light transition-colors hover:bg-black/[0.03] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.08]"
            >
              <XCircle size={15} strokeWidth={2} aria-hidden="true" />
              <span>Do not save</span>
            </button>
          </>
        ) : (
          <span className="inline-flex h-9 items-center rounded-full bg-black/[0.04] px-3 text-ui-body font-medium text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
            Decision recorded
          </span>
        )}
      </div>
    </article>
  )
}

function DecisionPanel({
  activeDecision,
  loading,
  onClose,
  onApprove,
  onReject,
}: {
  activeDecision: ActiveDecision
  loading: boolean
  onClose: () => void
  onApprove: (candidate: ContextCandidateSummary, form: ApprovalFormState) => Promise<void>
  onReject: (candidate: ContextCandidateSummary, reason: string) => Promise<void>
}) {
  const { candidate, mode } = activeDecision
  const [form, setForm] = useState<ApprovalFormState>(() => defaultApprovalForm(candidate))
  const [rejectReason, setRejectReason] = useState('')
  const approving = mode === 'approve'
  const title = candidateTitle(candidate)
  const requiresScopeId = form.scopeKind !== 'user'
  const canApprove =
    candidate.source_available &&
    (!requiresScopeId || (form.scopeId.trim().length > 0 && form.confirmExpansion))
  const approvalStatusId = `context-approval-status-${candidate.id}`
  const approvalStatus = !candidate.source_available
    ? SOURCE_MISSING_NEXT_STEP
    : !requiresScopeId
      ? 'Ready to save for your own account.'
      : !form.scopeId.trim()
        ? `Paste the ${scopeTargetReferenceLabel(form.scopeKind)} before saving.`
        : !form.confirmExpansion
          ? `Confirm ${reuseAudienceLabel(form.scopeKind)} can reuse this safely before saving.`
          : `Ready to save for ${reuseAudienceLabel(form.scopeKind)}.`

  function updateForm<K extends keyof ApprovalFormState>(key: K, value: ApprovalFormState[K]) {
    setForm((current) => ({ ...current, [key]: value }))
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (approving) {
      if (!canApprove) return
      await onApprove(candidate, form)
    } else {
      await onReject(candidate, rejectReason)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex justify-end bg-black/35 backdrop-blur-sm">
      <button
        type="button"
        aria-label="Close review panel"
        className="hidden flex-1 md:block"
        onClick={onClose}
      />
      <aside
        role="dialog"
        aria-modal="true"
        aria-label={approving ? `Save ${title}` : `Do not save ${title}`}
        className="flex h-full w-full max-w-md flex-col border-l border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-[#111417]"
      >
        <div className="border-b border-black/[0.06] px-4 py-4 dark:border-white/[0.06]">
          <div className="flex items-center justify-between gap-3">
            <div className="min-w-0">
              <p className="text-ui-caption font-semibold text-apple-blue">
                {approving ? 'Save for future work' : 'Do not save this'}
              </p>
              <h2 className="mt-1 truncate text-ui-title font-semibold">{title}</h2>
            </div>
            <button
              type="button"
              onClick={onClose}
              className="inline-flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-full text-secondary-light transition-colors hover:bg-black/[0.05] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:text-secondary-dark dark:hover:bg-white/[0.08]"
              title="Close"
            >
              <XCircle size={18} strokeWidth={2} aria-hidden="true" />
            </button>
          </div>
        </div>

        <form onSubmit={handleSubmit} className="flex min-h-0 flex-1 flex-col">
          <div className="flex-1 space-y-4 overflow-auto px-4 py-4">
            <CandidateMiniSummary candidate={candidate} />
            <DecisionChecklist approving={approving} />

            {approving ? (
              <>
                <div className="rounded-card bg-apple-blue/10 px-3 py-2 text-ui-body text-apple-blue">
                  Choose who can reuse it. Only me is the safest choice. My team or This project
                  shares it more broadly and needs the support reference from Settings.
                </div>

                {!candidate.source_available && (
                  <div className="flex items-start gap-2 rounded-card bg-apple-red/10 px-3 py-2 text-ui-body text-apple-red">
                    <AlertTriangle
                      size={16}
                      strokeWidth={2}
                      className="mt-0.5 flex-shrink-0"
                      aria-hidden="true"
                    />
                    <span>{SOURCE_MISSING_NEXT_STEP}</span>
                  </div>
                )}

                <Field label="Who can reuse it">
                  <select
                    data-testid="context-approval-scope-kind"
                    value={form.scopeKind}
                    onChange={(event) => {
                      const next = event.target.value as ContextScopeKind
                      setForm((current) => ({
                        ...current,
                        scopeKind: next,
                        scopeId: next === 'user' ? '' : current.scopeId,
                        confirmExpansion: next === 'user' ? false : current.confirmExpansion,
                      }))
                    }}
                    className={fieldClassName}
                  >
                    <option value="user">Only me</option>
                    <option value="team">My team</option>
                    <option value="project">This project</option>
                  </select>
                </Field>

                {requiresScopeId && (
                  <Field label={scopeTargetReferenceLabel(form.scopeKind)}>
                    <input
                      value={form.scopeId}
                      onChange={(event) => updateForm('scopeId', event.target.value)}
                      className={fieldClassName}
                      placeholder={`Paste the ${scopeTargetReferenceLabel(form.scopeKind)} from Settings…`}
                      name="scopeId"
                      autoComplete="off"
                      data-testid="context-approval-scope-id"
                    />
                  </Field>
                )}

                <Field label="Expiration (optional)">
                  <input
                    type="datetime-local"
                    value={form.ttlLocal}
                    onChange={(event) => updateForm('ttlLocal', event.target.value)}
                    className={fieldClassName}
                    name="ttl"
                  />
                </Field>

                <Field label="Sensitivity">
                  <select
                    value={form.sensitivity}
                    onChange={(event) =>
                      updateForm('sensitivity', event.target.value as ContextSensitivity)
                    }
                    className={fieldClassName}
                  >
                    {SENSITIVITIES.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </Field>

                <Field label="Note">
                  <textarea
                    value={form.reason}
                    onChange={(event) => updateForm('reason', event.target.value)}
                    className={cn(fieldClassName, 'min-h-20 resize-y py-2')}
                    placeholder="Why is this safe to reuse?"
                    name="approvalNote"
                  />
                </Field>

                <div className="space-y-2">
                  <Checkbox
                    checked={form.redacted}
                    onChange={(checked) => updateForm('redacted', checked)}
                    label="Hide sensitive content before saving"
                  />
                  <Checkbox
                    checked={form.userAttested}
                    onChange={(checked) => updateForm('userAttested', checked)}
                    label="I checked the sensitive content"
                  />
                  {requiresScopeId && (
                    <Checkbox
                      checked={form.confirmExpansion}
                      onChange={(checked) => updateForm('confirmExpansion', checked)}
                      label={`I checked ${reuseAudienceLabel(form.scopeKind)} can reuse this safely`}
                    />
                  )}
                </div>
              </>
            ) : (
              <Field label="Why not save it?">
                <textarea
                  value={rejectReason}
                  onChange={(event) => setRejectReason(event.target.value)}
                  className={cn(fieldClassName, 'min-h-32 resize-y py-2')}
                  placeholder="Why should this not be saved?"
                  name="rejectReason"
                  data-testid="context-reject-reason"
                />
              </Field>
            )}
          </div>

          <div className="flex flex-col gap-2 border-t border-black/[0.06] px-4 py-3 dark:border-white/[0.06]">
            {approving && (
              <p
                id={approvalStatusId}
                role="status"
                aria-live="polite"
                className="text-ui-caption text-secondary-light dark:text-secondary-dark"
              >
                {approvalStatus}
              </p>
            )}
            <div className="flex items-center justify-end gap-2">
              <button
                type="button"
                onClick={onClose}
                className="inline-flex h-9 items-center justify-center rounded-full px-3 text-ui-button font-medium text-secondary-light transition-colors hover:bg-black/[0.05] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:text-secondary-dark dark:hover:bg-white/[0.08]"
              >
                Review later
              </button>
              <button
                type="submit"
                disabled={loading || (approving && !canApprove)}
                aria-describedby={approving ? approvalStatusId : undefined}
                data-testid={approving ? 'context-approval-submit' : 'context-reject-submit'}
                className={cn(
                  'inline-flex h-9 items-center justify-center gap-2 rounded-full px-3 text-ui-button font-semibold text-white transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:cursor-not-allowed disabled:bg-black/20 disabled:text-black/45 dark:disabled:bg-white/10 dark:disabled:text-white/35',
                  approving
                    ? 'bg-apple-blue hover:bg-apple-blue-focus'
                    : 'bg-apple-red hover:bg-apple-red/90'
                )}
              >
                {loading ? (
                  <Loader2 size={15} strokeWidth={2} className="animate-spin" aria-hidden="true" />
                ) : null}
                <span>{approving ? 'Approve and save' : 'Do not save'}</span>
              </button>
            </div>
          </div>
        </form>
      </aside>
    </div>
  )
}

function CandidateMiniSummary({ candidate }: { candidate: ContextCandidateSummary }) {
  return (
    <div className="rounded-card bg-black/[0.04] p-3 text-ui-body dark:bg-white/[0.06]">
      <div className="flex flex-wrap items-center gap-2">
        <StatusPill state={candidate.state} />
        <span className="inline-flex items-center gap-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          <Clock3 size={13} strokeWidth={2} aria-hidden="true" />
          {formatTimestamp(candidate.created_at)}
        </span>
      </div>
      {candidatePreview(candidate) && (
        <p className="mt-2 line-clamp-3 text-secondary-light dark:text-secondary-dark">
          {candidatePreview(candidate)}
        </p>
      )}
    </div>
  )
}

function DecisionChecklist({ approving }: { approving: boolean }) {
  const items = approving ? APPROVE_CHECKLIST : REJECT_CHECKLIST
  return (
    <section
      data-testid="context-decision-checklist"
      className="rounded-card border border-black/[0.08] bg-white px-3 py-2.5 dark:border-white/[0.1] dark:bg-white/[0.04]"
    >
      <p className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
        {approving ? 'Save only when' : 'Do not save when'}
      </p>
      <ol className="mt-2 list-decimal space-y-1 pl-4 text-ui-caption text-secondary-light dark:text-secondary-dark">
        {items.map((item) => (
          <li key={item}>{item}</li>
        ))}
      </ol>
    </section>
  )
}

function SegmentedFilter<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string
  value: T
  options: Array<{ value: T; label: string }>
  onChange: (value: T) => void
}) {
  return (
    <div>
      <label className="mb-1 block text-ui-caption font-semibold text-secondary-light dark:text-secondary-dark">
        {label}
      </label>
      <div className="inline-flex rounded-full border border-black/[0.08] bg-white p-0.5 dark:border-white/[0.1] dark:bg-white/[0.06]">
        {options.map((option) => (
          <button
            key={option.value}
            type="button"
            onClick={() => onChange(option.value)}
            className={cn(
              'h-8 rounded-full px-3 text-ui-caption font-medium transition-transform active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
              option.value === value
                ? 'bg-apple-blue text-white'
                : 'text-secondary-light hover:text-foreground-light dark:text-secondary-dark dark:hover:text-foreground-dark'
            )}
          >
            {option.label}
          </button>
        ))}
      </div>
    </div>
  )
}

function SelectFilter<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string
  value: T
  options: Array<{ value: T; label: string }>
  onChange: (value: T) => void
}) {
  return (
    <label className="block">
      <span className="mb-1 block text-ui-caption font-semibold text-secondary-light dark:text-secondary-dark">
        {label}
      </span>
      <select
        value={value}
        onChange={(event) => onChange(event.target.value as T)}
        className="h-9 min-w-36 rounded-full border border-black/[0.08] bg-white px-3 text-ui-body text-foreground-light outline-none transition-colors focus:border-apple-blue focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  )
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1 block text-ui-caption font-semibold text-secondary-light dark:text-secondary-dark">
        {label}
      </span>
      {children}
    </label>
  )
}

function Checkbox({
  checked,
  onChange,
  label,
}: {
  checked: boolean
  onChange: (checked: boolean) => void
  label: string
}) {
  return (
    <label className="flex items-center gap-2 text-ui-body text-foreground-light dark:text-foreground-dark">
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        className="h-4 w-4 rounded border-black/[0.2] text-apple-blue focus:ring-apple-blue dark:border-white/[0.2]"
      />
      <span>{label}</span>
    </label>
  )
}

function StatusPill({ state }: { state: ContextCandidateState }) {
  const styles: Record<ContextCandidateState, string> = {
    pending: 'bg-apple-blue/10 text-apple-blue',
    approved: 'bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark',
    rejected: 'bg-apple-red/10 text-apple-red',
    superseded:
      'bg-black/[0.06] text-secondary-light dark:bg-white/[0.08] dark:text-secondary-dark',
  }
  const labels: Record<ContextCandidateState, string> = {
    pending: 'Waiting for review',
    approved: 'Saved',
    rejected: 'Not saved',
    superseded: 'Replaced',
  }
  return (
    <span
      className={cn(
        'inline-flex h-6 items-center rounded-full px-2 text-ui-caption font-semibold',
        styles[state]
      )}
    >
      {labels[state]}
    </span>
  )
}

function defaultApprovalForm(candidate: ContextCandidateSummary): ApprovalFormState {
  const proposed = candidate.proposed_scope_kind
  const scopeKind: ContextScopeKind =
    proposed === 'team' || proposed === 'project' || proposed === 'user' ? proposed : 'user'
  return {
    scopeKind,
    scopeId: '',
    ttlLocal: '',
    sensitivity: (previewString(candidate, 'sensitivity') as ContextSensitivity) || 'internal',
    reason: '',
    redacted: false,
    userAttested: false,
    confirmExpansion: false,
  }
}

function isContextCandidateMessage(data: unknown): data is WsContextCandidateMessage {
  if (!data || typeof data !== 'object') return false
  const type = (data as { type?: unknown }).type
  return (
    type === 'context_candidate.created' ||
    type === 'context_candidate.approved' ||
    type === 'context_candidate.rejected'
  )
}

function candidateTitle(candidate: ContextCandidateSummary): string {
  return (
    previewString(candidate, 'title') ||
    previewString(candidate, 'name') ||
    previewString(candidate, 'description') ||
    `${contextItemKindLabel(candidate.item_kind)} ready to review`
  )
}

function contextItemKindLabel(value: ContextCandidateKind): string {
  return value === 'skill' ? 'Saved instruction' : 'Saved note'
}

function reuseRangeLabel(value: ContextCandidateSummary['proposed_scope_kind']): string {
  if (value === 'org') return 'Team space'
  if (value === 'user') return 'Only me'
  if (value === 'team') return 'My team'
  if (value === 'project') return 'This project'
  return titleCase(value)
}

function reuseAudienceLabel(value: ContextCandidateSummary['proposed_scope_kind']): string {
  if (value === 'org') return 'everyone in the team space'
  if (value === 'user') return 'your own account'
  if (value === 'team') return 'your team'
  if (value === 'project') return 'this project'
  return `the selected ${value}`
}

function scopeTargetReferenceLabel(value: ContextScopeKind): string {
  return `${titleCase(value)} support reference`
}

function candidatePreview(candidate: ContextCandidateSummary): string {
  return (
    previewString(candidate, 'content_preview') ||
    previewString(candidate, 'description') ||
    previewString(candidate, 'trigger_pattern') ||
    ''
  )
}

function previewString(candidate: ContextCandidateSummary, key: string): string {
  const value = candidate.proposed_preview[key]
  return typeof value === 'string' ? value : ''
}

function titleCase(value: string): string {
  return value
    .split('_')
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ')
}

function formatTimestamp(value: string): string {
  const timestamp = Date.parse(value)
  if (!Number.isFinite(timestamp)) return value
  return new Intl.DateTimeFormat(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  }).format(timestamp)
}

function toIsoDateTime(value: string): string | null {
  if (!value) return null
  const timestamp = Date.parse(value)
  return Number.isFinite(timestamp) ? new Date(timestamp).toISOString() : null
}

const fieldClassName =
  'w-full rounded-card border border-black/[0.08] bg-white px-3 py-2 text-ui-body text-foreground-light outline-none transition-colors placeholder:text-secondary-light focus:border-apple-blue focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:placeholder:text-secondary-dark'
