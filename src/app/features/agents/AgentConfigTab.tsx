import { useEffect, useMemo, useState } from 'react'
import { FileText, RotateCcw, Save, Scissors, ShieldCheck, Sparkles } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
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
    label: 'Finish work',
    value:
      'You help finish assigned work. Ask early for missing information, keep changes scoped to the task you receive, preserve existing conventions, and report what you checked before sharing results.',
  },
  {
    id: 'result-check',
    label: 'Check results',
    value:
      'You check work before the team uses it. Start with anything that could break the result, create a security risk, or need a missing check. Explain the problem first, then point to the file or behavior that proves it.',
  },
  {
    id: 'sort-work',
    label: 'Sort work',
    value:
      "You help sort incoming work. Try the steps the user described, explain what happened in plain language, suggest the smallest safe next step, and ask for more information when it's needed.",
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
  return 'Open Agents, choose this simple chat agent again, then save again. If it keeps failing, ask an owner or admin to check your agent access. Answer guidance was not saved.'
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
  return label && !isMissingModelLabel(label)
    ? 'AI service choice selected'
    : 'Check AI service choice'
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
    } catch {
      setSaveError(promptProfileSaveErrorMessage())
    } finally {
      setSaving(false)
    }
  }

  const promptStatus = saveError
    ? saveError
    : saving
      ? 'Saving answer guidance…'
      : savedAt != null
        ? 'Answer guidance saved.'
        : dirty
          ? 'Unsaved changes. Save to use these instructions on future work.'
          : hasPrompt
            ? 'This agent already has saved guidance.'
            : 'Choose a template or write instructions before saving.'
  const updatePromptValue = (nextValue: string) => {
    setValue(nextValue)
    setSavedAt(null)
    if (saveError) {
      setSaveError(null)
    }
  }

  return (
    <div className={cn(uiStyles.cardPadded, 'flex flex-col gap-4 p-5')}>
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <Sparkles size={16} strokeWidth={2} className="text-apple-blue" aria-hidden="true" />
            <h2 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
              How this agent answers
            </h2>
          </div>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {agentAiServiceLabel(agent.provider)}. {modelLabel(agent.model)}
          </p>
        </div>
        <span className="inline-flex h-7 w-fit items-center gap-1.5 text-ui-body text-secondary-light dark:text-secondary-dark">
          <span
            className={cn(
              'h-1.5 w-1.5 rounded-full',
              dirty
                ? 'bg-apple-orange'
                : savedAt != null
                  ? 'bg-apple-blue'
                  : hasPrompt
                    ? 'bg-apple-green'
                    : 'bg-apple-gray-2'
            )}
          />
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
                'inline-flex h-8 items-center gap-2 rounded-button border px-3 text-ui-caption font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
                selected
                  ? 'border-black/[0.08] bg-black/[0.06] text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.08] dark:text-foreground-dark'
                  : 'border-black/[0.08] bg-white text-secondary-light hover:bg-black/[0.04] hover:text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark'
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
        className={cn(uiStyles.input, 'h-auto resize-none px-3 py-2')}
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
                ? 'Reset to the last saved guidance.'
                : 'Make an edit before reset is available.'
            }
            className={cn(uiStyles.secondaryButton, 'h-9 gap-2')}
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
            className={cn(uiStyles.dangerButton, 'h-9 gap-2')}
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
              ? 'Save this answer guidance for future work.'
              : 'Change the answer guidance before save is available.'
          }
          className={cn(uiStyles.primaryButton, 'h-9 gap-2 px-4')}
        >
          <Save size={14} strokeWidth={2} aria-hidden="true" />
          {saving ? 'Saving…' : 'Save answer guidance'}
        </button>
      </div>
    </div>
  )
}

function ConfigMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-card border border-black/[0.06] bg-black/[0.025] px-3 py-2 dark:border-white/[0.08] dark:bg-white/[0.04]">
      <p className="text-ui-caption font-medium uppercase tracking-normal text-secondary-light dark:text-secondary-dark">
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
      return 'Check tool selected in Settings'
    default:
      return 'Check tool selected in Settings'
  }
}

function CliRuntimeConfig({ agent }: { agent: AgentInfo }) {
  const hostCli = isHostCliAgent(agent)
  return (
    <div
      data-testid="agent-cli-config-summary"
      className={cn(uiStyles.cardPadded, 'flex flex-col gap-4 p-5')}
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
            This agent uses the saved tool selected in Settings. Confirm where it opens project
            files before sending Tasks or code changes.
          </p>
        </div>
        <span className={uiStyles.chip}>{hostCli ? 'This computer' : 'Project files'}</span>
      </div>

      <div className="grid gap-2 sm:grid-cols-2">
        <RuntimeRow label="Saved tool" value={cliToolLabel(agent.cliTool)} chip />
        <RuntimeRow
          label="Connection"
          value={
            hostCli
              ? agent.runtimeId
                ? 'Connected from this computer'
                : 'Open Agents and choose Connect this computer'
              : 'Ready with project files'
          }
        />
        <RuntimeRow
          label="Project for new tasks"
          value={agent.projectName ?? 'Open project settings first.'}
        />
        <RuntimeRow
          label="Folder agents open"
          value={agent.cwd ?? (hostCli ? 'Selected work folder' : 'Default project folder')}
        />
      </div>
    </div>
  )
}

function RuntimeRow({
  label,
  value,
  chip = false,
}: {
  label: string
  value: string
  chip?: boolean
}) {
  return (
    <div className="min-w-0 rounded-card bg-black/[0.03] px-3 py-2 text-ui-caption dark:bg-white/[0.04]">
      <span className="block text-secondary-light dark:text-secondary-dark">{label}</span>
      <span
        className={cn(
          'mt-0.5 truncate font-medium text-foreground-light dark:text-foreground-dark',
          chip ? uiStyles.chip : 'block'
        )}
      >
        {value}
      </span>
    </div>
  )
}
