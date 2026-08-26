import { Suspense, useEffect, useState } from 'react'
import { AlertTriangle, ArrowRight, CheckCircle2, Info, Terminal } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import {
  agentCredentialRecovery,
  agentStatusKey,
  agentStatusLabel,
  agentAvatarInitial,
  agentRuntimeLabel,
  agentToolLabel,
  isHostCliAgent,
  useAgentsStore,
  type AgentInfo,
} from '@app/entities/agent'
import { PreferenceGuideDisclosure } from '@app/entities/settings'
import {
  AgentConfigTab,
  AgentControlPanel,
  AgentKindBadge,
  AgentPluginsTab,
  AgentTasksTab,
  // Lazy component (wrapped in the barrel) — keeps xterm out of the agents
  // route's initial chunk; rendered behind the Suspense boundary below.
  AgentTerminalTab,
} from '@app/features/agents'
import { ChatView } from '@app/features/chat'
import {
  orchestrationApi,
  type FollowedSkill,
  type TaskSummary,
} from '@app/shared/api/orchestration'
import { formatRelativeTime } from '@app/shared/lib/time'

const STATUS_COLORS: Record<string, string> = {
  working: 'bg-[#1d1d1f] dark:bg-white',
  idle: 'bg-[#7a7a7a]',
  offline: 'bg-[#d2d2d7]',
}

const STATUS_FALLBACK_COLOR = 'bg-[#d2d2d7]'

function defaultGradient(): string {
  return 'bg-[#f5f5f7] text-[#1d1d1f] dark:bg-white/[0.08] dark:text-white'
}

type Tab = 'overview' | 'tasks' | 'history' | 'terminal' | 'plugins' | 'config'

interface AgentNextStep {
  title: string
  detail: string
  success: string
  ready: boolean
  targetTab?: Tab
  targetHref?: string
  targetBack?: boolean
  actionLabel?: string
}

// Live work attach is only available for platform-managed work environments.
// Agents joined from a computer keep the work window on that machine.
function tabsFor(agent: AgentInfo): { id: Tab; label: string }[] {
  const isCli = Boolean(agent.cliTool)
  const hasTerminal = isCli && !isHostCliAgent(agent)
  if (!isCli) {
    return [
      { id: 'overview', label: 'Overview' },
      { id: 'history', label: 'Chat' },
      { id: 'config', label: 'Chat instructions' },
    ]
  }
  return [
    { id: 'overview', label: 'Overview' },
    { id: 'tasks', label: 'Tasks' },
    { id: 'history', label: isCli ? 'History' : 'Chat' },
    ...(hasTerminal ? [{ id: 'terminal' as Tab, label: 'Live work' }] : []),
    { id: 'plugins', label: 'Tools' },
    { id: 'config', label: 'Instructions' },
  ]
}

function WorkspaceBoundaryNote({ agent }: { agent: AgentInfo }) {
  const hostCli = isHostCliAgent(agent)
  return (
    <div className={cn(uiStyles.note, 'flex gap-2 text-ui-caption')}>
      <Info
        size={13}
        strokeWidth={2.25}
        className="mt-0.5 shrink-0 text-apple-blue"
        aria-hidden="true"
      />
      {hostCli ? (
        <p>
          This computer does the work. Forge sends tasks and saves task history here; files stay in
          the folder where you pasted the setup text.
        </p>
      ) : agent.cliTool ? (
        <p>
          This agent can edit shared project files, which may include several projects. The selected
          project is just where new tasks begin. Use a separate set of project files when files must
          be kept apart.
        </p>
      ) : (
        <p>
          This agent answers in chat through an AI service. It can answer questions, write, and
          check text or results, but it cannot take Tasks, change code, use computer apps, or open
          project files on its own. For Tasks and code changes, use an agent with Project files or
          This computer.
        </p>
      )}
    </div>
  )
}

function agentFolderLabel(agent: AgentInfo): string {
  if (!agent.cliTool) {
    return 'No project files. Use an agent with Project files or This computer for Tasks and code changes.'
  }
  if (!agent.cwd || agent.cwd === '/workspace') {
    return isHostCliAgent(agent)
      ? 'Folder where you pasted the setup text'
      : 'Default project folder'
  }
  return isHostCliAgent(agent) ? `Selected work folder: ${agent.cwd}` : 'Shared project files'
}

function agentSetupSummary(agent: AgentInfo): string {
  if (isHostCliAgent(agent)) return 'This computer'
  if (agent.cliTool) return 'Project files'
  return 'Simple chat agent'
}

