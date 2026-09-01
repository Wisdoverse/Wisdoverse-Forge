import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  AlertTriangle,
  Bot,
  CheckCircle2,
  CircleDot,
  Clock3,
  Flag,
  MessageSquarePlus,
  Send,
  Trash2,
  XCircle,
  type LucideIcon,
} from 'lucide-react'
import { formatRelativeTime } from '@app/shared/lib/time'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import {
  CONTEXT_OVERFLOW_FAILURE_GUIDE,
  isContextOverflowFailure,
  taskAttemptNote,
  taskBlockedPreview,
  taskFailurePreview,
} from '@app/shared/lib/taskFailureCopy'
import { BeginnerLoadingState } from '@app/shared/ui/BeginnerLoadingState'
import { taskStateLabel } from '@app/entities/task'
import {
  orchestrationApi,
  taskResultArtifacts,
  type TaskComment,
  type TaskCommentKind,
  type TaskRunImageEvidence,
  type TaskRunSummary,
  type TaskSummary,
} from '@app/shared/api/orchestration'
import { taskDetailErrorMessage } from './taskDetailErrorMessages'
import { TASK_AGENT_NAME_LOADING_LABEL } from './model/taskAgentLabels'

interface HistoryTabProps {
  task: TaskSummary
}

export function HistoryTab({ task }: HistoryTabProps) {
  const events = taskHistoryEvents(task)
  const [runs, setRuns] = useState<TaskRunSummary[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError(null)
    orchestrationApi
      .getTaskRuns(task.id)
      .then((items) => {
        if (!cancelled) setRuns(items)
      })
      .catch((err) => {
        if (!cancelled) setError(taskDetailErrorMessage('loadRuns', err))
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [task.id])

  return (
    <div className="py-3" data-testid="task-updates">
      <div className="space-y-3">
        <AgentCheckIn task={task} />

        <section
          data-testid="task-updates-guide"
          className="rounded-card border border-apple-blue/15 bg-apple-blue/[0.055] px-3 py-2.5 dark:border-apple-blue/25 dark:bg-apple-blue/[0.09]"
        >
          <p className="text-ui-caption font-medium text-apple-blue">What to check now</p>
          <p className="mt-1 text-ui-caption leading-relaxed text-secondary-light dark:text-secondary-dark">
            {taskUpdateGuide(task)}
          </p>
        </section>

        <HumanUpdates task={task} />

        <section className="space-y-2">
          <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
            Task story
          </p>
          {events.map((event) => (
            <div
              key={event.id}
              className="flex gap-2 rounded-card bg-apple-gray-6/70 px-3 py-2 dark:bg-white/[0.035]"
            >
              <span
                className="mt-1 h-2 w-2 shrink-0 rounded-full bg-apple-blue"
                aria-hidden="true"
              />
              <div className="min-w-0">
                <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
                  {event.title}
                </p>
                <p className="mt-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
                  {event.detail}
                </p>
              </div>
            </div>
          ))}
        </section>

        <section className="space-y-2">
          <div className="flex items-center justify-between gap-2">
            <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
              Agent work history
            </p>
          </div>
          {loading && (
            <BeginnerLoadingState
              compact
              framed={false}
              title="Checking agent work history"
              detail="Forge is checking whether an agent has started work on this task."
              nextStep="If this takes more than a moment, open this task again from Tasks or ask an owner or admin to check task access."
              success="Success looks like an agent work row or a note that no work history is available yet."
            />
          )}
          {error && (
            <div
              role="alert"
              aria-live="polite"
              className="rounded-card bg-apple-red/10 px-3 py-2 text-ui-body text-apple-red"
            >
              {error}
            </div>
          )}
          {!loading && !error && runs.length === 0 && (
            <div className="rounded-card border border-dashed border-black/[0.1] px-3 py-2 text-ui-body text-secondary-light dark:border-white/[0.12] dark:text-secondary-dark">
              Work history appears after an agent starts. If this stays empty, check that an agent
              is chosen and the task has been started.
            </div>
          )}
          {runs.map((run) => (
            <TaskRunRow key={run.id} run={run} />
          ))}
        </section>
      </div>
    </div>
  )
}

function AgentCheckIn({ task }: { task: TaskSummary }) {
  const checkIn = taskCheckIn(task)
  const Icon = checkIn.Icon

  return (
    <section
      data-testid="task-agent-check-in"
      className={cn(
        'rounded-card border p-3',
        checkIn.tone === 'warn'
          ? 'border-apple-orange/25 bg-apple-orange/[0.06]'
          : checkIn.tone === 'success'
            ? 'border-apple-green/20 bg-apple-green/[0.06]'
            : checkIn.tone === 'danger'
              ? 'border-apple-red/20 bg-apple-red/[0.06]'
              : 'border-black/[0.08] bg-white/70 dark:border-white/[0.1] dark:bg-white/[0.035]'
      )}
    >
      <div className="mb-3 flex items-start gap-2">
        <span
          className={cn(
            'flex h-8 w-8 shrink-0 items-center justify-center rounded-card',
            checkIn.tone === 'warn'
              ? 'bg-apple-orange/12 text-apple-orange'
              : checkIn.tone === 'success'
                ? 'bg-apple-green/12 text-apple-green'
                : checkIn.tone === 'danger'
                  ? 'bg-apple-red/12 text-apple-red'
                  : 'bg-apple-blue/10 text-apple-blue'
          )}
        >
          <Icon size={16} strokeWidth={2.2} aria-hidden="true" />
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
            Current status
          </p>
          <p className="mt-0.5 text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
            {checkIn.title}
          </p>
          <p className="mt-1 text-ui-caption leading-relaxed text-secondary-light dark:text-secondary-dark">
            {checkIn.detail}
          </p>
        </div>
      </div>
      <div className="grid grid-cols-3 gap-2">
        <CheckInMetric
          label="Agent"
          value={
            task.assignedAgentName ??
            (task.assignedTo ? TASK_AGENT_NAME_LOADING_LABEL : 'Needs agent')
          }
        />
        <CheckInMetric label="State" value={taskStateLabel(task.state)} />
        <CheckInMetric label="Updated" value={formatRelativeTime(task.updatedAt)} />
      </div>
    </section>
  )
}

function CheckInMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 rounded-card bg-black/[0.035] px-2 py-1.5 dark:bg-white/[0.045]">
      <p className="truncate text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
        {label}
      </p>
      <p className="mt-0.5 truncate text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
        {value}
      </p>
    </div>
  )
}

