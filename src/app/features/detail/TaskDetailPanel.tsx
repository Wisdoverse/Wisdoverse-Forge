import { useEffect, useState } from 'react'
import { AlertTriangle, Bot, CheckCircle2, ListChecks, RotateCcw, Send, X } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { taskFailurePreview } from '@app/shared/lib/taskFailureCopy'
import { agentCapabilitySummary } from '@app/shared/lib/agentCapabilityCopy'
import {
  orchestrationApi,
  taskResultArtifacts,
  type ParticipantSummary,
  type TaskSummary,
} from '@app/shared/api/orchestration'
import { useBoardStore } from '@app/shared/model/board.store'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'
import { InjectionPreviewModal } from '@app/entities/context/ui/InjectionPreviewModal'
import type { ContextPreviewResponse } from '@shared/types/context'
import { TaskMetadata } from './TaskMetadata'
import { DescriptionTab } from './DescriptionTab'
import { ContextTab } from './ContextTab'
import { HistoryTab } from './HistoryTab'
import { ReviewSnapshotPanel } from './ReviewSnapshotPanel'
import { SkillDraftModal } from './SkillDraftModal'
import { taskDetailErrorMessage } from './taskDetailErrorMessages'

type TabId = 'description' | 'result' | 'context' | 'history' | 'review'

const BASE_TABS: { id: TabId; label: string }[] = [
  { id: 'description', label: 'Work' },
  { id: 'context', label: 'Saved items' },
  { id: 'history', label: 'Updates' },
]

interface TaskDetailPanelProps {
  task: TaskSummary
  onClose: () => void
}

