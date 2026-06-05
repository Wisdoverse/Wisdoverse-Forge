import { useState, type ReactNode } from 'react'
import {
  AlertTriangle,
  CheckCircle2,
  MessageSquareText,
  Play,
  RotateCcw,
  Trash2,
  type LucideIcon,
} from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { isHostCliAgent, useAgentsStore, type AgentInfo } from '@app/entities/agent'

interface AgentControlPanelProps {
  agent: AgentInfo
  onDeleted: () => void
}

export function AgentControlPanel({ agent, onDeleted }: AgentControlPanelProps) {
  const { sendPrompt, startAgent, restartAgent, deleteAgent, error } = useAgentsStore()

  const [prompt, setPrompt] = useState('')
  const [promptError, setPromptError] = useState<string | null>(null)
  const [sending, setSending] = useState(false)
  const [starting, setStarting] = useState(false)
  const [confirmRestart, setConfirmRestart] = useState(false)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const promptHelpId = 'agent-control-prompt-help'
  const promptErrorId = 'agent-control-prompt-error'

  const hostCli = isHostCliAgent(agent)
  const canStartContainer = Boolean(agent.cliTool && !agent.containerId && !hostCli)
  const canRestartContainer = Boolean(agent.cliTool && agent.containerId && !hostCli)
  const messageInputId = `agent-message-${agent.id}`
  const messageHelpId = `agent-message-help-${agent.id}`
  const controlSummary = getControlSummary(agent, {
    canStartContainer,
    canRestartContainer,
    hostCli,
  })
  const ControlSummaryIcon = controlSummary.Icon

  async function handleSendPrompt() {
    if (sending) return
    const trimmedPrompt = prompt.trim()
    if (!trimmedPrompt) {
      setPromptError('Write an instruction before sending it to this agent.')
      document.getElementById(messageInputId)?.focus()
      return
    }
    setPromptError(null)
    setSending(true)
    const ok = await sendPrompt(agent.id, trimmedPrompt)
    setSending(false)
    if (ok) setPrompt('')
  }

  async function handleStart() {
    if (starting) return
    setStarting(true)
    await startAgent(agent.id)
    setStarting(false)
  }

  async function handleRestart() {
    await restartAgent(agent.id)
    setConfirmRestart(false)
  }

  async function handleDelete() {
    const ok = await deleteAgent(agent.id)
    if (ok) onDeleted()
    setConfirmDelete(false)
  }

  return (
    <div className="flex flex-col gap-3">
      {error && (
        <div
          role="alert"
          className="flex gap-3 rounded-lg bg-apple-red/10 px-3 py-2 text-ui-caption text-apple-red"
        >
          <AlertTriangle size={16} strokeWidth={2} aria-hidden="true" className="mt-0.5 shrink-0" />
          <div className="flex flex-col gap-1">
            <span className="font-medium">Action did not finish</span>
            <span>{agentControlErrorMessage(error)}</span>
          </div>
        </div>
      )}

      <div
        className={cn(
          'rounded-card border border-black/[0.08] bg-white p-6 dark:border-white/[0.1] dark:bg-[#2a2a2c]',
          'flex flex-col gap-3'
        )}
      >
        <div className="flex items-start gap-3">
          <span className="flex size-9 shrink-0 items-center justify-center rounded-full bg-apple-blue/10 text-apple-blue">
            <MessageSquareText size={18} strokeWidth={2} aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <label
              htmlFor={messageInputId}
              className="text-ui-body font-semibold text-foreground-light dark:text-foreground-dark"
            >
              Send one instruction
            </label>
            <p
              id={messageHelpId}
              className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
            >
              Use this for a quick question or one small request. For work that needs a clear
              result, create a task instead.
            </p>
          </div>
        </div>
        <textarea
          id={messageInputId}
          value={prompt}
          onChange={(e) => {
            setPrompt(e.target.value)
            if (promptError) setPromptError(null)
          }}
          rows={3}
          aria-describedby={`${messageHelpId} ${promptHelpId}${
            promptError ? ` ${promptErrorId}` : ''
          }`}
          className="w-full resize-none rounded-[18px] border border-black/[0.08] bg-white px-4 py-3 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
          placeholder="Example: Check the latest run and tell me the next safe step."
          onKeyDown={(e) => {
            if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
              e.preventDefault()
              void handleSendPrompt()
            }
          }}
        />
        {promptError && (
          <div id={promptErrorId} className="text-ui-caption text-apple-red" role="alert">
            {promptError}
          </div>
        )}
        <p
          id={promptHelpId}
          className="text-ui-caption text-secondary-light dark:text-secondary-dark"
        >
          Send one concrete instruction, then watch this agent&apos;s history for progress.
        </p>
        <div className="flex justify-end">
          <button
            type="button"
            onClick={handleSendPrompt}
            disabled={sending}
            className={cn(
              'w-full rounded-full bg-apple-blue px-4 py-2 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus active:scale-95 sm:w-auto',
              sending && 'opacity-50 cursor-not-allowed'
            )}
          >
            {sending ? 'Sending...' : 'Send instruction'}
          </button>
        </div>
      </div>

      <div className="flex flex-col gap-4">
        <div className="flex items-start gap-3">
          <span className="flex size-9 shrink-0 items-center justify-center rounded-full bg-apple-green/10 text-apple-green">
            <ControlSummaryIcon size={18} strokeWidth={2} aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <h3 className="text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
              {controlSummary.title}
            </h3>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              {controlSummary.detail}
            </p>
          </div>
        </div>

        <div className="grid gap-3 md:grid-cols-2">
          {!canStartContainer && !canRestartContainer && (
            <ActionInfo
              icon={CheckCircle2}
              title="No recovery action needed"
              detail="Do not restart anything unless work stops updating. Use Tasks for work that needs a clear result."
            />
          )}

          {(canStartContainer || canRestartContainer) && (
            <>
              {canStartContainer ? (
                <ActionCard
                  icon={Play}
                  title="Start the agent workspace"
                  detail="Use this when the agent has no running workspace yet. Starting can take a short moment."
                >
                  <button
                    type="button"
                    onClick={handleStart}
                    disabled={starting}
                    className={cn(
                      'w-full rounded-full bg-apple-blue px-4 py-2 text-ui-button font-medium text-white sm:w-auto',
                      'transition-transform hover:bg-apple-blue-focus active:scale-95',
                      starting && 'opacity-50 cursor-not-allowed'
                    )}
                  >
                    {starting ? 'Starting...' : 'Start agent'}
                  </button>
                </ActionCard>
              ) : confirmRestart ? (
                <ConfirmAction
                  tone="blue"
                  icon={RotateCcw}
                  title="Restart this agent?"
                  detail="Use restart only when the live work window or task updates are stuck. Active work may stop and need to be sent again."
                  confirmLabel="Restart now"
                  cancelLabel="Keep running"
                  onConfirm={handleRestart}
                  onCancel={() => setConfirmRestart(false)}
                />
              ) : (
                <ActionCard
                  icon={RotateCcw}
                  title="Recover a stuck agent"
                  detail="Restart only after checking Tasks or the live work window and seeing no new progress."
                >
                  <button
                    type="button"
                    onClick={() => setConfirmRestart(true)}
                    className={cn(
                      'rounded-full border border-black/[0.08] px-4 py-2 text-ui-button font-medium',
                      'text-apple-blue transition-transform hover:bg-apple-blue/5 active:scale-95',
                      'dark:border-white/[0.1]'
                    )}
                  >
                    Restart agent
                  </button>
                </ActionCard>
              )}
            </>
          )}

          {confirmDelete ? (
            <ConfirmAction
              tone="red"
              icon={Trash2}
              title="Remove this agent?"
              detail="This removes the agent from future work. Existing task history stays available, but this agent will no longer receive new work."
              confirmLabel="Remove agent"
              cancelLabel="Keep agent"
              onConfirm={handleDelete}
              onCancel={() => setConfirmDelete(false)}
            />
          ) : (
            <ActionCard
              icon={Trash2}
              title="Remove this agent"
              detail="Use this only when replacing the agent or cleaning up one you no longer need."
            >
              <button
                type="button"
                onClick={() => setConfirmDelete(true)}
                className={cn(
                  'rounded-full border border-black/[0.08] px-4 py-2 text-ui-button font-medium',
                  'text-apple-red transition-colors hover:bg-apple-red/5 dark:border-white/[0.1]'
                )}
              >
                Remove agent
              </button>
            </ActionCard>
          )}
        </div>
      </div>
    </div>
  )
}

