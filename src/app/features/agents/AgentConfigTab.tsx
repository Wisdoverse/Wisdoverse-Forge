import { useEffect, useMemo, useState } from 'react'
import { FileText, RotateCcw, Save, Scissors, ShieldCheck, Sparkles } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import {
  agentAiServiceLabel,
  isHostCliAgent,
  useAgentsStore,
  type AgentInfo,
} from '@app/entities/agent'

interface AgentConfigTabProps {
  agentId: string
}

interface PromptStats {
  characters: number
  words: number
  lines: number
}

const PROMPT_TEMPLATES = [
  {
    id: 'delivery',
    label: 'Delivery',
    value:
      'You are a delivery-focused agent. Ask early for missing information, keep changes scoped to the assigned task, preserve existing conventions, and report what you checked before sharing results.',
  },
  {
    id: 'review',
    label: 'Review',
    value:
      'You are a code review agent. Prioritize correctness, regressions, security, missing tests, and unclear ownership. Lead with concrete findings and cite the exact files or behavior that prove each issue.',
  },
  {
    id: 'triage',
    label: 'Sort work',
    value:
      'You help sort incoming work. Recreate the reported behavior, separate symptoms from likely cause, identify the smallest safe fix, and leave a clear next action when more information is needed.',
  },
]

function promptStats(value: string): PromptStats {
  const trimmed = value.trim()
  return {
    characters: value.length,
    words: trimmed ? trimmed.split(/\s+/).length : 0,
    lines: value ? value.split('\n').length : 0,
  }
}

function promptProfileSaveErrorMessage(): string {
  return 'Refresh this agent, confirm it is still a chat-only agent, then save again. If it keeps failing, ask an owner or admin to check your agent access. Agent instructions were not saved.'
}

function isMissingModelLabel(label: string): boolean {
  const normalized = label.toLowerCase()
  return (
    normalized === 'unknown' ||
    (normalized.includes('model') && normalized.includes('not reported'))
  )
}

function modelLabel(model?: string | null): string {
  const label = model?.trim()
  return label && !isMissingModelLabel(label) ? 'AI model selected' : 'Refresh AI model'
}

