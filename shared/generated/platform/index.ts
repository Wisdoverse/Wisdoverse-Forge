/**
 * Generated gRPC Types and Clients
 *
 * Auto-generated from platform/proto/*.proto
 * DO NOT EDIT - regenerate with: npm run proto:gen:platform
 */

// Common types
export { Empty, Timestamp } from './common.js'

// Runtime service
export {
  RuntimeServiceClient,
  type CreateAgentRequest,
  type CreateAgentResponse,
  type AgentMount,
  type StartAgentRequest,
  type StopAgentRequest,
  type DestroyAgentRequest,
  type SendPromptRequest,
  type ResumeAgentRequest,
  type GetOutputRequest,
  type GetOutputResponse,
  type StreamOutputRequest,
  type OutputChunk,
  type ExecInAgentRequest,
  type ExecInAgentResponse,
  type IsAliveRequest,
  type IsAliveResponse,
  type SendRawInputRequest,
  type TerminalInput,
  type TerminalOutput,
  type TerminalResize,
  type AgentResourceLimits,
} from './runtime.js'

// Agent service (was Session service)
export {
  AgentServiceClient,
  type AgentIdRequest,
  type AgentState,
  type UpdateStateRequest,
  type HealthCheckResult,
  type AgentHealthEntry,
  type GitStatus as AgentGitStatus,
  type GitPollResponse,
  type TokenInfo,
  type ListActiveAgentsRequest,
  type ListActiveAgentsResponse,
} from './agent.js'

// Health service
export {
  HealthServiceClient,
  type HealthCheckRequest,
  type HealthCheckResponse,
  HealthCheckResponse_Status,
  type ComponentHealth,
  type ReadyResponse,
  type LiveResponse,
} from './health.js'

// Event service
export {
  EventServiceClient,
  type IngestEventRequest,
  type IngestEventResponse,
  type GetHistoryRequest,
  type GetHistoryResponse,
  type StreamEventsRequest,
  type EventPayload,
  type EventCountRequest,
  type EventCountResponse,
} from './event.js'

// Pool service
export {
  PoolServiceClient,
  type AllocateRequest,
  type AllocateResponse,
  type ReleaseRequest,
  type WarmPoolRequest,
  type WarmPoolResponse,
  type DrainRequest,
  type DrainResponse,
  type PoolStatus,
  type PoolContainer,
} from './pool.js'

// Docker service
export {
  DockerServiceClient,
  type CreateContainerRequest,
  type CreateContainerResponse,
  type ContainerIdRequest,
  type StopContainerRequest,
  type RemoveContainerRequest,
  type ExecRequest,
  type ExecResponse,
  type LogsRequest,
  type LogChunk,
  type ContainerInfo,
  type ResourceUsage,
  type ListContainersRequest,
  type ListContainersResponse,
  type Mount,
  type ResourceLimits,
  type NetworkConfig,
  type SecurityConfig,
} from './docker.js'

// WebSocket service
export {
  WebSocketServiceClient,
  type BroadcastEventRequest,
  type BroadcastAgentRequest,
  type BroadcastTokensRequest,
  type BroadcastAgentsRequest,
  type BroadcastPermissionRequest,
  type ConnectionCountResponse,
  type HasSubscribersRequest,
  type HasSubscribersResponse,
  type DisconnectAgentRequest,
} from './websocket.js'

// Worker service
export {
  WorkerServiceClient,
  type StartWorkersRequest,
  type WorkerStatusResponse,
  type WorkerInfo,
  type TokenPollResponse,
  type OutputPollResponse,
  type CleanupResponse,
} from './worker.js'

// Git service
export {
  GitServiceClient,
  type GitGetStatusRequest,
  type GitGetStatusResponse,
  type GitGetBranchRequest,
  type GitGetBranchResponse,
  type GitStreamStatusRequest,
  type GitStatusUpdate,
} from './git.js'

// DevEnv service
export {
  DevEnvServiceClient,
  DevEnvHealth,
  type DevEnvStatusRequest,
  type DevEnvStatusResponse,
  type DevEnvListRequest,
  type DevEnvListResponse,
  type DevEnvContainer,
  type DevEnvPort,
  type DevEnvCleanupRequest,
  type DevEnvCleanupResponse,
  type DevEnvQuotaRequest,
  type DevEnvQuotaResponse,
  type DevEnvQuotaLevel,
  type ExecStreamInput,
  type ExecStreamOutput,
  type DevEnvExecStart,
  type DevEnvExecResize,
} from './devenv.js'
