import { faker } from '@faker-js/faker'

type HookEventType =
  | 'pre_tool_use'
  | 'post_tool_use'
  | 'stop'
  | 'subagent_stop'
  | 'session_start'
  | 'session_end'
  | 'user_prompt_submit'
  | 'notification'
  | 'pre_compact'

interface BaseEvent {
  id: string
  timestamp: number
  type: HookEventType
  sessionId: string
  cwd: string
  runtimeId?: string
  orgId?: string
}

export class EventFactory {
  private static base(type: HookEventType, overrides: Partial<BaseEvent> = {}): BaseEvent {
    return {
      id: faker.string.uuid(),
      timestamp: Date.now(),
      type,
      sessionId: faker.string.uuid(),
      cwd: '/home/user/project',
      ...overrides,
    }
  }

  static preToolUse(overrides: Record<string, unknown> = {}) {
    return {
      ...this.base('pre_tool_use', overrides as Partial<BaseEvent>),
      tool: overrides.tool ?? 'Bash',
      toolInput: overrides.toolInput ?? { command: 'echo hello' },
      toolUseId: overrides.toolUseId ?? faker.string.uuid(),
      assistantText: overrides.assistantText ?? undefined,
    }
  }

  static postToolUse(overrides: Record<string, unknown> = {}) {
    return {
      ...this.base('post_tool_use', overrides as Partial<BaseEvent>),
      tool: overrides.tool ?? 'Bash',
      toolInput: overrides.toolInput ?? { command: 'echo hello' },
      toolResponse: overrides.toolResponse ?? { output: 'hello' },
      toolUseId: overrides.toolUseId ?? faker.string.uuid(),
      success: overrides.success ?? true,
      duration: overrides.duration ?? undefined,
    }
  }

  static userPromptSubmit(overrides: Record<string, unknown> = {}) {
    return {
      ...this.base('user_prompt_submit', overrides as Partial<BaseEvent>),
      prompt: overrides.prompt ?? faker.lorem.sentence(),
      imageUrls: overrides.imageUrls ?? undefined,
    }
  }

  static sessionStart(overrides: Record<string, unknown> = {}) {
    return {
      ...this.base('session_start', overrides as Partial<BaseEvent>),
      source: overrides.source ?? 'startup',
    }
  }

  static sessionEnd(overrides: Record<string, unknown> = {}) {
    return {
      ...this.base('session_end', overrides as Partial<BaseEvent>),
      reason: overrides.reason ?? 'other',
    }
  }

  static stop(overrides: Record<string, unknown> = {}) {
    return {
      ...this.base('stop', overrides as Partial<BaseEvent>),
      stopHookActive: overrides.stopHookActive ?? false,
      response: overrides.response ?? undefined,
    }
  }

  static notification(overrides: Record<string, unknown> = {}) {
    return {
      ...this.base('notification', overrides as Partial<BaseEvent>),
      message: overrides.message ?? faker.lorem.sentence(),
      notificationType: overrides.notificationType ?? undefined,
    }
  }
}
