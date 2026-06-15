import { lazy, Suspense, useEffect, useState } from 'react'
import { AlertTriangle, ArrowRight, CheckCircle2, Info } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import {
  agentStatusKey,
  agentStatusLabel,
  agentAvatarInitial,
  agentRuntimeLabel,
  agentToolLabel,
  isHostCliAgent,
  useAgentsStore,
  type AgentInfo,
} from '@app/entities/agent'
import { AgentConfigTab } from '@app/features/agents/AgentConfigTab'
import { AgentControlPanel } from '@app/features/agents/AgentControlPanel'
import { AgentKindBadge } from '@app/features/agents/AgentKindBadge'
import { AgentPluginsTab } from '@app/features/agents/AgentPluginsTab'
import { AgentTasksTab } from '@app/features/agents/AgentTasksTab'
import { ChatView } from '@app/features/chat/ChatView'
import { orchestrationApi, type TaskSummary } from '@app/shared/api/orchestration'
import { formatRelativeTime } from '@app/shared/lib/time'

// Lazy — keeps xterm out of the agents route's initial chunk
const AgentTerminalTab = lazy(() =>
  import('@app/features/agents/AgentTerminalTab').then((m) => ({ default: m.AgentTerminalTab }))
)

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
  actionLabel?: string
}

// Live work attach is only available for platform-managed work environments.
// Agents joined from a computer keep the work window on that machine.
function tabsFor(agent: AgentInfo): { id: Tab; label: string }[] {
  const isCli = Boolean(agent.cliTool)
  const hasTerminal = isCli && !isHostCliAgent(agent)
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
    <div className="flex gap-2 rounded-lg bg-apple-blue/10 px-3 py-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
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
          This agent can work in the shared workspace folder, which can include several projects.
          The selected project is just the starting project for new tasks. Use a separate workspace
          when files must be kept apart.
        </p>
      ) : (
        <p>
          This chat-only AI service can plan, write, and review in chat, but it cannot open project
          files on its own. Choose an agent on this computer or in a managed workspace when the task
          must inspect or edit files.
        </p>
      )}
    </div>
  )
}

function agentFolderLabel(agent: AgentInfo): string {
  if (!agent.cliTool) return 'No file access needed'
  if (!agent.cwd || agent.cwd === '/workspace') {
    return isHostCliAgent(agent)
      ? 'Folder where you pasted the setup text'
      : 'Workspace project folder'
  }
  return agent.cwd
}

function agentSetupSummary(agent: AgentInfo): string {
  if (isHostCliAgent(agent)) return 'This computer'
  if (agent.cliTool) return 'Managed workspace'
  return 'Chat-only AI service'
}

export function agentDetailHeaderSubtitle(agent: AgentInfo): string {
  return agentRuntimeLabel(agent)
}

function agentConnectionStatus(agent: AgentInfo): string {
  if (isHostCliAgent(agent)) {
    if (agent.status === 'offline') {
      return 'Paste setup text again on this computer'
    }
    return agent.runtimeId
      ? 'Connected from this computer'
      : 'Paste setup text on this computer again'
  }
  if (agent.cliTool) return agent.containerId ? 'Ready in managed workspace' : 'Waiting to start'
  return 'Uses AI service connection'
}

function agentAvailabilityLabel(agent: AgentInfo): string {
  if (agent.status === 'idle') return 'Can be assigned now'
  if (agent.status === 'working') return 'Already working'
  if (isHostCliAgent(agent)) return 'Paste setup text again on this computer'
  if (agent.cliTool) return 'Open Live work and start workspace'
  return 'Open Settings and check AI service'
}

interface AgentDetailViewProps {
  agent: AgentInfo
  onBack: () => void
}

