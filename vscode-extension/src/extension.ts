import * as vscode from 'vscode'
import WebSocket from 'ws'
import { OrchestratorClient } from './api/client'
import { OrchestratorAuthProvider } from './auth/oidc'
import { getConfig, onConfigChange } from './config'
import { StatusBarProvider } from './providers/statusBar'
import { ReviewPanelProvider } from './views/reviewPanel'
import { TaskBoardProvider } from './views/taskBoard'
import type { Task, TaskState } from './api/client'

const VALID_TRANSITIONS: TaskState[] = [
  'backlog',
  'ready',
  'in_progress',
  'in_review',
  'done',
  'blocked',
]

let ws: WebSocket | null = null
let reconnectTimer: ReturnType<typeof setTimeout> | null = null
let refreshTimer: ReturnType<typeof setInterval> | null = null

export function activate(context: vscode.ExtensionContext): void {
  const client = new OrchestratorClient()
  const authProvider = OrchestratorAuthProvider.register(context)
  const taskBoard = new TaskBoardProvider(client)
  const reviewPanel = new ReviewPanelProvider(client)
  const statusBar = new StatusBarProvider()

  context.subscriptions.push(
    vscode.window.createTreeView('orchestrator.taskBoard', {
      treeDataProvider: taskBoard,
      showCollapseAll: true,
    }),
    vscode.window.createTreeView('orchestrator.reviews', {
      treeDataProvider: reviewPanel,
      showCollapseAll: true,
    }),
    taskBoard,
    reviewPanel,
    statusBar
  )

  async function syncAuth(): Promise<void> {
    const token = await authProvider.getToken()
    client.setAccessToken(token)
  }

  function refreshAll(): void {
    taskBoard.refresh()
    reviewPanel.refresh()
  }

  context.subscriptions.push(
    vscode.authentication.onDidChangeSessions((e) => {
      if (e.provider.id === 'orchestrator-oidc') {
        void syncAuth().then(() => refreshAll())
      }
    })
  )

  registerCommands(context, client, taskBoard, reviewPanel, statusBar, authProvider, refreshAll)

  context.subscriptions.push(
    onConfigChange(() => {
      disconnectWebSocket()
      stopAutoRefresh()
      connectWebSocket(client, taskBoard, reviewPanel, statusBar)
      startAutoRefresh(taskBoard, reviewPanel, statusBar)
    })
  )

  void syncAuth()
    .then(() => {
      refreshAll()
      connectWebSocket(client, taskBoard, reviewPanel, statusBar)
      startAutoRefresh(taskBoard, reviewPanel, statusBar)
    })
    .catch((err: unknown) => {
      const msg = err instanceof Error ? err.message : String(err)
      console.error(`[Orchestrator] Auth sync failed: ${msg}`)
      vscode.window.showWarningMessage(`Orchestrator: Authentication failed. ${msg}`)
      refreshAll()
      connectWebSocket(client, taskBoard, reviewPanel, statusBar)
      startAutoRefresh(taskBoard, reviewPanel, statusBar)
    })

  context.subscriptions.push({
    dispose() {
      disconnectWebSocket()
      stopAutoRefresh()
    },
  })
}

export function deactivate(): void {
  disconnectWebSocket()
  stopAutoRefresh()
}

// --- Task picker helper ---

async function pickTask(
  client: OrchestratorClient,
  placeholder: string
): Promise<Task | undefined> {
  let tasks: Task[]
  try {
    tasks = await client.listTasks()
  } catch (err) {
    vscode.window.showErrorMessage(
      `Failed to fetch tasks: ${err instanceof Error ? err.message : String(err)}`
    )
    return undefined
  }
  if (tasks.length === 0) {
    vscode.window.showInformationMessage('No tasks available.')
    return undefined
  }
  const picked = await vscode.window.showQuickPick(
    tasks.map((t) => ({ label: t.title, description: t.state, detail: t.id, task: t })),
    { placeHolder: placeholder }
  )
  return picked?.task
}

// --- Command handlers ---

async function handleViewTask(client: OrchestratorClient, taskArg?: Task): Promise<void> {
  const selectedTask = taskArg ?? (await pickTask(client, 'Select a task to view'))
  if (!selectedTask) return

  let detail: Task
  try {
    detail = await client.getTask(selectedTask.id)
  } catch (err) {
    console.warn(`[Orchestrator] Failed to fetch task detail: ${err}`)
    detail = selectedTask
    vscode.window.showWarningMessage('Could not fetch latest task data — showing cached version.')
  }
  const content = [
    `# ${detail.title}`,
    '',
    `| Field | Value |`,
    `|-------|-------|`,
    `| ID | ${detail.id} |`,
    `| State | ${detail.state} |`,
    `| Priority | ${detail.priority ?? 'none'} |`,
    `| Assignee | ${detail.assignee ?? 'unassigned'} |`,
    `| Created | ${new Date(detail.createdAt).toLocaleString()} |`,
    `| Updated | ${new Date(detail.updatedAt).toLocaleString()} |`,
  ].join('\n')

  const doc = await vscode.workspace.openTextDocument({ content, language: 'markdown' })
  await vscode.window.showTextDocument(doc, { preview: true })
}

