import * as vscode from 'vscode'
import type { OrchestratorClient, Review } from '../api/client'

const STATUS_ORDER = ['pending', 'approved', 'rejected'] as const

const STATUS_ICONS: Record<Review['status'], vscode.ThemeIcon> = {
  pending: new vscode.ThemeIcon('request-changes', new vscode.ThemeColor('list.warningForeground')),
  approved: new vscode.ThemeIcon('check', new vscode.ThemeColor('testing.iconPassed')),
  rejected: new vscode.ThemeIcon('close', new vscode.ThemeColor('testing.iconFailed')),
}

const STATUS_LABELS: Record<Review['status'], string> = {
  pending: 'Pending',
  approved: 'Approved',
  rejected: 'Rejected',
}

type ReviewTreeItem = ReviewGroupItem | ReviewItem

class ReviewGroupItem extends vscode.TreeItem {
  constructor(
    public readonly status: Review['status'],
    public readonly count: number
  ) {
    const collapsed =
      status === 'pending'
        ? vscode.TreeItemCollapsibleState.Expanded
        : vscode.TreeItemCollapsibleState.Collapsed
    super(`${STATUS_LABELS[status]} (${count})`, collapsed)
    this.iconPath = STATUS_ICONS[status]
    this.contextValue = 'reviewGroup'
  }
}

class ReviewItem extends vscode.TreeItem {
  constructor(public readonly review: Review) {
    super(review.taskTitle, vscode.TreeItemCollapsibleState.None)

    this.id = review.id
    this.iconPath = STATUS_ICONS[review.status]
    this.contextValue = `review-${review.status}`

    if (review.status === 'pending') {
      this.resourceUri = vscode.Uri.parse(`orchestrator://review/${review.id}`)
    }

    this.description = review.reviewer ?? ''
    this.tooltip = new vscode.MarkdownString(
      [
        `**${review.taskTitle}**`,
        '',
        `Status: ${STATUS_LABELS[review.status]}`,
        review.reviewer ? `Reviewer: ${review.reviewer}` : null,
        review.summary ? `Summary: ${review.summary}` : null,
        `Submitted: ${new Date(review.submittedAt).toLocaleString()}`,
      ]
        .filter(Boolean)
        .join('\n')
    )
  }
}

export class ReviewPanelProvider implements vscode.TreeDataProvider<ReviewTreeItem> {
  private readonly _onDidChangeTreeData = new vscode.EventEmitter<
    ReviewTreeItem | undefined | void
  >()
  readonly onDidChangeTreeData = this._onDidChangeTreeData.event

  private reviews: Review[] = []

  constructor(private readonly client: OrchestratorClient) {}

  refresh(): void {
    this._onDidChangeTreeData.fire()
  }

  updateReviews(reviews: Review[]): void {
    this.reviews = reviews
    this._onDidChangeTreeData.fire()
  }

  getTreeItem(element: ReviewTreeItem): vscode.TreeItem {
    return element
  }

  async getChildren(element?: ReviewTreeItem): Promise<ReviewTreeItem[]> {
    if (!element) {
      try {
        this.reviews = await this.client.listReviews()
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err)
        vscode.window.showWarningMessage(`Failed to load reviews: ${msg}`)
        return []
      }
      return this.getGroups()
    }

    if (element instanceof ReviewGroupItem) {
      return this.reviews
        .filter((r) => r.status === element.status)
        .sort((a, b) => new Date(b.submittedAt).getTime() - new Date(a.submittedAt).getTime())
        .map((r) => new ReviewItem(r))
    }

    return []
  }

  private getGroups(): ReviewGroupItem[] {
    const counts = new Map<Review['status'], number>()
    for (const review of this.reviews) {
      counts.set(review.status, (counts.get(review.status) ?? 0) + 1)
    }

    return STATUS_ORDER.filter((status) => (counts.get(status) ?? 0) > 0).map(
      (status) => new ReviewGroupItem(status, counts.get(status) ?? 0)
    )
  }

  getPendingCount(): number {
    return this.reviews.filter((r) => r.status === 'pending').length
  }

  dispose(): void {
    this._onDidChangeTreeData.dispose()
  }
}
