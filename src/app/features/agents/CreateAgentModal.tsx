import { useForm } from 'react-hook-form'
import { useEffect, useMemo, useRef, useState } from 'react'
import {
  ArrowRight,
  Bug,
  Check,
  ChevronDown,
  ClipboardCheck,
  Code2,
  Copy,
  Plus,
  Search,
  X,
  type LucideIcon,
} from 'lucide-react'
import { waitingPlaceDisplayName } from '@app/entities/navigation/agent-group'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import {
  LOCAL_AGENT_SETUP_APP_LABEL,
  localAgentSetupPasteHint,
  useAgentsStore,
} from '@app/entities/agent'
import { useNavigationStore } from '@app/entities/navigation'
import { useSettingsStore } from '@app/entities/settings'
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

interface AgentStarterTemplate {
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

const AGENT_STARTER_TEMPLATES: AgentStarterTemplate[] = [
  {
    id: 'builder',
    label: 'Make a change',
    summary: 'Updates the work and checks it',
    name: 'Change Helper',
    systemPrompt:
      'You help with one requested change at a time. First restate the result in plain language, make only related edits, run the most useful check you can, and tell the user what changed and what to try next.',
    Icon: Code2,
  },
  {
    id: 'result-check',
    label: 'Check results',
    summary: 'Looks for risks before use',
    name: 'Result Check Helper',
    systemPrompt:
      'You check work before the team uses it. Look for confusing behavior, missing checks, risky changes, and unclear next steps. Explain each concern in plain language and end with a clear use, fix, or wait recommendation.',
    Icon: ClipboardCheck,
  },
  {
    id: 'investigator',
    label: 'Find the cause',
    summary: 'Checks the next useful clue',
    name: 'Investigation Helper',
    systemPrompt:
      'You help find the cause of a problem. Start with what the user already knows, separate what is confirmed from what is only a guess, check the smallest useful clue next, and end with the next action that can confirm the answer.',
    Icon: Search,
  },
  {
    id: 'fixer',
    label: 'Fix a bug',
    summary: 'Checks the problem and fixes it',
    name: 'Bug Fix Helper',
    systemPrompt:
      'You fix bugs in the smallest safe way. Reproduce what is broken when possible, change only what is related, check the broken case again, and explain what the user should try next.',
    Icon: Bug,
  },
]

const AGENT_KIND_OPTIONS: Array<{
  value: AgentKind
  label: string
  badge: string
  summary: string
}> = [
  {
    value: 'cli',
    label: 'Project files',
    badge: 'Best for project changes',
    summary: 'Tasks and code changes in the selected project.',
  },
  {
    value: 'local-cli',
    label: 'This computer',
    badge: 'Local files',
    summary: 'Tasks and code changes in a folder on this computer.',
  },
  {
    value: 'provider',
    label: 'Simple chat agent',
    badge: 'Questions only',
    summary: 'Questions only. It cannot take Tasks, change files, or use apps.',
  },
]

/**
 * A simple chat agent option, sourced from the configured AI services
 * in Settings. Each configured AI service carries
 * its own display name and model, so we no longer keep a hardcoded list.
 */
interface ProviderOption {
  /** The configured provider's id — the stable selection value. */
  id: string
  /** Provider key (e.g. `anthropic`, `zhipu_coding`) sent to the gateway. */
  provider: string
  label: string
  model: string
  ready: boolean
}

const DEFAULT_AGENT_CWD = '/workspace'
const NO_READY_AI_SERVICE_ERROR =
  'Open AI services in Settings, add a service, paste the key from that service, save it, then choose Check connection. Come back when it shows Ready.'
const SELECTED_AI_SERVICE_NOT_READY_ERROR =
  'Open AI services in Settings, choose Check connection for this service, then come back when it shows Ready.'
const NO_SELECTED_PROJECT_ERROR =
  'Open project settings, create or choose a project, then create this agent. Agents that work with files need a project first.'

/**
 * Build the simple chat agent options from configured AI services. Prefer
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
    ready: provider.lastTestStatus === 'passed',
  }))
}

function providerOptionModel(options: ProviderOption[], id: string): string {
  return options.find((option) => option.id === id)?.model ?? ''
}

function providerOptionLabel(options: ProviderOption[], id: string): string {
  const option = options.find((candidate) => candidate.id === id)
  return option ? `${option.label}` : 'AI service'
}

function providerOptionReady(options: ProviderOption[], id: string): boolean {
  return options.find((option) => option.id === id)?.ready === true
}

function providerOptionStatusLabel(option: ProviderOption): string {
  return option.ready ? 'Ready in Settings' : 'Check connection first'
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
      title: `${cliToolLabel(cliTool)} with project files`,
      detail: 'Best when the task needs the agent to open project files and run checks in Forge.',
      items: [
        { label: 'Where it works', value: 'Shared project folder' },
        { label: 'Files', value: 'Project files are included' },
        { label: 'Before use', value: 'Check Where agents work in Settings' },
      ],
    }
  }

  if (kind === 'local-cli') {
    return {
      title: `${cliToolLabel(cliTool)} on this computer`,
      detail:
        'Best when files or tools must stay on this computer. After setup, Forge still manages this agent here: tasks, status, and task history.',
      items: [
        { label: 'Where it works', value: 'This computer' },
        { label: 'Files', value: 'Your chosen folder' },
        { label: 'Before use', value: 'Follow the setup steps shown after creation' },
      ],
    }
  }

  return {
    title: `${providerLabel} for questions and result checks`,
    detail:
      'Best for questions, writing, and checking results. It cannot take Tasks, change files, or use apps.',
    items: [
      { label: 'Where it works', value: 'AI service only' },
      { label: 'Files', value: 'Does not open project files' },
      { label: 'Before use', value: 'Check AI services in Settings' },
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
    cliTool: 'codex',
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
  if (kind === 'provider') {
    return [
      { label: 'Where it works', value: runtimeTitle },
      { label: 'How to use it', value: 'Open this agent from Chat. It does not take Tasks.' },
      {
        label: 'Tasks and files',
        value: 'Need Tasks or code changes? Create an agent with Project files or This computer.',
      },
      {
        label: 'After creation',
        value: 'Ready for questions and result checks after the AI service is connected.',
      },
    ]
  }

  const startState =
    kind === 'local-cli'
      ? 'Forge creates the agent, then shows setup steps for this computer.'
      : 'Forge starts it after the project file area is ready.'

  const projectForTasks = projectName ?? 'Choose a project before sending tasks.'

  const taskQueue = selectedGroupName
    ? waitingPlaceDisplayName(selectedGroupName)
    : hasSelectedProject
      ? hasGroups
        ? 'Choose a place for new tasks now, or set it later from Tasks.'
        : 'Set up a place here when you want new tasks to wait together.'
      : 'Choose a project later before sending tasks.'

  const nextStep =
    kind === 'local-cli'
      ? 'Follow the setup steps shown after creation and keep that window open.'
      : 'Wait until it shows Ready, then send one small task from Tasks.'

  return [
    { label: 'Where it works', value: runtimeTitle },
    {
      label: 'Project for new tasks',
      value: projectForTasks,
    },
    { label: 'Place for new tasks', value: taskQueue },
    { label: 'Next step', value: nextStep },
    { label: 'After creation', value: startState },
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
  const providersLoading = useSettingsStore((s) => s.providersLoading)
  const loadProviders = useSettingsStore((s) => s.loadProviders)
  const selectedProjectId = useNavigationStore((s) => s.selectedProjectId)
  const projectsByTeam = useNavigationStore((s) => s.projects)
  const groups = useNavigationStore((s) => s.agentGroups)
  const createAgentGroup = useNavigationStore((s) => s.createAgentGroup)
  const [creatingGroup, setCreatingGroup] = useState(false)
  const [localEnrollment, setLocalEnrollment] = useState<LocalAgentEnrollmentResponse | null>(null)
  const [copiedCommand, setCopiedCommand] = useState(false)
  const [joinOs, setJoinOs] = useState<'posix' | 'windows'>('posix')
  const [copiedJoin, setCopiedJoin] = useState(false)
  const [backupSetupOpen, setBackupSetupOpen] = useState(false)
  const [copyError, setCopyError] = useState<string | null>(null)
  const [formError, setFormError] = useState<string | null>(null)
  // Simple chat agents pick from configured AI services, preferring tested
  // ones. No usable AI service = a clear hint, no
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
  const [runtimeDetailsOpen, setRuntimeDetailsOpen] = useState(false)
  const [starterTemplatesOpen, setStarterTemplatesOpen] = useState(false)
  const kind = watch('kind')
  const providerId = watch('providerId')
  const cliTool = watch('cliTool')
  const cwd = watch('cwd')
  const groupId = watch('groupId')
  const runtimeFit = runtimeFitFor(kind, cliTool, providerOptionLabel(providerOptions, providerId))
  const selectedProviderReady = providerOptionReady(providerOptions, providerId)
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
  const providersRequestedRef = useRef(false)
  const displayedError = formError ?? error
  const joinCommand = localEnrollment?.enrollment?.joinCommand ?? ''
  const joinCommandPowershell = localEnrollment?.enrollment?.joinCommandPowershell ?? ''
  const selectedJoinCommand = joinOs === 'posix' ? joinCommand : joinCommandPowershell
  const selectedJoinCommandReady = selectedJoinCommand.trim().length > 0
  const showBackupSetup = backupSetupOpen || !selectedJoinCommandReady
  const projectRequiredBeforeCreate =
    kind !== 'provider' && !selectedProject && Boolean(onOpenProjectsSetup)

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

  // Simple chat agent options are sourced from configured AI services in the
  // settings store, but only the Settings and Getting Started pages load
  // that store. Opening this modal from a deep link to /agents would otherwise
  // show an empty provider list even when the org has providers configured.
  // Self-load once per open so the dropdown is populated wherever the modal is
  // opened from. The ref guards against re-fetching when a completed load
  // legitimately returned zero providers.
  useEffect(() => {
    if (!createModalOpen) {
      providersRequestedRef.current = false
      return
    }
    if (providersRequestedRef.current) return
    if (providers.length === 0 && !providersLoading) {
      providersRequestedRef.current = true
      void loadProviders()
    }
  }, [createModalOpen, providers.length, providersLoading, loadProviders])

  // Reset form when modal opens.
  useEffect(() => {
    if (!createModalOpen) return

    reset(defaultValues)
    setSelectedTemplateId(null)
    setLocalEnrollment(null)
    setCopiedCommand(false)
    setCopiedJoin(false)
    setBackupSetupOpen(false)
    setJoinOs('posix')
    setError(null)
    setFormError(null)
    setCopyError(null)
    setRuntimeDetailsOpen(false)
    setStarterTemplatesOpen(false)
  }, [createModalOpen, defaultValues, reset, setError])

  useEffect(() => {
    setRuntimeDetailsOpen(false)
  }, [kind])

  // Keep the hidden submission value in sync with the selected AI service so
  // the create flow does not need to show a raw model name to first-time users.
  useEffect(() => {
    if (kind === 'provider' && !providerId && providerOptions[0]) {
      setValue('providerId', providerOptions[0].id)
      return
    }
    if (!providerId) return
    const model = providerOptionModel(providerOptions, providerId)
    if (model) setValue('model', model)
  }, [kind, providerId, providerOptions, setValue])

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
    if (data.kind !== 'provider' && !selectedProject) {
      setFormError(NO_SELECTED_PROJECT_ERROR)
      return
    }
    const base: {
      name: string
      workspaceId?: string
      projectId?: string
      groupId?: string
    } = {
      name: data.name.trim(),
      workspaceId: selectedProject?.workspaceId,
      projectId: selectedProjectId ?? undefined,
    }
    if (data.kind !== 'provider' && data.groupId) {
      base.groupId = data.groupId
    }
    if (data.kind === 'provider') {
      const selected = providerOptions.find((option) => option.id === data.providerId)
      const selectedModel = selected?.model.trim() || (data.model ?? '').trim()
      if (!selected) {
        setFormError(NO_READY_AI_SERVICE_ERROR)
        return
      }
      if (!selected.ready) {
        setFormError(SELECTED_AI_SERVICE_NOT_READY_ERROR)
        return
      }
      if (!selectedModel) {
        setFormError(SELECTED_AI_SERVICE_NOT_READY_ERROR)
        return
      }
      await createAgent({
        ...base,
        kind: 'provider',
        provider: selected.provider,
        model: selectedModel,
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
        setBackupSetupOpen(false)
      }
    } else {
      await createAgent({ ...base, kind: 'cli', cliTool: data.cliTool, cwd: data.cwd || undefined })
    }
  }

  async function handleCreateDefaultGroup() {
    if (!selectedProjectId) {
      setError(
        'Select a project before setting up a place for new tasks. Each place belongs to one project.'
      )
      return
    }

    setCreatingGroup(true)
    setError(null)
    try {
      const group = await createAgentGroup(selectedProjectId, {
        name: 'Default Task Queue',
        description:
          'Starter place for this project. New tasks wait here until an agent can take them.',
      })
      setValue('groupId', group.id, { shouldDirty: true })
    } catch (err) {
      setError(createAgentWorkLaneErrorMessage(err))
    } finally {
      setCreatingGroup(false)
    }
  }

  function applyStarterTemplate(template: AgentStarterTemplate) {
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
    setBackupSetupOpen(false)
  }

  const CLIPBOARD_UNAVAILABLE =
    'Copy did not work. Select the setup text in the box, then copy it yourself.'

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
    setBackupSetupOpen(false)
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
          'rounded-panel border border-black/[0.08] bg-white p-6 dark:border-white/[0.1] dark:bg-surface-dark'
        )}
      >
        <div className="flex items-center justify-between mb-4">
          <div className="min-w-0">
            <h2
              id="create-agent-title"
              className="text-ui-title font-semibold text-foreground-light dark:text-foreground-dark"
            >
              {localEnrollment ? 'Connect this computer' : 'New agent'}
            </h2>
            {!localEnrollment && (
              <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                Start with what this agent should be allowed to touch. Keep the suggested choices
                unless an owner gives you a different setup.
              </p>
            )}
          </div>
          <button
            type="button"
            onClick={handleClose}
            aria-label="Close dialog"
            className={cn(uiStyles.subtleButton, 'h-8 w-8 shrink-0 px-0')}
          >
            <X size={15} strokeWidth={2} aria-hidden="true" />
          </button>
        </div>

        {displayedError && (
          <div
            ref={errorBannerRef}
            role="alert"
            aria-live="polite"
            className="mb-4 rounded-card bg-apple-red/10 px-3 py-2 text-ui-caption text-apple-red"
          >
            <div className="flex flex-wrap items-center gap-2">
              <span className="min-w-0 flex-1">{displayedError}</span>
              {(displayedError === NO_READY_AI_SERVICE_ERROR ||
                displayedError === SELECTED_AI_SERVICE_NOT_READY_ERROR) && (
                <a
                  href="/settings/providers"
                  className={cn(
                    uiStyles.dangerButton,
                    'h-7 shrink-0 gap-1 border border-apple-red/20 bg-white/70 dark:bg-white/[0.08]'
                  )}
                >
                  <span>Open AI services</span>
                  <ArrowRight size={12} strokeWidth={2.25} aria-hidden="true" />
                </a>
              )}
              {displayedError === NO_SELECTED_PROJECT_ERROR && onOpenProjectsSetup && (
                <button
                  type="button"
                  onClick={handleOpenProjectsSetup}
                  className={cn(
                    uiStyles.dangerButton,
                    'h-7 shrink-0 gap-1 border border-apple-red/20 bg-white/70 dark:bg-white/[0.08]'
                  )}
                >
                  <span>Open project settings</span>
                  <ArrowRight size={12} strokeWidth={2.25} aria-hidden="true" />
                </button>
              )}
              {displayedError.includes('Where agents work in Settings') && (
                <a
                  href="/settings/runtime"
                  className={cn(
                    uiStyles.dangerButton,
                    'h-7 shrink-0 gap-1 border border-apple-red/20 bg-white/70 dark:bg-white/[0.08]'
                  )}
                >
                  <span>Open Where agents work</span>
                  <ArrowRight size={12} strokeWidth={2.25} aria-hidden="true" />
                </a>
              )}
            </div>
          </div>
        )}

        {localEnrollment ? (
          <div className="flex flex-col gap-4">
            <div className="rounded-card border border-black/[0.08] bg-surface-pearl p-4 dark:border-white/[0.1] dark:bg-white/[0.04]">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <div className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                    This computer handles tasks
                  </div>
                  <div className="mt-1 text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
                    {localEnrollment.agent?.name ?? 'This computer agent'}
                  </div>
                </div>
                <span className={uiStyles.chip}>This computer</span>
              </div>
              <p className="mt-3 text-ui-caption text-secondary-light dark:text-secondary-dark">
                {localEnrollment.enrollment?.joinCommand
                  ? `Open ${LOCAL_AGENT_SETUP_APP_LABEL} on this computer, paste the setup text there, and keep that window open while it works. Forge will show it as an agent here, let you send tasks to it, and keep its status and history. Files stay on that computer.`
                  : `Open ${LOCAL_AGENT_SETUP_APP_LABEL} on that computer, paste the setup text there, and keep that window open while it works. Forge will manage its tasks, status, and history while files stay on that computer.`}
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
                          'rounded-button border px-3 py-1 text-ui-caption font-medium transition-colors',
                          joinOs === option.value
                            ? 'border-black/[0.08] bg-black/[0.06] text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.08] dark:text-foreground-dark'
                            : 'border-black/[0.08] bg-white text-secondary-light hover:bg-black/[0.04] hover:text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark'
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
                    className={cn(
                      uiStyles.input,
                      'h-auto resize-none px-4 py-3 font-mono text-ui-caption'
                    )}
                  />
                ) : (
                  <div
                    role="note"
                    className="rounded-card border border-apple-orange/30 bg-apple-orange/10 px-4 py-3 text-ui-caption text-secondary-light dark:text-secondary-dark"
                  >
                    Windows setup needs backup setup text. Copy the backup setup text below, paste
                    it into the setup app for Windows, and keep that window open.
                  </div>
                )}
                <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                  The pairing code inside expires in 15 minutes. If it expires, choose Add another
                  agent to get fresh setup text.
                </p>
                <p
                  data-testid="local-agent-paste-hint"
                  className="mt-1 text-ui-caption font-medium text-foreground-light dark:text-foreground-dark"
                >
                  {selectedJoinCommandReady
                    ? localAgentSetupPasteHint(joinOs)
                    : 'Use the backup setup text below for Windows.'}
                </p>
                <div className="mt-2 grid gap-1.5 rounded-card border border-black/[0.06] bg-black/[0.025] px-3 py-2 text-ui-caption text-secondary-light dark:border-white/[0.08] dark:bg-white/[0.04] dark:text-secondary-dark">
                  <p>1. Copy the setup text.</p>
                  <p>2. Paste it into that window on the computer that will do the work.</p>
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
                <details className="mt-3" open={showBackupSetup}>
                  <summary
                    className="cursor-pointer text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                    onClick={(event) => {
                      event.preventDefault()
                      if (selectedJoinCommandReady) {
                        setBackupSetupOpen((open) => !open)
                      }
                    }}
                  >
                    If the setup text does not work
                  </summary>
                  {showBackupSetup && (
                    <>
                      <p className="mt-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
                        Use this backup only if the setup text above does not work on this computer.
                        Copy this backup setup text into the same window, then keep that window
                        open.
                      </p>
                      <textarea
                        id="local-agent-command"
                        aria-label="Backup setup text"
                        readOnly
                        value={localEnrollment.enrollment?.shellExports ?? ''}
                        rows={6}
                        className={cn(
                          uiStyles.input,
                          'mt-2 h-auto resize-none px-4 py-3 font-mono text-ui-caption'
                        )}
                      />
                      <div className="mt-2 flex justify-end">
                        <button
                          type="button"
                          onClick={() => void handleCopyCommand()}
                          className={cn(uiStyles.secondaryButton, 'gap-2 text-ui-caption')}
                        >
                          {copiedCommand ? (
                            <Check size={13} strokeWidth={2.25} aria-hidden="true" />
                          ) : (
                            <Copy size={13} strokeWidth={2.25} aria-hidden="true" />
                          )}
                          {copiedCommand ? 'Copied' : 'Copy backup setup'}
                        </button>
                      </div>
                    </>
                  )}
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
                  className={cn(
                    uiStyles.input,
                    'h-auto resize-none px-4 py-3 font-mono text-ui-caption'
                  )}
                />
                <div className="mt-2 grid gap-1.5 rounded-card border border-black/[0.06] bg-black/[0.025] px-3 py-2 text-ui-caption text-secondary-light dark:border-white/[0.08] dark:bg-white/[0.04] dark:text-secondary-dark">
                  <p>1. Copy the setup text.</p>
                  <p>
                    2. Paste it into {LOCAL_AGENT_SETUP_APP_LABEL} on the computer that will do the
                    work.
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
              </div>
            )}

            {copyError && (
              <p role="alert" aria-live="polite" className="text-ui-caption text-apple-red">
                {copyError}
              </p>
            )}

            <div className="flex flex-wrap justify-end gap-2">
              <button
                type="button"
                onClick={handleCreateAnother}
                className={cn(uiStyles.secondaryButton, 'px-4')}
              >
                Add another agent
              </button>
              {localEnrollment.enrollment?.joinCommand ? (
                <button
                  type="button"
                  onClick={() => {
                    if (selectedJoinCommandReady) void handleCopyJoinCommand(selectedJoinCommand)
                  }}
                  disabled={!selectedJoinCommandReady}
                  className={cn(uiStyles.primaryButton, 'gap-2 px-4')}
                >
                  {copiedJoin ? (
                    <Check size={14} strokeWidth={2.25} aria-hidden="true" />
                  ) : (
                    <Copy size={14} strokeWidth={2.25} aria-hidden="true" />
                  )}
                  {!selectedJoinCommandReady
                    ? 'Use backup setup text'
                    : copiedJoin
                      ? 'Copied'
                      : 'Copy setup text'}
                </button>
              ) : (
                <button
                  type="button"
                  onClick={() => void handleCopyCommand()}
                  className={cn(uiStyles.primaryButton, 'gap-2 px-4')}
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
                className={cn(uiStyles.secondaryButton, 'px-4')}
              >
                Close and watch Agents
              </button>
            </div>
          </div>
        ) : (
          <form onSubmit={handleSubmit(handleFormSubmit)} className="flex flex-col gap-4">
            <div>
              <label className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                Where should this agent work?
              </label>
              <div
                className="grid gap-2 sm:grid-cols-3"
                role="radiogroup"
                aria-label="Where should this agent work?"
              >
                {AGENT_KIND_OPTIONS.map((option) => {
                  const selected = kind === option.value
                  return (
                    <label
                      key={option.value}
                      className={cn(
                        'min-h-[96px] cursor-pointer rounded-card border px-3 py-2.5 text-left transition-colors',
                        selected
                          ? 'border-black/[0.08] bg-black/[0.06] text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.08] dark:text-foreground-dark'
                          : 'border-black/[0.08] bg-white text-foreground-light hover:bg-black/[0.03] dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.07]'
                      )}
                    >
                      <input
                        type="radio"
                        value={option.value}
                        {...register('kind')}
                        className="sr-only"
                      />
                      <span className="flex items-start justify-between gap-2">
                        <span className="text-ui-button font-semibold">{option.label}</span>
                        <span className={cn(uiStyles.badge, 'shrink-0')}>{option.badge}</span>
                      </span>
                      <span className="mt-2 block text-ui-caption leading-snug text-secondary-light dark:text-secondary-dark">
                        {option.summary}
                      </span>
                    </label>
                  )
                })}
              </div>
              <p
                data-testid="agent-kind-recommendation"
                className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
              >
                {kind === 'cli'
                  ? 'Recommended: Project files for most tasks that need shared files or code changes.'
                  : kind === 'local-cli'
                    ? 'Use this only when files must stay on this computer. Forge still tracks tasks, status, and history.'
                    : 'Use this for questions and result checks in chat. It cannot take Tasks, change files, or use apps.'}
              </p>
            </div>

            <div className="flex flex-col gap-2">
              <button
                type="button"
                data-testid="agent-runtime-details-toggle"
                aria-expanded={runtimeDetailsOpen}
                aria-controls="agent-runtime-fit"
                onClick={() => setRuntimeDetailsOpen((open) => !open)}
                className={cn(uiStyles.secondaryButton, 'min-h-8 w-fit')}
              >
                <ChevronDown
                  size={14}
                  strokeWidth={2.25}
                  aria-hidden="true"
                  className={cn('transition-transform', runtimeDetailsOpen ? 'rotate-180' : '')}
                />
                <span>{runtimeDetailsOpen ? 'Hide option details' : 'Why this option?'}</span>
              </button>

              {runtimeDetailsOpen ? (
                <section
                  id="agent-runtime-fit"
                  data-testid="agent-runtime-fit"
                  className="rounded-card border border-black/[0.06] bg-black/[0.025] px-3 py-2.5 dark:border-white/[0.08] dark:bg-white/[0.04]"
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
                    <span className={cn(uiStyles.chip, 'shrink-0')}>
                      {kind === 'cli'
                        ? 'Can edit files'
                        : kind === 'local-cli'
                          ? 'Uses this computer'
                          : 'Questions only'}
                    </span>
                  </div>
                  <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                    {runtimeFit.detail}
                  </p>
                  <div className="mt-2 grid gap-1.5 sm:grid-cols-3">
                    {runtimeFit.items.map((item) => (
                      <div
                        key={item.label}
                        className="min-w-0 rounded-card bg-white px-2 py-1.5 dark:bg-black/20"
                      >
                        <span className="block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                          {item.label}
                        </span>
                        <span className="mt-0.5 block truncate text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
                          {item.value}
                        </span>
                      </div>
                    ))}
                  </div>
                </section>
              ) : null}
            </div>

            <div className="flex flex-col gap-2">
              <button
                type="button"
                data-testid="agent-starter-template-toggle"
                aria-expanded={starterTemplatesOpen}
                aria-controls="agent-starter-templates"
                onClick={() => setStarterTemplatesOpen((open) => !open)}
                className={cn(uiStyles.secondaryButton, 'min-h-8 w-fit')}
              >
                <ChevronDown
                  size={14}
                  strokeWidth={2.25}
                  aria-hidden="true"
                  className={cn('transition-transform', starterTemplatesOpen ? 'rotate-180' : '')}
                />
                <span>
                  {starterTemplatesOpen ? 'Hide starter templates' : 'Choose a starter template'}
                </span>
              </button>

              {starterTemplatesOpen ? (
                <section
                  id="agent-starter-templates"
                  className="rounded-card border border-black/[0.06] bg-black/[0.025] px-3 py-2.5 dark:border-white/[0.08] dark:bg-white/[0.04]"
                >
                  <div className="mb-2 flex items-center justify-between gap-2">
                    <span className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                      Pick a starter template
                    </span>
                    <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                      Fills in the name and how this agent should work
                    </span>
                  </div>
                  <div
                    role="group"
                    aria-label="Agent starter templates"
                    className="grid gap-2 sm:grid-cols-2"
                  >
                    {AGENT_STARTER_TEMPLATES.map((template) => (
                      <button
                        key={template.id}
                        type="button"
                        onClick={() => applyStarterTemplate(template)}
                        aria-pressed={selectedTemplateId === template.id}
                        className={cn(
                          'flex min-h-16 items-center gap-3 rounded-card border px-3 py-2 text-left transition-colors',
                          selectedTemplateId === template.id
                            ? 'border-black/[0.08] bg-black/[0.06] text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.08] dark:text-foreground-dark'
                            : 'border-black/[0.08] bg-black/[0.02] text-foreground-light hover:bg-black/[0.04] dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.07]'
                        )}
                      >
                        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-card bg-white text-apple-blue dark:bg-black/20">
                          <template.Icon size={15} strokeWidth={2.25} aria-hidden="true" />
                        </span>
                        <span className="min-w-0">
                          <span className="block text-ui-button font-semibold">
                            {template.label}
                          </span>
                          <span className="block truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
                            {template.summary}
                          </span>
                        </span>
                      </button>
                    ))}
                  </div>
                </section>
              ) : selectedTemplateId ? (
                <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                  Starter template selected. Open templates again to choose a different one.
                </p>
              ) : (
                <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                  Optional. Skip this if you already know the name and instructions.
                </p>
              )}
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
                className={cn(uiStyles.input, 'h-10 px-4')}
                placeholder="e.g. Result checker"
                autoFocus
              />
            </div>

            <div data-testid="agent-work-readiness">
              <div className="mb-1 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                {kind === 'provider' ? 'Where to use it' : 'Project for new tasks'}
              </div>
              <div className="w-full rounded-card border border-black/[0.08] bg-white px-4 py-2 text-ui-body text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark">
                {kind === 'provider'
                  ? 'Open this agent from Chat. It does not need a project or a place for new tasks.'
                  : (selectedProject?.name ?? 'Choose a project later')}
              </div>
              <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                {selectedProject
                  ? kind === 'local-cli'
                    ? 'Project ready. Tasks default to this project. File access stays on the joined computer.'
                    : kind === 'provider'
                      ? 'Use it for questions and result checks. It cannot take Tasks or change project files.'
                      : 'Project ready. Tasks default to this project. Forge opens the shared project folder for this agent.'
                  : kind === 'provider'
                    ? 'Use it for questions and result checks. It cannot take Tasks or change project files.'
                    : 'Open project settings to create or choose a project before creating this project files agent.'}
              </p>
              {!selectedProject && onOpenProjectsSetup ? (
                <button
                  type="button"
                  onClick={handleOpenProjectsSetup}
                  className={cn(uiStyles.secondaryButton, 'mt-2')}
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
                  {kind === 'local-cli'
                    ? 'Tool for files on this computer'
                    : 'Tool for file changes'}
                </label>
                <select
                  id="agent-cli-tool"
                  {...register('cliTool')}
                  className={cn(uiStyles.select, 'h-10 w-full px-4')}
                >
                  {CLI_TOOLS.map((tool) => (
                    <option key={tool.value} value={tool.value}>
                      {tool.label}
                    </option>
                  ))}
                </select>
                <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                  Keep the suggested tool unless an owner or admin tells you which tool this team
                  uses to change project files.
                </p>
              </div>
            )}

            {kind === 'provider' && (
              <>
                <div
                  data-testid="simple-chat-limits"
                  className="rounded-card border border-apple-orange/20 bg-apple-orange/[0.06] px-3 py-2.5 text-ui-caption text-secondary-light dark:text-secondary-dark"
                >
                  <p className="text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
                    Simple chat answers questions only
                  </p>
                  <p className="mt-1">
                    It can answer questions and check text in chat. It cannot take Tasks, edit
                    files, or use apps.
                  </p>
                  <p className="mt-1 font-medium text-foreground-light dark:text-foreground-dark">
                    Need Tasks or code changes? Choose Project files for shared project files, or
                    This computer for local files and apps.
                  </p>
                </div>
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
                        className={cn(uiStyles.select, 'h-10 w-full px-4')}
                      >
                        {providerOptions.map((option) => (
                          <option key={option.id} value={option.id}>
                            {option.label} · {providerOptionStatusLabel(option)}
                          </option>
                        ))}
                      </select>
                      <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                        {selectedProviderReady
                          ? 'Choose the AI service you set up in Settings.'
                          : 'Choose Check connection in Settings before creating this simple chat agent.'}
                      </p>
                    </div>
                    <div>
                      <div className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                        Answer setting from Settings
                      </div>
                      <div className="w-full rounded-card border border-black/[0.08] bg-black/[0.025] px-4 py-2 text-ui-body text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark">
                        {selectedProviderReady ? 'Ready' : 'Check connection first'}
                      </div>
                      <p
                        id="agent-model-help"
                        className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
                      >
                        {selectedProviderReady
                          ? 'Forge uses the answer setting that is already checked in Settings. You do not need to choose anything else here.'
                          : 'Open AI services in Settings and choose Check connection before creating this simple chat agent.'}
                      </p>
                    </div>
                  </>
                ) : (
                  <div
                    data-testid="provider-empty-hint"
                    className="rounded-card border border-apple-orange/20 bg-apple-orange/[0.06] px-3 py-2.5"
                  >
                    <p className="text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
                      Add and check an AI service first
                    </p>
                    <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                      Open AI services in Settings, add a service, paste the key from that service,
                      save it, then choose Check connection. Come back when the service shows Ready.
                    </p>
                    <a href="/settings/providers" className={cn(uiStyles.secondaryButton, 'mt-2')}>
                      <span>Open AI services</span>
                      <ArrowRight size={13} strokeWidth={2.25} aria-hidden="true" />
                    </a>
                  </div>
                )}
                <div>
                  <label
                    htmlFor="systemPrompt"
                    className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                  >
                    Tell this agent how to answer
                  </label>
                  <textarea
                    id="systemPrompt"
                    {...register('systemPrompt')}
                    rows={4}
                    placeholder="e.g. Help check task results, explain risks in plain language, and list the next step."
                    className={cn(uiStyles.input, 'h-auto resize-none px-4 py-3')}
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
                  className={cn(uiStyles.input, 'h-10 px-4')}
                  placeholder={kind === 'local-cli' ? '/Users/me/projects/app' : DEFAULT_AGENT_CWD}
                />
                <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                  {kind === 'local-cli'
                    ? 'Leave blank to use the folder where you paste the setup text.'
                    : 'Keep the suggested folder unless an owner gives you a different one. New tasks start from the project shown above.'}
                </p>
                {kind === 'local-cli' && (
                  <div
                    data-testid="local-agent-before-create"
                    className={cn(uiStyles.note, 'mt-2')}
                  >
                    <p className="font-semibold text-foreground-light dark:text-foreground-dark">
                      Before you create this computer agent
                    </p>
                    <ol className="mt-1 list-decimal space-y-1 pl-4">
                      <li>
                        Choose the folder this computer should work in. If you are not sure, leave
                        it blank.
                      </li>
                      <li>
                        After you choose Add agent, Forge shows the setup text and the app to paste
                        it into.
                      </li>
                      <li>Success looks like this agent changing to Ready on the Agents page.</li>
                    </ol>
                  </div>
                )}
              </div>
            )}

            {selectedProjectId && kind !== 'provider' && (
              <div>
                <label
                  htmlFor="agent-group"
                  className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                >
                  Place for new tasks
                </label>
                {groups.length > 0 ? (
                  <>
                    <select
                      id="agent-group"
                      {...register('groupId')}
                      className={cn(uiStyles.select, 'h-10 w-full px-4')}
                    >
                      <option value="">Set this later</option>
                      {groups.map((g) => (
                        <option key={g.id} value={g.id}>
                          {waitingPlaceDisplayName(g.name)}
                        </option>
                      ))}
                    </select>
                    <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                      New tasks wait here until an available agent can take them.
                    </p>
                  </>
                ) : (
                  <div>
                    <button
                      type="button"
                      onClick={handleCreateDefaultGroup}
                      disabled={creatingGroup}
                      className={cn(uiStyles.secondaryButton, 'h-10 w-full px-4')}
                    >
                      <Plus size={14} strokeWidth={2.25} aria-hidden="true" />
                      {creatingGroup ? 'Creating…' : 'Set up place'}
                    </button>
                    <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                      This creates a starter place for this project so new tasks have somewhere to
                      wait.
                    </p>
                  </div>
                )}
              </div>
            )}

            <section data-testid="agent-create-review" className={cn(uiStyles.note, 'px-3 py-2.5')}>
              <p className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
                Before you create
              </p>
              <div className="mt-2 grid gap-1.5 sm:grid-cols-2">
                {reviewItems.map((item) => (
                  <div
                    key={item.label}
                    className="min-w-0 rounded-card bg-white px-2 py-1.5 dark:bg-black/20"
                  >
                    <span className="block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
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
                className={cn(uiStyles.secondaryButton, 'px-4')}
              >
                Cancel
              </button>
              <button
                type={projectRequiredBeforeCreate ? 'button' : 'submit'}
                onClick={projectRequiredBeforeCreate ? handleOpenProjectsSetup : undefined}
                disabled={loading}
                className={cn(uiStyles.primaryButton, 'px-4')}
              >
                {projectRequiredBeforeCreate
                  ? 'Open project settings'
                  : loading
                    ? 'Adding…'
                    : 'Add agent'}
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  )
}