async function handleTransitionTask(
  client: OrchestratorClient,
  taskBoard: TaskBoardProvider,
  statusBar: StatusBarProvider,
  taskArg?: Task
): Promise<void> {
  const selectedTask = taskArg ?? (await pickTask(client, 'Select a task to transition'))
  if (!selectedTask) return

  const targetState = await vscode.window.showQuickPick(
    VALID_TRANSITIONS.filter((s) => s !== selectedTask.state).map((s) => ({ label: s })),
    { placeHolder: `Transition "${selectedTask.title}" from ${selectedTask.state} to...` }
  )
  if (!targetState) return

  try {
    await client.transitionTask(selectedTask.id, targetState.label as TaskState)
    vscode.window.showInformationMessage(
      `Task "${selectedTask.title}" transitioned to ${targetState.label}`
    )
    taskBoard.refresh()
    updateStatusBar(taskBoard, statusBar)
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    vscode.window.showErrorMessage(`Transition failed: ${msg}`)
  }
}

async function handleApproveReview(
  client: OrchestratorClient,
  reviewPanel: ReviewPanelProvider,
  statusBar: StatusBarProvider,
  item?: { review?: { id: string; taskTitle: string } }
): Promise<void> {
  const review = item?.review
  if (!review) {
    vscode.window.showWarningMessage('Select a pending review to approve.')
    return
  }

  try {
    await client.approveReview(review.id)
    vscode.window.showInformationMessage(`Review approved: ${review.taskTitle}`)
    reviewPanel.refresh()
    statusBar.setReviewCount(reviewPanel.getPendingCount())
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    vscode.window.showErrorMessage(`Approve failed: ${msg}`)
  }
}

async function handleRejectReview(
  client: OrchestratorClient,
  reviewPanel: ReviewPanelProvider,
  statusBar: StatusBarProvider,
  item?: { review?: { id: string; taskTitle: string } }
): Promise<void> {
  const review = item?.review
  if (!review) {
    vscode.window.showWarningMessage('Select a pending review to reject.')
    return
  }

  const reason = await vscode.window.showInputBox({
    prompt: 'Rejection reason (optional)',
    placeHolder: 'Enter reason...',
  })

  try {
    await client.rejectReview(review.id, reason ?? undefined)
    vscode.window.showInformationMessage(`Review rejected: ${review.taskTitle}`)
    reviewPanel.refresh()
    statusBar.setReviewCount(reviewPanel.getPendingCount())
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    vscode.window.showErrorMessage(`Reject failed: ${msg}`)
  }
}

async function handleListWorkflows(client: OrchestratorClient): Promise<void> {
  try {
    const workflows = await client.listWorkflows()
    if (workflows.length === 0) {
      vscode.window.showInformationMessage('No workflows configured.')
      return
    }
    const picked = await vscode.window.showQuickPick(
      workflows.map((w) => ({
        label: w.name,
        description: w.id,
        detail: w.description ?? `States: ${w.states.join(' -> ')}`,
      })),
      { placeHolder: 'Workflows' }
    )
    if (picked) {
      vscode.window.showInformationMessage(`Workflow: ${picked.label} (${picked.description})`)
    }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    vscode.window.showErrorMessage(`Failed to load workflows: ${msg}`)
  }
}

async function handleShowDashboard(client: OrchestratorClient): Promise<void> {
  try {
    const metrics = await client.getDashboard()
    const lines = [
      `Total tasks: ${metrics.totalTasks}`,
      `Completed today: ${metrics.completedToday}`,
      `Pending reviews: ${metrics.pendingReviews}`,
      '',
      'By state:',
      ...Object.entries(metrics.byState).map(([state, count]) => `  ${state}: ${count}`),
    ]
    vscode.window.showInformationMessage(lines.join('\n'), { modal: true })
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    vscode.window.showErrorMessage(`Failed to load dashboard: ${msg}`)
  }
}

// --- Command registration ---