export function agentDetailHeaderSubtitle(agent: AgentInfo): string {
  return agentRuntimeLabel(agent)
}

function agentConnectionStatus(agent: AgentInfo): string {
  if (isHostCliAgent(agent)) {
    if (agent.status === 'offline') {
      return 'Use Connect this computer in Agents'
    }
    return agent.runtimeId ? 'Connected from this computer' : 'Connect from Agents'
  }
  if (agent.cliTool) return agent.containerId ? 'Ready with project files' : 'Waiting to start'
  return 'AI service is ready for chat'
}

function agentAvailabilityLabel(agent: AgentInfo): string {
  if (!agent.cliTool && !isHostCliAgent(agent) && agent.status === 'idle')
    return 'Ready for direct chat'
  if (agent.status === 'idle') return 'Ready for work'
  if (agent.status === 'working') return 'Already working'
  if (isHostCliAgent(agent)) return 'Open Agents and connect this computer again'
  if (agent.cliTool) return 'Open Live work and start project files'
  return 'Open AI services in Settings and choose Check connection'
}

interface AgentDetailViewProps {
  agent: AgentInfo
  onBack: () => void
}

export function AgentDetailView({ agent, onBack }: AgentDetailViewProps) {
  const [activeTab, setActiveTab] = useState<Tab>('overview')
  const [recentTasks, setRecentTasks] = useState<TaskSummary[]>([])
  const [recentTasksError, setRecentTasksError] = useState<string | null>(null)
  const ratePercent = Math.round(agent.successRate * 100)
  const stats = agent.cliTool
    ? [
        { label: 'Tasks done', value: String(agent.tasksCompleted) },
        { label: 'In progress', value: String(agent.tasksInProgress) },
        { label: 'Finished cleanly', value: `${ratePercent}%` },
        { label: 'Work type', value: agentSetupSummary(agent) },
      ]
    : [
        { label: 'Messages answered', value: String(agent.tasksCompleted) },
        { label: 'Replies in progress', value: String(agent.tasksInProgress) },
        { label: 'Answer success', value: `${ratePercent}%` },
        { label: 'Work type', value: agentSetupSummary(agent) },
      ]
  const tabs = tabsFor(agent)
  const statusKey = agentStatusKey(agent.status)
  const statusLabel = agentStatusLabel(agent.status)
  const detailRows = agent.cliTool
    ? [
        { label: 'Where it works', value: agentRuntimeLabel(agent) },
        { label: 'Can take work', value: statusLabel },
        { label: 'Project files it can use', value: agent.workspaceName ?? 'Shared project files' },
        { label: 'Project for new tasks', value: agent.projectName ?? 'Choose when sending work' },
        { label: 'Folder agents open', value: agentFolderLabel(agent) },
        { label: 'How it connects', value: agentConnectionStatus(agent) },
      ]
    : [
        { label: 'Where it works', value: agentRuntimeLabel(agent) },
        { label: 'Can answer', value: statusLabel },
        { label: 'What it can use', value: 'Connected AI service' },
        { label: 'Where to start', value: 'Chat' },
        { label: 'File access', value: agentFolderLabel(agent) },
        { label: 'How it connects', value: agentConnectionStatus(agent) },
      ]

  useEffect(() => {
    if (!tabs.some((tab) => tab.id === activeTab)) setActiveTab('overview')
  }, [activeTab, tabs])

  useEffect(() => {
    let cancelled = false
    if (!agent.cliTool) {
      setRecentTasks([])
      setRecentTasksError(null)
      return () => {
        cancelled = true
      }
    }
    setRecentTasksError(null)
    orchestrationApi
      .getTasksByAgent(agent.id, { limit: 5 })
      .then((tasks) => {
        if (!cancelled) {
          setRecentTasks(tasks)
          setRecentTasksError(null)
        }
      })
      .catch(() => {
        if (!cancelled) {
          setRecentTasks([])
          setRecentTasksError(
            "Go back to Agents and choose this agent again, or open Tasks to check this agent's latest task state."
          )
        }
      })
    return () => {
      cancelled = true
    }
  }, [agent.id])

  return (
    <div className="flex flex-col gap-4">
      {/* Header */}
      <div className="flex items-center gap-3">
        <button
          type="button"
          data-testid="agent-back"
          onClick={onBack}
          className={cn(uiStyles.secondaryButton, 'h-8 w-8 shrink-0 px-0')}
          aria-label="Back"
        >
          ‹
        </button>

        {/* Provider-colored avatar */}
        <div
          className={cn(
            'flex h-10 w-10 shrink-0 items-center justify-center rounded-card',
            'border border-black/[0.08] text-ui-body font-semibold select-none dark:border-white/[0.1]',
            defaultGradient()
          )}
        >
          {agentAvatarInitial(agent)}
        </div>

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <h2 className="truncate text-ui-doc-title font-medium text-foreground-light dark:text-foreground-dark">
              {agent.name}
            </h2>
            <AgentKindBadge cliTool={agent.cliTool} runtimeKind={agent.runtimeKind} />
          </div>
          <p className={cn(uiStyles.chip, 'max-w-full truncate')}>
            {agentDetailHeaderSubtitle(agent)}
          </p>
        </div>

        {/* Status badge */}
        <div className="flex items-center gap-1.5 text-ui-body text-secondary-light dark:text-secondary-dark">
          <div
            className={cn(
              'h-1.5 w-1.5 shrink-0 rounded-full',
              STATUS_COLORS[statusKey] ?? STATUS_FALLBACK_COLOR
            )}
          />
          <span>{statusLabel}</span>
        </div>
      </div>

      {/* Tab bar */}
      <div className="flex gap-1 rounded-button bg-black/5 p-1 dark:bg-white/5">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            onClick={() => setActiveTab(tab.id)}
            className={cn(
              'flex-1 rounded-button py-1.5 text-ui-button font-medium transition-colors',
              activeTab === tab.id
                ? 'bg-black/[0.06] text-foreground-light dark:bg-white/[0.08] dark:text-foreground-dark'
                : 'text-secondary-light hover:bg-black/[0.04] hover:text-foreground-light dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark'
            )}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {/* Tab content */}
      {activeTab === 'overview' && (
        <div className="flex flex-col gap-4">
          <AgentNextStepCard
            step={agentNextStep(agent, recentTasks, recentTasksError)}
            onOpenTab={(tab) => setActiveTab(tab)}
            onBack={onBack}
          />
          <AssignmentFitCard
            agent={agent}
            recentTasks={recentTasks}
            recentTasksError={recentTasksError}
          />

          {/* Stats grid */}
          <div className="grid grid-cols-2 gap-3">
            {stats.map((stat) => (
              <StatCard key={stat.label} label={stat.label} value={stat.value} />
            ))}
          </div>

          {/* Agent info */}
          <div className={cn(uiStyles.card, 'flex flex-col gap-2 px-4 py-3')}>
            <span className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
              Agent overview
            </span>
            <div className="grid grid-cols-2 gap-x-4 gap-y-1.5 text-ui-caption">
              {detailRows.map((row) => (
                <DetailRow
                  key={row.label}
                  label={row.label}
                  value={row.value}
                  chip={row.label === 'Where it works'}
                />
              ))}
            </div>
            <WorkspaceBoundaryNote agent={agent} />
          </div>

          {/* Control panel */}
          <AgentControlPanel agent={agent} onDeleted={onBack} />
        </div>
      )}

      {activeTab === 'tasks' && <AgentTasksTab agentId={agent.id} onBackToAgents={onBack} />}

      {activeTab === 'history' && <ChatView agentId={agent.id} />}

      {activeTab === 'terminal' && agent.cliTool && !isHostCliAgent(agent) && agent.containerId && (
        <Suspense
          fallback={
            <div
              className={cn(
                uiStyles.cardPadded,
                'px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark'
              )}
            >
              Checking this agent's project files...
            </div>
          }
        >
          <AgentTerminalTab
            agentId={agent.id}
            agentName={agent.name}
            cliTool={agent.cliTool}
            containerId={agent.containerId}
            agentStatus={agent.status}
          />
        </Suspense>
      )}

      {activeTab === 'terminal' &&
        agent.cliTool &&
        !isHostCliAgent(agent) &&
        !agent.containerId && <PendingTerminal agent={agent} />}

      {activeTab === 'plugins' && agent.cliTool && (
        <AgentPluginsTab agentId={agent.id} onBackToAgents={onBack} />
      )}

      {activeTab === 'config' && <AgentConfigTab agentId={agent.id} />}
    </div>
  )
}

