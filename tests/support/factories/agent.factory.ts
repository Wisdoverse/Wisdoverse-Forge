import { faker } from '@faker-js/faker'

export type AgentStatus = 'idle' | 'working' | 'waiting' | 'attention' | 'offline'
export type GroupRole = 'manager' | 'worker' | 'none'

export interface AgentData {
  id: string
  name: string
  runtimeId: string | null
  containerId: string | null
  status: AgentStatus
  cliSessionId: string | null
  createdAt: Date
  lastActivity: Date
  cwd: string | null
  currentTool: string | null
  tokens: { current: number; cumulative: number } | null
  gitStatus: Record<string, unknown> | null
  zonePosition: { q: number; r: number } | null
  groupId: string | null
  groupRole: GroupRole
  userId: string
  orgId: string
  projectId: string | null
  claudeFlags: Record<string, boolean> | null
  cliTool: string
}

export class AgentFactory {
  static create(overrides: Partial<AgentData> = {}): AgentData {
    const now = new Date()
    return {
      id: faker.string.uuid(),
      name: faker.lorem.words(3),
      runtimeId: null,
      containerId: null,
      status: 'idle',
      cliSessionId: null,
      createdAt: now,
      lastActivity: now,
      cwd: '/home/user/project',
      currentTool: null,
      tokens: null,
      gitStatus: null,
      zonePosition: null,
      groupId: null,
      groupRole: 'none',
      userId: faker.string.uuid(),
      orgId: faker.string.uuid(),
      projectId: faker.string.uuid(),
      claudeFlags: null,
      cliTool: 'claude',
      ...overrides,
    }
  }

  static createMany(count: number, overrides: Partial<AgentData> = {}): AgentData[] {
    return Array.from({ length: count }, () => this.create(overrides))
  }

  static createWorking(overrides: Partial<AgentData> = {}): AgentData {
    return this.create({ status: 'working', currentTool: 'Bash', ...overrides })
  }

  static createOffline(overrides: Partial<AgentData> = {}): AgentData {
    return this.create({ status: 'offline', ...overrides })
  }

  static createInGroup(
    groupId: string,
    role: GroupRole = 'worker',
    overrides: Partial<AgentData> = {},
  ): AgentData {
    return this.create({ groupId, groupRole: role, ...overrides })
  }
}
