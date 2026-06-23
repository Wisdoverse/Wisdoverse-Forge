import { create } from 'zustand'

/**
 * Live status of a single workflow node, as carried by a `workflow:node_status`
 * realtime event relayed from the orchestrator.
 */
export interface WorkflowNodeState {
  name: string
  status: string
  detail?: string
}

/** Per-workflow map of nodeId -> node state. */
export type WorkflowNodes = Record<string, WorkflowNodeState>

interface WorkflowState {
  /** workflowId -> (nodeId -> node state). */
  workflows: Record<string, WorkflowNodes>
  upsertNodeStatus: (workflowId: string, nodeId: string, node: WorkflowNodeState) => void
  reset: () => void
}

const initialState = {
  workflows: {} as Record<string, WorkflowNodes>,
}

// ponytail: a flat per-workflow node map is the minimal store the realtime
// events need. No timeline ordering, no 3D/graph view — a rich workflow
// progress UI is a separate follow-up (see plan #792, Deferred).
export const useWorkflowStore = create<WorkflowState>((set) => ({
  ...initialState,
  upsertNodeStatus: (workflowId, nodeId, node) =>
    set((s) => ({
      workflows: {
        ...s.workflows,
        [workflowId]: { ...s.workflows[workflowId], [nodeId]: node },
      },
    })),
  reset: () => set({ workflows: {} }),
}))