interface ControlSummaryOptions {
  canStartContainer: boolean
  canRestartContainer: boolean
  hostCli: boolean
}

function getControlSummary(
  agent: AgentInfo,
  { canStartContainer, canRestartContainer, hostCli }: ControlSummaryOptions
): { title: string; detail: string; Icon: LucideIcon } {
  if (canStartContainer) {
    return {
      title: 'Agent workspace needs to start',
      detail:
        'Start this agent workspace before sending file work or opening its live work window.',
      Icon: Play,
    }
  }

  if (canRestartContainer) {
    return {
      title: 'Agent workspace controls',
      detail: 'Most agents do not need manual recovery. Restart only when progress has stopped.',
      Icon: RotateCcw,
    }
  }

  if (hostCli) {
    return {
      title: 'This computer controls',
      detail:
        'This agent runs on a joined computer. Start or stop the connection tool on that computer; use this page for messages and cleanup.',
      Icon: CheckCircle2,
    }
  }

  if (agent.cliTool) {
    return {
      title: 'Agent controls',
      detail: 'The workspace looks ready. Use messages for quick help and Tasks for tracked work.',
      Icon: CheckCircle2,
    }
  }

  return {
    title: 'Chat-only agent controls',
    detail:
      'This agent replies through its connected AI service. Use messages for quick help and Tasks for tracked work.',
    Icon: CheckCircle2,
  }
}