export function TaskDetailPanel({ task, onClose }: TaskDetailPanelProps) {
  const upsertTask = useBoardStore((state) => state.upsertTask)
  const contextVisible = useContextFeaturesStore((s) => s.governance || s.injection)
  const canPublishWithContext = useContextFeaturesStore((s) => s.preview && s.injection)
  const resultArtifacts = taskResultArtifacts(task.result)
  const hasResult = resultArtifacts.length > 0
  const failurePreview =
    task.state === 'failed' && task.error ? taskFailurePreview(task.error) : null
  const baseTabs = contextVisible ? BASE_TABS : BASE_TABS.filter((tab) => tab.id !== 'context')
  const coreTabs = hasResult
    ? [baseTabs[0], { id: 'result' as TabId, label: 'Result' }, ...baseTabs.slice(1)]
    : baseTabs
  // Self-fix tasks expose a dedicated PR review/approve tab (plan D4 — NOT the
  // pre-dispatch waiting_approval button).
  const tabs = task.selfFix ? [...coreTabs, { id: 'review' as TabId, label: 'Review' }] : coreTabs
  const [activeTab, setActiveTab] = useState<TabId>('description')
  const [participants, setParticipants] = useState<ParticipantSummary[]>([])
  const [selectedAgentId, setSelectedAgentId] = useState('')
  const [previewOpen, setPreviewOpen] = useState(false)
  const [preview, setPreview] = useState<ContextPreviewResponse | null>(null)
  const [previewLoading, setPreviewLoading] = useState(false)
  const [previewError, setPreviewError] = useState<string | null>(null)
  const [publishing, setPublishing] = useState(false)
  const [skillDraftOpen, setSkillDraftOpen] = useState(false)
  const [recoveryAction, setRecoveryAction] = useState<'retry' | 'approve' | null>(null)
  const [recoveryError, setRecoveryError] = useState<string | null>(null)
  const [taskAction, setTaskAction] = useState<'block' | 'cancel' | null>(null)
  const [taskActionError, setTaskActionError] = useState<string | null>(null)
  const [confirmCancelTask, setConfirmCancelTask] = useState(false)

  useEffect(() => {
    if (!contextVisible && activeTab === 'context') setActiveTab('description')
  }, [activeTab, contextVisible])

  const showActions = task.state === 'working' || task.state === 'queued'
  const canAssign =
    canPublishWithContext &&
    (task.state === 'backlog' ||
      task.state === 'queued' ||
      (task.state === 'blocked' && task.blockedReason === 'waiting_agent'))
  const canRetry = task.state === 'failed' || task.state === 'canceled'
  const canApprove = task.state === 'blocked' && task.blockedReason === 'waiting_approval'
  const recoveryGuidance = taskRecoveryGuidance(canRetry, canApprove)
  const liveActionGuidance = taskLiveActionGuidance(task.state)

  useEffect(() => {
    if (!canAssign) return
    let cancelled = false
    orchestrationApi
      .getParticipants('available')
      .then((items) => {
        if (cancelled) return
        setParticipants(items)
        setSelectedAgentId((current) => current || items[0]?.agentId || '')
      })
      .catch((err) => {
        if (!cancelled) setPreviewError(taskDetailErrorMessage('loadAgents', err))
      })
    return () => {
      cancelled = true
    }
  }, [canAssign])

  async function openContextPreview(agentId: string) {
    if (!agentId) return
    setPreviewOpen(true)
    setPreview(null)
    setPreviewError(null)
    setPreviewLoading(true)
    try {
      setPreview(await orchestrationApi.previewContext(task.id, agentId))
    } catch (err) {
      setPreviewError(taskDetailErrorMessage('previewContext', err))
    } finally {
      setPreviewLoading(false)
    }
  }

  async function publishWithContext(selection: { pinnedIds: string[]; removedIds: string[] }) {
    if (!preview) return
    setPublishing(true)
    setPreviewError(null)
    try {
      const response = await orchestrationApi.publishWithContext(task.id, {
        contextPreviewId: preview.contextPreviewId,
        previewHash: preview.previewHash,
        pinnedIds: selection.pinnedIds,
        removedIds: selection.removedIds,
      })
      if (response.ok && response.task) upsertTask(response.task)
      setPreviewOpen(false)
    } catch (err) {
      setPreviewError(taskDetailErrorMessage('publishTask', err))
    } finally {
      setPublishing(false)
    }
  }

  async function handleRecovery(action: 'retry' | 'approve') {
    setRecoveryAction(action)
    setRecoveryError(null)
    try {
      const response =
        action === 'retry'
          ? await orchestrationApi.retryTask(task.id)
          : await orchestrationApi.approveTask(task.id)
      if (response.ok && response.task) upsertTask(response.task)
    } catch (err) {
      setRecoveryError(
        taskDetailErrorMessage(action === 'retry' ? 'retryTask' : 'approveTask', err)
      )
    } finally {
      setRecoveryAction(null)
    }
  }

  async function handleTaskAction(action: 'block' | 'cancel') {
    if (action === 'block') setConfirmCancelTask(false)
    setTaskAction(action)
    setTaskActionError(null)
    try {
      const response =
        action === 'block'
          ? await orchestrationApi.updateTask(task.id, { state: 'blocked' })
          : await orchestrationApi.cancelTask(task.id)
      if (response.ok && response.task) upsertTask(response.task)
      if (action === 'cancel') setConfirmCancelTask(false)
    } catch (err) {
      setTaskActionError(
        taskDetailErrorMessage(action === 'block' ? 'blockTask' : 'cancelTask', err)
      )
    } finally {
      setTaskAction(null)
    }
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between mb-1">
        <span className="text-[10px] text-secondary-light dark:text-secondary-dark">
          {taskReferenceLabel(task.id)}
        </span>
        <button
          data-testid="detail-close"
          onClick={onClose}
          className={cn(
            'w-8 h-8 flex items-center justify-center rounded-lg',
            'text-secondary-light dark:text-secondary-dark',
            'hover:bg-black/[0.06] dark:hover:bg-white/[0.06]',
            'transition-colors'
          )}
          aria-label="Close detail panel"
        >
          <X size={15} strokeWidth={2} />
        </button>
      </div>

      {/* Title */}
      <h2 className="text-sm font-semibold text-foreground-light dark:text-foreground-dark leading-snug">
        {task.params.task}
      </h2>

      {/* Metadata */}
      <div className="border-b border-black/[0.04] dark:border-white/[0.04]">
        <TaskMetadata task={task} />
      </div>

      {/* Tab bar */}
      <div className="flex gap-1 pt-3 pb-0 border-b border-black/[0.04] dark:border-white/[0.04]">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={cn(
              'text-xs px-3 py-1.5 rounded-t font-medium transition-colors',
              activeTab === tab.id
                ? 'text-apple-blue border-b-2 border-apple-blue -mb-px'
                : 'text-secondary-light dark:text-secondary-dark hover:text-foreground-light dark:hover:text-foreground-dark'
            )}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {/* Error banner for failed tasks */}
      {failurePreview && (
        <div
          data-testid="task-detail-failure-preview"
          className="mt-2 px-3 py-2 rounded-lg bg-apple-red/10 text-apple-red text-xs"
        >
          {failurePreview}
        </div>
      )}

      {/* Tab content */}
      <div className="flex-1 overflow-y-auto">
        {activeTab === 'description' && (
          <DescriptionTab
            task={task}
            onOpenResult={hasResult ? () => setActiveTab('result') : undefined}
            onOpenContext={contextVisible ? () => setActiveTab('context') : undefined}
            onDraftSkill={task.state === 'completed' ? () => setSkillDraftOpen(true) : undefined}
          />
        )}
        {contextVisible && activeTab === 'context' && <ContextTab taskId={task.id} />}
        {activeTab === 'result' && hasResult && (
          <div className="py-3 space-y-3">
            <ResultReviewGuide task={task} artifactCount={resultArtifacts.length} />
            {resultArtifacts.map((artifact, i) => (
              <div
                key={i}
                className="rounded-lg border border-black/[0.06] bg-white p-3 dark:border-white/[0.08] dark:bg-white/[0.04]"
              >
                <div className="flex items-center justify-between mb-2">
                  <span className="text-xs font-medium text-foreground-light dark:text-foreground-dark">
                    {artifact.name}
                  </span>
                  <span className="text-[10px] text-secondary-light dark:text-secondary-dark">
                    {resultFileKindLabel(artifact.mimeType)}
                  </span>
                </div>
                <p className="mb-2 text-[10px] leading-relaxed text-secondary-light dark:text-secondary-dark">
                  Use this result as evidence for the task outcome. If it does not answer the brief,
                  go back to Work and decide whether to retry, review saved notes and instructions,
                  or create a follow-up task.
                </p>
                <pre className="text-xs text-foreground-light dark:text-foreground-dark whitespace-pre-wrap break-words font-mono leading-relaxed max-h-[300px] overflow-y-auto">
                  {artifact.data}
                </pre>
              </div>
            ))}
          </div>
        )}
        {activeTab === 'history' && <HistoryTab task={task} />}
        {activeTab === 'review' && task.selfFix && <ReviewSnapshotPanel task={task} />}
      </div>

      {/* Action buttons */}
      {canAssign && (
        <div className="space-y-2 pt-3 border-t border-black/[0.04] dark:border-white/[0.04]">
          <div className="flex items-center justify-between gap-2">
            <span className="text-[10px] font-medium uppercase text-secondary-light dark:text-secondary-dark">
              Available agents
            </span>
            <span className="text-[10px] tabular-nums text-secondary-light dark:text-secondary-dark">
              {participants.length} ready
            </span>
          </div>
          {participants.length > 0 ? (
            <div className="grid gap-2">
              {participants.map((participant) => (
                <AgentChoice
                  key={participant.agentId}
                  participant={participant}
                  selected={participant.agentId === selectedAgentId}
                  onSelect={() => setSelectedAgentId(participant.agentId)}
                />
              ))}
            </div>
          ) : (
            <div className="rounded-lg border border-dashed border-black/[0.1] px-3 py-2 text-xs dark:border-white/[0.12]">
              <p className="font-medium text-foreground-light dark:text-foreground-dark">
                No agent can take this task right now
              </p>
              <p className="mt-1 leading-relaxed text-secondary-light dark:text-secondary-dark">
                Open Agents to start or connect an agent, then return here and refresh this task.
              </p>
            </div>
          )}
          <div className="flex justify-end">
            <button
              type="button"
              onClick={() => void openContextPreview(selectedAgentId)}
              disabled={!selectedAgentId}
              className={cn(
                'inline-flex h-8 items-center justify-center gap-1.5 rounded-lg px-3 text-xs font-medium',
                'bg-apple-blue text-white transition-colors hover:bg-apple-blue/90',
                'disabled:cursor-not-allowed disabled:opacity-50'
              )}
              aria-label={
                selectedAgentId
                  ? 'Preview and send task'
                  : 'Choose an available agent before sending'
              }
              title={
                selectedAgentId
                  ? 'Preview and send task'
                  : 'Choose an available agent before sending'
              }
            >
              <Send size={14} strokeWidth={2} />
              <span>Preview and send</span>
            </button>
          </div>
        </div>
      )}

      {(canRetry || canApprove) && (
        <div
          data-testid="task-recovery-actions"
          className="space-y-2 pt-3 border-t border-black/[0.04] dark:border-white/[0.04]"
        >
          {recoveryGuidance && (
            <div
              data-testid="task-recovery-guidance"
              className="rounded-lg bg-apple-blue/10 px-3 py-2 text-xs text-foreground-light dark:text-foreground-dark"
            >
              <p className="font-semibold">{recoveryGuidance.title}</p>
              <p className="mt-0.5 leading-relaxed text-secondary-light dark:text-secondary-dark">
                {recoveryGuidance.detail}
              </p>
            </div>
          )}
          {recoveryError && (
            <div className="rounded-lg bg-apple-red/10 px-3 py-2 text-xs text-apple-red">
              {recoveryError}
            </div>
          )}
          <div className="flex gap-2">
            {canRetry && (
              <button
                type="button"
                onClick={() => void handleRecovery('retry')}
                disabled={recoveryAction !== null}
                className={cn(
                  'inline-flex flex-1 items-center justify-center gap-1.5 rounded-button py-1.5 text-xs font-medium',
                  'bg-apple-blue/10 text-apple-blue transition-colors hover:bg-apple-blue/15',
                  'disabled:cursor-not-allowed disabled:opacity-50'
                )}
              >
                <RotateCcw size={13} strokeWidth={2.25} aria-hidden="true" />
                <span>{recoveryAction === 'retry' ? 'Retrying…' : 'Retry task'}</span>
              </button>
            )}
            {canApprove && (
              <button
                type="button"
                onClick={() => void handleRecovery('approve')}
                disabled={recoveryAction !== null}
                className={cn(
                  'inline-flex flex-1 items-center justify-center gap-1.5 rounded-button py-1.5 text-xs font-medium',
                  'bg-apple-green/10 text-apple-green transition-colors hover:bg-apple-green/15',
                  'disabled:cursor-not-allowed disabled:opacity-50'
                )}
              >
                <CheckCircle2 size={13} strokeWidth={2.25} aria-hidden="true" />
                <span>{recoveryAction === 'approve' ? 'Allowing…' : 'Allow and continue'}</span>
              </button>
            )}
          </div>
        </div>
      )}

      {showActions && (
        <div className="space-y-2 pt-3 border-t border-black/[0.04] dark:border-white/[0.04]">
          {liveActionGuidance && (
            <div
              data-testid="task-live-action-guidance"
              className="rounded-lg bg-apple-orange/10 px-3 py-2 text-xs text-foreground-light dark:text-foreground-dark"
            >
              <p className="font-semibold">{liveActionGuidance.title}</p>
              <p className="mt-0.5 leading-relaxed text-secondary-light dark:text-secondary-dark">
                {liveActionGuidance.detail}
              </p>
            </div>
          )}
          {taskActionError && (
            <div
              role="alert"
              className="rounded-lg bg-apple-red/10 px-3 py-2 text-xs text-apple-red"
            >
              {taskActionError}
            </div>
          )}
          {confirmCancelTask ? (
            <div className="space-y-2 rounded-lg border border-apple-red/20 bg-apple-red/10 px-3 py-2 text-xs text-apple-red">
              <div className="flex gap-2">
                <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
                <p>
                  Canceling stops the current agent work. Use Needs help instead if you only need to
                  pause for missing input.
                </p>
              </div>
              <div className="flex gap-2">
                <button
                  type="button"
                  onClick={() => void handleTaskAction('cancel')}
                  disabled={taskAction !== null}
                  className={cn(
                    'flex-1 rounded-button bg-apple-red px-2 py-1.5 font-medium text-white',
                    'transition-colors hover:bg-apple-red/90',
                    'disabled:cursor-not-allowed disabled:opacity-50'
                  )}
                >
                  {taskAction === 'cancel' ? 'Canceling…' : 'Cancel task'}
                </button>
                <button
                  type="button"
                  onClick={() => setConfirmCancelTask(false)}
                  disabled={taskAction !== null}
                  className={cn(
                    'flex-1 rounded-button bg-white/70 px-2 py-1.5 font-medium text-foreground-light',
                    'transition-colors hover:bg-white',
                    'disabled:cursor-not-allowed disabled:opacity-50 dark:bg-black/20 dark:text-foreground-dark dark:hover:bg-black/30'
                  )}
                >
                  Keep running
                </button>
              </div>
            </div>
          ) : (
            <div className="flex gap-2">
              <button
                type="button"
                onClick={() => void handleTaskAction('block')}
                disabled={taskAction !== null}
                className={cn(
                  'flex-1 text-xs font-medium py-1.5 rounded-button',
                  'bg-apple-orange/10 text-apple-orange',
                  'hover:bg-apple-orange/20 transition-colors',
                  'disabled:cursor-not-allowed disabled:opacity-50'
                )}
              >
                {taskAction === 'block' ? 'Marking…' : 'Needs help'}
              </button>
              <button
                type="button"
                onClick={() => setConfirmCancelTask(true)}
                disabled={taskAction !== null}
                className={cn(
                  'flex-1 text-xs font-medium py-1.5 rounded-button',
                  'bg-apple-red/10 text-apple-red',
                  'hover:bg-apple-red/20 transition-colors',
                  'disabled:cursor-not-allowed disabled:opacity-50'
                )}
              >
                Cancel
              </button>
            </div>
          )}
        </div>
      )}

      <InjectionPreviewModal
        isOpen={previewOpen}
        preview={preview}
        loading={previewLoading}
        publishing={publishing}
        error={previewError}
        onClose={() => {
          if (!publishing) setPreviewOpen(false)
        }}
        onConfirm={(selection) => void publishWithContext(selection)}
      />
      <SkillDraftModal
        open={skillDraftOpen}
        task={task}
        artifacts={resultArtifacts}
        onClose={() => setSkillDraftOpen(false)}
      />
    </div>
  )
}

