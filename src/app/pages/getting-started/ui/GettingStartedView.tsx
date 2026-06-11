import { useEffect, useMemo, useRef, useState, type ComponentType, type SVGProps } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import {
  ArrowRight,
  Activity,
  Bot,
  CheckCircle2,
  Circle,
  KeyRound,
  Layers3,
  ListTodo,
  Rocket,
  WandSparkles,
  Users,
} from 'lucide-react'
import { useNavigationStore } from '@app/entities/navigation'
import {
  orchestrationApi,
  taskResultArtifacts,
  type TaskSummary,
} from '@app/shared/api/orchestration'
import { isHostCliAgent, useAgentsStore } from '@app/entities/agent'
import { useBoardStore } from '@app/shared/model/board.store'
import { useSettingsStore } from '@app/shared/model/settings.store'
import { useSkillsStore } from '@app/shared/model/skills.store'
import { cn } from '@app/shared/lib/utils'

type IconComponent = ComponentType<SVGProps<SVGSVGElement> & { size?: number | string }>

interface TaskSnapshot {
  total: number
  assigned: number
  completed: number
  artifacts: number
  appliedSkills: number
}

interface SetupStep {
  id: string
  title: string
  detail: string
  why: string
  success: string
  complete: boolean
  path: string
  cta: string
  Icon: IconComponent
}