interface ActionCardProps {
  icon: LucideIcon
  title: string
  detail: string
  children: ReactNode
}

function ActionCard({ icon: Icon, title, detail, children }: ActionCardProps) {
  return (
    <div className="flex flex-col gap-3 rounded-lg border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-[#2a2a2c]">
      <div className="flex items-start gap-3">
        <span className="flex size-8 shrink-0 items-center justify-center rounded-full bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
          <Icon size={16} strokeWidth={2} aria-hidden="true" />
        </span>
        <div className="min-w-0">
          <h4 className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
            {title}
          </h4>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {detail}
          </p>
        </div>
      </div>
      <div className="flex flex-col justify-end gap-2 sm:flex-row sm:flex-wrap">{children}</div>
    </div>
  )
}

interface ActionInfoProps {
  icon: LucideIcon
  title: string
  detail: string
}

function ActionInfo({ icon: Icon, title, detail }: ActionInfoProps) {
  return (
    <div className="flex items-start gap-3 rounded-lg border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-[#2a2a2c]">
      <span className="flex size-8 shrink-0 items-center justify-center rounded-full bg-apple-green/10 text-apple-green">
        <Icon size={16} strokeWidth={2} aria-hidden="true" />
      </span>
      <div className="min-w-0">
        <h4 className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
          {title}
        </h4>
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {detail}
        </p>
      </div>
    </div>
  )
}

function agentControlErrorMessage(error: string): string {
  const normalized = error.toLowerCase()

  if (
    normalized.includes('permission') ||
    normalized.includes('forbidden') ||
    /\b403\b/.test(error)
  ) {
    return 'You do not have permission to change this agent. Ask an owner or admin to update what you can do, then try again.'
  }
  if (normalized.includes('unauthorized') || /\b401\b/.test(error)) {
    return 'Sign in again, reopen this agent, then try the action once more.'
  }
  if (normalized.includes('conflict') || /\b409\b/.test(error)) {
    return 'This agent changed while you were working. Refresh this agent, confirm the latest status, then try again.'
  }
  if (normalized.includes('rate limit') || /\b429\b/.test(error)) {
    return 'The agent controls are busy. Wait a moment, refresh this agent, then try again.'
  }
  if (/\b5\d\d\b/.test(error)) {
    return 'Forge could not update this agent right now. Refresh this agent and try again. If it keeps failing, ask an owner or admin to check Agent Work Setup.'
  }

  return 'Refresh this agent and confirm the latest status before trying once more. For Start or Restart, wait for Idle or Working. If it keeps failing, ask an owner or admin to check what you can do and Agent Work Setup.'
}

interface ConfirmActionProps {
  tone: 'blue' | 'red'
  icon: LucideIcon
  title: string
  detail: string
  confirmLabel: string
  cancelLabel: string
  onConfirm: () => void | Promise<void>
  onCancel: () => void
}

function ConfirmAction({
  tone,
  icon: Icon,
  title,
  detail,
  confirmLabel,
  cancelLabel,
  onConfirm,
  onCancel,
}: ConfirmActionProps) {
  return (
    <div className="flex flex-col gap-3 rounded-lg border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-[#2a2a2c]">
      <div className="flex items-start gap-3">
        <span
          className={cn(
            'flex size-8 shrink-0 items-center justify-center rounded-full',
            tone === 'red' ? 'bg-apple-red/10 text-apple-red' : 'bg-apple-blue/10 text-apple-blue'
          )}
        >
          <Icon size={16} strokeWidth={2} aria-hidden="true" />
        </span>
        <div className="min-w-0">
          <h4 className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
            {title}
          </h4>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {detail}
          </p>
        </div>
      </div>
      <div className="flex flex-col justify-end gap-2 sm:flex-row sm:flex-wrap">
        <button
          type="button"
          onClick={onCancel}
          className="w-full rounded-full bg-apple-gray-5 px-3 py-1.5 text-ui-button font-medium text-foreground-light dark:bg-white/[0.06] dark:text-foreground-dark sm:w-auto"
        >
          {cancelLabel}
        </button>
        <button
          type="button"
          onClick={() => void onConfirm()}
          className={cn(
            'w-full rounded-full px-3 py-1.5 text-ui-button font-medium text-white sm:w-auto',
            tone === 'red'
              ? 'bg-apple-red hover:bg-apple-red/90'
              : 'bg-apple-blue hover:bg-apple-blue-focus'
          )}
        >
          {confirmLabel}
        </button>
      </div>
    </div>
  )
}
