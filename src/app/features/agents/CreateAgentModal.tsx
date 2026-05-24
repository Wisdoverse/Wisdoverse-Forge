import { useForm } from 'react-hook-form'
import { useEffect, useMemo, useRef, useState } from 'react'
import { Bug, ClipboardCheck, Code2, Plus, Search, type LucideIcon } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { useAgentsStore } from '@app/shared/model/agents.store'
import { useNavigationStore } from '@app/entities/navigation'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { LlmProviderConfig } from '@app/shared/api/legacy/settingsApi'
import type { CliTool } from '@shared/types'

type AgentKind = 'cli' | 'provider'

interface CreateAgentFormData {
  name: string
  kind: AgentKind
  cliTool: CliTool
  provider: string
  model: string
  cwd: string
  groupId: string
  systemPrompt: string
}

const CLI_TOOLS: { value: CliTool; label: string }[] = [
  { value: 'claude', label: 'Claude' },
  { value: 'codex', label: 'Codex' },
  { value: 'gemini', label: 'Gemini' },
  { value: 'opencode', label: 'OpenCode' },
]

interface AgentRoleTemplate {
  id: string
  label: string
  summary: string
  name: string
  systemPrompt: string
  Icon: LucideIcon
}

interface RuntimeFitSummary {
  title: string
  detail: string
  items: { label: string; value: string }[]
}

const AGENT_ROLE_TEMPLATES: AgentRoleTemplate[] = [
  {
    id: 'builder',
    label: 'Builder',
    summary: 'Implementation and tests',
    name: 'Builder Agent',
    systemPrompt:
      'You turn scoped requests into working changes. Keep edits narrow, explain tradeoffs when requirements conflict, and verify with the most relevant checks before handing work back.',
    Icon: Code2,
  },
  {
    id: 'reviewer',
    label: 'Reviewer',
    summary: 'Risk and release checks',
    name: 'Review Agent',
    systemPrompt:
      'You review changes for regressions, security issues, missing tests, and release risk. Lead with concrete findings and cite the exact files or checks that prove each point.',
    Icon: ClipboardCheck,
  },
  {
    id: 'investigator',
    label: 'Investigator',
    summary: 'Root-cause analysis',
    name: 'Investigation Agent',
    systemPrompt:
      'You investigate uncertain failures by gathering evidence first, separating facts from hypotheses, and ending with the smallest next action that can disprove or confirm the cause.',
    Icon: Search,
  },
  {
    id: 'fixer',
    label: 'Fixer',
    summary: 'Bug repair loop',
    name: 'Bug Fix Agent',
    systemPrompt:
      'You reproduce bugs, identify the smallest responsible path, patch the defect without unrelated refactors, and verify both the failing case and the nearby regression surface.',
    Icon: Bug,
  },
]

/// Providers the backend LLM gateway can route to. Models are free-text so new
/// model releases don't require a frontend redeploy.
const PROVIDERS: { value: string; label: string; defaultModel: string }[] = [
  { value: 'anthropic', label: 'Anthropic', defaultModel: 'claude-sonnet-4-6' },
  { value: 'openai', label: 'OpenAI', defaultModel: 'gpt-4o' },
  { value: 'google', label: 'Google', defaultModel: 'gemini-2.0-pro' },
  { value: 'ollama', label: 'Ollama (local)', defaultModel: 'llama3.2' },
  { value: 'groq', label: 'Groq', defaultModel: 'llama-3.3-70b-versatile' },
  { value: 'deepseek', label: 'DeepSeek', defaultModel: 'deepseek-chat' },
  { value: 'xai', label: 'xAI', defaultModel: 'grok-3-mini' },
  { value: 'openrouter', label: 'OpenRouter', defaultModel: 'openai/gpt-4o-mini' },
  { value: 'together', label: 'Together AI', defaultModel: 'openai/gpt-oss-20b' },
  {
    value: 'fireworks',
    label: 'Fireworks AI',
    defaultModel: 'accounts/fireworks/models/qwen3-30b-a3b',
  },
  { value: 'litellm', label: 'LiteLLM Gateway', defaultModel: 'gpt-4o-mini' },
  { value: 'openai_compatible', label: 'OpenAI-Compatible', defaultModel: '' },
]

