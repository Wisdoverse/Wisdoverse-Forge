import { useEffect, useState } from 'react'
import { cn } from '@app/shared/lib/utils'
import { useAgentsStore } from '@app/shared/model/agents.store'

interface AgentConfigTabProps {
  agentId: string
}

export function AgentConfigTab({ agentId }: AgentConfigTabProps) {
  const agents = useAgentsStore((s) => s.agents)
  const updateAgentSystemPrompt = useAgentsStore((s) => s.updateAgentSystemPrompt)
  const agent = agents.find((a) => a.id === agentId)

  const initial = agent?.systemPrompt ?? ''
  const [value, setValue] = useState(initial)
  const [saving, setSaving] = useState(false)
  const [savedAt, setSavedAt] = useState<number | null>(null)

  // Keep local state in sync when store updates (e.g. after save refresh).
  useEffect(() => {
    setValue(agent?.systemPrompt ?? '')
  }, [agent?.systemPrompt])

  if (!agent) {
    return <div className="text-ui-body text-secondary-light">Agent not found.</div>
  }

  if (agent.cliTool) {
    return (
      <div
        className={cn(
          'rounded-card border border-black/[0.08] bg-white px-4 py-6 dark:border-white/[0.1] dark:bg-[#2a2a2c]',
          'text-center text-ui-body text-secondary-light dark:text-secondary-dark'
        )}
      >
        System prompt edit is only available for provider+prompt agents.
      </div>
    )
  }

  async function handleSave() {
    if (saving) return
    setSaving(true)
    try {
      // Empty string clears the stored prompt (backend T12 `CASE WHEN $6 = '' THEN NULL`).
      await updateAgentSystemPrompt(agentId, value.trim())
      setSavedAt(Date.now())
    } finally {
      setSaving(false)
    }
  }

  const dirty = value !== initial

  return (
    <div
      className={cn(
        'flex flex-col gap-3 rounded-card border border-black/[0.08] bg-white p-6 dark:border-white/[0.1] dark:bg-[#2a2a2c]'
      )}
    >
      <div className="flex items-center justify-between">
        <label
          htmlFor="config-system-prompt"
          className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
        >
          System prompt
        </label>
        {savedAt != null && !dirty && (
          <span className="text-ui-caption text-apple-blue">Saved</span>
        )}
      </div>
      <textarea
        id="config-system-prompt"
        rows={6}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        placeholder="e.g. You are a concise, Pythonic code reviewer."
        className={cn(
          'w-full resize-none rounded-lg px-3 py-2 text-ui-body outline-none',
          'border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-white/[0.04]',
          'text-foreground-light dark:text-foreground-dark',
          'focus:ring-2 focus:ring-apple-blue-focus'
        )}
      />
      <div className="flex justify-end gap-2">
        <button
          type="button"
          onClick={handleSave}
          disabled={saving || !dirty}
          className={cn(
            'rounded-full bg-apple-blue px-4 py-2 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus active:scale-95',
            (saving || !dirty) && 'cursor-not-allowed opacity-50'
          )}
        >
          {saving ? 'Saving…' : 'Save'}
        </button>
      </div>
    </div>
  )
}
