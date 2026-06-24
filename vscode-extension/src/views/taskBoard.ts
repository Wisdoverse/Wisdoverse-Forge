import * as vscode from 'vscode'
import type { OrchestratorClient, Task, TaskState } from '../api/client'

const STATE_ORDER: TaskState[] = ['in_progress', 'in_review', 'ready', 'blocked', 'backlog', 'done']

const STATE_ICONS: Record<TaskState, vscode.ThemeIcon> = {
  backlog: new vscode.ThemeIcon('circle-outline'),
  ready: new vscode.ThemeIcon('circle-filled'),
  in_progress: new vscode.ThemeIcon('loading~spin'),
  in_review: new vscode.ThemeIcon('eye'),
  done: new vscode.ThemeIcon('check'),
  blocked: new vscode.ThemeIcon('error'),
}

const STATE_LABELS: Record<TaskState, string> = {
  backlog: 'Backlog',
  ready: 'Ready',
  in_progress: 'In Progress',
  in_review: 'In Review',
  done: 'Done',
  blocked: 'Blocked',
}

const PRIORITY_ICONS: Record<string, string> = {
  critical: '!!',
  high: '!',
  medium: '',
  low: '',
}

type TaskBoardItem = TaskGroupItem | TaskItem

class TaskGroupItem extends vscode.TreeItem {
  constructor(
    public readonly state: TaskState,
    public readonly count: number
  ) {
    super(`${STATE_LABELS[state]} (${count})`, vscode.TreeItemCollapsibleState.Expanded)
    this.iconPath = STATE_ICONS[state]
    this.contextValue = 'taskGroup'
  }
}

class TaskItem extends vscode.TreeItem {
  constructor(public readonly task: Task) {
    const prefix = task.priority ? PRIORITY_ICONS[task.priority] : ''
    const label = prefix ? `${prefix} ${task.title}` : task.title
    super(label, vscode.TreeItemCollapsibleState.None)

    this.id = task.id
    this.iconPath = STATE_ICONS[task.state]
    this.contextValue = 'task'
    this.tooltip = new vscode.MarkdownString(
      [
        `**${task.title}**`,
        '',
        `State: ${STATE_LABELS[task.state]}`,
        task.assignee ? `Assignee: ${task.assignee}` : null,
        task.priority ? `Priority: ${task.priority}` : null,
        `Updated: ${new Date(task.updatedAt).toLocaleString()}`,
      ]
        .filter(Boolean)
        .join('\n')
    )
    this.command = {
      command: 'orchestrator.viewTask',
      title: 'View Task',
      arguments: [task],
    }
  }
}

export class TaskBoardProvider implements vscode.TreeDataProvider<TaskBoardItem> {
  private readonly _onDidChangeTreeData = new vscode.EventEmitter<
    TaskBoardItem | undefined | void
  >()
  readonly onDidChangeTreeData = this._onDidChangeTreeData.event

  private tasks: Task[] = []

  constructor(private readonly client: OrchestratorClient) {}

  refresh(): void {
    this._onDidChangeTreeData.fire()
  }

  updateTasks(tasks: Task[]): void {
    this.tasks = tasks
    this._onDidChangeTreeData.fire()
  }

  getTreeItem(element: TaskBoardItem): vscode.TreeItem {
    return element
  }

  async getChildren(element?: TaskBoardItem): Promise<TaskBoardItem[]> {
    if (!element) {
      try {
        this.tasks = await this.client.listTasks()
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err)
        vscode.window.showWarningMessage(`Failed to load tasks: ${msg}`)
        return []
      }
      return this.getGroups()
    }

    if (element instanceof TaskGroupItem) {
      return this.tasks
        .filter((t) => t.state === element.state)
        .sort((a, b) => {
          const priorityOrder = ['critical', 'high', 'medium', 'low']
          const pa = priorityOrder.indexOf(a.priority ?? 'medium')
          const pb = priorityOrder.indexOf(b.priority ?? 'medium')
          if (pa !== pb) return pa - pb
          return new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime()
        })
        .map((t) => new TaskItem(t))
    }

    return []
  }

  private getGroups(): TaskGroupItem[] {
    const counts = new Map<TaskState, number>()
    for (const task of this.tasks) {
      counts.set(task.state, (counts.get(task.state) ?? 0) + 1)
    }

    return STATE_ORDER.filter((state) => (counts.get(state) ?? 0) > 0).map(
      (state) => new TaskGroupItem(state, counts.get(state) ?? 0)
    )
  }

  getActiveTask(): Task | undefined {
    return this.tasks.find((t) => t.state === 'in_progress')
  }

  dispose(): void {
    this._onDidChangeTreeData.dispose()
  }
}
