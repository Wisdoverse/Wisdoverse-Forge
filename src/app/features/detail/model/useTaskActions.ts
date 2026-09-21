import { useEffect, useState } from 'react'
import { useContextFeaturesStore } from '@app/entities/context/model/context-features.store'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
import {
  orchestrationApi,
  type ParticipantSummary,
  type TaskSummary,
} from '@app/shared/api/orchestration'
import type { ContextPreviewResponse } from '@shared/types/context'
import { taskDetailErrorMessage } from '../taskDetailErrorMessages'

export function useTaskActions(task: TaskSummary) {
  const upsertTask = useBoardStore((state) => state.upsertTask)
  const canPublishWithContext = useContextFeaturesStore((s) => s.preview && s.injection)
  const [participants, setParticipants] = useState<ParticipantSummary[]>([])
  const [selectedAgentId, setSelectedAgentId] = useState('')
  const [previewOpen, setPreviewOpen] = useState(false)
  const [preview, setPreview] = useState<ContextPreviewResponse | null>(null)
  const [previewLoading, setPreviewLoading] = useState(false)
  const [previewError, setPreviewError] = useState<string | null>(null)
  const [publishing, setPublishing] = useState(false)
  const [recoveryAction, setRecoveryAction] = useState<'retry' | 'approve' | null>(null)
  const [recoveryError, setRecoveryError] = useState<string | null>(null)
  const [taskAction, setTaskAction] = useState<'cancel' | null>(null)
  const [taskActionError, setTaskActionError] = useState<string | null>(null)
  const [confirmCancelTask, setConfirmCancelTask] = useState(false)

  const showActions = task.state === 'queued'
  const canAssign =
    canPublishWithContext &&
    (task.state === 'backlog' ||
      task.state === 'queued' ||
      (task.state === 'blocked' && task.blockedReason === 'waiting_agent'))
  const canRetry =
    task.state === 'failed' ||
    task.state === 'canceled' ||
    (task.state === 'blocked' && task.blockedReason === 'waiting_verification')
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

  async function handleCancel() {
    setTaskAction('cancel')
    setTaskActionError(null)
    try {
      const response = await orchestrationApi.cancelTask(task.id)
      if (response.ok && response.task) upsertTask(response.task)
      setConfirmCancelTask(false)
    } catch (err) {
      setTaskActionError(taskDetailErrorMessage('cancelTask', err))
    } finally {
      setTaskAction(null)
    }
  }

  return {
    showActions,
    canAssign,
    canRetry,
    canApprove,
    participants,
    selectedAgentId,
    setSelectedAgentId,
    assign: openContextPreview,
    retry: () => handleRecovery('retry'),
    approve: () => handleRecovery('approve'),
    cancel: handleCancel,
    error: previewError ?? recoveryError ?? taskActionError,
    previewState: {
      isOpen: previewOpen,
      preview,
      loading: previewLoading,
      publishing,
      error: previewError,
      onClose: () => {
        if (!publishing) setPreviewOpen(false)
      },
      onConfirm: publishWithContext,
    },
    recoveryAction,
    recoveryError,
    taskAction,
    taskActionError,
    confirmCancelTask,
    setConfirmCancelTask,
  }
}