export function GettingStartedView() {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const teams = useNavigationStore((state) => state.teams)
  const projectsByTeam = useNavigationStore((state) => state.projects)
  const selectedProjectId = useNavigationStore((state) => state.selectedProjectId)
  const agentGroups = useNavigationStore((state) => state.agentGroups)
  const loadOrgs = useNavigationStore((state) => state.loadOrgs)
  const providers = useSettingsStore((state) => state.providers)
  const loadProviders = useSettingsStore((state) => state.loadProviders)
  const runtimeSettings = useSettingsStore((state) => state.runtimeSettings)
  const loadRuntimeSettings = useSettingsStore((state) => state.loadRuntimeSettings)
  const preferences = useSettingsStore((state) => state.preferences)
  const preferencesLoaded = useSettingsStore((state) => state.preferencesLoaded)
  const loadPreferences = useSettingsStore((state) => state.loadPreferences)
  const setGettingStartedDismissed = useSettingsStore((state) => state.setGettingStartedDismissed)
  const agents = useAgentsStore((state) => state.agents)
  const loadAgents = useAgentsStore((state) => state.loadAgents)
  const boardColumns = useBoardStore((state) => state.columns)
  const selectedGroupId = useBoardStore((state) => state.selectedGroupId)
  const skills = useSkillsStore((state) => state.skills)
  const loadSkills = useSkillsStore((state) => state.loadSkills)
  const [loadedTasks, setLoadedTasks] = useState<TaskSummary[]>([])

  useEffect(() => {
    void Promise.allSettled([
      loadOrgs(),
      loadProviders(),
      loadRuntimeSettings(),
      loadPreferences(),
      loadAgents(),
      loadSkills(),
    ])
  }, [loadAgents, loadOrgs, loadPreferences, loadProviders, loadRuntimeSettings, loadSkills])

  const projects = useMemo(() => Object.values(projectsByTeam).flat(), [projectsByTeam])
  const selectedProject = useMemo(
    () => projects.find((project) => project.id === selectedProjectId) ?? null,
    [projects, selectedProjectId]
  )
  const taskGroupId = selectedGroupId ?? agentGroups[0]?.id ?? null
  const localTasks = useMemo(() => Object.values(boardColumns).flat(), [boardColumns])
  const taskSnapshot = useMemo(
    () => summarizeTasks([...localTasks, ...loadedTasks]),
    [loadedTasks, localTasks]
  )
  const firstAgent = useMemo(() => agents[0] ?? null, [agents])
  const cliExecutionAgent = useMemo(() => agents.find((agent) => agent.cliTool) ?? null, [agents])
  const verifiedProvider = useMemo(
    () =>
      providers.find((provider) => provider.isEnabled && provider.lastTestStatus === 'passed') ??
      null,
    [providers]
  )
  const runtimeReady = Boolean(
    runtimeSettings &&
    runtimeSettings.availableRuntimes.length > 0 &&
    runtimeSettings.availableCliTools.length > 0
  )
  const executionCredentialReady = Boolean(verifiedProvider || cliExecutionAgent)
  const executionCredentialPath = verifiedProvider
    ? '/settings/providers'
    : providers.length > 0
      ? '/settings/providers'
      : runtimeReady
        ? '/agents'
        : '/settings/providers'
  const workspaceDetail =
    selectedProject?.name ??
    projects[0]?.name ??
    teams[0]?.name ??
    t('gettingStarted.steps.workspace.empty')
  const hasReusableLearning = skills.length > 0 || taskSnapshot.appliedSkills > 0

  const steps = useMemo<SetupStep[]>(
    () => [
      {
        id: 'workspace',
        title: t('gettingStarted.steps.workspace.title'),
        detail: workspaceDetail,
        why: t('gettingStarted.steps.workspace.why'),
        success: t('gettingStarted.steps.workspace.success'),
        complete: teams.length > 0 && projects.length > 0,
        path: '/settings/projects',
        cta:
          teams.length > 0 && projects.length > 0
            ? t('gettingStarted.steps.workspace.review')
            : t('gettingStarted.steps.workspace.create'),
        Icon: Users,
      },
      {
        id: 'runtime',
        title: t('gettingStarted.steps.runtime.title'),
        detail: runtimeSettings
          ? t('gettingStarted.steps.runtime.ready', {
              location: workLocationLabel(runtimeSettings.defaultRuntime, t),
            })
          : t('gettingStarted.steps.runtime.empty'),
        why: t('gettingStarted.steps.runtime.why'),
        success: t('gettingStarted.steps.runtime.success'),
        complete: runtimeReady,
        path: '/settings/runtime',
        cta: runtimeReady
          ? t('gettingStarted.steps.runtime.review')
          : t('gettingStarted.steps.runtime.open'),
        Icon: Activity,
      },
      {
        id: 'provider',
        title: t('gettingStarted.steps.provider.title'),
        detail: verifiedProvider
          ? verifiedProvider.displayName || verifiedProvider.provider
          : cliExecutionAgent
            ? t('gettingStarted.steps.provider.cliReady', {
                name: cliExecutionAgent.name,
                location: isHostCliAgent(cliExecutionAgent)
                  ? t('gettingStarted.workLocations.local')
                  : t('gettingStarted.workLocations.managed'),
              })
            : providers.length > 0
              ? t('gettingStarted.steps.provider.needsTest')
              : t('gettingStarted.steps.provider.empty'),
        why: t('gettingStarted.steps.provider.why'),
        success: t('gettingStarted.steps.provider.success'),
        complete: executionCredentialReady,
        path: executionCredentialPath,
        cta: executionCredentialReady
          ? verifiedProvider
            ? t('gettingStarted.steps.provider.reviewProviders')
            : t('gettingStarted.steps.provider.reviewAgents')
          : providers.length > 0
            ? t('gettingStarted.steps.provider.test')
            : runtimeReady
              ? t('gettingStarted.steps.provider.connectCli')
              : t('gettingStarted.steps.provider.create'),
        Icon: KeyRound,
      },
      {
        id: 'agent',
        title: t('gettingStarted.steps.agent.title'),
        detail: firstAgent?.name ?? t('gettingStarted.steps.agent.empty'),
        why: t('gettingStarted.steps.agent.why'),
        success: t('gettingStarted.steps.agent.success'),
        complete: agents.length > 0,
        path: '/agents',
        cta:
          agents.length > 0
            ? t('gettingStarted.steps.agent.review')
            : t('gettingStarted.steps.agent.create'),
        Icon: Bot,
      },
      {
        id: 'routing',
        title: t('gettingStarted.steps.routing.title'),
        detail:
          taskGroupId && agentGroups.length > 0
            ? (agentGroups.find((group) => group.id === taskGroupId)?.name ?? agentGroups[0].name)
            : selectedProject
              ? t('gettingStarted.steps.routing.emptyWithProject')
              : t('gettingStarted.steps.routing.emptyWithoutProject'),
        why: t('gettingStarted.steps.routing.why'),
        success: t('gettingStarted.steps.routing.success'),
        complete: Boolean(taskGroupId),
        path: '/agents',
        cta: taskGroupId
          ? t('gettingStarted.steps.routing.review')
          : t('gettingStarted.steps.routing.create'),
        Icon: Layers3,
      },
      {
        id: 'task',
        title: t('gettingStarted.steps.task.title'),
        detail:
          taskSnapshot.total > 0
            ? t('gettingStarted.steps.task.ready', { count: taskSnapshot.total })
            : taskGroupId
              ? t('gettingStarted.steps.task.emptyWithRouting')
              : t('gettingStarted.steps.task.emptyWithoutRouting'),
        why: t('gettingStarted.steps.task.why'),
        success: t('gettingStarted.steps.task.success'),
        complete: taskSnapshot.total > 0,
        path: '/tasks',
        cta:
          taskSnapshot.total > 0
            ? t('gettingStarted.steps.task.open')
            : t('gettingStarted.steps.task.create'),
        Icon: ListTodo,
      },
      {
        id: 'review',
        title: t('gettingStarted.steps.review.title'),
        detail:
          taskSnapshot.completed > 0
            ? t('gettingStarted.steps.review.ready', { count: taskSnapshot.completed })
            : taskSnapshot.assigned > 0
              ? t('gettingStarted.steps.review.inFlight')
              : t('gettingStarted.steps.review.empty'),
        why: t('gettingStarted.steps.review.why'),
        success: t('gettingStarted.steps.review.success'),
        complete: taskSnapshot.completed > 0 || taskSnapshot.artifacts > 0,
        path: '/tasks',
        cta: t('gettingStarted.steps.review.open'),
        Icon: Rocket,
      },
      {
        id: 'reuse',
        title: t('gettingStarted.steps.reuse.title'),
        detail: hasReusableLearning
          ? t('gettingStarted.steps.reuse.ready')
          : t('gettingStarted.steps.reuse.empty'),
        why: t('gettingStarted.steps.reuse.why'),
        success: t('gettingStarted.steps.reuse.success'),
        complete: hasReusableLearning,
        path: hasReusableLearning ? '/skills' : '/context',
        cta: hasReusableLearning
          ? t('gettingStarted.steps.reuse.open')
          : t('gettingStarted.steps.reuse.review'),
        Icon: WandSparkles,
      },
    ],
    [
      agentGroups,
      agents,
      cliExecutionAgent,
      executionCredentialPath,
      executionCredentialReady,
      firstAgent,
      projects,
      providers.length,
      runtimeReady,
      runtimeSettings,
      selectedProject,
      hasReusableLearning,
      taskGroupId,
      taskSnapshot,
      t,
      teams,
      verifiedProvider,
      workspaceDetail,
    ]
  )

  useEffect(() => {
    if (!taskGroupId) {
      setLoadedTasks([])
      return
    }
    let cancelled = false
    orchestrationApi
      .getTasks(taskGroupId)
      .then((tasks) => {
        if (!cancelled) setLoadedTasks(tasks)
      })
      .catch(() => {
        if (!cancelled) setLoadedTasks([])
      })
    return () => {
      cancelled = true
    }
  }, [taskGroupId])

  const completeCount = steps.filter((step) => step.complete).length
  const progress = Math.round((completeCount / steps.length) * 100)
  const nextStep = steps.find((step) => !step.complete) ?? steps[steps.length - 1]
  const NextStepIcon = nextStep.Icon
  const setupComplete = completeCount === steps.length

  // Once every step is done, hide the guide from the sidebar automatically.
  // Persist exactly once: wait for the stored preference (so an already
  // dismissed guide is not re-patched) and remember the write across renders.
  // The page itself stays reachable at /start either way.
  const autoDismissPersistedRef = useRef(false)
  useEffect(() => {
    if (!setupComplete || !preferencesLoaded) return
    if (preferences?.gettingStartedDismissed === true) return
    if (autoDismissPersistedRef.current) return
    autoDismissPersistedRef.current = true
    void setGettingStartedDismissed(true)
  }, [setupComplete, preferencesLoaded, preferences, setGettingStartedDismissed])

  function go(path: string) {
    void navigate({ to: path })
  }

  function skipGuide() {
    // Optimistic store update hides the sidebar entry immediately; the store
    // reverts it if the server rejects the patch.
    void setGettingStartedDismissed(true)
    void navigate({ to: '/tasks' })
  }

  return (
    <div data-testid="page-start" className="min-h-full overflow-y-auto px-4 py-5 sm:px-6">
      <section className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_280px]">
        <div className="rounded-card border border-black/[0.08] bg-white p-5 dark:border-white/[0.1] dark:bg-[#2a2a2c]">
          <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
            <div className="min-w-0">
              <p className="text-ui-caption font-semibold uppercase tracking-[0.08em] text-apple-blue">
                {t('gettingStarted.eyebrow')}
              </p>
              <h2 className="mt-1 text-ui-title font-semibold text-foreground-light dark:text-foreground-dark">
                {t('gettingStarted.title')}
              </h2>
              <p className="mt-1 max-w-2xl text-ui-body text-secondary-light dark:text-secondary-dark">
                {t('gettingStarted.description')}
              </p>
              <button
                type="button"
                data-testid="getting-started-skip"
                onClick={skipGuide}
                className="mt-2 text-ui-caption font-medium text-secondary-light underline-offset-2 transition-colors hover:text-foreground-light hover:underline focus-visible:underline focus-visible:outline-none dark:text-secondary-dark dark:hover:text-foreground-dark"
              >
                {t('gettingStarted.skip')}
              </button>
            </div>
            <div className="shrink-0 rounded-lg border border-black/[0.08] px-4 py-3 text-right dark:border-white/[0.1]">
              <p className="text-ui-metric font-semibold tabular-nums text-foreground-light dark:text-foreground-dark">
                {progress}%
              </p>
              <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                {t('gettingStarted.progressCount', {
                  complete: completeCount,
                  total: steps.length,
                })}
              </p>
            </div>
          </div>
          {!setupComplete && (
            <div className="mt-5 h-2 overflow-hidden rounded-full bg-black/[0.06] dark:bg-white/[0.08]">
              <div
                className="h-full rounded-full bg-apple-blue"
                style={{ width: `${progress}%` }}
              />
            </div>
          )}
        </div>

        <div className="rounded-card border border-black/[0.08] bg-white p-5 dark:border-white/[0.1] dark:bg-[#2a2a2c]">
          <p className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            {setupComplete ? t('gettingStarted.readyTitle') : t('gettingStarted.nextTitle')}
          </p>
          <p className="mt-1 text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
            {nextStep.title}
          </p>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {setupComplete ? t('gettingStarted.readyDetail') : nextStep.why}
          </p>
          <div className="mt-3 rounded-lg bg-black/[0.035] px-3 py-2 text-ui-caption text-secondary-light dark:bg-white/[0.05] dark:text-secondary-dark">
            <span className="font-medium text-foreground-light dark:text-foreground-dark">
              {t('gettingStarted.successLabel')}
            </span>{' '}
            {nextStep.success}
          </div>
          <button
            type="button"
            onClick={() => go(nextStep.path)}
            className="mt-4 inline-flex h-10 w-full items-center justify-center gap-2 rounded-full bg-apple-blue px-4 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
          >
            <NextStepIcon width={14} height={14} aria-hidden="true" />
            {nextStep.cta}
            <ArrowRight size={14} strokeWidth={2.25} aria-hidden="true" />
          </button>
          <div className="mt-4 border-t border-black/[0.06] pt-3 dark:border-white/[0.08]">
            <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
              {t('gettingStarted.currentProject')}
            </p>
            <p className="mt-1 truncate text-ui-body text-secondary-light dark:text-secondary-dark">
              {selectedProject?.name ?? t('gettingStarted.noProject')}
            </p>
          </div>
        </div>
      </section>

      <section className="mt-4 grid gap-3 lg:grid-cols-2">
        {steps.map((step, index) => (
          <SetupStepItem
            key={step.id}
            step={step}
            index={index}
            isNext={!setupComplete && nextStep.id === step.id}
            onNavigate={go}
          />
        ))}
      </section>
    </div>
  )
}

