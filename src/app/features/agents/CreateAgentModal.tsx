import { useForm } from 'react-hook-form'
import { useEffect, useMemo, useRef, useState } from 'react'
import {
  ArrowRight,
  Bug,
  Check,
  ClipboardCheck,
  Code2,
  Copy,
  Plus,
  Search,
  X,
  type LucideIcon,
} from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { useAgentsStore } from '@app/entities/agent'
import { useNavigationStore } from '@app/entities/navigation'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { AgentCreateInitialKind, LocalAgentEnrollmentResponse } from '@app/entities/agent'
import type { LlmProviderConfig } from '@app/shared/api/legacy/settingsApi'
import type { CliTool } from '@shared/types'
import { createAgentWorkLaneErrorMessage } from './model/createAgentWorkLaneErrorMessage'

type AgentKind = AgentCreateInitialKind

interface CreateAgentFormData {
  name: string
  kind: AgentKind
  cliTool: CliTool
  /** Selected configured-provider id (the gateway link). Empty when none exist. */
  providerId: string
  model: string
  cwd: string
  groupId: string
  systemPrompt: string
}

interface CreateAgentModalProps {
  onOpenProjectsSetup?: () => void
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

interface AgentCreateReviewItem {
  label: string
  value: string
}

const AGENT_ROLE_TEMPLATES: AgentRoleTemplate[] = [
  {
    id: 'builder',
    label: 'Make a change',
    summary: 'Updates the work and checks it',
    name: 'Change Helper',
    systemPrompt:
      'You help turn a clear request into a working change. Keep edits narrow, explain any tradeoffs in plain language, and run the most relevant checks before handing work back.',
    Icon: Code2,
  },
  {
    id: 'reviewer',
    label: 'Review work',
    summary: 'Looks for risks before use',
    name: 'Review Helper',
    systemPrompt:
      'You review work before it is used. Point out concrete risks, missing checks, confusing behavior, and the next safest step. Use plain language and cite files or checks when you have them.',
    Icon: ClipboardCheck,
  },
  {
    id: 'investigator',
    label: 'Find the cause',
    summary: 'Tracks down unclear failures',
    name: 'Investigation Helper',
    systemPrompt:
      'You investigate unclear failures by gathering evidence first. Separate what is known from what is only a guess, then end with the smallest next action that can confirm the cause.',
    Icon: Search,
  },
  {
    id: 'fixer',
    label: 'Fix a bug',
    summary: 'Reproduces and fixes bugs',
    name: 'Bug Fix Helper',
    systemPrompt:
      'You reproduce bugs, find the smallest cause, fix the defect without unrelated changes, and verify both the failing case and nearby behavior before handing work back.',
    Icon: Bug,
  },
]

/**
 * A Provider + Prompt agent option, sourced from the configured LLM providers
 * (the gateway) in Settings → LLM Providers. Each configured provider carries
 * its own display name and model, so we no longer keep a hardcoded list.
 */
interface ProviderOption {
  /** The configured provider's id — the stable selection value. */
  id: string
  /** Provider key (e.g. `anthropic`, `zhipu_coding`) sent to the gateway. */
  provider: string
  label: string
  model: string
}

const DEFAULT_AGENT_CWD = '/workspace'

function setupCommandPasteHint(os: 'posix' | 'windows'): string {
  return os === 'windows'
    ? 'Open PowerShell on Windows, then paste this setup text.'
    : 'Open Terminal on macOS or your Linux terminal, then paste this setup text.'
}

/**
 * Build the Provider + Prompt options from configured providers. Prefer
 * providers that passed a connection test; fall back to all enabled providers
 * so a freshly-added (untested) provider is still usable.
 */
function buildProviderOptions(providers: LlmProviderConfig[]): ProviderOption[] {
  const enabled = providers.filter((provider) => provider.isEnabled)
  const tested = enabled.filter((provider) => provider.lastTestStatus === 'passed')
  const source = tested.length > 0 ? tested : enabled
  return source.map((provider) => ({
    id: provider.id,
    provider: provider.provider,
    label: provider.displayName,
    model: provider.model,
  }))
}

function providerOptionModel(options: ProviderOption[], id: string): string {
  return options.find((option) => option.id === id)?.model ?? ''
}

function providerOptionLabel(options: ProviderOption[], id: string): string {
  const option = options.find((candidate) => candidate.id === id)
  return option ? `${option.label}` : 'Provider'
}

function cliToolLabel(cliTool: CliTool): string {
  return CLI_TOOLS.find((tool) => tool.value === cliTool)?.label ?? cliTool
}

function runtimeFitFor(
  kind: AgentKind,
  cliTool: CliTool,
  providerLabel: string
): RuntimeFitSummary {
  if (kind === 'cli') {
    return {
      title: `${cliToolLabel(cliTool)} in a managed workspace`,
      detail: 'Best when the task needs project files or work tools prepared by Forge.',
      items: [
        { label: 'Agent location', value: 'Managed workspace' },
        { label: 'Files', value: 'Project files included' },
        { label: 'Before use', value: 'Check Where agents run in Settings' },
      ],
    }
  }

  if (kind === 'local-cli') {
    return {
      title: `${cliToolLabel(cliTool)} on this computer`,
      detail:
        'Best when files or tools must stay on this computer. After setup, Forge still manages this agent here: tasks, status, and task history.',
      items: [
        { label: 'Agent location', value: 'This computer' },
        { label: 'Files', value: 'Your chosen folder' },
        { label: 'Before use', value: 'Paste setup text on this computer' },
      ],
    }
  }

  return {
    title: `${providerLabel} simple chat agent`,
    detail: 'Best for questions, planning, writing, and review that do not need project files.',
    items: [
      { label: 'Agent location', value: 'Chat-only AI service' },
      { label: 'Files', value: 'Does not open project files' },
      { label: 'Before use', value: 'Check AI service in Settings' },
    ],
  }
}

function buildDefaultValues(
  provider: LlmProviderConfig | null,
  initialKind: AgentKind | null
): CreateAgentFormData {
  return {
    name: '',
    kind: initialKind ?? (provider ? 'provider' : 'cli'),
    cliTool: 'claude',
    providerId: provider?.id ?? '',
    model: provider?.model ?? '',
    cwd: DEFAULT_AGENT_CWD,
    groupId: '',
    systemPrompt: '',
  }
}

function createReviewItems({
  kind,
  runtimeTitle,
  projectName,
  hasSelectedProject,
  selectedGroupName,
  hasGroups,
}: {
  kind: AgentKind
  runtimeTitle: string
  projectName: string | null
  hasSelectedProject: boolean
  selectedGroupName: string | null
  hasGroups: boolean
}): AgentCreateReviewItem[] {
  const startState =
    kind === 'local-cli'
      ? 'Forge creates the agent, then shows setup steps for this computer.'
      : kind === 'provider'
        ? 'Ready for chat and review after the AI service is connected.'
        : 'Ready to start from Agents after the managed workspace is prepared.'

  const taskQueue = selectedGroupName
    ? selectedGroupName
    : hasSelectedProject
      ? hasGroups
        ? 'Choose a task queue now, or assign one later from Tasks.'
        : 'Create a task queue here when you want new tasks to wait in one place.'
      : 'Choose a project later before assigning tasks.'

  const nextStep =
    kind === 'local-cli'
      ? 'Paste the setup text on this computer and keep that window open.'
      : kind === 'provider'
        ? 'Ask a first question or assign review work that does not need files.'
        : 'Start the agent, then send one small task from Tasks.'

  return [
    { label: 'Work style', value: runtimeTitle },
    {
      label: 'Primary project',
      value: projectName ?? 'Choose a project before assigning tasks.',
    },
    { label: 'Task queue', value: taskQueue },
    { label: 'Next step', value: nextStep },
    { label: 'Created state', value: startState },
  ]
}

export function CreateAgentModal({ onOpenProjectsSetup }: CreateAgentModalProps = {}) {
  const {
    createModalOpen,
    createModalInitialKind,
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
  const [joinOs, setJoinOs] = useState<'posix' | 'windows'>('posix')
  const [copiedJoin, setCopiedJoin] = useState(false)
  const [copyError, setCopyError] = useState<string | null>(null)
  const [formError, setFormError] = useState<string | null>(null)
  // Provider + Prompt agents pick from the configured providers (the LLM
  // gateway), preferring tested ones. No usable provider = a clear hint, no
  // broken dropdown.
  const providerOptions = useMemo(() => buildProviderOptions(providers), [providers])
  const hasProviderOptions = providerOptions.length > 0
  const verifiedProvider = useMemo(
    () =>
      providers.find((provider) => provider.isEnabled && provider.lastTestStatus === 'passed') ??
      null,
    [providers]
  )
  const defaultValues = useMemo(
    () => buildDefaultValues(verifiedProvider, createModalInitialKind),
    [createModalInitialKind, verifiedProvider]
  )

  const {
    register,
    handleSubmit,
    reset,
    watch,
    setValue,
    formState: { submitCount },
  } = useForm<CreateAgentFormData>({
    defaultValues,
  })
  const [selectedTemplateId, setSelectedTemplateId] = useState<string | null>(null)
  const kind = watch('kind')
  const providerId = watch('providerId')
  const cliTool = watch('cliTool')
  const cwd = watch('cwd')
  const groupId = watch('groupId')
  const runtimeFit = runtimeFitFor(kind, cliTool, providerOptionLabel(providerOptions, providerId))
  const selectedProject = selectedProjectId
    ? (Object.values(projectsByTeam)
        .flat()
        .find((project) => project.id === selectedProjectId) ?? null)
    : null
  const selectedGroup = useMemo(
    () => groups.find((group) => group.id === groupId) ?? null,
    [groupId, groups]
  )
  const reviewItems = useMemo(
    () =>
      createReviewItems({
        kind,
        runtimeTitle: runtimeFit.title,
        projectName: selectedProject?.name ?? null,
        hasSelectedProject: Boolean(selectedProjectId),
        selectedGroupName: selectedGroup?.name ?? null,
        hasGroups: groups.length > 0,
      }),
    [groups.length, kind, runtimeFit.title, selectedGroup, selectedProject, selectedProjectId]
  )
  const dialogRef = useRef<HTMLDivElement>(null)
  const errorBannerRef = useRef<HTMLDivElement>(null)
  const displayedError = formError ?? error
  const joinCommand = localEnrollment?.enrollment?.joinCommand ?? ''
  const joinCommandPowershell = localEnrollment?.enrollment?.joinCommandPowershell ?? ''
  const selectedJoinCommand = joinOs === 'posix' ? joinCommand : joinCommandPowershell
  const selectedJoinCommandReady = selectedJoinCommand.trim().length > 0

  function handleOpenProjectsSetup() {
    setCreateModalOpen(false)
    setError(null)
    setFormError(null)
    onOpenProjectsSetup?.()
  }

  // The error banner sits above the form in a scrollable dialog while the
  // submit button sits at the bottom, so a failed submit can leave the banner
  // entirely off-screen and look like a dead click. `submitCount` is in the
  // dependencies so a repeat submit with the SAME message scrolls again.
  useEffect(() => {
    if (displayedError)
      errorBannerRef.current?.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
  }, [displayedError, submitCount])

  useEffect(() => {
    if (!createModalOpen) return
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        setCreateModalOpen(false)
        setError(null)
        setFormError(null)
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
    setCopiedJoin(false)
    setJoinOs('posix')
    setError(null)
    setFormError(null)
    setCopyError(null)
  }, [createModalOpen, defaultValues, reset, setError])

  // When the user switches the configured provider, seed the model box with
  // that provider's model so the agent never points at a mismatched model.
  useEffect(() => {
    if (!providerId) return
    const model = providerOptionModel(providerOptions, providerId)
    if (model) setValue('model', model)
  }, [providerId, providerOptions, setValue])

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
    setFormError(null)
    if (!data.name.trim()) {
      setFormError('Name this agent before creating it.')
      return
    }
    const base = {
      name: data.name.trim(),
      workspaceId: selectedProject?.workspaceId,
      projectId: selectedProjectId ?? undefined,
      groupId: data.groupId || undefined,
    }
    if (data.kind === 'provider') {
      const selected = providerOptions.find((option) => option.id === data.providerId)
      if (!selected) {
        setFormError(
          'Open Settings > AI services, add a service, save it, then click Check until it says Ready.'
        )
        return
      }
      if (!data.model.trim()) {
        setFormError('Choose an AI service and AI model before creating this agent.')
        return
      }
      await createAgent({
        ...base,
        kind: 'provider',
        provider: selected.provider,
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
      setError('Select a project before creating a task queue. Task queues belong to one project.')
      return
    }

    setCreatingGroup(true)
    setError(null)
    try {
      const group = await createAgentGroup(selectedProjectId, {
        name: 'Default Task Queue',
        description:
          'Starter queue for this project. New tasks wait here until an agent can take them.',
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
    setFormError(null)
    setCopyError(null)
    setLocalEnrollment(null)
    setCopiedCommand(false)
    setCopiedJoin(false)
  }

  const CLIPBOARD_UNAVAILABLE =
    'Forge cannot copy from this browser. Select the setup text in the box, then copy it manually.'

  async function handleCopyCommand() {
    const command = localEnrollment?.enrollment?.shellExports
    if (!command) return
    if (!navigator.clipboard?.writeText) {
      setCopyError(CLIPBOARD_UNAVAILABLE)
      return
    }
    try {
      await navigator.clipboard.writeText(command)
      setCopiedCommand(true)
      setCopyError(null)
    } catch {
      setCopiedCommand(false)
      setCopyError(CLIPBOARD_UNAVAILABLE)
    }
  }

  async function handleCopyJoinCommand(command: string) {
    if (!navigator.clipboard?.writeText) {
      setCopyError(CLIPBOARD_UNAVAILABLE)
      return
    }
    try {
      await navigator.clipboard.writeText(command)
      setCopiedJoin(true)
      setCopyError(null)
    } catch {
      setCopiedJoin(false)
      setCopyError(CLIPBOARD_UNAVAILABLE)
    }
  }

  function handleCreateAnother() {
    setLocalEnrollment(null)
    setCopiedCommand(false)
    setCopiedJoin(false)
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
          'relative mx-4 max-h-[86vh] w-full max-w-[520px] overflow-y-auto',
          'rounded-panel border border-black/[0.08] bg-white p-6 dark:border-white/[0.1] dark:bg-[#2a2a2c]'
        )}
      >
        <div className="flex items-center justify-between mb-4">
          <h2
            id="create-agent-title"
            className="text-ui-title font-semibold text-foreground-light dark:text-foreground-dark"
          >
            {localEnrollment ? 'Connect this computer' : 'Create an agent'}
          </h2>
          <button
            type="button"
            onClick={handleClose}
            aria-label="Close dialog"
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-secondary-light transition-colors hover:bg-black/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/40 dark:text-secondary-dark dark:hover:bg-white/5"
          >
            <X size={15} strokeWidth={2} aria-hidden="true" />
          </button>
        </div>

        {displayedError && (
          <div
            ref={errorBannerRef}
            role="alert"
            aria-live="polite"
            className="mb-4 rounded-lg bg-apple-red/10 px-3 py-2 text-ui-caption text-apple-red"
          >
            {displayedError}
          </div>
        )}

        {localEnrollment ? (
          <div className="flex flex-col gap-4">
            <div className="rounded-lg border border-black/[0.08] bg-surface-pearl p-4 dark:border-white/[0.1] dark:bg-white/[0.04]">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <div className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                    Agent managed by Forge
                  </div>
                  <div className="mt-1 text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
                    {localEnrollment.agent?.name ?? 'This computer agent'}
                  </div>
                </div>
                <span className="rounded-full border border-apple-green/20 bg-white px-2.5 py-1 text-ui-caption text-apple-green dark:bg-white/[0.04]">
                  This computer
                </span>
              </div>
              <p className="mt-3 text-ui-caption text-secondary-light dark:text-secondary-dark">
                {localEnrollment.enrollment?.joinCommand
                  ? 'Paste the setup text into Terminal or PowerShell on the computer where this agent should work. Forge will show it as an agent here, assign tasks to it, and keep its status and history. Files stay on that computer.'
                  : 'Paste this setup text on the computer where this agent should work. Forge will manage its tasks, status, and history while files stay on that computer.'}
              </p>
            </div>

            {localEnrollment.enrollment?.joinCommand ? (
              <div>
                <div className="mb-2 flex items-center justify-between gap-2">
                  <span className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                    Setup text
                  </span>
                  <div role="group" aria-label="Computer type" className="flex gap-1">
                    {(
                      [
                        { value: 'posix', label: 'macOS / Linux' },
                        { value: 'windows', label: 'Windows' },
                      ] as const
                    ).map((option) => (
                      <button
                        key={option.value}
                        type="button"
                        aria-pressed={joinOs === option.value}
                        onClick={() => {
                          setJoinOs(option.value)
                          setCopiedJoin(false)
                        }}
                        className={cn(
                          'rounded-full px-3 py-1 text-ui-caption font-medium transition-colors',
                          joinOs === option.value
                            ? 'bg-apple-blue text-white'
                            : 'border border-black/[0.08] bg-white text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark'
                        )}
                      >
                        {option.label}
                      </button>
                    ))}
                  </div>
                </div>
                {selectedJoinCommandReady ? (
                  <textarea
                    id="local-agent-join-command"
                    aria-label="Setup text"
                    readOnly
                    value={selectedJoinCommand}
                    rows={3}
                    className="w-full resize-none rounded-[18px] border border-black/[0.08] bg-white px-4 py-3 font-mono text-ui-caption text-foreground-light outline-none dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                  />
                ) : (
                  <div
                    role="note"
                    className="rounded-[18px] border border-apple-orange/30 bg-apple-orange/10 px-4 py-3 text-ui-caption text-secondary-light dark:text-secondary-dark"
                  >
                    One-line Windows setup text is not ready for this agent. Open the backup setup
                    values below, copy them into PowerShell, and keep that window open.
                  </div>
                )}
                <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                  The pairing code inside expires in 15 minutes. If it expires, create the agent
                  again to get a fresh command.
                </p>
                <p
                  data-testid="local-agent-paste-hint"
                  className="mt-1 text-ui-caption font-medium text-foreground-light dark:text-foreground-dark"
                >
                  {selectedJoinCommandReady
                    ? setupCommandPasteHint(joinOs)
                    : 'Use the backup setup values below for Windows.'}
                </p>
                <div className="mt-2 grid gap-1.5 rounded-lg border border-black/[0.06] bg-black/[0.025] px-3 py-2 text-ui-caption text-secondary-light dark:border-white/[0.08] dark:bg-white/[0.04] dark:text-secondary-dark">
                  <p>1. Copy the setup text.</p>
                  <p>
                    2. Paste it into Terminal or PowerShell on the computer that will do the work.
                  </p>
                  <p>
                    3. Keep that window open. Success looks like: the agent changes from Not
                    connected to Ready on the Agents page.
                  </p>
                  <p>
                    4. Closing that window disconnects this agent until you paste the setup text
                    again.
                  </p>
                  <p>
                    5. Come back to Forge, open Agents, and send one small task when it is Ready.
                  </p>
                </div>
                <details className="mt-3">
                  <summary className="cursor-pointer text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                    If the setup text does not work
                  </summary>
                  <p className="mt-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
                    Use this backup only if the setup text above does not work on this computer.
                    Copy these backup setup values into the same Terminal or PowerShell window, then
                    keep that window open.
                  </p>
                  <textarea
                    id="local-agent-command"
                    aria-label="Backup setup values"
                    readOnly
                    value={localEnrollment.enrollment?.shellExports ?? ''}
                    rows={6}
                    className="mt-2 w-full resize-none rounded-[18px] border border-black/[0.08] bg-white px-4 py-3 font-mono text-ui-caption text-foreground-light outline-none dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                  />
                  <div className="mt-2 flex justify-end">
                    <button
                      type="button"
                      onClick={() => void handleCopyCommand()}
                      className="inline-flex items-center gap-2 rounded-full border border-black/[0.08] px-3 py-1.5 text-ui-caption font-medium text-foreground-light dark:border-white/[0.1] dark:text-foreground-dark"
                    >
                      {copiedCommand ? (
                        <Check size={13} strokeWidth={2.25} aria-hidden="true" />
                      ) : (
                        <Copy size={13} strokeWidth={2.25} aria-hidden="true" />
                      )}
                      {copiedCommand ? 'Copied' : 'Copy backup setup'}
                    </button>
                  </div>
                </details>
              </div>
            ) : (
              <div>
                <label
                  htmlFor="local-agent-command"
                  className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                >
                  Setup text
                </label>
                <textarea
                  id="local-agent-command"
                  readOnly
                  value={localEnrollment.enrollment?.shellExports ?? ''}
                  rows={8}
                  className="w-full resize-none rounded-[18px] border border-black/[0.08] bg-white px-4 py-3 font-mono text-ui-caption text-foreground-light outline-none dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                />
                <div className="mt-2 grid gap-1.5 rounded-lg border border-black/[0.06] bg-black/[0.025] px-3 py-2 text-ui-caption text-secondary-light dark:border-white/[0.08] dark:bg-white/[0.04] dark:text-secondary-dark">
                  <p>1. Copy the setup text.</p>
                  <p>2. Paste it into the terminal app on the computer that will do the work.</p>
                  <p>
                    3. Keep that window open. Success looks like: the agent changes from Not
                    connected to Ready on the Agents page.
                  </p>
                  <p>
                    4. Closing that window disconnects this agent until you paste the setup text
                    again.
                  </p>
                  <p>
                    5. Come back to Forge, open Agents, and send one small task when it is Ready.
                  </p>
                </div>
              </div>
            )}

            {copyError && (
              <p role="alert" className="text-ui-caption text-apple-red">
                {copyError}
              </p>
            )}

            <div className="flex flex-wrap justify-end gap-2">
              <button
                type="button"
                onClick={handleCreateAnother}
                className="rounded-full bg-surface-pearl px-4 py-2 text-ui-button font-medium text-foreground-light ring-1 ring-black/[0.04] transition-transform active:scale-95 dark:bg-white/[0.06] dark:text-foreground-dark"
              >
                Create another
              </button>
              {localEnrollment.enrollment?.joinCommand ? (
                <button
                  type="button"
                  onClick={() => {
                    if (selectedJoinCommandReady) void handleCopyJoinCommand(selectedJoinCommand)
                  }}
                  disabled={!selectedJoinCommandReady}
                  className={cn(
                    'inline-flex items-center gap-2 rounded-full bg-apple-blue px-4 py-2 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus active:scale-95',
                    !selectedJoinCommandReady && 'cursor-not-allowed opacity-60'
                  )}
                >
                  {copiedJoin ? (
                    <Check size={14} strokeWidth={2.25} aria-hidden="true" />
                  ) : (
                    <Copy size={14} strokeWidth={2.25} aria-hidden="true" />
                  )}
                  {!selectedJoinCommandReady
                    ? 'Use backup setup values'
                    : copiedJoin
                      ? 'Copied'
                      : 'Copy setup text'}
                </button>
              ) : (
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
                  {copiedCommand ? 'Copied' : 'Copy setup text'}
                </button>
              )}
              <button
                type="button"
                onClick={handleClose}
                className="rounded-full bg-apple-gray-5 px-4 py-2 text-ui-button font-medium text-foreground-light transition-transform active:scale-95 dark:bg-white/[0.06] dark:text-foreground-dark"
              >
                Close and watch Agents
              </button>
            </div>
          </div>
        ) : (
          <form onSubmit={handleSubmit(handleFormSubmit)} className="flex flex-col gap-4">
            <div>
              <div className="mb-2 flex items-center justify-between gap-2">
                <span className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                  Pick a starter template
                </span>
                <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                  {kind === 'provider'
                    ? 'Fills in name and instructions'
                    : 'Fills in the agent name'}
                </span>
              </div>
              <div
                role="group"
                aria-label="Agent starter templates"
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
                placeholder="e.g. Frontend Agent…"
                autoFocus
              />
            </div>

            <div>
              <label className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                Choose work style
              </label>
              <div className="flex gap-2" role="radiogroup" aria-label="Choose work style">
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
                  Simple chat agent
                </label>
              </div>
              <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                {kind === 'cli'
                  ? 'Uses a ready workspace managed by Forge for file and command work.'
                  : kind === 'local-cli'
                    ? 'Uses files and commands on your computer. Forge still manages the agent here with tasks, status, and history.'
                    : 'Uses a connected AI service for planning, writing, and review. It does not open files or run commands.'}
              </p>
              <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                Not sure? Use Managed workspace for project-file work, This computer when files must
                stay local, or Simple chat agent after an AI service is ready.
              </p>
            </div>

            <section
              data-testid="agent-runtime-fit"
              className="rounded-lg border border-black/[0.06] bg-black/[0.025] px-3 py-2.5 dark:border-white/[0.08] dark:bg-white/[0.04]"
            >
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                    Best for
                  </p>
                  <p className="mt-0.5 text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
                    {runtimeFit.title}
                  </p>
                </div>
                <span className="shrink-0 rounded-full bg-apple-blue/10 px-2 py-0.5 text-ui-caption font-medium text-apple-blue">
                  {kind === 'cli'
                    ? 'Can edit files'
                    : kind === 'local-cli'
                      ? 'Uses this computer'
                      : 'Chat only'}
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
                Primary Project
              </div>
              <div className="w-full rounded-[18px] border border-black/[0.08] bg-white px-4 py-2 text-ui-body text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark">
                {selectedProject?.name ?? 'Choose a project later'}
              </div>
              <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                {selectedProject
                  ? kind === 'local-cli'
                    ? 'Project ready. Tasks default to this project. File access stays on the joined computer.'
                    : 'Project ready. Tasks default to this project. Forge prepares this project workspace for the agent.'
                  : 'Open project settings to create or choose a project before assigning tasks. The agent can still be created first.'}
              </p>
              {!selectedProject && onOpenProjectsSetup ? (
                <button
                  type="button"
                  onClick={handleOpenProjectsSetup}
                  className="mt-2 inline-flex h-8 items-center justify-center gap-1.5 rounded-full border border-apple-blue/20 bg-apple-blue/[0.08] px-3 text-ui-button font-medium text-apple-blue transition-colors hover:bg-apple-blue/[0.12] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35"
                >
                  <span>Open project settings</span>
                  <ArrowRight size={13} strokeWidth={2.25} aria-hidden="true" />
                </button>
              ) : null}
            </div>

            {kind !== 'provider' && (
              <div>
                <label
                  htmlFor="agent-cli-tool"
                  className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                >
                  {kind === 'local-cli' ? 'Work tool on this computer' : 'Work tool'}
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
                {hasProviderOptions ? (
                  <>
                    <div>
                      <label
                        htmlFor="agent-provider"
                        className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                      >
                        AI service
                      </label>
                      <select
                        id="agent-provider"
                        {...register('providerId')}
                        className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                      >
                        {providerOptions.map((option) => (
                          <option key={option.id} value={option.id}>
                            {option.label} · {option.model}
                          </option>
                        ))}
                      </select>
                      <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                        Choose a checked AI service from Settings. The model is set by that service.
                      </p>
                    </div>
                    <div>
                      <label
                        htmlFor="agent-model"
                        className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                      >
                        AI model
                      </label>
                      <input
                        id="agent-model"
                        {...register('model')}
                        readOnly
                        className="h-10 w-full rounded-full border border-black/[0.08] bg-black/[0.025] px-4 text-ui-body text-foreground-light outline-none dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                        placeholder="Set by the selected AI service"
                      />
                    </div>
                  </>
                ) : (
                  <div
                    data-testid="provider-empty-hint"
                    className="rounded-lg border border-apple-orange/20 bg-apple-orange/[0.06] px-3 py-2.5"
                  >
                    <p className="text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
                      No AI service ready yet
                    </p>
                    <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                      Open Settings &gt; AI services, add a service, paste its access key, save it,
                      then click Check. Come back when the service says Ready.
                    </p>
                    <a
                      href="/settings/providers"
                      className="mt-2 inline-flex h-8 items-center justify-center gap-1.5 rounded-full border border-apple-blue/20 bg-apple-blue/[0.08] px-3 text-ui-button font-medium text-apple-blue transition-colors hover:bg-apple-blue/[0.12] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35"
                    >
                      <span>Open AI services settings</span>
                      <ArrowRight size={13} strokeWidth={2.25} aria-hidden="true" />
                    </a>
                  </div>
                )}
                <div>
                  <label
                    htmlFor="systemPrompt"
                    className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                  >
                    Agent instructions
                  </label>
                  <textarea
                    id="systemPrompt"
                    {...register('systemPrompt')}
                    rows={4}
                    placeholder="e.g. Help review tasks, explain risks in plain language, and list the next step."
                    className="w-full resize-none rounded-[18px] border border-black/[0.08] bg-white px-4 py-3 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                  />
                </div>
              </>
            )}

            {kind !== 'provider' && (
              <div>
                <label
                  htmlFor="agent-cwd"
                  className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                >
                  {kind === 'local-cli' ? 'Folder on this computer' : 'Work folder'}
                </label>
                <input
                  id="agent-cwd"
                  {...register('cwd')}
                  className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                  placeholder={kind === 'local-cli' ? '/Users/me/projects/app' : DEFAULT_AGENT_CWD}
                />
                <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                  {kind === 'local-cli'
                    ? 'Leave blank to use the folder where you paste the setup text.'
                    : 'Keep the suggested folder unless an owner gives you a different one. New tasks start from the Primary Project selected above.'}
                </p>
              </div>
            )}

            {selectedProjectId && (
              <div>
                <label
                  htmlFor="agent-group"
                  className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                >
                  Task queue
                </label>
                {groups.length > 0 ? (
                  <>
                    <select
                      id="agent-group"
                      {...register('groupId')}
                      className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                    >
                      <option value="">Choose a task queue later</option>
                      {groups.map((g) => (
                        <option key={g.id} value={g.id}>
                          {g.name}
                        </option>
                      ))}
                    </select>
                    <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                      New tasks can wait in this queue until an available agent can take them.
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
                      {creatingGroup ? 'Creating…' : 'Create task queue'}
                    </button>
                    <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                      This creates a starter queue for this project so new tasks have a clear place
                      to wait.
                    </p>
                  </div>
                )}
              </div>
            )}

            <section
              data-testid="agent-create-review"
              className="rounded-lg border border-apple-blue/20 bg-apple-blue/10 px-3 py-2.5"
            >
              <p className="text-ui-caption font-semibold text-apple-blue">Before you create</p>
              <div className="mt-2 grid gap-1.5 sm:grid-cols-2">
                {reviewItems.map((item) => (
                  <div
                    key={item.label}
                    className="min-w-0 rounded-md bg-white px-2 py-1.5 dark:bg-black/20"
                  >
                    <span className="block text-[10px] font-medium text-secondary-light dark:text-secondary-dark">
                      {item.label}
                    </span>
                    <span className="mt-0.5 block text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
                      {item.value}
                    </span>
                  </div>
                ))}
              </div>
            </section>

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
                {loading ? 'Creating…' : 'Create agent'}
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  )
}
