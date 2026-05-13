import { useEffect, useMemo, type ComponentType, type SVGProps } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import {
  ArrowRight,
  Bot,
  CheckCircle2,
  Circle,
  FolderKanban,
  KeyRound,
  Layers3,
  MessageSquare,
  Users,
} from 'lucide-react'
import { useNavigationStore } from '@app/entities/navigation'
import { useAgentsStore } from '@app/shared/model/agents.store'
import { useSettingsStore } from '@app/shared/model/settings.store'
import { cn } from '@app/shared/lib/utils'

type IconComponent = ComponentType<SVGProps<SVGSVGElement> & { size?: number | string }>

interface SetupStep {
  id: string
  title: string
  detail: string
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
  const agents = useAgentsStore((state) => state.agents)
  const loadAgents = useAgentsStore((state) => state.loadAgents)

  useEffect(() => {
    void Promise.allSettled([loadOrgs(), loadProviders(), loadAgents()])
  }, [loadAgents, loadOrgs, loadProviders])

  const projects = useMemo(() => Object.values(projectsByTeam).flat(), [projectsByTeam])
  const selectedProject = useMemo(
    () => projects.find((project) => project.id === selectedProjectId) ?? null,
    [projects, selectedProjectId]
  )
  const firstProviderAgent = useMemo(
    () => agents.find((agent) => !agent.cliTool) ?? agents[0] ?? null,
    [agents]
  )

  const steps = useMemo<SetupStep[]>(
    () => [
      {
        id: 'team',
        title: t('gettingStarted.steps.team.title'),
        detail: teams.length > 0 ? teams[0].name : t('gettingStarted.steps.team.empty'),
        complete: teams.length > 0,
        path: '/settings/teams',
        cta:
          teams.length > 0
            ? t('gettingStarted.steps.team.review')
            : t('gettingStarted.steps.team.create'),
        Icon: Users,
      },
      {
        id: 'project',
        title: t('gettingStarted.steps.project.title'),
        detail:
          projects.length > 0
            ? (selectedProject?.name ?? projects[0].name)
            : t('gettingStarted.steps.project.empty'),
        complete: projects.length > 0,
        path: '/settings/projects',
        cta:
          projects.length > 0
            ? t('gettingStarted.steps.project.review')
            : t('gettingStarted.steps.project.create'),
        Icon: FolderKanban,
      },
      {
        id: 'provider',
        title: t('gettingStarted.steps.provider.title'),
        detail:
          providers.length > 0
            ? providers[0].displayName || providers[0].provider
            : t('gettingStarted.steps.provider.empty'),
        complete: providers.length > 0,
        path: '/settings/providers',
        cta:
          providers.length > 0
            ? t('gettingStarted.steps.provider.review')
            : t('gettingStarted.steps.provider.create'),
        Icon: KeyRound,
      },
      {
        id: 'agent',
        title: t('gettingStarted.steps.agent.title'),
        detail: firstProviderAgent?.name ?? t('gettingStarted.steps.agent.empty'),
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
          agentGroups.length > 0
            ? agentGroups[0].name
            : selectedProject
              ? t('gettingStarted.steps.routing.emptyWithProject')
              : t('gettingStarted.steps.routing.emptyWithoutProject'),
        complete: agentGroups.length > 0,
        path: '/agents',
        cta:
          agentGroups.length > 0
            ? t('gettingStarted.steps.routing.review')
            : t('gettingStarted.steps.routing.create'),
        Icon: Layers3,
      },
      {
        id: 'agent-history',
        title: t('gettingStarted.steps.history.title'),
        detail:
          agents.length > 0
            ? t('gettingStarted.steps.history.ready')
            : t('gettingStarted.steps.history.empty'),
        complete: agents.length > 0,
        path: '/agents',
        cta: t('gettingStarted.steps.history.open'),
        Icon: MessageSquare,
      },
    ],
    [agentGroups, agents, firstProviderAgent, projects, providers, selectedProject, t, teams]
  )

  const completeCount = steps.filter((step) => step.complete).length
  const progress = Math.round((completeCount / steps.length) * 100)

  function go(path: string) {
    void navigate({ to: path })
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
          <div className="mt-5 h-2 overflow-hidden rounded-full bg-black/[0.06] dark:bg-white/[0.08]">
            <div className="h-full rounded-full bg-apple-blue" style={{ width: `${progress}%` }} />
          </div>
        </div>

        <div className="rounded-card border border-black/[0.08] bg-white p-5 dark:border-white/[0.1] dark:bg-[#2a2a2c]">
          <p className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            {t('gettingStarted.currentProject')}
          </p>
          <p className="mt-2 truncate text-ui-body text-secondary-light dark:text-secondary-dark">
            {selectedProject?.name ?? t('gettingStarted.noProject')}
          </p>
          <button
            type="button"
            onClick={() => go('/settings/projects')}
            className="mt-4 inline-flex h-10 items-center justify-center gap-2 rounded-full bg-black/[0.04] px-4 text-ui-button font-medium text-foreground-light transition-colors hover:bg-black/[0.08] dark:bg-white/[0.06] dark:text-foreground-dark dark:hover:bg-white/[0.1]"
          >
            <FolderKanban size={14} strokeWidth={2.25} aria-hidden="true" />
            {t('gettingStarted.projects')}
          </button>
        </div>
      </section>

      <section className="mt-4 grid gap-3 lg:grid-cols-2">
        {steps.map((step) => (
          <SetupStepItem key={step.id} step={step} onNavigate={go} />
        ))}
      </section>
    </div>
  )
}

function SetupStepItem({
  step,
  onNavigate,
}: {
  step: SetupStep
  onNavigate: (path: string) => void
}) {
  const StatusIcon = step.complete ? CheckCircle2 : Circle
  const Icon = step.Icon

  return (
    <article
      className={cn(
        'flex min-w-0 items-center gap-4 rounded-card border bg-white p-4 dark:bg-[#2a2a2c]',
        step.complete ? 'border-apple-blue/25' : 'border-black/[0.08] dark:border-white/[0.1]'
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
            {step.title}
          </h3>
        </div>
        <p className="mt-1 truncate text-ui-body text-secondary-light dark:text-secondary-dark">
          {step.detail}
        </p>
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