export function AgentDetailView({ agent, onBack }: AgentDetailViewProps) {
  const [activeTab, setActiveTab] = useState<Tab>('overview')
  const [recentTasks, setRecentTasks] = useState<TaskSummary[]>([])
  const ratePercent = Math.round(agent.successRate * 100)
  const tabs = tabsFor(agent)
  const statusKey = agentStatusKey(agent.status)
  const statusLabel = agentStatusLabel(agent.status)

  useEffect(() => {
    if (!tabs.some((tab) => tab.id === activeTab)) setActiveTab('overview')
  }, [activeTab, tabs])

  useEffect(() => {
    let cancelled = false
    orchestrationApi
      .getTasksByAgent(agent.id, { limit: 5 })
      .then((tasks) => {
        if (!cancelled) setRecentTasks(tasks)
      })
      .catch(() => {
        if (!cancelled) setRecentTasks([])
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
          className={cn(
            'flex items-center justify-center w-8 h-8 rounded-lg shrink-0',
            'bg-white dark:bg-[#2a2a2c] border border-black/[0.08] dark:border-white/[0.1]',
            'text-secondary-light dark:text-secondary-dark hover:text-foreground-light dark:hover:text-foreground-dark',
            'transition-colors'
          )}
          aria-label="Back"
        >
          ‹
        </button>

        {/* Provider-colored avatar */}
        <div
          className={cn(
            'w-10 h-10 rounded-xl flex items-center justify-center shrink-0',
            'border border-black/[0.08] text-ui-body font-semibold select-none dark:border-white/[0.1]',
            defaultGradient()
          )}
        >
          {agentAvatarInitial(agent)}
        </div>

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <h1 className="truncate text-ui-title font-semibold text-foreground-light dark:text-foreground-dark">
              {agent.name}
            </h1>
            <AgentKindBadge cliTool={agent.cliTool} runtimeKind={agent.runtimeKind} />
          </div>
          <p className="truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
            {agentDetailHeaderSubtitle(agent)}
          </p>
        </div>

        {/* Status badge */}
        <div
          className={cn(
            'flex items-center gap-1.5 px-2.5 py-1 rounded-full',
            'border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-[#2a2a2c]'
          )}
        >
          <div
            className={cn(
              'w-2 h-2 rounded-full shrink-0',
              STATUS_COLORS[statusKey] ?? STATUS_FALLBACK_COLOR
            )}
          />
          <span className="text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
            {statusLabel}
          </span>
        </div>
      </div>

      {/* Tab bar */}
      <div className={cn('flex gap-1 p-1 rounded-xl', 'bg-black/5 dark:bg-white/5')}>
        {tabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            onClick={() => setActiveTab(tab.id)}
            className={cn(
              'flex-1 rounded-lg py-1.5 text-ui-button font-medium transition-colors',
              activeTab === tab.id
                ? 'bg-white dark:bg-[#3a3a3c] text-foreground-light dark:text-foreground-dark shadow-sm'
                : 'text-secondary-light dark:text-secondary-dark hover:text-foreground-light dark:hover:text-foreground-dark'
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
            step={agentNextStep(agent, recentTasks)}
            onOpenTab={(tab) => setActiveTab(tab)}
          />
          <AssignmentFitCard agent={agent} recentTasks={recentTasks} />

          {/* Stats grid */}
          <div className="grid grid-cols-2 gap-3">
            <StatCard label="Tasks done" value={String(agent.tasksCompleted)} />
            <StatCard label="In progress" value={String(agent.tasksInProgress)} />
            <StatCard label="Finished cleanly" value={`${ratePercent}%`} />
            <StatCard label="Work setup" value={agentSetupSummary(agent)} />
          </div>

          {/* Agent info */}
          <div
            className={cn(
              'bg-white dark:bg-[#2c2c2e] rounded-xl px-4 py-3',
              'border border-black/[0.08] dark:border-white/[0.1]',
              'flex flex-col gap-2'
            )}
          >
            <span className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
              Details
            </span>
            <div className="grid grid-cols-2 gap-x-4 gap-y-1.5 text-ui-caption">
              <DetailRow label="Where it works" value={agentRuntimeLabel(agent)} />
              <DetailRow label="Status" value={statusLabel} />
              <DetailRow
                label="Workspace it can use"
                value={agent.workspaceName ?? 'Default workspace'}
              />
              <DetailRow
                label="Starting project for tasks"
                value={agent.projectName ?? 'Choose when assigning work'}
              />
              <DetailRow label="Starting folder" value={agentFolderLabel(agent)} />
              <DetailRow label="Connection" value={agentConnectionStatus(agent)} />
            </div>
            <WorkspaceBoundaryNote agent={agent} />
          </div>

          {/* Control panel */}
          <AgentControlPanel agent={agent} onDeleted={onBack} />
        </div>
      )}

      {activeTab === 'tasks' && <AgentTasksTab agentId={agent.id} />}

      {activeTab === 'history' && <ChatView agentId={agent.id} />}

      {activeTab === 'terminal' && agent.cliTool && !isHostCliAgent(agent) && agent.containerId && (
        <Suspense
          fallback={
            <div
              className={cn(
                'bg-white dark:bg-[#2c2c2e] rounded-xl px-4 py-6',
                'border border-black/[0.08] dark:border-white/[0.1]',
                'text-center text-ui-body text-secondary-light dark:text-secondary-dark'
              )}
            >
              Loading live work...
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

      {activeTab === 'plugins' && <AgentPluginsTab agentId={agent.id} />}

      {activeTab === 'config' && <AgentConfigTab agentId={agent.id} />}
    </div>
  )
}

function agentNextStep(agent: AgentInfo, recentTasks: TaskSummary[]): AgentNextStep {
  const activeTask = recentTasks.find((task) => task.state === 'working' || task.state === 'queued')
  const latestTask = recentTasks[0]
  const hostCli = isHostCliAgent(agent)
  const hasContainerTerminal = Boolean(agent.cliTool && !hostCli)

  if (agent.status === 'offline') {
    if (hostCli) {
      return {
        title: 'Paste setup text on this computer again',
        detail:
          'Go to the computer where this agent was connected. Open Terminal or PowerShell in the project folder, paste the setup text again, and keep that window open.',
        success: 'The status changes from Not connected to Ready or Working now.',
        ready: false,
      }
    }

    if (hasContainerTerminal) {
      return {
        title: 'Start this workspace',
        detail:
          'Open Live work, choose Start workspace, and wait until this agent shows Ready before sending file work.',
        success: 'The agent returns to Ready and can receive tasks.',
        ready: false,
        targetTab: 'terminal',
        actionLabel: 'Open live work',
      }
    }

    return {
      title: 'Fix setup before sending work',
      detail:
        'This chat-only AI service is offline. Open Settings and check that the AI service is ready before sending work.',
      success: 'The agent returns to Ready and can receive tasks.',
      ready: false,
      targetHref: '/settings/providers',
      actionLabel: 'Open AI service settings',
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

  if (agent.status === 'idle') {
    return {
      title: 'Send a small first task',
      detail: hostCli
        ? 'Use Tasks to send a small, low-risk task. The work window stays on this computer while Forge tracks results.'
        : 'Use Tasks to send a small, low-risk task. Choose this agent, or choose a task queue this agent can receive.',
      success: 'A task appears as Waiting to start or Working for this agent.',
      ready: true,
      targetTab: 'tasks',
      actionLabel: 'Open tasks',
    }
  }

  return {
    title: 'Go to Tasks to review recent activity',
    detail: latestTask
      ? `The latest task was "${latestTask.params.task}" updated ${formatRelativeTime(latestTask.updatedAt)}.`
      : "Go to Tasks to load this agent's work history and decide what to send next.",
    success: 'You can decide whether to reuse the agent, review evidence, or assign another task.',
    ready: true,
    targetTab: 'tasks',
    actionLabel: 'Open tasks',
  }
}

function AgentNextStepCard({
  step,
  onOpenTab,
}: {
  step: AgentNextStep
  onOpenTab: (tab: Tab) => void
}) {
  const targetTab = step.targetTab
  const targetHref = step.targetHref
  const actionLabel = step.actionLabel

  return (
    <section
      data-testid="agent-next-step"
      className={cn(
        'rounded-xl border px-4 py-3',
        step.ready
          ? 'border-apple-green/20 bg-apple-green/5'
          : 'border-apple-blue/20 bg-apple-blue/[0.04]'
      )}
    >
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            {step.ready ? (
              <CheckCircle2
                size={17}
                strokeWidth={2.25}
                className="shrink-0 text-apple-green"
                aria-hidden="true"
              />
            ) : (
              <AlertTriangle
                size={17}
                strokeWidth={2.25}
                className="shrink-0 text-apple-blue"
                aria-hidden="true"
              />
            )}
            <p className="text-ui-caption font-semibold uppercase text-secondary-light dark:text-secondary-dark">
              {step.ready ? 'Ready' : 'Do this next'}
            </p>
          </div>
          <h2 className="mt-1 text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
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
            className={cn(
              'inline-flex h-9 shrink-0 items-center justify-center gap-1.5 rounded-full border border-black/[0.08] bg-white px-3 text-ui-button font-medium text-foreground-light transition-colors hover:bg-black/[0.03] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.08]'
            )}
          >
            <span>{actionLabel}</span>
            <ArrowRight size={13} strokeWidth={2} aria-hidden="true" />
          </button>
        )}
        {targetHref && actionLabel && (
          <a
            href={targetHref}
            className={cn(
              'inline-flex h-9 shrink-0 items-center justify-center gap-1.5 rounded-full border border-black/[0.08] bg-white px-3 text-ui-button font-medium text-foreground-light transition-colors hover:bg-black/[0.03] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.08]'
            )}
          >
            <span>{actionLabel}</span>
            <ArrowRight size={13} strokeWidth={2} aria-hidden="true" />
          </a>
        )}
      </div>
    </section>
  )
}

function AssignmentFitCard({
  agent,
  recentTasks,
}: {
  agent: AgentInfo
  recentTasks: TaskSummary[]
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
  const runtime = agentRuntimeLabel(agent)
  let credential = 'Settings shows whether the AI service is ready.'
  if (hostCli) {
    credential = 'Uses the tool accounts and project files available on this computer.'
  } else if (agent.cliTool === 'codex') {
    credential = 'Settings shows whether this tool account is connected.'
  } else if (agent.cliTool) {
    credential = 'Forge adds project file access when the managed workspace starts.'
  }

  return (
    <section
      data-testid="agent-assignment-fit"
      className={cn(
        'bg-white dark:bg-[#2c2c2e] rounded-xl px-4 py-3',
        'border border-black/[0.08] dark:border-white/[0.1]',
        'flex flex-col gap-3'
      )}
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
        <span
          className={cn(
            'inline-flex h-7 w-fit items-center rounded-full px-2.5 text-ui-caption font-medium',
            available
              ? 'bg-apple-green/10 text-apple-green'
              : agent.status === 'working'
                ? 'bg-apple-orange/10 text-apple-orange'
                : 'bg-apple-gray-5 text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark'
          )}
        >
          {agentStatusLabel(agent.status)}
        </span>
      </div>

      <div className="grid gap-2 text-ui-caption sm:grid-cols-2">
        <ProfileSummaryRow
          label="Current work"
          value={activeTask?.params.task ?? agent.currentTask ?? 'Ready for a task'}
        />
        <ProfileSummaryRow
          label="Recent update"
          value={
            latestTask
              ? `${latestTask.params.task} updated ${formatRelativeTime(latestTask.updatedAt)}`
              : 'Send a task to create the first update.'
          }
        />
        <ProfileSummaryRow label="Where it works" value={runtime} />
        <ProfileSummaryRow
          label="Saved instructions"
          value={
            appliedSkillCount > 0
              ? `${appliedSkillCount} saved instruction${appliedSkillCount === 1 ? '' : 's'} used in recent work`
              : 'Finish a task, then save useful steps.'
          }
        />
        <ProfileSummaryRow label="Account and file access" value={credential} />
      </div>
    </section>
  )
}

function ProfileSummaryRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 rounded-lg bg-black/[0.03] px-3 py-2 dark:bg-white/[0.04]">
      <span className="block text-secondary-light dark:text-secondary-dark">{label}</span>
      <span className="mt-0.5 block truncate font-medium text-foreground-light dark:text-foreground-dark">
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
    <div
      className={cn(
        'bg-white dark:bg-[#2c2c2e] rounded-xl px-4 py-3',
        'border border-black/[0.08] dark:border-white/[0.1]',
        'flex flex-col gap-1'
      )}
    >
      <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">{label}</span>
      <span className="text-ui-metric font-semibold text-foreground-light dark:text-foreground-dark">
        {value}
      </span>
    </div>
  )
}

function DetailRow({ label, value }: { label: string; value: string }) {
  return (
    <>
      <span className="text-secondary-light dark:text-secondary-dark">{label}</span>
      <span className="text-foreground-light dark:text-foreground-dark font-medium truncate">
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

  return (
    <div
      className={cn(
        'bg-white dark:bg-[#2c2c2e] rounded-xl px-4 py-6',
        'border border-black/[0.08] dark:border-white/[0.1]',
        'flex flex-col items-center gap-3 text-center'
      )}
    >
      <div className="flex flex-col gap-1">
        <span className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
          Start this workspace to open Live work
        </span>
        <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
          {agent.cliTool
            ? `${agentToolLabel(agent.cliTool)} is ready. Start the workspace before this agent works on files.`
            : 'This agent does not need a managed workspace.'}
        </span>
        {agent.cliTool && (
          <span className="max-w-xl text-ui-caption text-secondary-light dark:text-secondary-dark">
            Success looks like the agent status changing to Ready or Working now. If it stays stuck,
            ask an owner or admin to check this agent setup.
          </span>
        )}
      </div>
      {showStartProblem && (
        <div
          role="alert"
          aria-live="polite"
          className="rounded-lg bg-apple-red/10 px-3 py-2 text-ui-caption text-apple-red"
        >
          Check the agent status, then choose Start workspace again. If it keeps failing, ask an
          owner or admin to check this agent setup.
        </div>
      )}
      {agent.cliTool && (
        <button
          type="button"
          onClick={handleStart}
          disabled={starting}
          className={cn(
            'rounded-full px-4 py-2 text-ui-button font-medium',
            'bg-apple-blue text-white hover:bg-apple-blue/90 transition-colors',
            starting && 'opacity-50'
          )}
        >
          {starting ? 'Starting...' : 'Start workspace'}
        </button>
      )}
    </div>
  )
}