function summarizeTasks(tasks: TaskSummary[]): TaskSnapshot {
  const byId = new Map<string, TaskSummary>()
  for (const task of tasks) byId.set(task.id, task)

  let assigned = 0
  let completed = 0
  let artifacts = 0
  let appliedSkills = 0

  for (const task of byId.values()) {
    if (task.assignedTo || task.assignedAgentName) assigned += 1
    if (task.state === 'completed') completed += 1
    artifacts += taskResultArtifacts(task.result).length
    appliedSkills += task.contextCounts?.appliedSkills ?? 0
  }

  return {
    total: byId.size,
    assigned,
    completed,
    artifacts,
    appliedSkills,
  }
}

function workLocationLabel(runtime: string, t: (key: string) => string): string {
  switch (runtime) {
    case 'container':
      return t('gettingStarted.workLocations.managed')
    case 'cli':
      return t('gettingStarted.workLocations.local')
    case 'api':
      return t('gettingStarted.workLocations.textOnly')
    default:
      return t('gettingStarted.workLocations.ready')
  }
}

function SetupStepItem({
  step,
  index,
  isNext,
  onNavigate,
}: {
  step: SetupStep
  index: number
  isNext: boolean
  onNavigate: (path: string) => void
}) {
  const { t } = useTranslation()
  const StatusIcon = step.complete ? CheckCircle2 : Circle
  const Icon = step.Icon
  const statusLabel = step.complete
    ? t('gettingStarted.stepStatus.done')
    : isNext
      ? t('gettingStarted.stepStatus.next')
      : t('gettingStarted.stepStatus.later')

  return (
    <article
      className={cn(
        'flex min-w-0 items-center gap-4 rounded-card border bg-white p-4 dark:bg-[#2a2a2c]',
        step.complete || isNext
          ? 'border-apple-blue/30'
          : 'border-black/[0.08] dark:border-white/[0.1]'
      )}
    >
      <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-full bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
        <Icon size={18} strokeWidth={2} aria-hidden="true" />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <StatusIcon
            size={16}
            strokeWidth={2.25}
            className={
              step.complete ? 'text-apple-blue' : 'text-secondary-light dark:text-secondary-dark'
            }
            aria-hidden="true"
          />
          <h3 className="truncate text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            <span className="mr-1 tabular-nums text-secondary-light dark:text-secondary-dark">
              {index + 1}.
            </span>
            {step.title}
          </h3>
          <span
            className={cn(
              'shrink-0 rounded-full px-2 py-0.5 text-[10px] font-semibold',
              step.complete || isNext
                ? 'bg-apple-blue/10 text-apple-blue'
                : 'bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark'
            )}
          >
            {statusLabel}
          </span>
        </div>
        <p className="mt-1 truncate text-ui-body text-secondary-light dark:text-secondary-dark">
          {step.detail}
        </p>
        {isNext && (
          <p className="mt-1 line-clamp-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {step.why}
          </p>
        )}
      </div>
      <button
        type="button"
        onClick={() => onNavigate(step.path)}
        className="inline-flex h-10 shrink-0 items-center justify-center gap-2 rounded-full bg-apple-blue px-4 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
      >
        <span>{step.cta}</span>
        <ArrowRight size={14} strokeWidth={2.25} aria-hidden="true" />
      </button>
    </article>
  )
}