const DEFAULT_AGENT_CWD = '/workspace'

function providerDefaultModel(provider: string): string {
  return PROVIDERS.find((candidate) => candidate.value === provider)?.defaultModel ?? ''
}

function providerLabel(provider: string): string {
  return PROVIDERS.find((candidate) => candidate.value === provider)?.label ?? provider
}

function cliToolLabel(cliTool: CliTool): string {
  return CLI_TOOLS.find((tool) => tool.value === cliTool)?.label ?? cliTool
}

function runtimeFitFor(kind: AgentKind, cliTool: CliTool, provider: string): RuntimeFitSummary {
  if (kind === 'cli') {
    return {
      title: `${cliToolLabel(cliTool)} container worker`,
      detail: 'Best when the task needs repository files, terminal tools, or local CLI sessions.',
      items: [
        { label: 'Execution', value: 'Container CLI' },
        { label: 'Files', value: '/workspace mounted' },
        { label: 'Before use', value: 'Runtime container must start' },
      ],
    }
  }

  return {
    title: `${providerLabel(provider)} prompt worker`,
    detail:
      'Best for planning, review, and lightweight coordination that does not need filesystem tools.',
    items: [
      { label: 'Execution', value: 'Provider API' },
      { label: 'Files', value: 'No direct workspace mount' },
      { label: 'Before use', value: 'Provider key must be ready' },
    ],
  }
}

function buildDefaultValues(provider: LlmProviderConfig | null): CreateAgentFormData {
  const providerKey = provider?.provider ?? PROVIDERS[0].value
  return {
    name: '',
    kind: provider ? 'provider' : 'cli',
    cliTool: 'claude',
    provider: providerKey,
    model: provider?.model || providerDefaultModel(providerKey),
    cwd: DEFAULT_AGENT_CWD,
    groupId: '',
    systemPrompt: '',
  }
}

