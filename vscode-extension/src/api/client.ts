import { getConfig } from '../config';

export interface Task {
  id: string;
  title: string;
  state: TaskState;
  assignee?: string;
  priority?: 'low' | 'medium' | 'high' | 'critical';
  workflowId?: string;
  createdAt: string;
  updatedAt: string;
}

export type TaskState =
  | 'backlog'
  | 'ready'
  | 'in_progress'
  | 'in_review'
  | 'done'
  | 'blocked';

export interface Review {
  id: string;
  taskId: string;
  taskTitle: string;
  status: 'pending' | 'approved' | 'rejected';
  reviewer?: string;
  submittedAt: string;
  summary?: string;
}

export interface DashboardMetrics {
  totalTasks: number;
  byState: Record<TaskState, number>;
  pendingReviews: number;
  completedToday: number;
}

export interface Workflow {
  id: string;
  name: string;
  description?: string;
  states: string[];
}

interface ApiResponse {
  ok: boolean;
  error?: string;
}

type TasksResponse = ApiResponse & { tasks: Task[] };
type TaskResponse = ApiResponse & { task: Task };
type ReviewsResponse = ApiResponse & { reviews: Review[] };
type DashboardResponse = ApiResponse & { metrics: DashboardMetrics };
type WorkflowsResponse = ApiResponse & { workflows: Workflow[] };

export class OrchestratorClient {
  private accessToken: string | null = null;

  setAccessToken(token: string | null): void {
    this.accessToken = token;
  }

  private async request<T>(path: string, options: RequestInit = {}): Promise<T> {
    const { serverUrl } = getConfig();
    const url = `${serverUrl}${path}`;

    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      ...((options.headers as Record<string, string>) || {}),
    };

    if (this.accessToken) {
      headers['Authorization'] = `Bearer ${this.accessToken}`;
    }

    const response = await fetch(url, {
      ...options,
      headers,
    });

    if (!response.ok) {
      const text = await response.text().catch(() => '');
      throw new Error(`API request failed: ${response.status} ${response.statusText}${text ? ` — ${text}` : ''}`);
    }

    const data = (await response.json()) as T & { ok: boolean; error?: string };

    if (data.ok === false) {
      throw new Error(data.error ?? 'Unknown API error');
    }

    return data;
  }

  async listTasks(): Promise<Task[]> {
    const data = await this.request<TasksResponse>('/api/v1/tasks');
    return data.tasks;
  }

  async getTask(id: string): Promise<Task> {
    const data = await this.request<TaskResponse>(`/api/v1/tasks/${encodeURIComponent(id)}`);
    return data.task;
  }

  async transitionTask(id: string, toState: TaskState): Promise<Task> {
    const data = await this.request<TaskResponse>(
      `/api/v1/tasks/${encodeURIComponent(id)}/transition`,
      {
        method: 'POST',
        body: JSON.stringify({ state: toState }),
      },
    );
    return data.task;
  }

  async listReviews(): Promise<Review[]> {
    const data = await this.request<ReviewsResponse>('/api/v1/reviews');
    return data.reviews;
  }

  async approveReview(id: string): Promise<void> {
    await this.request(`/api/v1/reviews/${encodeURIComponent(id)}/approve`, {
      method: 'POST',
    });
  }

  async rejectReview(id: string, reason?: string): Promise<void> {
    await this.request(`/api/v1/reviews/${encodeURIComponent(id)}/reject`, {
      method: 'POST',
      body: JSON.stringify({ reason }),
    });
  }

  async getDashboard(): Promise<DashboardMetrics> {
    const data = await this.request<DashboardResponse>('/api/v1/metrics/dashboard');
    return data.metrics;
  }

  async listWorkflows(): Promise<Workflow[]> {
    const data = await this.request<WorkflowsResponse>('/api/v1/workflows');
    return data.workflows;
  }
}