function taskReferenceLabel(id: string): string {
  const trimmed = id.trim()
  if (!trimmed) return 'Refresh task details'
  return `Task reference ${trimmed.length > 8 ? trimmed.slice(0, 8) : trimmed}`
}

function resultFileKindLabel(mimeType: string): string {
  const normalized = mimeType.trim().toLowerCase()
  if (!normalized) return 'Result file'
  if (normalized.startsWith('text/') || normalized.includes('markdown')) return 'Text result'
  if (normalized.includes('json')) return 'Data result'
  if (normalized.startsWith('image/')) return 'Image result'
  if (normalized === 'application/pdf') return 'PDF result'
  return 'Result file'
}

function taskRecoveryGuidance(
  canRetry: boolean,
  canApprove: boolean
): { title: string; detail: string } | null {
  if (canRetry) {
    return {
      title: 'Try the task again when the request is still useful',
      detail:
        'Use Retry task after checking the brief. The task goes back to the queue so an agent can make another attempt.',
    }
  }
  if (canApprove) {
    return {
      title: 'Let the task continue when it has what it needs',
      detail:
        'Check the request first. Then choose Allow and continue to return the task to the queue for an agent to continue.',
    }
  }
  return null
}

function taskLiveActionGuidance(
  state: TaskSummary['state']
): { title: string; detail: string } | null {
  if (state === 'working') {
    return {
      title: 'Need to pause or stop this work?',
      detail:
        'Use Needs help when the agent needs your input. Use Cancel only when this task should stop.',
    }
  }
  if (state === 'queued') {
    return {
      title: 'Need to change this waiting task?',
      detail:
        'Use Needs help when something is missing. Use Cancel only if this task should not run.',
    }
  }
  return null
}

