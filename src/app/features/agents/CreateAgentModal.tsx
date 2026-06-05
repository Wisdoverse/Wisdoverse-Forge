import { useForm } from 'react-hook-form'
import { useEffect, useMemo, useRef, useState } from 'react'
import {
  Bug,
  Check,
  ClipboardCheck,
  Code2,
  Copy,
  Plus,
  Search,
  type LucideIcon,
} from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { useAgentsStore } from '@app/entities/agent'
import { useNavigationStore } from '@app/entities/navigation'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { LocalAgentEnrollmentResponse } from '@app/entities/agent'
import type { LlmProviderConfig } from '@app/shared/api/legacy/settingsApi'
import type { CliTool } from '@shared/types'
import { createAgentWorkLaneErrorMessage } from './model/createAgentWorkLaneErrorMessage'

type AgentKind = 'cli' | 'local-cli' | 'provider'

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
      title: `${cliToolLabel(cliTool)} in a managed workspace`,
      detail: 'Best when the task needs project files and a ready workspace to run commands.',
      items: [
        { label: 'Work type', value: 'Managed workspace' },
        { label: 'Files', value: 'Project files available' },
        { label: 'Before use', value: 'Workspace must be online' },
      ],
    }
  }

  if (kind === 'local-cli') {
    return {
      title: `${cliToolLabel(cliTool)} on this computer`,
      detail:
        'Best when the work tool already runs on this computer and the platform should manage identity and tasks.',
      items: [
        { label: 'Work type', value: 'This computer' },
        { label: 'Files', value: 'Your local folder' },
        { label: 'Before use', value: 'Run the join command' },
      ],
    }
  }

  return {
    title: `${providerLabel(provider)} text-only model`,
    detail:
      'Best for planning, review, and lightweight coordination that does not need filesystem tools.',
    items: [
      { label: 'Work type', value: 'Text-only model' },
      { label: 'Files', value: 'No file access' },
      { label: 'Before use', value: 'Model service checked' },
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
  const {
    createModalOpen,
    setCreateModalOpen,
    createAgent,
    enrollLocalAgent,
    loading,
    error,
    setError,
  } = useAgentsStore()
  const providers = useSettingsStore((s) => s.providers)
  const selectedProjectId = useNavigationStore((s) => s.selectedProjectId)
  const projectsByTeam = useNavigationStore((s) => s.projects)
  const groups = useNavigationStore((s) => s.agentGroups)
  const createAgentGroup = useNavigationStore((s) => s.createAgentGroup)
  const [creatingGroup, setCreatingGroup] = useState(false)
  const [localEnrollment, setLocalEnrollment] = useState<LocalAgentEnrollmentResponse | null>(null)
  const [copiedCommand, setCopiedCommand] = useState(false)
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
  const cwd = watch('cwd')
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
    setLocalEnrollment(null)
    setCopiedCommand(false)
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

  useEffect(() => {
    if (kind === 'local-cli' && cwd === DEFAULT_AGENT_CWD) {
      setValue('cwd', '')
      return
    }
    if (kind === 'cli' && !cwd) {
      setValue('cwd', DEFAULT_AGENT_CWD)
    }
  }, [cwd, kind, setValue])

  if (!createModalOpen) return null

  async function handleFormSubmit(data: CreateAgentFormData) {
    if (!data.name.trim()) {
      setError('Name this agent before creating it. Example: Review Agent or File Work Agent.')
      return
    }
    const base = {
      name: data.name.trim(),
      workspaceId: selectedProject?.workspaceId,
      projectId: selectedProjectId ?? undefined,
      groupId: data.groupId || undefined,
    }
    if (data.kind === 'provider') {
      if (!data.provider || !data.model.trim()) {
        setError('Choose a model service and model before creating this text-only model agent.')
        return
      }
      await createAgent({
        ...base,
        kind: 'provider',
        provider: data.provider,
        model: data.model.trim(),
        systemPrompt: data.systemPrompt.trim() || undefined,
      })
    } else if (data.kind === 'local-cli') {
      const enrollment = await enrollLocalAgent({
        name: data.name.trim(),
        cliTool: data.cliTool,
        cwd: data.cwd.trim() || undefined,
        workspaceId: selectedProject?.workspaceId,
        projectId: selectedProjectId ?? undefined,
      })
      if (enrollment) {
        setLocalEnrollment(enrollment)
        setCopiedCommand(false)
      }
    } else {
      await createAgent({ ...base, kind: 'cli', cliTool: data.cliTool, cwd: data.cwd || undefined })
    }
  }

  async function handleCreateDefaultGroup() {
    if (!selectedProjectId) {
      setError('Select a project before creating a work lane. Work lanes belong to one project.')
      return
    }

    setCreatingGroup(true)
    setError(null)
    try {
      const group = await createAgentGroup(selectedProjectId, {
        name: 'Default Work Lane',
        description: 'This work lane lets agents receive board tasks.',
      })
      setValue('groupId', group.id, { shouldDirty: true })
    } catch (err) {
      setError(createAgentWorkLaneErrorMessage(err))
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
    setLocalEnrollment(null)
    setCopiedCommand(false)
  }

  async function handleCopyCommand() {
    const command = localEnrollment?.enrollment?.shellExports
    if (!command || !navigator.clipboard?.writeText) return
    try {
      await navigator.clipboard.writeText(command)
      setCopiedCommand(true)
    } catch {
      setCopiedCommand(false)
    }
  }

  function handleCreateAnother() {
    setLocalEnrollment(null)
    setCopiedCommand(false)
    setSelectedTemplateId(null)
    reset(defaultValues)
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
          'relative max-h-[calc(100vh-2rem)] w-[480px] max-w-[calc(100vw-2rem)] overflow-y-auto sm:max-h-[80vh]',
          'rounded-panel border border-black/[0.08] bg-white p-6 dark:border-white/[0.1] dark:bg-[#2a2a2c]'
        )}
      >
        <div className="flex items-center justify-between mb-4">
          <h2
            id="create-agent-title"
            className="text-ui-title font-semibold text-foreground-light dark:text-foreground-dark"
          >
            {localEnrollment ? 'Join agent on this computer' : 'New agent'}
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
          <div
            role="alert"
            className="mb-4 rounded-lg bg-apple-red/10 px-3 py-2 text-ui-caption text-apple-red"
          >
            {error}
          </div>
        )}

        {localEnrollment ? (
          <div className="flex flex-col gap-4">
            <div className="rounded-lg border border-black/[0.08] bg-surface-pearl p-4 dark:border-white/[0.1] dark:bg-white/[0.04]">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <div className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                    Managed agent
                  </div>
                  <div className="mt-1 text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
                    {localEnrollment.agent?.name ?? 'Local agent'}
                  </div>
                </div>
                <span className="rounded-full border border-apple-green/20 bg-white px-2.5 py-1 text-ui-caption text-apple-green dark:bg-white/[0.04]">
                  This computer
                </span>
              </div>
              <p className="mt-3 text-ui-caption text-secondary-light dark:text-secondary-dark">
                Copy the full command below and run it in the project folder on this computer. The
                agent will appear online after the connection tool starts.
              </p>
            </div>

            <div className="rounded-lg border border-black/[0.08] bg-white px-4 py-3 dark:border-white/[0.1] dark:bg-white/[0.04]">
              <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                What to do next
              </p>
              <ol className="mt-2 list-decimal space-y-1 pl-4 text-ui-caption text-secondary-light dark:text-secondary-dark">
                <li>Paste the command into the terminal for this project folder.</li>
                <li>Keep that terminal open while the agent is working.</li>
                <li>If you close it, run the same command again to reconnect this agent.</li>
              </ol>
            </div>

            <div>
              <label
                htmlFor="local-agent-command"
                className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
              >
                Join command
              </label>
              <textarea
                id="local-agent-command"
                readOnly
                value={localEnrollment.enrollment?.shellExports ?? ''}
                rows={8}
                className="w-full resize-none rounded-[18px] border border-black/[0.08] bg-white px-4 py-3 font-mono text-ui-caption text-foreground-light outline-none dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
              />
            </div>

            <div className="flex flex-wrap justify-end gap-2">
              <button
                type="button"
                onClick={handleCreateAnother}
                className="rounded-full bg-surface-pearl px-4 py-2 text-ui-button font-medium text-foreground-light ring-1 ring-black/[0.04] transition-transform active:scale-95 dark:bg-white/[0.06] dark:text-foreground-dark"
              >
                Create another
              </button>
              <button
                type="button"
                onClick={() => void handleCopyCommand()}
                className="inline-flex items-center gap-2 rounded-full bg-apple-blue px-4 py-2 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus active:scale-95"
              >
                {copiedCommand ? (
                  <Check size={14} strokeWidth={2.25} aria-hidden="true" />
                ) : (
                  <Copy size={14} strokeWidth={2.25} aria-hidden="true" />
                )}
                {copiedCommand ? 'Copied' : 'Copy command'}
              </button>
              <button
                type="button"
                onClick={handleClose}
                className="rounded-full bg-apple-gray-5 px-4 py-2 text-ui-button font-medium text-foreground-light transition-transform active:scale-95 dark:bg-white/[0.06] dark:text-foreground-dark"
              >
                Done
              </button>
            </div>
          </div>
        ) : (
          <form onSubmit={handleSubmit(handleFormSubmit)} className="flex flex-col gap-4">
            <div>
              <div className="mb-2 flex items-center justify-between gap-2">
                <span className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                  Starter role
                </span>
                <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                  {kind === 'provider'
                    ? 'Adds a starter name and instructions'
                    : 'Adds a starter name for file work'}
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
                {...register('name')}
                className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                placeholder="e.g. Review work agent"
                autoFocus
              />
            </div>

            <div>
              <label className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                Work type
              </label>
              <div className="flex gap-2" role="radiogroup" aria-label="Agent work type">
                <label
                  className={cn(
                    'flex-1 cursor-pointer rounded-full px-4 py-2 text-center text-ui-button font-medium transition-transform active:scale-95',
                    kind === 'cli'
                      ? 'bg-apple-blue text-white'
                      : 'border border-black/[0.08] bg-white text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark'
                  )}
                >
                  <input type="radio" value="cli" {...register('kind')} className="sr-only" />
                  Managed workspace
                </label>
                <label
                  className={cn(
                    'flex-1 cursor-pointer rounded-full px-4 py-2 text-center text-ui-button font-medium transition-transform active:scale-95',
                    kind === 'local-cli'
                      ? 'bg-apple-blue text-white'
                      : 'border border-black/[0.08] bg-white text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark'
                  )}
                >
                  <input type="radio" value="local-cli" {...register('kind')} className="sr-only" />
                  This computer
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
                  Text-only model
                </label>
              </div>
              <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                {kind === 'cli'
                  ? 'Runs the selected work tool in a managed project workspace.'
                  : kind === 'local-cli'
                    ? 'Runs a local work tool on your computer while this platform manages identity and tasks.'
                    : 'Uses a connected model for text-only work; no files or terminal.'}
              </p>
            </div>

            <section
              data-testid="agent-runtime-fit"
              className="rounded-lg border border-black/[0.06] bg-black/[0.025] px-3 py-2.5 dark:border-white/[0.08] dark:bg-white/[0.04]"
            >
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                    Work fit
                  </p>
                  <p className="mt-0.5 text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
                    {runtimeFit.title}
                  </p>
                </div>
                <span className="shrink-0 rounded-full bg-apple-blue/10 px-2 py-0.5 text-ui-caption font-medium text-apple-blue">
                  {kind === 'cli'
                    ? 'File work'
                    : kind === 'local-cli'
                      ? 'Local work'
                      : 'Text-only work'}
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

            <div data-testid="agent-work-readiness">
              <div className="mb-1 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                Starting Project
              </div>
              <div className="w-full rounded-[18px] border border-black/[0.08] bg-white px-4 py-2 text-ui-body text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark">
                {selectedProject?.name ?? 'Choose a starting project'}
              </div>
              <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                {selectedProject
                  ? kind === 'local-cli'
                    ? 'Project ready. New tasks start in this project. File access stays on the joined computer.'
                    : 'Project ready. New tasks start in this project. File access uses the selected project workspace.'
                  : kind === 'local-cli'
                    ? 'Choose a starting project first. Tasks can still be assigned later. Select a project in the sidebar before creating.'
                    : 'Choose a starting project first. Tasks can still be assigned later. Select a project in the sidebar to set the work area.'}
              </p>
            </div>

            {kind !== 'provider' && (
              <div>
                <label
                  htmlFor="agent-cli-tool"
                  className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                >
                  {kind === 'local-cli' ? 'Local work tool' : 'Managed work tool'}
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
                    Model service
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
                    {...register('model')}
                    className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                    placeholder="e.g. claude-sonnet-4-6…"
                  />
                </div>
                <div>
                  <label
                    htmlFor="systemPrompt"
                    className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                  >
                    Instructions for this agent
                  </label>
                  <textarea
                    id="systemPrompt"
                    {...register('systemPrompt')}
                    rows={4}
                    placeholder="e.g. Review code clearly, list risks first, and cite exact files."
                    className="w-full resize-none rounded-[18px] border border-black/[0.08] bg-white px-4 py-3 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                  />
                  <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                    Optional. Use this to tell the agent how to behave every time it works.
                  </p>
                </div>
              </>
            )}

            {kind !== 'provider' && (
              <div>
                <label
                  htmlFor="agent-cwd"
                  className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                >
                  {kind === 'local-cli'
                    ? 'Project folder on this computer'
                    : 'Project files folder'}
                </label>
                <input
                  id="agent-cwd"
                  {...register('cwd')}
                  className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                  placeholder={kind === 'local-cli' ? '/Users/me/projects/app' : DEFAULT_AGENT_CWD}
                />
                <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                  {kind === 'local-cli'
                    ? 'Leave blank to use the folder where you run the join command.'
                    : 'The managed workspace can include several projects. Starting Project chooses where new tasks begin; it is not a private user folder.'}
                </p>
              </div>
            )}

            {selectedProjectId && (
              <div>
                <label
                  htmlFor="agent-group"
                  className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                >
                  Work lane
                </label>
                {groups.length > 0 ? (
                  <>
                    <select
                      id="agent-group"
                      {...register('groupId')}
                      className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                    >
                      <option value="">No work lane</option>
                      {groups.map((g) => (
                        <option key={g.id} value={g.id}>
                          {g.name}
                        </option>
                      ))}
                    </select>
                    <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                      Choose the work lane this agent watches for board tasks.
                    </p>
                  </>
                ) : (
                  <div>
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
                      {creatingGroup ? 'Creating...' : 'Create work lane'}
                    </button>
                    <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                      This creates the first work lane so the agent can receive tasks.
                    </p>
                  </div>
                )}
              </div>
            )}

            <div className="mt-2 flex flex-col gap-2 sm:flex-row sm:justify-end">
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
                {loading ? 'Creating...' : 'Create agent'}
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  )
}