function agentNextStep(
  agent: AgentInfo,
  recentTasks: TaskSummary[],
  recentTasksError: string | null
): AgentNextStep {
  const activeTask = recentTasks.find((task) => task.state === 'working' || task.state === 'queued')
  const latestTask = recentTasks[0]
  const hostCli = isHostCliAgent(agent)
  const hasContainerTerminal = Boolean(agent.cliTool && !hostCli)

  if (agent.status === 'offline') {
    if (hostCli) {
      return {
        title: 'Reconnect this computer from Agents',
        detail:
          'Go back to Agents, choose Connect this computer, copy the new setup text, and paste it in the setup app shown there on the computer where this agent should work.',
        success: 'The status changes from Not connected to Ready or Working now.',
        ready: false,
        targetBack: true,
        actionLabel: 'Back to Agents',
      }
    }

    if (hasContainerTerminal) {
      return {
        title: 'Start project files',
        detail:
          'Open Live work, choose Start project files, and wait until this agent shows Ready before sending Tasks or code changes.',
        success: 'The agent returns to Ready and can receive tasks.',
        ready: false,
        targetTab: 'terminal',
        actionLabel: 'Open live work',
      }
    }

    return {
      title: 'Check the AI service before sending a message',
      detail:
        'Open AI services in Settings, choose Check connection for this service, then return to Agents and choose this agent again before sending a message.',
      success: 'The agent returns to Ready and can answer in chat.',
      ready: false,
      targetHref: '/settings/providers',
      actionLabel: 'Open AI services',
    }
  }

  if (!agent.cliTool) {
    return {
      title: 'Send a message in Chat',
      detail:
        'Use Chat for direct questions, writing, and result checks. It cannot take Tasks or change project files.',
      success: 'You can see the answer in this chat history.',
      ready: true,
      targetTab: 'history',
      actionLabel: 'Open chat',
    }
  }

  if (activeTask) {
    return {
      title: 'Check what this agent is doing',
      detail: `${agent.name} is already handling "${activeTask.params.task}". Go to Tasks to follow progress or handle anything that needs your help.`,
      success: 'You can see the active task state and decide whether someone needs to step in.',
      ready: false,
      targetTab: 'tasks',
      actionLabel: 'Open tasks',
    }
  }

  if (recentTasksError) {
    return {
      title: 'Choose this agent again or open Tasks',
      detail:
        "This page could not load the agent's recent task history. Go back to Agents and choose this agent again, or open Tasks to confirm the latest task state before sending another task.",
      success: 'You can see the latest task state before deciding what to do next.',
      ready: false,
      targetTab: 'tasks',
      actionLabel: 'Open tasks',
    }
  }

  if (agent.status === 'idle') {
    return {
      title: 'Send a small first task',
      detail: hostCli
        ? 'Use Tasks to send a small, low-risk task. The work window stays on this computer while Forge tracks results.'
        : 'Use Tasks to send a small, low-risk task. Choose this agent directly, or let another agent take it.',
      success: 'A task appears as Waiting to start or Working for this agent.',
      ready: true,
      targetTab: 'tasks',
      actionLabel: 'Open tasks',
    }
  }

  return {
    title: 'Go to Tasks to check recent activity',
    detail: latestTask
      ? `The latest task was "${latestTask.params.task}" updated ${formatRelativeTime(latestTask.updatedAt)}.`
      : "Go to Tasks to load this agent's task history and decide what task to send next.",
    success: 'You can decide whether to reuse the agent, check result files, or send another task.',
    ready: true,
    targetTab: 'tasks',
    actionLabel: 'Open tasks',
  }
}