function ResultReviewGuide({ task, artifactCount }: { task: TaskSummary; artifactCount: number }) {
  const completed = task.state === 'completed'
  return (
    <section
      data-testid="task-result-review-guide"
      className="rounded-lg border border-black/[0.08] bg-white p-3 dark:border-white/[0.1] dark:bg-[#2a2a2c]"
    >
      <div className="mb-3 flex items-start gap-2">
        <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-apple-blue/10 text-apple-blue">
          <ListChecks size={15} strokeWidth={2.2} aria-hidden="true" />
        </span>
        <div className="min-w-0">
          <h3 className="text-xs font-semibold text-foreground-light dark:text-foreground-dark">
            Review the result before closing
          </h3>
          <p className="mt-1 text-[11px] leading-relaxed text-secondary-light dark:text-secondary-dark">
            {completed
              ? 'The task is marked done. Confirm the result matches the brief before you rely on it.'
              : 'A result is attached, but the task is not fully done yet. Check it before deciding the next step.'}
          </p>
        </div>
      </div>
      <div className="grid">
        <ResultReviewStep
          number="1"
          title="Compare with the brief"
          detail={`Expected work: ${task.params.task}`}
        />
        <ResultReviewStep
          number="2"
          title="Check the evidence"
          detail={`${artifactCount} result file${artifactCount === 1 ? '' : 's'} attached for review.`}
        />
        <ResultReviewStep
          number="3"
          title="Choose the next action"
          detail={
            completed
              ? 'Accept the result, save repeatable steps, or create a follow-up task if something is missing.'
              : 'Use Work or Updates to decide whether the agent needs more input.'
          }
        />
      </div>
    </section>
  )
}

