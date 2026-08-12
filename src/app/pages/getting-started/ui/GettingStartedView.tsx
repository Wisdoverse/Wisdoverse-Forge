import { useEffect, useMemo, useRef, useState } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { ArrowRight, CheckCircle2 } from 'lucide-react'
import { useNavigationStore } from '@app/entities/navigation'
import { orchestrationApi, type TaskSummary } from '@app/shared/api/orchestration'
import { agentAiServiceLabel, isHostCliAgent, useAgentsStore } from '@app/entities/agent'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
import { PreferenceGuideDisclosure, useSettingsStore } from '@app/entities/settings'
import type { LlmProviderConfig } from '@app/shared/api/legacy/settingsApi'
import { useSkillsStore } from '@app/entities/skill'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import {
  getGettingStartedProgress,
  summarizeGettingStartedTasks,
} from '@app/shared/lib/gettingStartedProgress'

interface SetupStep {
  id: string
  title: string
  detail: string
  why: string
  success: string
  complete: boolean
  path: string
  cta: string
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
  const [skipSaving, setSkipSaving] = useState(false)
  const [skipError, setSkipError] = useState<string | null>(null)

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
    () => summarizeGettingStartedTasks([...localTasks, ...loadedTasks]),
    [loadedTasks, localTasks]
  )
  const firstAgent = useMemo(() => agents[0] ?? null, [agents])
  const cliExecutionAgent = useMemo(
    () =>
      agents.find(
        (agent) =>
          agent.cliTool &&
          agent.status !== 'offline' &&
          (isHostCliAgent(agent) || Boolean(agent.containerId))
      ) ?? null,
    [agents]
  )
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
  const verifiedProviderLabel = verifiedProvider ? providerDisplayLabel(verifiedProvider) : null
  const executionCredentialPath = verifiedProvider
    ? '/settings/providers'
    : cliExecutionAgent
      ? '/agents'
      : providers.length > 0
        ? '/settings/providers'
        : runtimeReady
          ? '/settings/work-tool-sign-ins'
          : '/settings/providers'
  const workspaceDetail =
    selectedProject?.name ??
    projects[0]?.name ??
    teams[0]?.name ??
    t('gettingStarted.steps.workspace.empty')
  const hasReusableLearning = skills.length > 0 || taskSnapshot.appliedSkills > 0
  const checklistProgress = useMemo(
    () =>
      getGettingStartedProgress({
        hasWorkspace: teams.length > 0 && projects.length > 0,
        runtimeReady,
        executionCredentialReady,
        hasAgent: agents.length > 0,
        hasRouting: Boolean(taskGroupId),
        taskSnapshot,
        hasReusableLearning,
      }),
    [
      agents.length,
      executionCredentialReady,
      hasReusableLearning,
      projects.length,
      runtimeReady,
      taskGroupId,
      taskSnapshot,
      teams.length,
    ]
  )
  const { completion, completeCount, total: stepCount } = checklistProgress
  const firstTaskPath = taskGroupId ? '/tasks' : selectedProject ? '/agents' : '/settings/projects'
  const firstTaskCta =
    taskSnapshot.total > 0
      ? t('gettingStarted.steps.task.open')
      : taskGroupId
        ? t('gettingStarted.steps.task.create')
        : selectedProject
          ? t('gettingStarted.steps.routing.create')
          : t('gettingStarted.steps.workspace.create')

  const steps = useMemo<SetupStep[]>(
    () => [
      {
        id: 'workspace',
        title: t('gettingStarted.steps.workspace.title'),
        detail: workspaceDetail,
        why: t('gettingStarted.steps.workspace.why'),
        success: t('gettingStarted.steps.workspace.success'),
        complete: completion.workspace,
        path: '/settings/projects',
        cta:
          teams.length > 0 && projects.length > 0
            ? t('gettingStarted.steps.workspace.review')
            : t('gettingStarted.steps.workspace.create'),
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
        complete: completion.runtime,
        path: '/settings/runtime',
        cta: runtimeReady
          ? t('gettingStarted.steps.runtime.review')
          : t('gettingStarted.steps.runtime.open'),
      },
      {
        id: 'provider',
        title: t('gettingStarted.steps.provider.title'),
        detail: verifiedProviderLabel
          ? verifiedProviderLabel
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
        complete: completion.provider,
        path: executionCredentialPath,
        cta: executionCredentialReady
          ? verifiedProvider
            ? t('gettingStarted.steps.provider.reviewProviders')
            : t('gettingStarted.steps.provider.reviewAgents')
          : providers.length > 0
            ? t('gettingStarted.steps.provider.test')
            : runtimeReady
              ? t('gettingStarted.steps.provider.signInTool')
              : t('gettingStarted.steps.provider.create'),
      },
      {
        id: 'agent',
        title: t('gettingStarted.steps.agent.title'),
        detail: firstAgent?.name ?? t('gettingStarted.steps.agent.empty'),
        why: t('gettingStarted.steps.agent.why'),
        success: t('gettingStarted.steps.agent.success'),
        complete: completion.agent,
        path: '/agents',
        cta:
          agents.length > 0
            ? t('gettingStarted.steps.agent.review')
            : t('gettingStarted.steps.agent.create'),
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
        complete: completion.routing,
        path: '/agents',
        cta: taskGroupId
          ? t('gettingStarted.steps.routing.review')
          : t('gettingStarted.steps.routing.create'),
      },
      {
        id: 'task',
        title: t('gettingStarted.steps.task.title'),
        detail:
          taskSnapshot.total > 0
            ? t('gettingStarted.steps.task.ready', { count: taskSnapshot.total })
            : taskGroupId
              ? t('gettingStarted.steps.task.emptyWithRouting')
              : selectedProject
                ? t('gettingStarted.steps.task.emptyWithoutRouting')
                : t('gettingStarted.steps.task.emptyWithoutProject'),
        why: t('gettingStarted.steps.task.why'),
        success: t('gettingStarted.steps.task.success'),
        complete: completion.task,
        path: firstTaskPath,
        cta: firstTaskCta,
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
        complete: completion.review,
        path: '/tasks',
        cta: t('gettingStarted.steps.review.open'),
      },
      {
        id: 'reuse',
        title: t('gettingStarted.steps.reuse.title'),
        detail: hasReusableLearning
          ? t('gettingStarted.steps.reuse.ready')
          : t('gettingStarted.steps.reuse.empty'),
        why: t('gettingStarted.steps.reuse.why'),
        success: t('gettingStarted.steps.reuse.success'),
        complete: completion.reuse,
        path: hasReusableLearning ? '/skills' : '/context',
        cta: hasReusableLearning
          ? t('gettingStarted.steps.reuse.open')
          : t('gettingStarted.steps.reuse.review'),
      },
    ],
    [
      agentGroups,
      agents,
      cliExecutionAgent,
      completion,
      executionCredentialPath,
      executionCredentialReady,
      firstAgent,
      firstTaskCta,
      firstTaskPath,
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
      verifiedProviderLabel,
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

  const nextStep = steps.find((step) => !step.complete) ?? steps[steps.length - 1]
  const setupComplete = completeCount === stepCount
  const completedSteps = steps.filter((step) => step.complete)
  const laterSteps = setupComplete
    ? []
    : steps.filter((step) => !step.complete && step.id !== nextStep.id)

  // Once every step is done, hide the guide from the sidebar automatically.
  // Persist exactly once: wait for the stored preference (so an already
  // dismissed guide is not re-patched) and remember the write across renders.
  // Fresh /start visits skip the guide when the stored preference is already hidden.
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

  async function skipGuide() {
    // Optimistic store update hides the sidebar entry immediately; the store
    // reverts it if the server rejects the patch.
    setSkipError(null)
    setSkipSaving(true)
    const ok = await setGettingStartedDismissed(true)
    setSkipSaving(false)
    if (!ok) {
      setSkipError(t('gettingStarted.skipError'))
      return
    }
    void navigate({ to: '/tasks' })
  }

  return (
    <div
      data-testid="page-start"
      className="min-h-full overflow-y-auto bg-background-light px-4 py-5 dark:bg-background-dark sm:px-6"
    >
      <header className="rounded-card border border-black/[0.08] bg-white px-4 py-3 dark:border-white/[0.1] dark:bg-surface-dark">
        <div className="flex flex-wrap items-center gap-3">
          <h2 className="min-w-0 text-ui-doc-title text-foreground-light dark:text-foreground-dark">
            {t('gettingStarted.title')}
          </h2>
          <span
            data-testid="getting-started-progress"
            className="rounded-button bg-black/[0.05] px-2 py-1 text-ui-caption font-medium tabular-nums text-secondary-light dark:bg-white/[0.08] dark:text-secondary-dark"
          >
            {completeCount}/{stepCount}
          </span>
          <button
            type="button"
            data-testid="getting-started-skip"
            onClick={skipGuide}
            disabled={skipSaving}
            className="ml-auto text-ui-caption font-medium text-secondary-light underline-offset-2 transition-colors hover:text-foreground-light hover:underline focus-visible:underline focus-visible:outline-none disabled:cursor-wait disabled:opacity-60 dark:text-secondary-dark dark:hover:text-foreground-dark"
          >
            {skipSaving ? t('gettingStarted.skipSaving') : t('gettingStarted.skip')}
          </button>
        </div>
        {skipError && (
          <p
            role="alert"
            aria-live="polite"
            className="mt-2 text-ui-caption font-medium text-apple-red"
          >
            {skipError}
          </p>
        )}
      </header>

      <section className="mt-4 grid gap-3">
        {!setupComplete && (
          <SetupStepItem
            step={nextStep}
            index={steps.findIndex((step) => step.id === nextStep.id)}
            isNext
            onNavigate={go}
          />
        )}
        {completedSteps.length > 0 && (
          <SetupStepGroup
            title={t('gettingStarted.completedStepsTitle')}
            steps={completedSteps}
            allSteps={steps}
            testId="getting-started-completed-steps"
            collapsed
          />
        )}
        {laterSteps.length > 0 && (
          <SetupStepGroup
            title={t('gettingStarted.laterStepsTitle')}
            steps={laterSteps}
            allSteps={steps}
            testId="getting-started-later-steps"
          />
        )}
      </section>
    </div>
  )
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

function providerDisplayLabel(provider: LlmProviderConfig): string {
  const displayName = provider.displayName.trim()
  return displayName || agentAiServiceLabel(provider.provider)
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
  const statusLabel = step.complete
    ? t('gettingStarted.stepStatus.done')
    : isNext
      ? t('gettingStarted.stepStatus.next')
      : t('gettingStarted.stepStatus.later')

  return (
    <article
      data-testid="getting-started-expanded-step"
      className="rounded-card border border-black/[0.08] bg-white px-4 py-3 dark:border-white/[0.1] dark:bg-surface-dark"
    >
      <div className="flex min-w-0 items-start gap-3">
        <span
          aria-hidden="true"
          className={cn(
            'mt-1.5 h-2 w-2 shrink-0 rounded-full',
            step.complete
              ? 'bg-apple-green'
              : isNext
                ? 'bg-apple-blue'
                : 'bg-secondary-light/60 dark:bg-secondary-dark/60'
          )}
        />
        <div className="min-w-0 flex-1">
          <div className="flex flex-col gap-2 sm:flex-row sm:items-start">
            <div className="min-w-0 flex-1">
              <h3 className="truncate text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
                <span className="mr-1 tabular-nums text-secondary-light dark:text-secondary-dark">
                  {index + 1}.
                </span>
                {step.title}
                <span className="sr-only"> — {statusLabel}</span>
              </h3>
              <p className="mt-0.5 truncate text-ui-body text-secondary-light dark:text-secondary-dark">
                {step.detail}
              </p>
            </div>
            <button
              type="button"
              onClick={() => onNavigate(step.path)}
              className={cn(uiStyles.primaryButton, 'w-full shrink-0 px-4 sm:w-auto')}
            >
              <span>{step.cta}</span>
              <ArrowRight size={14} strokeWidth={2.25} aria-hidden="true" />
            </button>
          </div>
          <PreferenceGuideDisclosure
            guideKey={`getting-started-${step.id}-success`}
            icon={<CheckCircle2 />}
            title={t('gettingStarted.successLabel')}
            className="mt-2"
            dismissible={false}
          >
            <p>{step.success}</p>
          </PreferenceGuideDisclosure>
        </div>
      </div>
    </article>
  )
}

function SetupStepGroup({
  title,
  steps,
  allSteps,
  testId,
  collapsed = false,
}: {
  title: string
  steps: SetupStep[]
  allSteps: SetupStep[]
  testId: string
  collapsed?: boolean
}) {
  const { t } = useTranslation()

  return (
    <details
      open={!collapsed}
      data-testid={testId}
      className="group rounded-card border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-surface-dark"
    >
      <summary className="flex cursor-pointer list-none items-center justify-between gap-3 px-4 py-3 marker:hidden">
        <span className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
          {title}
        </span>
        <span className="rounded-button bg-black/[0.04] px-2.5 py-1 text-ui-caption font-medium text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
          {steps.length}
        </span>
      </summary>
      <ol className="divide-y divide-black/[0.06] border-t border-black/[0.06] px-4 dark:divide-white/[0.08] dark:border-white/[0.08]">
        {steps.map((step) => {
          const statusLabel = step.complete
            ? t('gettingStarted.stepStatus.done')
            : t('gettingStarted.stepStatus.later')
          const stepNumber = allSteps.findIndex((candidate) => candidate.id === step.id) + 1

          return (
            <li key={step.id} className="flex min-w-0 items-start gap-3 py-3">
              <span
                aria-hidden="true"
                className={cn(
                  'mt-1.5 h-2 w-2 shrink-0 rounded-full',
                  step.complete
                    ? 'bg-apple-green'
                    : 'bg-secondary-light/60 dark:bg-secondary-dark/60'
                )}
              />
              <div className="min-w-0 flex-1">
                <p className="truncate text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
                  <span className="mr-1 tabular-nums text-secondary-light dark:text-secondary-dark">
                    {stepNumber}.
                  </span>
                  {step.title}
                  <span className="sr-only"> — {statusLabel}</span>
                </p>
                <p className="mt-0.5 truncate text-ui-body text-secondary-light dark:text-secondary-dark">
                  {step.detail}
                </p>
                <PreferenceGuideDisclosure
                  guideKey={`getting-started-${step.id}-success`}
                  icon={<CheckCircle2 />}
                  title={t('gettingStarted.successLabel')}
                  className="mt-2"
                  dismissible={false}
                >
                  <p>{step.success}</p>
                </PreferenceGuideDisclosure>
              </div>
            </li>
          )
        })}
      </ol>
    </details>
  )
}
