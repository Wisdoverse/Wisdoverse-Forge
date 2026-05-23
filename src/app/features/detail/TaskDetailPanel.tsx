import { useEffect, useState } from 'react'
import { CheckCircle2, RotateCcw, Send, X } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
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
import { SkillDraftModal } from './SkillDraftModal'

type TabId = 'description' | 'result' | 'context' | 'history'

const BASE_TABS: { id: TabId; label: string }[] = [
  { id: 'description', label: 'Work' },
  { id: 'context', label: 'Context' },
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
  const baseTabs = contextVisible ? BASE_TABS : BASE_TABS.filter((tab) => tab.id !== 'context')
  const tabs = hasResult
    ? [baseTabs[0], { id: 'result' as TabId, label: 'Result' }, ...baseTabs.slice(1)]
    : baseTabs
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
        if (!cancelled)
          setPreviewError(err instanceof Error ? err.message : 'Failed to load agents')
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
      setPreviewError(err instanceof Error ? err.message : 'Failed to load context preview')
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
      setPreviewError(err instanceof Error ? err.message : 'Failed to publish task')
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
      setRecoveryError(err instanceof Error ? err.message : 'Failed to update task')
    } finally {
      setRecoveryAction(null)
    }
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between mb-1">
        <span className="text-[10px] font-mono text-secondary-light dark:text-secondary-dark">
          {task.id.slice(0, 8)}
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
      {task.state === 'failed' && task.error && (
        <div className="mt-2 px-3 py-2 rounded-lg bg-apple-red/10 text-apple-red text-xs">
          {task.error}
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
            {resultArtifacts.map((artifact, i) => (
              <div key={i} className="rounded-lg bg-apple-gray-6 dark:bg-white/[0.04] p-3">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-xs font-medium text-foreground-light dark:text-foreground-dark">
                    {artifact.name}
                  </span>
                  <span className="text-[10px] text-secondary-light dark:text-secondary-dark">
                    {artifact.mimeType}
                  </span>
                </div>
                <pre className="text-xs text-foreground-light dark:text-foreground-dark whitespace-pre-wrap break-words font-mono leading-relaxed max-h-[300px] overflow-y-auto">
                  {artifact.data}
                </pre>
              </div>
            ))}
          </div>
        )}
        {activeTab === 'history' && <HistoryTab task={task} />}
      </div>

      {/* Action buttons */}
      {canAssign && (
        <div className="space-y-2 pt-3 border-t border-black/[0.04] dark:border-white/[0.04]">
          <label className="block text-[10px] font-medium text-secondary-light dark:text-secondary-dark">
            Assign with context
          </label>
          <div className="flex gap-2">
            <select
              value={selectedAgentId}
              onChange={(event) => setSelectedAgentId(event.target.value)}
              className="min-w-0 flex-1 rounded-lg bg-apple-gray-6 px-2 py-1.5 text-xs outline-none dark:bg-white/[0.06]"
            >
              <option value="">No available agent</option>
              {participants.map((participant) => (
                <option key={participant.agentId} value={participant.agentId}>
                  {participant.name}
                </option>
              ))}
            </select>
            <button
              type="button"
              onClick={() => void openContextPreview(selectedAgentId)}
              disabled={!selectedAgentId}
              className={cn(
                'flex h-8 w-8 shrink-0 items-center justify-center rounded-lg',
                'bg-apple-blue text-white transition-colors hover:bg-apple-blue/90',
                'disabled:cursor-not-allowed disabled:opacity-50'
              )}
              aria-label="Preview and publish task"
              title="Preview and publish task"
            >
              <Send size={14} strokeWidth={2} />
            </button>
          </div>
        </div>
      )}

      {(canRetry || canApprove) && (
        <div
          data-testid="task-recovery-actions"
          className="space-y-2 pt-3 border-t border-black/[0.04] dark:border-white/[0.04]"
        >
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
                <span>{recoveryAction === 'approve' ? 'Approving…' : 'Approve and continue'}</span>
              </button>
            )}
          </div>
        </div>
      )}

      {showActions && (
        <div className="flex gap-2 pt-3 border-t border-black/[0.04] dark:border-white/[0.04]">
          <button
            onClick={() => {
              orchestrationApi.updateTask(task.id, { state: 'blocked' }).catch(console.error)
            }}
            className={cn(
              'flex-1 text-xs font-medium py-1.5 rounded-button',
              'bg-apple-orange/10 text-apple-orange',
              'hover:bg-apple-orange/20 transition-colors'
            )}
          >
            Block
          </button>
          <button
            onClick={() => {
              orchestrationApi.cancelTask(task.id).catch(console.error)
            }}
            className={cn(
              'flex-1 text-xs font-medium py-1.5 rounded-button',
              'bg-apple-red/10 text-apple-red',
              'hover:bg-apple-red/20 transition-colors'
            )}
          >
            Cancel
          </button>
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
