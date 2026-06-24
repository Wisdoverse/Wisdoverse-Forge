import * as vscode from 'vscode'
import type { Task } from '../api/client'

export class StatusBarProvider implements vscode.Disposable {
  private readonly taskItem: vscode.StatusBarItem
  private readonly reviewItem: vscode.StatusBarItem

  constructor() {
    this.taskItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100)
    this.taskItem.command = 'orchestrator.viewTask'
    this.taskItem.name = 'Orchestrator Task'

    this.reviewItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 99)
    this.reviewItem.command = 'orchestrator.refreshReviews'
    this.reviewItem.name = 'Orchestrator Reviews'

    this.setIdle()
    this.setReviewCount(0)

    this.taskItem.show()
    this.reviewItem.show()
  }

  setActiveTask(task: Task): void {
    const priorityIndicator =
      task.priority === 'critical' || task.priority === 'high' ? '$(alert) ' : ''
    const truncatedTitle = task.title.length > 30 ? `${task.title.slice(0, 27)}...` : task.title
    this.taskItem.text = `$(loading~spin) ${priorityIndicator}${truncatedTitle}`
    this.taskItem.tooltip = `Active: ${task.title}\nState: ${task.state}\nClick to view details`
    this.taskItem.backgroundColor = undefined
    this.taskItem.command = {
      command: 'orchestrator.viewTask',
      title: 'View Task',
      arguments: [task],
    }
  }

  setIdle(): void {
    this.taskItem.text = '$(circle-outline) Orchestrator'
    this.taskItem.tooltip = 'No active task'
    this.taskItem.backgroundColor = undefined
    this.taskItem.command = 'orchestrator.refreshTasks'
  }

  setError(message: string): void {
    this.taskItem.text = '$(error) Orchestrator'
    this.taskItem.tooltip = `Error: ${message}`
    this.taskItem.backgroundColor = new vscode.ThemeColor('statusBarItem.errorBackground')
  }

  setReviewCount(count: number): void {
    if (count === 0) {
      this.reviewItem.text = '$(check) Reviews'
      this.reviewItem.tooltip = 'No pending reviews'
      this.reviewItem.backgroundColor = undefined
    } else {
      this.reviewItem.text = `$(request-changes) ${count} review${count !== 1 ? 's' : ''}`
      this.reviewItem.tooltip = `${count} pending review${count !== 1 ? 's' : ''}`
      this.reviewItem.backgroundColor = new vscode.ThemeColor('statusBarItem.warningBackground')
    }
  }

  dispose(): void {
    this.taskItem.dispose()
    this.reviewItem.dispose()
  }
}