export function AgentConfigTab({ agentId }: AgentConfigTabProps) {
  const agents = useAgentsStore((s) => s.agents)
  const updateAgentSystemPrompt = useAgentsStore((s) => s.updateAgentSystemPrompt)
  const agent = agents.find((a) => a.id === agentId)

  const initial = agent?.systemPrompt ?? ''
  const [value, setValue] = useState(initial)
  const [saving, setSaving] = useState(false)
  const [saveError, setSaveError] = useState<string | null>(null)
  const [savedAt, setSavedAt] = useState<number | null>(null)
  const stats = useMemo(() => promptStats(value), [value])

  // Keep local state in sync when store updates (e.g. after save refresh).
  useEffect(() => {
    setValue(agent?.systemPrompt ?? '')
  }, [agent?.systemPrompt])

  if (!agent) {
    return (
      <div className="text-ui-body text-secondary-light">
        This agent could not be found. Open Agents, choose a current agent, then return to settings.
      </div>
    )
  }

  if (agent.cliTool) {
    return <CliRuntimeConfig agent={agent} />
  }

  const dirty = value !== initial
  const hasPrompt = value.trim().length > 0
  const activeTemplateId = PROMPT_TEMPLATES.find((template) => value === template.value)?.id ?? null
  const promptHelpId = `${agentId}-system-prompt-help`
  const promptStatusId = `${agentId}-system-prompt-status`

  async function handleSave() {
    if (saving || !dirty) return
    setSaving(true)
    setSaveError(null)
    try {
      // Empty string clears the stored prompt (backend T12 `CASE WHEN $6 = '' THEN NULL`).
      const saved = await updateAgentSystemPrompt(agentId, value.trim())
      if (saved) {
        setSavedAt(Date.now())
      } else {
        setSaveError(promptProfileSaveErrorMessage())
      }
    } finally {
      setSaving(false)
    }
  }

  const promptStatus = saveError
    ? saveError
    : saving
      ? 'Saving agent instructions…'
      : dirty
        ? 'Unsaved changes. Save to use these instructions on future work.'
        : savedAt != null
          ? 'Agent instructions saved.'
          : hasPrompt
            ? 'This agent already has saved instructions.'
            : 'Choose a template or write instructions before saving.'
  const updatePromptValue = (nextValue: string) => {
    setValue(nextValue)
    if (saveError) {
      setSaveError(null)
    }
  }

  return (
    <div
      className={cn(
        'flex flex-col gap-4 rounded-card border border-black/[0.08] bg-white p-5 dark:border-white/[0.1] dark:bg-[#2a2a2c]'
      )}
    >
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <Sparkles size={16} strokeWidth={2} className="text-apple-blue" aria-hidden="true" />
            <h2 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
              Agent instructions
            </h2>
          </div>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {agentAiServiceLabel(agent.provider)}. {modelLabel(agent.model)}
          </p>
        </div>
        <span
          className={cn(
            'inline-flex h-7 w-fit items-center rounded-full px-2.5 text-ui-caption font-medium',
            dirty
              ? 'bg-apple-orange/10 text-apple-orange'
              : savedAt != null
                ? 'bg-apple-blue/10 text-apple-blue'
                : hasPrompt
                  ? 'bg-apple-green/10 text-apple-green'
                  : 'bg-black/[0.05] text-secondary-light dark:bg-white/[0.08] dark:text-secondary-dark'
          )}
        >
          {dirty
            ? 'Unsaved'
            : savedAt != null
              ? 'Saved'
              : hasPrompt
                ? 'Has instructions'
                : 'Add instructions'}
        </span>
      </div>

      <div data-testid="agent-config-summary" className="grid grid-cols-3 gap-2">
        <ConfigMetric label="Words" value={String(stats.words)} />
        <ConfigMetric label="Lines" value={String(stats.lines)} />
        <ConfigMetric label="Characters" value={String(stats.characters)} />
      </div>

      <div className="flex flex-wrap gap-2" role="group" aria-label="Instruction templates">
        {PROMPT_TEMPLATES.map((template) => {
          const selected = activeTemplateId === template.id
          return (
            <button
              key={template.id}
              type="button"
              aria-pressed={selected}
              onClick={() => updatePromptValue(template.value)}
              className={cn(
                'inline-flex h-8 items-center gap-2 rounded-lg border px-3 text-ui-caption font-medium transition-colors hover:border-apple-blue/35 hover:text-apple-blue focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
                selected
                  ? 'border-apple-blue/50 bg-apple-blue/10 text-apple-blue dark:border-apple-blue/60 dark:bg-apple-blue/15'
                  : 'border-black/[0.08] bg-white text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.05] dark:text-foreground-dark'
              )}
            >
              <FileText size={13} strokeWidth={2} aria-hidden="true" />
              {template.label}
            </button>
          )
        })}
      </div>

      <div className="flex flex-col gap-1">
        <label
          htmlFor="config-system-prompt"
          className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
        >
          Instructions for this agent
        </label>
        <p
          id={promptHelpId}
          className="text-ui-caption text-secondary-light dark:text-secondary-dark"
        >
          Start from a template or write everyday instructions. Tell this agent the outcome, what to
          avoid, and how to report progress.
        </p>
      </div>
      <textarea
        id="config-system-prompt"
        name="systemPrompt"
        autoComplete="off"
        rows={9}
        value={value}
        onChange={(e) => updatePromptValue(e.target.value)}
        aria-describedby={`${promptHelpId} ${promptStatusId}`}
        placeholder="Example: Report progress in plain language, protect existing work, and list checks run…"
        className={cn(
          'w-full resize-none rounded-lg px-3 py-2 text-ui-body outline-none',
          'border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-white/[0.04]',
          'text-foreground-light dark:text-foreground-dark',
          'focus:ring-2 focus:ring-apple-blue-focus'
        )}
      />
      <p
        id={promptStatusId}
        role={saveError ? 'alert' : 'status'}
        aria-live="polite"
        className={cn(
          'text-ui-caption',
          saveError ? 'text-apple-red' : 'text-secondary-light dark:text-secondary-dark'
        )}
      >
        {promptStatus}
      </p>
      <div className="flex flex-col gap-2 sm:flex-row sm:justify-between">
        <div className="flex gap-2">
          <button
            type="button"
            onClick={() => updatePromptValue(initial)}
            disabled={!dirty}
            title={
              dirty
                ? 'Reset to the last saved instructions.'
                : 'Make an edit before reset is available.'
            }
            className={cn(
              'inline-flex h-9 items-center gap-2 rounded-lg border border-black/[0.08] bg-white px-3 text-ui-button font-medium text-foreground-light transition-colors hover:border-apple-blue/35 hover:text-apple-blue focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.05] dark:text-foreground-dark',
              !dirty && 'cursor-not-allowed opacity-50'
            )}
          >
            <RotateCcw size={14} strokeWidth={2} aria-hidden="true" />
            Reset
          </button>
          <button
            type="button"
            onClick={() => updatePromptValue('')}
            disabled={!hasPrompt}
            title={
              hasPrompt
                ? 'Clear the instruction text.'
                : 'Add instructions before clear is available.'
            }
            className={cn(
              'inline-flex h-9 items-center gap-2 rounded-lg border border-black/[0.08] bg-white px-3 text-ui-button font-medium text-foreground-light transition-colors hover:border-apple-red/35 hover:text-apple-red focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.05] dark:text-foreground-dark',
              !hasPrompt && 'cursor-not-allowed opacity-50'
            )}
          >
            <Scissors size={14} strokeWidth={2} aria-hidden="true" />
            Clear
          </button>
        </div>
        <button
          type="button"
          onClick={handleSave}
          disabled={saving || !dirty}
          title={
            dirty
              ? 'Save these instructions for future work.'
              : 'Change the instructions before save is available.'
          }
          className={cn(
            'inline-flex h-9 items-center justify-center gap-2 rounded-lg bg-apple-blue px-4 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus active:scale-95',
            (saving || !dirty) && 'cursor-not-allowed opacity-50'
          )}
        >
          <Save size={14} strokeWidth={2} aria-hidden="true" />
          {saving ? 'Saving…' : 'Save Instructions'}
        </button>
      </div>
    </div>
  )
}

function ConfigMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-black/[0.06] bg-black/[0.025] px-3 py-2 dark:border-white/[0.08] dark:bg-white/[0.04]">
      <p className="text-[10px] font-medium uppercase tracking-normal text-secondary-light dark:text-secondary-dark">
        {label}
      </p>
      <p className="mt-1 text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
        {value}
      </p>
    </div>
  )
}

function cliToolLabel(tool?: AgentInfo['cliTool'] | string): string {
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
      return 'Refresh work tool setup'
    default:
      return 'Check work tool setup'
  }
}

function CliRuntimeConfig({ agent }: { agent: AgentInfo }) {
  const hostCli = isHostCliAgent(agent)
  return (
    <div
      data-testid="agent-cli-config-summary"
      className={cn(
        'flex flex-col gap-4 rounded-card border border-black/[0.08] bg-white p-5 dark:border-white/[0.1] dark:bg-[#2a2a2c]'
      )}
    >
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <ShieldCheck
              size={16}
              strokeWidth={2}
              className="text-apple-green"
              aria-hidden="true"
            />
            <h2 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
              Where this agent works
            </h2>
          </div>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            This agent follows the setup for its work tool. Confirm where it can open files before
            assigning work.
          </p>
        </div>
        <span className="inline-flex h-7 w-fit items-center rounded-full bg-apple-blue/10 px-2.5 text-ui-caption font-medium text-apple-blue">
          {hostCli ? 'This computer' : 'Project files'}
        </span>
      </div>

      <div className="grid gap-2 sm:grid-cols-2">
        <RuntimeRow label="Work tool" value={cliToolLabel(agent.cliTool)} />
        <RuntimeRow
          label="Connection"
          value={
            hostCli
              ? agent.runtimeId
                ? 'Connected from this computer'
                : 'Open setup again for this computer'
              : 'Ready with project files'
          }
        />
        <RuntimeRow
          label="Starting project"
          value={agent.projectName ?? 'Open project settings first.'}
        />
        <RuntimeRow
          label="Starting folder"
          value={agent.cwd ?? (hostCli ? 'Folder selected during setup' : 'Default project folder')}
        />
      </div>
    </div>
  )
}

function RuntimeRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 rounded-lg bg-black/[0.03] px-3 py-2 text-ui-caption dark:bg-white/[0.04]">
      <span className="block text-secondary-light dark:text-secondary-dark">{label}</span>
      <span className="mt-0.5 block truncate font-medium text-foreground-light dark:text-foreground-dark">
        {value}
      </span>
    </div>
  )
}