function registerCommands(
  context: vscode.ExtensionContext,
  client: OrchestratorClient,
  taskBoard: TaskBoardProvider,
  reviewPanel: ReviewPanelProvider,
  statusBar: StatusBarProvider,
  authProvider: OrchestratorAuthProvider,
  refreshAll: () => void
): void {
  context.subscriptions.push(
    vscode.commands.registerCommand('orchestrator.refreshTasks', () => {
      taskBoard.refresh()
      updateStatusBar(taskBoard, statusBar)
    }),
    vscode.commands.registerCommand('orchestrator.refreshReviews', () => {
      reviewPanel.refresh()
      statusBar.setReviewCount(reviewPanel.getPendingCount())
    }),
    vscode.commands.registerCommand('orchestrator.viewTask', (task?: Task) =>
      handleViewTask(client, task)
    ),
    vscode.commands.registerCommand('orchestrator.transitionTask', (task?: Task) =>
      handleTransitionTask(client, taskBoard, statusBar, task)
    ),
    vscode.commands.registerCommand(
      'orchestrator.approveReview',
      (item?: { review?: { id: string; taskTitle: string } }) =>
        handleApproveReview(client, reviewPanel, statusBar, item)
    ),
    vscode.commands.registerCommand(
      'orchestrator.rejectReview',
      (item?: { review?: { id: string; taskTitle: string } }) =>
        handleRejectReview(client, reviewPanel, statusBar, item)
    ),
    vscode.commands.registerCommand('orchestrator.listWorkflows', () =>
      handleListWorkflows(client)
    ),
    vscode.commands.registerCommand('orchestrator.showDashboard', () =>
      handleShowDashboard(client)
    ),
    vscode.commands.registerCommand('orchestrator.login', async () => {
      try {
        await vscode.authentication.getSession(
          'orchestrator-oidc',
          ['openid', 'profile', 'email'],
          {
            createIfNone: true,
          }
        )
        vscode.window.showInformationMessage('Signed in to Orchestrator')
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err)
        vscode.window.showErrorMessage(`Sign in failed: ${msg}`)
      }
    }),
    vscode.commands.registerCommand('orchestrator.logout', async () => {
      const sessions = await authProvider.getSessions()
      for (const session of sessions) {
        await authProvider.removeSession(session.id)
      }
      client.setAccessToken(null)
      vscode.window.showInformationMessage('Signed out of Orchestrator')
      refreshAll()
    })
  )
}

// --- Status bar ---

function updateStatusBar(taskBoard: TaskBoardProvider, statusBar: StatusBarProvider): void {
  const active = taskBoard.getActiveTask()
  if (active) {
    statusBar.setActiveTask(active)
  } else {
    statusBar.setIdle()
  }
}

// --- WebSocket ---

interface WsMessage {
  type: string
  payload?: Record<string, unknown>
}

function connectWebSocket(
  client: OrchestratorClient,
  taskBoard: TaskBoardProvider,
  reviewPanel: ReviewPanelProvider,
  statusBar: StatusBarProvider
): void {
  const config = getConfig()
  if (!config.notificationsEnabled) return

  const url = `${config.wsUrl}/ws/orchestrator`

  try {
    ws = new WebSocket(url)
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    console.error(`[Orchestrator] WebSocket connection failed: ${msg} (url: ${url})`)
    scheduleReconnect(client, taskBoard, reviewPanel, statusBar)
    return
  }

  ws.on('open', () => {
    console.info('[Orchestrator] WebSocket connected')
  })

  ws.on('message', (data) => {
    try {
      const msg = JSON.parse(data.toString()) as WsMessage
      handleWsMessage(msg, taskBoard, reviewPanel, statusBar)
    } catch (err) {
      const preview = data.toString().substring(0, 200)
      console.warn(`[Orchestrator] Failed to process WebSocket message: ${err}. Data: ${preview}`)
    }
  })

  ws.on('close', () => {
    ws = null
    scheduleReconnect(client, taskBoard, reviewPanel, statusBar)
  })

  ws.on('error', (err) => {
    console.error(`[Orchestrator] WebSocket error: ${err.message}`)
    ws?.close()
    ws = null
  })
}

function handleWsMessage(
  msg: WsMessage,
  taskBoard: TaskBoardProvider,
  reviewPanel: ReviewPanelProvider,
  statusBar: StatusBarProvider
): void {
  switch (msg.type) {
    case 'task.created':
    case 'task.updated':
    case 'task.transitioned':
    case 'task.deleted':
      taskBoard.refresh()
      updateStatusBar(taskBoard, statusBar)
      break

    case 'review.submitted':
    case 'review.approved':
    case 'review.rejected':
      reviewPanel.refresh()
      if (msg.type === 'review.submitted') {
        vscode.window.showInformationMessage(
          `New review submitted: ${(msg.payload?.taskTitle as string) ?? 'Unknown task'}`
        )
      }
      break

    default:
      break
  }
}

function scheduleReconnect(
  client: OrchestratorClient,
  taskBoard: TaskBoardProvider,
  reviewPanel: ReviewPanelProvider,
  statusBar: StatusBarProvider
): void {
  if (reconnectTimer) return
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null
    connectWebSocket(client, taskBoard, reviewPanel, statusBar)
  }, 5_000)
}

function disconnectWebSocket(): void {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer)
    reconnectTimer = null
  }
  if (ws) {
    ws.removeAllListeners()
    ws.close()
    ws = null
  }
}

// --- Auto-refresh ---

function startAutoRefresh(
  taskBoard: TaskBoardProvider,
  reviewPanel: ReviewPanelProvider,
  statusBar: StatusBarProvider
): void {
  stopAutoRefresh()
  const interval = getConfig().autoRefreshInterval * 1000
  refreshTimer = setInterval(() => {
    taskBoard.refresh()
    reviewPanel.refresh()
    updateStatusBar(taskBoard, statusBar)
  }, interval)
}

function stopAutoRefresh(): void {
  if (refreshTimer) {
    clearInterval(refreshTimer)
    refreshTimer = null
  }
}