function AgentNextStepCard({
  step,
  onOpenTab,
  onBack,
}: {
  step: AgentNextStep
  onOpenTab: (tab: Tab) => void
  onBack: () => void
}) {
  const targetTab = step.targetTab
  const targetHref = step.targetHref
  const targetBack = step.targetBack
  const actionLabel = step.actionLabel

  return (
    <div data-testid="agent-next-step">
      <PreferenceGuideDisclosure
        guideKey="agent-detail-next-step"
        icon={step.ready ? <CheckCircle2 /> : <AlertTriangle />}
        title={step.ready ? 'Ready' : 'Do this next'}
      >
        <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
          <div className="min-w-0">
            <h2 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
              {step.title}
            </h2>
            <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
              {step.detail}
            </p>
            <p className="mt-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
              What success looks like: {step.success}
            </p>
          </div>
          {targetTab && actionLabel && (
            <button
              type="button"
              onClick={() => onOpenTab(targetTab)}
              className={cn(uiStyles.secondaryButton, 'h-9 shrink-0')}
            >
              <span>{actionLabel}</span>
              <ArrowRight size={13} strokeWidth={2} aria-hidden="true" />
            </button>
          )}
          {targetHref && actionLabel && (
            <a href={targetHref} className={cn(uiStyles.secondaryButton, 'h-9 shrink-0')}>
              <span>{actionLabel}</span>
              <ArrowRight size={13} strokeWidth={2} aria-hidden="true" />
            </a>
          )}
          {targetBack && actionLabel && (
            <button
              type="button"
              onClick={onBack}
              className={cn(uiStyles.secondaryButton, 'h-9 shrink-0')}
            >
              <span>{actionLabel}</span>
              <ArrowRight size={13} strokeWidth={2} aria-hidden="true" />
            </button>
          )}
        </div>
      </PreferenceGuideDisclosure>
    </div>
  )
}

