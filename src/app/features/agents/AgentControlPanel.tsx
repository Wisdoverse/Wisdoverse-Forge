import { useState } from 'react'
import { cn } from '@app/shared/lib/utils'
import { isHostCliAgent, useAgentsStore, type AgentInfo } from '@app/shared/model/agents.store'

interface AgentControlPanelProps {
  agent: AgentInfo
  onDeleted: () => void
}

export function AgentControlPanel({ agent, onDeleted }: AgentControlPanelProps) {
  const { sendPrompt, startAgent, restartAgent, deleteAgent, error } = useAgentsStore()

  const [prompt, setPrompt] = useState('')
  const [sending, setSending] = useState(false)
  const [starting, setStarting] = useState(false)
  const [confirmRestart, setConfirmRestart] = useState(false)
  const [confirmDelete, setConfirmDelete] = useState(false)

  const hostCli = isHostCliAgent(agent)
  const canStartContainer = Boolean(agent.cliTool && !agent.containerId && !hostCli)
  const canRestartContainer = Boolean(agent.cliTool && agent.containerId && !hostCli)

  async function handleSendPrompt() {
    if (!prompt.trim() || sending) return
    setSending(true)
    const ok = await sendPrompt(agent.id, prompt.trim())
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
      {/* Error display */}
      {error && (
        <div className="rounded-lg bg-apple-red/10 px-3 py-2 text-ui-caption text-apple-red">
          {error}
        </div>
      )}

      {/* Prompt input */}
      <div
        className={cn(
          'rounded-card border border-black/[0.08] bg-white p-6 dark:border-white/[0.1] dark:bg-[#2a2a2c]',
          'flex flex-col gap-2'
        )}
      >
        <label className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
          Send Prompt
        </label>
        <textarea
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          rows={3}
          className="w-full resize-none rounded-[18px] border border-black/[0.08] bg-white px-4 py-3 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
          placeholder="Enter a prompt for this agent…"
          onKeyDown={(e) => {
            if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
              e.preventDefault()
              void handleSendPrompt()
            }
          }}
        />
        <div className="flex justify-end">
          <button
            type="button"
            onClick={handleSendPrompt}
            disabled={!prompt.trim() || sending}
            className={cn(
              'rounded-full bg-apple-blue px-4 py-2 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus active:scale-95',
              (!prompt.trim() || sending) && 'opacity-50 cursor-not-allowed'
            )}
          >
            {sending ? 'Sending…' : 'Send'}
          </button>
        </div>
      </div>

      {/* Action buttons */}
      <div className="flex gap-2">
        {/* Start / Restart */}
        {(canStartContainer || canRestartContainer) && (
          <div className="flex-1">
            {canStartContainer ? (
              <button
                type="button"
                onClick={handleStart}
                disabled={starting}
                className={cn(
                  'w-full rounded-full px-3 py-2 text-ui-button font-medium',
                  'border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-[#2a2a2c]',
                  'text-apple-blue hover:bg-apple-blue/5 transition-transform active:scale-95',
                  starting && 'opacity-50 cursor-not-allowed'
                )}
              >
                {starting ? 'Starting…' : 'Start Agent'}
              </button>
            ) : confirmRestart ? (
              <div
                className={cn(
                  'flex items-center gap-2 px-3 py-2 rounded-xl',
                  'border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-[#2a2a2c]'
                )}
              >
                <span className="flex-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                  Restart this agent?
                </span>
                <button
                  type="button"
                  onClick={handleRestart}
                  className="rounded-full bg-apple-blue px-3 py-1.5 text-ui-button font-medium text-white hover:bg-apple-blue-focus"
                >
                  Confirm
                </button>
                <button
                  type="button"
                  onClick={() => setConfirmRestart(false)}
                  className="rounded-full bg-apple-gray-5 px-2.5 py-1 text-ui-button font-medium text-foreground-light dark:bg-white/[0.06] dark:text-foreground-dark"
                >
                  Cancel
                </button>
              </div>
            ) : (
              <button
                type="button"
                onClick={() => setConfirmRestart(true)}
                className={cn(
                  'w-full rounded-full px-3 py-2 text-ui-button font-medium',
                  'border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-[#2a2a2c]',
                  'text-apple-blue hover:bg-apple-blue/5 transition-transform active:scale-95'
                )}
              >
                Restart Agent
              </button>
            )}
          </div>
        )}

        {/* Delete */}
        <div className="flex-1">
          {confirmDelete ? (
            <div
              className={cn(
                'flex items-center gap-2 px-3 py-2 rounded-xl',
                'border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-[#2a2a2c]'
              )}
            >
              <span className="flex-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                Delete permanently?
              </span>
              <button
                type="button"
                onClick={handleDelete}
                className="rounded-full bg-apple-red px-2.5 py-1 text-ui-button font-medium text-white hover:bg-apple-red/90"
              >
                Delete
              </button>
              <button
                type="button"
                onClick={() => setConfirmDelete(false)}
                className="rounded-full bg-apple-gray-5 px-2.5 py-1 text-ui-button font-medium text-foreground-light dark:bg-white/[0.06] dark:text-foreground-dark"
              >
                Cancel
              </button>
            </div>
          ) : (
            <button
              type="button"
              onClick={() => setConfirmDelete(true)}
              className={cn(
                'w-full rounded-full px-3 py-2 text-ui-button font-medium',
                'border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-[#2a2a2c]',
                'text-apple-red hover:bg-apple-red/5 transition-colors'
              )}
            >
              Delete Agent
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