export function CreateAgentModal() {
  const { createModalOpen, setCreateModalOpen, createAgent, loading, error, setError } =
    useAgentsStore()
  const providers = useSettingsStore((s) => s.providers)
  const selectedProjectId = useNavigationStore((s) => s.selectedProjectId)
  const projectsByTeam = useNavigationStore((s) => s.projects)
  const groups = useNavigationStore((s) => s.agentGroups)
  const createAgentGroup = useNavigationStore((s) => s.createAgentGroup)
  const [creatingGroup, setCreatingGroup] = useState(false)
  const verifiedProvider = useMemo(
    () =>
      providers.find((provider) => provider.isEnabled && provider.lastTestStatus === 'passed') ??
      null,
    [providers]
  )
  const defaultValues = useMemo(() => buildDefaultValues(verifiedProvider), [verifiedProvider])

  const { register, handleSubmit, reset, watch, setValue } = useForm<CreateAgentFormData>({
    defaultValues,
  })
  const [selectedTemplateId, setSelectedTemplateId] = useState<string | null>(null)
  const kind = watch('kind')
  const provider = watch('provider')
  const cliTool = watch('cliTool')
  const runtimeFit = runtimeFitFor(kind, cliTool, provider)
  const selectedProject = selectedProjectId
    ? (Object.values(projectsByTeam)
        .flat()
        .find((project) => project.id === selectedProjectId) ?? null)
    : null

  const dialogRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!createModalOpen) return
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        setCreateModalOpen(false)
        setError(null)
      }
    }
    document.addEventListener('keydown', handleKeyDown)
    return () => document.removeEventListener('keydown', handleKeyDown)
  }, [createModalOpen, setCreateModalOpen, setError])

  // Reset form when modal opens.
  useEffect(() => {
    if (!createModalOpen) return

    reset(defaultValues)
    setSelectedTemplateId(null)
    setError(null)
  }, [createModalOpen, defaultValues, reset, setError])

  // When the user switches provider, seed the model box with that provider's default
  // so the "OpenAI + claude-sonnet-4-6" mismatch doesn't happen by accident.
  useEffect(() => {
    if (provider === verifiedProvider?.provider) {
      setValue('model', verifiedProvider.model || providerDefaultModel(provider))
      return
    }
    const defaultModel = providerDefaultModel(provider)
    if (defaultModel) setValue('model', defaultModel)
  }, [provider, setValue, verifiedProvider])

  if (!createModalOpen) return null

  async function handleFormSubmit(data: CreateAgentFormData) {
    if (!data.name.trim()) {
      setError('Name is required')
      return
    }
    const base = {
      name: data.name.trim(),
      cwd: data.cwd || undefined,
      workspaceId: selectedProject?.workspaceId,
      projectId: selectedProjectId ?? undefined,
      groupId: data.groupId || undefined,
    }
    if (data.kind === 'provider') {
      if (!data.provider || !data.model.trim()) {
        setError('Provider and model are required')
        return
      }
      await createAgent({
        ...base,
        kind: 'provider',
        provider: data.provider,
        model: data.model.trim(),
        systemPrompt: data.systemPrompt.trim() || undefined,
      })
    } else {
      await createAgent({ ...base, kind: 'cli', cliTool: data.cliTool })
    }
  }

  async function handleCreateDefaultGroup() {
    if (!selectedProjectId) {
      setError('Select a project before creating a task group.')
      return
    }

    setCreatingGroup(true)
    setError(null)
    try {
      const group = await createAgentGroup(selectedProjectId, {
        name: 'Default Task Group',
        description: 'Agents in this group can receive tasks from the board.',
      })
      setValue('groupId', group.id, { shouldDirty: true })
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create task group')
    } finally {
      setCreatingGroup(false)
    }
  }

  function applyRoleTemplate(template: AgentRoleTemplate) {
    setSelectedTemplateId(template.id)
    setValue('name', template.name, { shouldDirty: true })
    setValue('systemPrompt', template.systemPrompt, { shouldDirty: true })
  }

  function handleClose() {
    setCreateModalOpen(false)
    setError(null)
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        className="absolute inset-0 bg-black/40 backdrop-blur-sm"
        onClick={handleClose}
        aria-hidden="true"
      />
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="create-agent-title"
        className={cn(
          'relative w-[480px] max-h-[80vh] overflow-y-auto',
          'rounded-panel border border-black/[0.08] bg-white p-6 dark:border-white/[0.1] dark:bg-[#2a2a2c]'
        )}
      >
        <div className="flex items-center justify-between mb-4">
          <h2
            id="create-agent-title"
            className="text-ui-title font-semibold text-foreground-light dark:text-foreground-dark"
          >
            New Agent
          </h2>
          <button
            type="button"
            onClick={handleClose}
            aria-label="Close dialog"
            className="text-ui-body text-secondary-light dark:text-secondary-dark"
          >
            ✕
          </button>
        </div>

        {error && (
          <div className="mb-4 rounded-lg bg-apple-red/10 px-3 py-2 text-ui-caption text-apple-red">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit(handleFormSubmit)} className="flex flex-col gap-4">
          <div>
            <div className="mb-2 flex items-center justify-between gap-2">
              <span className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                Role template
              </span>
              <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                {kind === 'provider' ? 'Prompt ready' : 'Name only for Container CLI'}
              </span>
            </div>
            <div
              role="group"
              aria-label="Agent role templates"
              className="grid gap-2 sm:grid-cols-2"
            >
              {AGENT_ROLE_TEMPLATES.map((template) => (
                <button
                  key={template.id}
                  type="button"
                  onClick={() => applyRoleTemplate(template)}
                  aria-pressed={selectedTemplateId === template.id}
                  className={cn(
                    'flex min-h-16 items-center gap-3 rounded-lg border px-3 py-2 text-left transition-colors',
                    selectedTemplateId === template.id
                      ? 'border-apple-blue/40 bg-apple-blue/10 text-foreground-light dark:text-foreground-dark'
                      : 'border-black/[0.08] bg-black/[0.02] text-foreground-light hover:bg-black/[0.04] dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.07]'
                  )}
                >
                  <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-white text-apple-blue shadow-sm dark:bg-black/20">
                    <template.Icon size={15} strokeWidth={2.25} aria-hidden="true" />
                  </span>
                  <span className="min-w-0">
                    <span className="block text-ui-button font-semibold">{template.label}</span>
                    <span className="block truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
                      {template.summary}
                    </span>
                  </span>
                </button>
              ))}
            </div>
          </div>

          <div>
            <label
              htmlFor="agent-name"
              className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
            >
              Name
            </label>
            <input
              id="agent-name"
              {...register('name', { required: true })}
              className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
              placeholder="e.g. Frontend Agent…"
              autoFocus
            />
          </div>

          <div>
            <label className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
              Agent kind
            </label>
            <div className="flex gap-2" role="radiogroup" aria-label="Agent kind">
              <label
                className={cn(
                  'flex-1 cursor-pointer rounded-full px-4 py-2 text-center text-ui-button font-medium transition-transform active:scale-95',
                  kind === 'cli'
                    ? 'bg-apple-blue text-white'
                    : 'border border-black/[0.08] bg-white text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark'
                )}
              >
                <input type="radio" value="cli" {...register('kind')} className="sr-only" />
                Container CLI
              </label>
              <label
                className={cn(
                  'flex-1 cursor-pointer rounded-full px-4 py-2 text-center text-ui-button font-medium transition-transform active:scale-95',
                  kind === 'provider'
                    ? 'bg-apple-blue text-white'
                    : 'border border-black/[0.08] bg-white text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark'
                )}
              >
                <input type="radio" value="provider" {...register('kind')} className="sr-only" />
                Provider + Prompt
              </label>
            </div>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              {kind === 'cli'
                ? 'Runs claude/codex/gemini/opencode inside a container.'
                : 'Calls the LLM provider directly — no container, no terminal.'}
            </p>
          </div>

          <section
            data-testid="agent-runtime-fit"
            className="rounded-lg border border-black/[0.06] bg-black/[0.025] px-3 py-2.5 dark:border-white/[0.08] dark:bg-white/[0.04]"
          >
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                  Runtime fit
                </p>
                <p className="mt-0.5 text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
                  {runtimeFit.title}
                </p>
              </div>
              <span className="shrink-0 rounded-full bg-apple-blue/10 px-2 py-0.5 text-ui-caption font-medium text-apple-blue">
                {kind === 'cli' ? 'File work' : 'Prompt work'}
              </span>
            </div>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              {runtimeFit.detail}
            </p>
            <div className="mt-2 grid gap-1.5 sm:grid-cols-3">
              {runtimeFit.items.map((item) => (
                <div
                  key={item.label}
                  className="min-w-0 rounded-md bg-white px-2 py-1.5 dark:bg-black/20"
                >
                  <span className="block text-[10px] font-medium text-secondary-light dark:text-secondary-dark">
                    {item.label}
                  </span>
                  <span className="mt-0.5 block truncate text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
                    {item.value}
                  </span>
                </div>
              ))}
            </div>
          </section>

          <div>
            <div className="mb-1 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
              Primary Project
            </div>
            <div className="w-full rounded-[18px] border border-black/[0.08] bg-white px-4 py-2 text-ui-body text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark">
              {selectedProject?.name ?? 'No primary project'}
            </div>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              {selectedProject
                ? 'Tasks default to this project. Container access is the selected project workspace.'
                : 'Tasks can still be assigned later. Container access uses the default workspace.'}
            </p>
          </div>

          {kind === 'cli' && (
            <div>
              <label
                htmlFor="agent-cli-tool"
                className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
              >
                Container CLI
              </label>
              <select
                id="agent-cli-tool"
                {...register('cliTool')}
                className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
              >
                {CLI_TOOLS.map((tool) => (
                  <option key={tool.value} value={tool.value}>
                    {tool.label}
                  </option>
                ))}
              </select>
            </div>
          )}

          {kind === 'provider' && (
            <>
              <div>
                <label
                  htmlFor="agent-provider"
                  className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                >
                  Provider
                </label>
                <select
                  id="agent-provider"
                  {...register('provider')}
                  className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                >
                  {PROVIDERS.map((p) => (
                    <option key={p.value} value={p.value}>
                      {p.label}
                    </option>
                  ))}
                </select>
              </div>
              <div>
                <label
                  htmlFor="agent-model"
                  className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                >
                  Model
                </label>
                <input
                  id="agent-model"
                  {...register('model', { required: true })}
                  className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                  placeholder="e.g. claude-sonnet-4-6…"
                />
              </div>
              <div>
                <label
                  htmlFor="systemPrompt"
                  className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                >
                  System prompt
                </label>
                <textarea
                  id="systemPrompt"
                  {...register('systemPrompt')}
                  rows={4}
                  placeholder="e.g. You are a concise, Pythonic code reviewer…"
                  className="w-full resize-none rounded-[18px] border border-black/[0.08] bg-white px-4 py-3 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                />
              </div>
            </>
          )}

          {kind === 'cli' && (
            <div>
              <label
                htmlFor="agent-cwd"
                className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
              >
                Working Directory
              </label>
              <input
                id="agent-cwd"
                {...register('cwd')}
                className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                placeholder={DEFAULT_AGENT_CWD}
              />
              <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                /workspace is the shared workspace mount and may contain multiple projects. Primary
                Project sets default context; it is not a private user directory.
              </p>
            </div>
          )}

          {selectedProjectId && (
            <div>
              <label
                htmlFor="agent-group"
                className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
              >
                Task Group
              </label>
              {groups.length > 0 ? (
                <>
                  <select
                    id="agent-group"
                    {...register('groupId')}
                    className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                  >
                    <option value="">No task group</option>
                    {groups.map((g) => (
                      <option key={g.id} value={g.id}>
                        {g.name}
                      </option>
                    ))}
                  </select>
                  <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                    Agents in a group can receive tasks from the board
                  </p>
                </>
              ) : (
                <button
                  type="button"
                  onClick={handleCreateDefaultGroup}
                  disabled={creatingGroup}
                  className={cn(
                    'flex h-10 w-full items-center justify-center gap-2 rounded-full px-4 text-ui-button font-medium transition-transform active:scale-95',
                    'border border-black/[0.08] bg-white text-apple-blue hover:bg-apple-blue/5',
                    'dark:bg-white/[0.06] dark:text-foreground-dark dark:hover:bg-white/[0.1]',
                    creatingGroup && 'cursor-not-allowed opacity-60'
                  )}
                >
                  <Plus size={14} strokeWidth={2.25} aria-hidden="true" />
                  {creatingGroup ? 'Creating…' : 'Create Task Group'}
                </button>
              )}
            </div>
          )}

          <div className="flex justify-end gap-2 mt-2">
            <button
              type="button"
              onClick={handleClose}
              className="rounded-full bg-surface-pearl px-4 py-2 text-ui-button font-medium text-foreground-light ring-1 ring-black/[0.04] transition-transform active:scale-95 dark:bg-white/[0.06] dark:text-foreground-dark"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={loading}
              className={cn(
                'rounded-full bg-apple-blue px-4 py-2 text-ui-button font-medium text-white',
                'transition-transform hover:bg-apple-blue-focus active:scale-95',
                loading && 'opacity-50 cursor-not-allowed'
              )}
            >
              {loading ? 'Creating…' : 'Create Agent'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