function ResultReviewStep({
  number,
  title,
  detail,
}: {
  number: string
  title: string
  detail: string
}) {
  return (
    <div className="flex gap-2 border-t border-black/[0.06] py-2 first:border-t-0 first:pt-0 last:pb-0 dark:border-white/[0.08]">
      <span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-black/[0.04] text-[10px] font-semibold text-apple-blue dark:bg-white/[0.06]">
        {number}
      </span>
      <div className="min-w-0">
        <p className="text-[11px] font-semibold text-foreground-light dark:text-foreground-dark">
          {title}
        </p>
        <p className="mt-0.5 text-[10px] leading-relaxed text-secondary-light dark:text-secondary-dark">
          {detail}
        </p>
      </div>
    </div>
  )
}

function AgentChoice({
  participant,
  selected,
  onSelect,
}: {
  participant: ParticipantSummary
  selected: boolean
  onSelect: () => void
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={selected}
      className={cn(
        'flex min-w-0 items-center gap-2 rounded-lg border px-2.5 py-2 text-left transition-colors',
        selected
          ? 'border-apple-blue/35 bg-apple-blue/10'
          : 'border-black/[0.08] bg-black/[0.02] hover:border-apple-blue/25 dark:border-white/[0.1] dark:bg-white/[0.035]'
      )}
    >
      <span
        className={cn(
          'flex h-8 w-8 shrink-0 items-center justify-center rounded-lg',
          selected
            ? 'bg-apple-blue text-white'
            : 'bg-white text-secondary-light dark:bg-white/[0.07] dark:text-secondary-dark'
        )}
      >
        <Bot size={15} strokeWidth={2.25} aria-hidden="true" />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-xs font-semibold text-foreground-light dark:text-foreground-dark">
          {participant.name}
        </span>
        <span className="block truncate text-[10px] text-secondary-light dark:text-secondary-dark">
          {participant.capabilities.length > 0
            ? agentCapabilitySummary(participant.capabilities)
            : 'Ready to take this task'}
        </span>
      </span>
      <span
        className={cn(
          'shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium',
          selected
            ? 'bg-apple-blue text-white'
            : 'bg-apple-green/10 text-apple-green dark:bg-apple-green/15'
        )}
      >
        {selected ? 'Selected' : 'Ready'}
      </span>
    </button>
  )
}