function AssignmentFitCard({
  agent,
  recentTasks,
  recentTasksError,
}: {
  agent: AgentInfo
  recentTasks: TaskSummary[]
  recentTasksError: string | null
}) {
  const available = agent.status === 'idle'
  const activeTask = recentTasks.find((task) => task.state === 'working' || task.state === 'queued')
  const latestTask = recentTasks[0]
  const appliedSkillCount = recentTasks.reduce(
    (sum, task) => sum + (task.contextCounts?.appliedSkills ?? 0),
    0
  )
  const availability = agentAvailabilityLabel(agent)
  const hostCli = isHostCliAgent(agent)
  const chatOnly = !agent.cliTool && !hostCli
  const runtime = agentRuntimeLabel(agent)
  let credential =
    'Open AI services in Settings to confirm this simple chat agent can answer. It cannot take Tasks, change code, or use computer apps.'
  if (hostCli) {
    credential = 'Uses the tool accounts and project files available on this computer.'
  } else if (agent.cliTool === 'codex') {
    credential = 'Settings shows whether this tool account is connected.'
  } else if (agent.cliTool) {
    credential = 'Forge adds project file access when project files start.'
  }

  return (
    <section
      data-testid="agent-assignment-fit"
      className={cn(uiStyles.card, 'flex flex-col gap-3 px-4 py-3')}
    >
      <div className="flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
            Availability
          </p>
          <h2 className="mt-0.5 text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            {availability}
          </h2>
        </div>
        <span className="inline-flex h-7 w-fit items-center gap-1.5 text-ui-body text-secondary-light dark:text-secondary-dark">
          <span
            className={cn(
              'h-1.5 w-1.5 rounded-full',
              available
                ? 'bg-apple-green'
                : agent.status === 'working'
                  ? 'bg-apple-orange'
                  : 'bg-apple-gray-2'
            )}
          />
          {agentStatusLabel(agent.status)}
        </span>
      </div>

      <div className="grid gap-2 text-ui-caption sm:grid-cols-2">
        <ProfileSummaryRow
          label={chatOnly ? 'Current chat' : 'Current work'}
          value={
            chatOnly
              ? 'Ready for direct chat'
              : (activeTask?.params.task ?? agent.currentTask ?? 'Ready for a task')
          }
        />
        <ProfileSummaryRow
          label="Recent update"
          value={
            chatOnly
              ? 'Send a message in Chat to create the first reply.'
              : recentTasksError
                ? recentTasksError
                : latestTask
                  ? `${latestTask.params.task} updated ${formatRelativeTime(latestTask.updatedAt)}`
                  : 'Send a task to create the first update.'
          }
        />
        <ProfileSummaryRow label="Where it works" value={runtime} />
        <ProfileSummaryRow
          label="Skills"
          value={
            appliedSkillCount > 0
              ? `${appliedSkillCount} skill${appliedSkillCount === 1 ? '' : 's'} used in recent ${chatOnly ? 'replies' : 'work'}`
              : chatOnly
                ? 'Save useful chat notes after a reply.'
                : 'Finish a task, then save useful steps.'
          }
        />
        <ProfileSummaryRow label="Account and file access" value={credential} />
      </div>
      <FollowedSkillsSection agentId={agent.id} />
    </section>
  )
}

