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

const LOCAL_AGENT_CONTROL_FAILURE = {
  sendInstruction: 'local-send-instruction-failed',
  startWorkspace: 'local-start-workspace-failed',
  restartWorkspace: 'local-restart-workspace-failed',
  removeAgent: 'local-remove-agent-failed',
} as const

const CHAT_ONLY_AI_SERVICE_RECOVERY_DETAIL =
  'Open AI service settings, choose Check connection for this service, then return to Agents and choose this agent again when it shows Ready.'

interface AgentControlPanelProps {
  agent: AgentInfo
  onDeleted: () => void
}

export function AgentControlPanel({ agent, onDeleted }: AgentControlPanelProps) {
  const { sendPrompt, startAgent, restartAgent, deleteAgent, error } = useAgentsStore()

  const [prompt, setPrompt] = useState('')
  const [promptError, setPromptError] = useState<string | null>(null)
  const [localActionError, setLocalActionError] = useState<string | null>(null)
  const [localActionStatus, setLocalActionStatus] = useState<string | null>(null)
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
  const readyActionInfo = getReadyActionInfo(agent, { hostCli })
  const ControlSummaryIcon = controlSummary.Icon
  const controlError = error ?? localActionError
  const messageAvailability = getMessageAvailability(agent, { canStartContainer, hostCli })
  const messageDisabled = sending || !messageAvailability.canSend

  async function handleSendPrompt() {
    if (sending) return
    if (!messageAvailability.canSend) {
      setPromptError(messageAvailability.detail)
      return
    }
    const trimmedPrompt = prompt.trim()
    if (!trimmedPrompt) {
      setPromptError('Write a message before sending it to this agent.')
      document.getElementById(messageInputId)?.focus()
      return
    }
    setPromptError(null)
    setLocalActionError(null)
    setLocalActionStatus(null)
    setSending(true)
    try {
      const ok = await sendPrompt(agent.id, trimmedPrompt)
      if (ok) {
        setPrompt('')
        setLocalActionStatus(
          "Message sent. Watch this agent's history for progress, or create a task next time when you need a tracked result others can find later."
        )
      } else {
        setLocalActionStatus(null)
        setLocalActionError(LOCAL_AGENT_CONTROL_FAILURE.sendInstruction)
      }
    } catch {
      setLocalActionStatus(null)
      setLocalActionError(LOCAL_AGENT_CONTROL_FAILURE.sendInstruction)
    } finally {
      setSending(false)
    }
  }

  async function handleStart() {
    if (starting) return
    setLocalActionError(null)
    setLocalActionStatus(null)
    setStarting(true)
    try {
      const ok = await startAgent(agent.id)
      if (ok) {
        setLocalActionStatus(
          'File work start requested. Go back to Agents, choose this agent again when it shows Ready, then send a message or create a task.'
        )
      } else {
        setLocalActionError(LOCAL_AGENT_CONTROL_FAILURE.startWorkspace)
      }
    } catch {
      setLocalActionStatus(null)
      setLocalActionError(LOCAL_AGENT_CONTROL_FAILURE.startWorkspace)
    } finally {
      setStarting(false)
    }
  }

  async function handleRestart() {
    setLocalActionError(null)
    setLocalActionStatus(null)
    try {
      const ok = await restartAgent(agent.id)
      if (ok) {
        setLocalActionStatus(
          'Restart requested. Wait until this agent shows Ready before sending new work.'
        )
      } else {
        setLocalActionError(LOCAL_AGENT_CONTROL_FAILURE.restartWorkspace)
      }
    } catch {
      setLocalActionStatus(null)
      setLocalActionError(LOCAL_AGENT_CONTROL_FAILURE.restartWorkspace)
    } finally {
      setConfirmRestart(false)
    }
  }

  async function handleDelete() {
    setLocalActionError(null)
    setLocalActionStatus(null)
    try {
      const ok = await deleteAgent(agent.id)
      if (ok) {
        onDeleted()
      } else {
        setLocalActionError(LOCAL_AGENT_CONTROL_FAILURE.removeAgent)
      }
    } catch {
      setLocalActionError(LOCAL_AGENT_CONTROL_FAILURE.removeAgent)
    } finally {
      setConfirmDelete(false)
    }
  }

  return (
    <div className="flex flex-col gap-3">
      {localActionStatus && !controlError && (
        <div
          role="status"
          aria-live="polite"
          className="flex gap-3 rounded-lg bg-apple-green/10 px-3 py-2 text-ui-caption text-apple-green"
        >
          <CheckCircle2 size={16} strokeWidth={2} aria-hidden="true" className="mt-0.5 shrink-0" />
          <span>{localActionStatus}</span>
        </div>
      )}

      {controlError && (
        <div
          role="alert"
          aria-live="polite"
          className="flex gap-3 rounded-lg bg-apple-red/10 px-3 py-2 text-ui-caption text-apple-red"
        >
          <AlertTriangle size={16} strokeWidth={2} aria-hidden="true" className="mt-0.5 shrink-0" />
          <div className="flex flex-col gap-1">
            <span className="font-medium">Action did not finish</span>
            <span>Follow the step below, then try again.</span>
            <span>{agentControlErrorMessage(controlError)}</span>
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
              Send a quick message
            </label>
            <p
              id={messageHelpId}
              className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
            >
              {messageAvailability.canSend
                ? 'Use this for a quick question or one small request. For work that needs a clear result, create a task instead.'
                : messageAvailability.detail}
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
          disabled={!messageAvailability.canSend}
          aria-describedby={`${messageHelpId} ${promptHelpId}${
            promptError ? ` ${promptErrorId}` : ''
          }`}
          className={cn(
            'w-full resize-none rounded-[18px] border border-black/[0.08] bg-white px-4 py-3 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark',
            !messageAvailability.canSend && 'cursor-not-allowed opacity-60'
          )}
          placeholder={messageAvailability.placeholder}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
              e.preventDefault()
              if (messageAvailability.canSend) void handleSendPrompt()
            }
          }}
        />
        {promptError && (
          <div
            id={promptErrorId}
            className="text-ui-caption text-apple-red"
            role="alert"
            aria-live="polite"
          >
            {promptError}
          </div>
        )}
        <p
          id={promptHelpId}
          className="text-ui-caption text-secondary-light dark:text-secondary-dark"
        >
          {messageAvailability.help}
        </p>
        <div className="flex justify-end">
          <button
            type="button"
            onClick={handleSendPrompt}
            disabled={messageDisabled}
            className={cn(
              'w-full rounded-full bg-apple-blue px-4 py-2 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus active:scale-95 sm:w-auto',
              messageDisabled && 'opacity-50 cursor-not-allowed'
            )}
          >
            {sending ? 'Sending message...' : 'Send message'}
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
              title={readyActionInfo.title}
              detail={readyActionInfo.detail}
            />
          )}

          {(canStartContainer || canRestartContainer) && (
            <>
              {canStartContainer ? (
                <ActionCard
                  icon={Play}
                  title="Start file work"
                  detail="Use this when file work has not started yet. Wait for Ready before sending file work."
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
                    {starting ? 'Starting file work...' : 'Start file work'}
                  </button>
                </ActionCard>
              ) : confirmRestart ? (
                <ConfirmAction
                  tone="blue"
                  icon={RotateCcw}
                  title="Restart this agent?"
                  detail="Use restart only when Tasks or Live work stops showing progress. Active work may stop and need to be sent again."
                  confirmLabel="Restart now"
                  cancelLabel="Keep running"
                  onConfirm={handleRestart}
                  onCancel={() => setConfirmRestart(false)}
                />
              ) : (
                <ActionCard
                  icon={RotateCcw}
                  title="Fix stuck file work"
                  detail="Restart only after checking Tasks or Live work and seeing no new progress."
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
      title: 'File work needs to start',
      detail: 'Start file work before sending file tasks or opening Live work.',
      Icon: Play,
    }
  }

  if (canRestartContainer) {
    return {
      title: 'Agent file-work controls',
      detail: 'Most agents do not need manual recovery. Restart only when progress has stopped.',
      Icon: RotateCcw,
    }
  }

  if (hostCli) {
    return hostCliControlSummary(agent.status)
  }

  if (agent.cliTool) {
    return {
      title: 'Agent controls',
      detail: 'File work looks ready. Use messages for quick help and Tasks for tracked work.',
      Icon: CheckCircle2,
    }
  }

  if (agent.status === 'offline') {
    return {
      title: 'AI service needs a check',
      detail: CHAT_ONLY_AI_SERVICE_RECOVERY_DETAIL,
      Icon: AlertTriangle,
    }
  }

  return {
    title: 'Chat-only AI service controls',
    detail:
      'This agent replies through its AI service. Use messages for quick help and Tasks for tracked work.',
    Icon: CheckCircle2,
  }
}

function getReadyActionInfo(
  agent: AgentInfo,
  { hostCli }: { hostCli: boolean }
): { title: string; detail: string } {
  if (hostCli) {
    return hostCliReadyActionInfo(agent.status)
  }

  if (agent.cliTool) {
    return {
      title: 'Ready for messages and tasks',
      detail:
        'Send a quick message here, or create a Task when you need file work with a clear result.',
    }
  }

  if (agent.status === 'offline') {
    return {
      title: 'Check AI service before sending',
      detail: CHAT_ONLY_AI_SERVICE_RECOVERY_DETAIL,
    }
  }

  return {
    title: 'Ready for chat and tracked tasks',
    detail:
      'Send a quick message here, or create a Task when you need a question answered, writing help, or a result check with a clear outcome.',
  }
}

function getMessageAvailability(
  agent: AgentInfo,
  {
    canStartContainer,
    hostCli,
  }: {
    canStartContainer: boolean
    hostCli: boolean
  }
): { canSend: boolean; detail: string; placeholder: string; help: string } {
  if (canStartContainer) {
    return {
      canSend: false,
      detail:
        'Start file work first. When this agent shows Ready, you can send a message or create a task.',
      placeholder: 'Start file work before sending a message.',
      help: 'Use Start file work, wait for Ready, then send a message here.',
    }
  }

  if (agent.status !== 'offline') {
    return {
      canSend: true,
      detail: '',
      placeholder: 'Example: Check the latest run and tell me the next safe step.',
      help: "Send one concrete message, then watch this agent's history for progress.",
    }
  }

  if (hostCli) {
    return {
      canSend: false,
      detail:
        'This computer is not connected. Go back to Agents, choose Connect this computer, and use the new setup text before sending a message.',
      placeholder: 'Reconnect this computer before sending a message.',
      help: 'Use Connect this computer on the Agents list, wait for Ready, then send a message here.',
    }
  }

  if (agent.cliTool) {
    return {
      canSend: false,
      detail:
        'File work is not connected. Go back to Agents and choose this agent again, or start file work before sending a message.',
      placeholder:
        'Start file work or open Agents and choose this agent again before sending a message.',
      help: 'Go back to Agents and choose this agent again, or start file work, then wait for Ready before sending a message here.',
    }
  }

  return {
    canSend: false,
    detail: CHAT_ONLY_AI_SERVICE_RECOVERY_DETAIL,
    placeholder: 'Check this AI service before sending a message.',
    help: 'Open AI service settings, choose Check connection for this service, then return to Agents and choose this agent again.',
  }
}

function hostCliControlSummary(status: AgentInfo['status']): {
  title: string
  detail: string
  Icon: LucideIcon
} {
  if (status === 'offline') {
    return {
      title: 'Reconnect this computer from Agents',
      detail:
        "Use Back to return to Agents, choose Connect this computer, then paste the new setup text in that computer's command app.",
      Icon: AlertTriangle,
    }
  }

  return {
    title: 'This computer is connected',
    detail:
      "This computer is already connected. Keep that computer's command app open while it works; close that app only when you want it offline.",
    Icon: CheckCircle2,
  }
}

function hostCliReadyActionInfo(status: AgentInfo['status']): { title: string; detail: string } {
  if (status === 'offline') {
    return {
      title: 'Use Connect this computer',
      detail:
        'Copy the new setup text from Agents, paste it in the work folder on that computer, then come back here to send messages or tasks.',
    }
  }

  return {
    title: 'Keep this computer online',
    detail:
      "Keep that computer's command app open while it works. Use this page for quick messages, tracked tasks, or cleanup.",
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
  if (error === LOCAL_AGENT_CONTROL_FAILURE.sendInstruction) {
    return 'Open Agents, choose this agent again, confirm it still shows Ready, then resend the message. If it still fails, create a task instead or ask an owner or admin to check agent messaging.'
  }
  if (error === LOCAL_AGENT_CONTROL_FAILURE.startWorkspace) {
    return 'Go back to Agents, choose this agent again, then choose Start file work again. If it still does not show Ready, ask an owner or admin to check Where agents work in Settings.'
  }
  if (error === LOCAL_AGENT_CONTROL_FAILURE.restartWorkspace) {
    return "Open Agents, choose this agent again, then choose Restart agent again only if Tasks or Live work still shows no progress. If it keeps failing, ask an owner or admin to check this agent's connection and access in Agents."
  }
  if (error === LOCAL_AGENT_CONTROL_FAILURE.removeAgent) {
    return 'Open Agents, choose this agent again, then choose Remove agent again. If it keeps failing, ask an owner or admin to check your agent access.'
  }

  const normalized = error.toLowerCase()

  if (
    normalized.includes('permission') ||
    normalized.includes('forbidden') ||
    /\b403\b/.test(error)
  ) {
    return 'Ask an owner or admin to let you manage this agent, then try again. You do not have permission to change this agent.'
  }
  if (normalized.includes('unauthorized') || /\b401\b/.test(error)) {
    return 'Sign in again, reopen this agent, then try the action once more.'
  }
  if (normalized.includes('conflict') || /\b409\b/.test(error)) {
    return 'Open Agents, choose this agent again, confirm the latest status, then try again. This agent changed while you were working.'
  }
  if (normalized.includes('rate limit') || /\b429\b/.test(error)) {
    return 'Wait a moment, then open Agents and choose this agent again. The agent controls are busy.'
  }
  if (
    normalized === 'network error' ||
    normalized === 'failed to fetch' ||
    normalized.includes('networkerror')
  ) {
    return 'Check your connection, then open Agents and choose this agent again. Forge could not connect while changing this agent.'
  }
  if (/\b5\d\d\b/.test(error)) {
    return "Open Agents, choose this agent again, then try again. If it keeps failing, ask an owner or admin to check this agent's connection and access in Agents. Forge could not finish the change right now."
  }

  return "Open Agents, choose this agent again, and confirm the latest status before trying once more. If you started or restarted file work, wait for Ready or Working. If it keeps failing, ask an owner or admin to check your agent access and this agent's connection in Agents."
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