const TASK_COMMENT_KINDS: TaskCommentKind[] = ['comment', 'blocker', 'unblock']

function commentKindMeta(kind: TaskCommentKind): {
  label: string
  Icon: LucideIcon
  dotClass: string
} {
  switch (kind) {
    case 'blocker':
      return { label: 'Block', Icon: Flag, dotClass: 'bg-apple-red' }
    case 'unblock':
      return { label: 'Unblock', Icon: CheckCircle2, dotClass: 'bg-apple-green' }
    case 'comment':
    default:
      return { label: 'Note', Icon: MessageSquarePlus, dotClass: 'bg-apple-blue' }
  }
}

function HumanUpdates({ task }: { task: TaskSummary }) {
  const { t } = useTranslation()
  const [comments, setComments] = useState<TaskComment[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError(null)
    orchestrationApi
      .getTaskComments(task.id)
      .then((items) => {
        if (!cancelled) setComments(Array.isArray(items) ? items : [])
      })
      .catch((err) => {
        if (!cancelled) setError(taskDetailErrorMessage('loadComments', err))
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [task.id])

  function handlePosted(comment: TaskComment) {
    setComments((prev) => [...prev, comment])
  }

  function handleDeleted(commentId: string) {
    setComments((prev) => prev.filter((comment) => comment.id !== commentId))
  }

  return (
    <section className="space-y-2" data-testid="task-human-updates">
      <div className="flex items-center justify-between gap-2">
        <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
          {t('taskComments.title')}
        </p>
      </div>
      {loading && (
        <BeginnerLoadingState
          compact
          framed={false}
          title={t('taskComments.loadingTitle')}
          detail={t('taskComments.loadingDetail')}
          nextStep={t('taskComments.loadingNext')}
          success={t('taskComments.loadingSuccess')}
        />
      )}
      {error && (
        <div
          role="alert"
          aria-live="polite"
          className="rounded-card bg-apple-red/10 px-3 py-2 text-ui-body text-apple-red"
        >
          {error}
        </div>
      )}
      {!loading && !error && comments.length === 0 && (
        <div className="rounded-card border border-dashed border-black/[0.1] px-3 py-2 text-ui-body text-secondary-light dark:border-white/[0.12] dark:text-secondary-dark">
          {t('taskComments.empty')}
        </div>
      )}
      {comments.map((comment) => (
        <CommentRow key={comment.id} comment={comment} onDeleted={handleDeleted} />
      ))}
      <CommentComposer taskId={task.id} onPosted={handlePosted} />
    </section>
  )
}

function CommentRow({
  comment,
  onDeleted,
}: {
  comment: TaskComment
  onDeleted: (commentId: string) => void
}) {
  const { t } = useTranslation()
  const [confirming, setConfirming] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const kind = commentKindMeta(comment.kind)
  const isMine = currentUserId() === comment.author.id

  async function handleDelete() {
    if (!confirming) {
      setConfirming(true)
      return
    }
    setDeleting(true)
    try {
      await orchestrationApi.deleteTaskComment(comment.taskId, comment.id)
      onDeleted(comment.id)
    } catch (err) {
      setConfirming(false)
      window.alert(taskDetailErrorMessage('deleteComment', err))
    } finally {
      setDeleting(false)
    }
  }

  return (
    <div className="flex gap-2 rounded-card bg-apple-gray-6/70 px-3 py-2 dark:bg-white/[0.035]">
      <span
        aria-hidden="true"
        className={cn('mt-1.5 h-2 w-2 shrink-0 rounded-full', kind.dotClass)}
      />
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
          <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
            {comment.author.name || t('taskComments.unknownAuthor')}
          </p>
          <span className="rounded-button bg-black/[0.05] px-1.5 py-0.5 text-ui-caption font-medium text-secondary-light dark:bg-white/[0.08] dark:text-secondary-dark">
            {kind.label}
          </span>
          <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
            {formatRelativeTime(comment.createdAt)}
          </p>
        </div>
        <p className="mt-1 whitespace-pre-wrap text-ui-body leading-relaxed text-foreground-light dark:text-foreground-dark">
          {comment.body}
        </p>
        {isMine && (
          <button
            type="button"
            onClick={() => void handleDelete()}
            disabled={deleting}
            className="mt-1 inline-flex items-center gap-1 rounded-button px-1.5 py-0.5 text-ui-caption font-medium text-secondary-light underline-offset-2 transition-colors hover:text-apple-red disabled:cursor-wait disabled:opacity-60 dark:text-secondary-dark"
            aria-label={t('taskComments.deleteLabel')}
          >
            <Trash2 size={13} strokeWidth={2} aria-hidden="true" />
            {deleting
              ? t('taskComments.deleting')
              : confirming
                ? t('taskComments.confirmDelete')
                : t('taskComments.delete')}
          </button>
        )}
      </div>
    </div>
  )
}

function CommentComposer({
  taskId,
  onPosted,
}: {
  taskId: string
  onPosted: (comment: TaskComment) => void
}) {
  const { t } = useTranslation()
  const [kind, setKind] = useState<TaskCommentKind>('comment')
  const [body, setBody] = useState('')
  const [posting, setPosting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault()
    const text = body.trim()
    if (!text || posting) return
    setPosting(true)
    setError(null)
    try {
      const comment = await orchestrationApi.createTaskComment(taskId, { kind, body: text })
      onPosted(comment)
      setBody('')
      setKind('comment')
    } catch (err) {
      setError(taskDetailErrorMessage('postComment', err))
    } finally {
      setPosting(false)
    }
  }

  return (
    <form
      data-testid="task-comment-composer"
      onSubmit={(event) => void handleSubmit(event)}
      className="space-y-2 rounded-card border border-black/[0.08] px-3 py-2.5 dark:border-white/[0.1]"
    >
      <label className="flex flex-col gap-1">
        <span className="sr-only">{t('taskComments.inputLabel')}</span>
        <textarea
          value={body}
          onChange={(event) => setBody(event.target.value)}
          rows={2}
          placeholder={t('taskComments.placeholder')}
          className="w-full resize-y rounded-button border border-black/[0.08] bg-white px-2.5 py-2 text-ui-body text-foreground-light outline-none placeholder:text-secondary-light focus-visible:outline-2 focus-visible:outline-[rgb(var(--ring))] dark:border-white/[0.1] dark:bg-surface-dark dark:text-foreground-dark"
        />
      </label>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div
          role="radiogroup"
          aria-label={t('taskComments.kindLabel')}
          className="flex items-center gap-1"
        >
          {TASK_COMMENT_KINDS.map((option) => (
            <button
              key={option}
              type="button"
              role="radio"
              aria-checked={kind === option}
              onClick={() => setKind(option)}
              className={cn(
                'rounded-button px-2.5 py-1 text-ui-caption font-medium transition-colors',
                kind === option
                  ? 'bg-apple-blue text-white'
                  : 'bg-black/[0.04] text-secondary-light hover:bg-black/[0.07] dark:bg-white/[0.06] dark:text-secondary-dark dark:hover:bg-white/[0.09]'
              )}
            >
              {commentKindMeta(option).label}
            </button>
          ))}
        </div>
        <button
          type="submit"
          disabled={!body.trim() || posting}
          className="inline-flex items-center gap-1.5 rounded-button bg-apple-blue px-3 py-1.5 text-ui-caption font-semibold text-white transition-colors hover:bg-apple-blue/90 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Send size={13} strokeWidth={2.25} aria-hidden="true" />
          {posting ? t('taskComments.posting') : t('taskComments.post')}
        </button>
      </div>
      {error && (
        <p role="alert" aria-live="polite" className="text-ui-caption font-medium text-apple-red">
          {error}
        </p>
      )}
    </form>
  )
}

function currentUserId(): string | null {
  try {
    const raw = typeof localStorage !== 'undefined' ? localStorage.getItem('af:auth:user') : null
    if (!raw) return null
    const parsed = JSON.parse(raw) as { id?: unknown }
    return typeof parsed.id === 'string' ? parsed.id : null
  } catch {
    return null
  }
}

function TaskRunRow({ run }: { run: TaskRunSummary }) {
  const runSource = runSourceLabel(run)
  const finished = run.finishedAt ? formatRelativeTime(run.finishedAt) : 'Still running'
  const status = readableRunStatus(run.status)
  const showWorkAttemptReference = runSourceNeedsCheck(runSource)

  return (
    <div className="rounded-card bg-apple-gray-6/70 px-3 py-2 dark:bg-white/[0.035]">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="truncate text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
            Agent work: {readableRunStatus(run.status)}
          </p>
          <p className="mt-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Started {formatRelativeTime(run.startedAt)} · {finished} ·{' '}
            <span className={uiStyles.chip}>{`Used ${runSource}`}</span>
          </p>
          {showWorkAttemptReference && (
            <p className="mt-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
              {workAttemptReferenceLabel(run.id)}
            </p>
          )}
          {run.image ? (
            <WorkImageEvidence image={run.image} />
          ) : run.runtimeKind === 'container' ? (
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              This run did not record work image evidence. New container runs record it
              automatically; ask an admin to check this Agent before relying on the result.
            </p>
          ) : !run.runtimeKind ? (
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              This run did not record whether it used a work image.
            </p>
          ) : null}
        </div>
        <span className="inline-flex shrink-0 items-center gap-1.5 text-ui-body text-secondary-light dark:text-secondary-dark">
          <span
            aria-hidden="true"
            className={cn('h-1.5 w-1.5 rounded-full', runStatusDot(run.status))}
          />
          {status}
        </span>
      </div>
    </div>
  )
}

function WorkImageEvidence({ image }: { image: TaskRunImageEvidence }) {
  return (
    <details className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
      <summary className="w-fit cursor-pointer break-all rounded-sm focus-visible:outline-2 focus-visible:outline-[rgb(var(--ring))]">
        Work image{image.version ? ` ${image.version}` : ''} · {image.source}
        {image.trust ? ` · ${workImageTrustLabel(image.trust)}` : ''}
      </summary>
      <dl className="mt-1 grid gap-1 border-l border-black/[0.08] pl-2 dark:border-white/[0.1]">
        <div>
          <dt className="inline font-medium">Version source: </dt>
          <dd className="inline">{workImageVersionSourceLabel(image.versionSource)}</dd>
        </div>
        <div>
          <dt className="font-medium">Image ID</dt>
          <dd>
            <code className="select-all break-all">{image.imageId}</code>
          </dd>
        </div>
        {image.manifestDigest ? (
          <div>
            <dt className="font-medium">Manifest digest</dt>
            <dd>
              <code className="select-all break-all">{image.manifestDigest}</code>
            </dd>
          </div>
        ) : null}
      </dl>
    </details>
  )
}

function workImageTrustLabel(trust: NonNullable<TaskRunImageEvidence['trust']>): string {
  return trust === 'verified-signature' ? 'Verified signature' : 'Trusted by this host'
}

function workImageVersionSourceLabel(source: TaskRunImageEvidence['versionSource']): string {
  return source === 'docker-label' ? 'Docker image label' : 'Not reported'
}

function workAttemptReferenceLabel(id: string): string {
  const trimmed = id.trim()
  if (!trimmed) {
    return 'Open this task again from the Tasks page to check the work help text.'
  }
  return `Work help text ${trimmed.length > 8 ? trimmed.slice(0, 8) : trimmed}`
}

function runSourceNeedsCheck(runSource: string): boolean {
  return runSource.includes('selected in Settings') || runSource.includes('shown in Settings')
}

function runSourceLabel(run: TaskRunSummary): string {
  const cliTool = workToolLabel(run.cliTool)
  if (cliTool) return cliTool
  const provider = aiServiceLabel(run.providerName)
  if (provider) return provider

  switch (run.runtimeKind) {
    case 'container':
      return 'project files'
    case 'cli':
    case 'host':
      return 'this computer'
    case 'api':
    case 'provider':
      return 'an AI service'
    default:
      return run.maxContextTokens ? 'the chosen agent' : 'an agent'
  }
}

function aiServiceLabel(providerName?: string): string | null {
  const trimmed = providerName?.trim()
  if (!trimmed) return null

  const normalized = trimmed.toLowerCase()
  switch (normalized) {
    case 'anthropic':
      return 'Anthropic'
    case 'openai':
      return 'OpenAI'
    case 'google':
    case 'gemini':
      return 'Google'
    case 'openai_compatible':
    case 'openai-compatible':
    case 'custom':
      return 'a custom AI service'
    case 'azure_openai':
    case 'azure-openai':
      return 'Azure OpenAI'
    case 'ollama':
    case 'local':
      return 'a local AI service'
    default:
      return looksLikeSlug(trimmed, normalized) ? 'an AI service shown in Settings' : trimmed
  }
}

function looksLikeSlug(value: string, normalized: string): boolean {
  return value === normalized && /^[a-z0-9]+(?:[_-][a-z0-9]+)+$/.test(normalized)
}

function workToolLabel(tool?: string): string | null {
  switch (tool?.trim().toLowerCase()) {
    case 'claude':
      return 'Claude'
    case 'codex':
      return 'Codex'
    case 'gemini':
      return 'Gemini'
    case 'opencode':
      return 'OpenCode'
    case undefined:
    case '':
      return null
    default:
      return 'the saved tool selected in Settings'
  }
}

function taskCheckIn(task: TaskSummary): {
  title: string
  detail: string
  tone: 'default' | 'success' | 'warn' | 'danger'
  Icon: LucideIcon
} {
  const hasAssignedAgent = Boolean(task.assignedAgentName || task.assignedTo)
  const agentName = task.assignedAgentName ?? (task.assignedTo ? 'The chosen agent' : 'The agent')
  const artifactCount = taskResultArtifacts(task.result).length

  switch (task.state) {
    case 'backlog':
      return task.assignedAgentName
        ? {
            title: `${agentName} is ready to start`,
            detail: 'Start the task when you are ready for the agent to begin.',
            tone: 'default',
            Icon: Send,
          }
        : {
            title: 'Choose an agent to start this task',
            detail: 'Choose an agent before this task can start.',
            tone: 'warn',
            Icon: Bot,
          }
    case 'queued':
      return hasAssignedAgent
        ? {
            title: `${agentName} is waiting to start`,
            detail:
              'If this stays here, check the work history below. If nothing starts, choose another agent.',
            tone: 'default',
            Icon: Clock3,
          }
        : {
            title: 'Waiting for an agent',
            detail: 'Choose or start an agent so this task has someone to begin the work.',
            tone: 'warn',
            Icon: Clock3,
          }
    case 'working':
      return {
        title: `${agentName} is working at ${task.progress}%`,
        detail:
          task.progress >= 80
            ? 'Prepare to check the result when the task finishes.'
            : 'Progress is active. Watch for requests that need your decision.',
        tone: 'default',
        Icon: CircleDot,
      }
    case 'blocked':
      return {
        title: `${agentName} needs your answer`,
        detail: taskBlockedPreview({
          blockedHint: task.blockedHint,
          blockedReason: task.blockedReason,
          error: task.error,
        }),
        tone: 'warn',
        Icon: AlertTriangle,
      }
    case 'completed':
      return {
        title: `${agentName} finished the task`,
        detail:
          artifactCount > 0
            ? `${artifactCount} result file${artifactCount === 1 ? '' : 's'} ready to check.`
            : 'Check the outcome, then save repeatable steps or create a follow-up task if something is missing.',
        tone: 'success',
        Icon: CheckCircle2,
      }
    case 'failed':
      return {
        title: `${agentName} could not finish`,
        detail: taskFailurePreview(task.error),
        tone: 'danger',
        Icon: XCircle,
      }
    case 'canceled':
      return {
        title: 'Decide whether to continue',
        detail: 'The task was canceled; reopen or create follow-up work if needed.',
        tone: 'default',
        Icon: XCircle,
      }
    default:
      return {
        title: 'Check latest task updates',
        detail:
          'Open the latest updates before deciding whether to start, retry, or close this task.',
        tone: 'warn',
        Icon: AlertTriangle,
      }
  }
}

function taskHistoryEvents(task: TaskSummary): { id: string; title: string; detail: string }[]
function taskHistoryEvents(task: TaskSummary): { id: string; title: string; detail: string }[] {
  const events = [
    {
      id: 'created',
      title: 'Task created',
      detail: formatRelativeTime(task.createdAt),
    },
  ]

  if (task.assignedAgentName) {
    events.push({
      id: 'assigned',
      title: `Agent chosen: ${task.assignedAgentName}`,
      detail: assignedAgentStoryDetail(task),
    })
  }

  if (task.state === 'working') {
    events.push({
      id: 'progress',
      title: `Work in progress at ${task.progress}%`,
      detail: `Updated ${formatRelativeTime(task.updatedAt)}`,
    })
  }

  if (task.state === 'blocked') {
    events.push({
      id: 'blocked',
      title: 'Needs your input',
      detail: taskBlockedPreview({
        blockedHint: task.blockedHint,
        blockedReason: task.blockedReason,
        error: task.error,
      }),
    })
  }

  if (task.state === 'failed') {
    events.push({
      id: 'failed',
      title: 'Work stopped',
      detail: taskFailurePreview(task.error),
    })
  }

  if (task.state === 'completed') {
    events.push({
      id: 'completed',
      title: 'Work completed',
      detail: task.completedAt
        ? formatRelativeTime(task.completedAt)
        : `Updated ${formatRelativeTime(task.updatedAt)}`,
    })
  }

  return events
}

function assignedAgentStoryDetail(task: TaskSummary): string {
  switch (task.state) {
    case 'working':
      return 'This agent is working on the task now.'
    case 'blocked':
      return 'This agent needs your answer before it can continue.'
    case 'completed':
      return 'This agent finished the task.'
    case 'failed':
      return 'This agent tried the task.'
    case 'canceled':
      return 'This agent was chosen before the task stopped.'
    default:
      return 'This agent will handle the next step.'
  }
}

function taskUpdateGuide(task: TaskSummary): string {
  switch (task.state) {
    case 'backlog':
      return task.assignedAgentName
        ? 'The task has an agent. Check the brief, then start the task.'
        : 'Choose an agent first, then start the task.'
    case 'queued':
      return task.assignedAgentName || task.assignedTo
        ? 'The task is waiting to begin. If it stays here, check work history below, then choose another agent if needed.'
        : 'The task is waiting for an agent. Choose or start an agent before expecting work history.'
    case 'working':
      return 'The agent is working. Watch for requests that need your decision, then check the result when it finishes.'
    case 'blocked':
      return 'The task needs your input. Read the reason, decide what to provide, then allow it to continue or update the task.'
    case 'completed':
      return 'Open Results next. Check the answer, then accept it, save repeatable steps, or create a follow-up task.'
    case 'failed': {
      const attemptNote = taskAttemptNote(task.attempt)
      const guide = isContextOverflowFailure(task.error)
        ? CONTEXT_OVERFLOW_FAILURE_GUIDE
        : 'Read the latest update, fix the cause if you can, then retry or create a clearer follow-up task.'
      return attemptNote ? `${attemptNote} ${guide}` : guide
    }
    case 'canceled':
      return 'No one is working on this task now. Reopen it or create follow-up work if it still matters.'
    default:
      return 'Check the latest updates before deciding whether to start, retry, or close this task.'
  }
}

function readableRunStatus(status: string): string {
  const normalized = normalizeRunStatus(status)
  switch (normalized) {
    case 'completed':
    case 'succeeded':
    case 'success':
      return 'Finished'
    case 'running':
    case 'working':
    case 'in_progress':
      return 'In progress'
    case 'queued':
    case 'pending':
      return 'Waiting to start'
    case 'failed':
    case 'error':
      return 'Check retry steps'
    case 'canceled':
    case 'cancelled':
      return 'Stopped'
    default:
      return normalized ? 'Check task status' : 'Open task details to check status'
  }
}

function normalizeRunStatus(status: string): string {
  return status.trim().toLowerCase()
}

function runStatusDot(status: string): string {
  switch (normalizeRunStatus(status)) {
    case 'completed':
    case 'succeeded':
    case 'success':
      return 'bg-apple-green'
    case 'running':
    case 'working':
    case 'in_progress':
      return 'bg-apple-blue'
    case 'failed':
    case 'error':
      return 'bg-apple-red'
    case 'queued':
    case 'pending':
      return 'bg-apple-orange'
    default:
      return 'bg-apple-gray-3'
  }
}