function FollowedSkillsSection({ agentId }: { agentId: string }) {
  const [skills, setSkills] = useState<FollowedSkill[]>([])
  useEffect(() => {
    let cancelled = false
    orchestrationApi
      .getAgentFollowedSkills(agentId)
      .then((list) => {
        if (!cancelled) setSkills(Array.isArray(list) ? list : [])
      })
      .catch(() => {
        // Quiet degrade: the profile summary already explains the reuse path.
      })
    return () => {
      cancelled = true
    }
  }, [agentId])

  if (!Array.isArray(skills) || skills.length === 0) return null

  return (
    <div className="mt-3" data-testid="agent-followed-skills">
      <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
        Followed skills
      </p>
      <div className="mt-1 flex flex-wrap gap-1.5">
        {skills.map((skill) => (
          <span
            key={skill.skillId}
            className={cn(
              'rounded-button bg-apple-blue/10 px-2 py-0.5 text-ui-caption font-medium text-apple-blue',
              skill.state === 'revoked' && 'opacity-60 line-through'
            )}
          >
            {skill.name}
          </span>
        ))}
      </div>
    </div>
  )
}

function ProfileSummaryRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 rounded-card bg-black/[0.03] px-3 py-2 dark:bg-white/[0.04]">
      <span className="block text-secondary-light dark:text-secondary-dark">{label}</span>
      <span className="mt-0.5 block break-words font-medium text-foreground-light dark:text-foreground-dark">
        {value}
      </span>
    </div>
  )
}

interface StatCardProps {
  label: string
  value: string
}

function StatCard({ label, value }: StatCardProps) {
  return (
    <div className={cn(uiStyles.card, 'flex flex-col gap-1 px-4 py-3')}>
      <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">{label}</span>
      <span
        className={cn(
          'text-ui-metric font-semibold text-foreground-light dark:text-foreground-dark',
          label === 'Work type' && uiStyles.chip
        )}
      >
        {value}
      </span>
    </div>
  )
}

function DetailRow({
  label,
  value,
  chip = false,
}: {
  label: string
  value: string
  chip?: boolean
}) {
  return (
    <>
      <span className="text-secondary-light dark:text-secondary-dark">{label}</span>
      <span
        className={cn(
          'truncate font-medium text-foreground-light dark:text-foreground-dark',
          chip && uiStyles.chip
        )}
      >
        {value}
      </span>
    </>
  )
}

function PendingTerminal({ agent }: { agent: AgentInfo }) {
  const { startAgent, error } = useAgentsStore()
  const [starting, setStarting] = useState(false)
  const [startFailed, setStartFailed] = useState(false)

  async function handleStart() {
    if (starting || !agent.cliTool) return
    setStarting(true)
    setStartFailed(false)
    try {
      const started = await startAgent(agent.id)
      if (started === false) setStartFailed(true)
    } catch {
      setStartFailed(true)
    } finally {
      setStarting(false)
    }
  }

  const showStartProblem = Boolean(error || startFailed)
  const credentialRecovery = agentCredentialRecovery(error)
  const credentialError = credentialRecovery ? error : null

  return (
    <div
      className={cn(uiStyles.cardPadded, 'flex flex-col items-center gap-3 px-4 py-6 text-center')}
    >
      <Terminal
        size={20}
        strokeWidth={1.9}
        className="text-secondary-light dark:text-secondary-dark"
        aria-hidden="true"
      />
      <PreferenceGuideDisclosure
        guideKey="agent-detail-live-work-help"
        icon={<Terminal />}
        title="Start project files to open Live work"
        className="w-full max-w-xl text-left"
        dismissible={false}
      >
        <p>
          {agent.cliTool
            ? `${agentToolLabel(agent.cliTool)} is ready. Start project files before this agent changes project files.`
            : 'This agent does not change project files.'}
        </p>
        {agent.cliTool && (
          <p className="mt-1 text-ui-caption">
            Success looks like the agent status changing to Ready or Working now. If it stays stuck,
            ask an owner or admin to check this agent's connection and access in Agents.
          </p>
        )}
      </PreferenceGuideDisclosure>
      {showStartProblem && (
        <div
          role="alert"
          aria-live="polite"
          className="rounded-card bg-apple-red/10 px-3 py-2 text-ui-caption text-apple-red"
        >
          {credentialError ?? (
            <>
              Check the agent status, then choose Start project files again. If it keeps failing,
              ask an owner or admin to check this agent's connection and access in Agents.
            </>
          )}
          {credentialRecovery && (
            <a href={credentialRecovery.path} className="ml-1 font-medium underline">
              {credentialRecovery.label}
            </a>
          )}
        </div>
      )}
      {agent.cliTool && (
        <button
          type="button"
          onClick={handleStart}
          disabled={starting}
          className={cn(uiStyles.primaryButton, 'px-4')}
        >
          {starting ? 'Opening project files...' : 'Start project files'}
        </button>
      )}
    </div>
  )
}
