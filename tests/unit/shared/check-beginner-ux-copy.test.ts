import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'

import { checkBeginnerUxCopy } from '../../../scripts/check-beginner-ux-copy.mjs'

let tempRoots: string[] = []

function fixture(files: Record<string, string>) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'beginner-ux-copy-'))
  tempRoots.push(root)
  for (const [file, content] of Object.entries(files)) {
    const fullPath = path.join(root, file)
    fs.mkdirSync(path.dirname(fullPath), { recursive: true })
    fs.writeFileSync(fullPath, content)
  }
  return root
}

afterEach(() => {
  for (const root of tempRoots) {
    fs.rmSync(root, { recursive: true, force: true })
  }
  tempRoots = []
})

describe('check-beginner-ux-copy.mjs', () => {
  it('accepts an empty state when it gives a clear next action', () => {
    const cwd = fixture({
      'src/app/features/tasks/TaskEmptyState.tsx': `
export function TaskEmptyState() {
  return (
    <section>
      <h2>No tasks yet</h2>
      <p>Create a task from the board so agents know what to work on.</p>
    </section>
  )
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags empty states that leave beginners without a next step', () => {
    const cwd = fixture({
      'src/app/features/tasks/TaskEmptyState.tsx': `
export function TaskEmptyState() {
  return <h2>No tasks yet</h2>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'empty-state-next-action',
        location: 'src/app/features/tasks/TaskEmptyState.tsx:3',
      }),
    ])
  })

  it('flags raw transport errors in likely user-visible copy', () => {
    const cwd = fixture({
      'src/app/features/chat/ChatError.tsx': `
export function ChatError() {
  return <p>Failed to fetch</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'raw-error-copy',
        location: 'src/app/features/chat/ChatError.tsx:3',
      }),
    ])
  })

  it('flags generic error copy that gives beginners no recovery step', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  common: {
    error: 'An error occurred',
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'raw-error-copy',
        location: 'src/app/shared/i18n/locales/en.ts:4',
      }),
    ])
  })

  it('flags system-style permission copy in user-visible text', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  agent: {
    title: 'Operation not permitted on this agent',
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'raw-error-copy',
        location: 'src/app/shared/i18n/locales/en.ts:4',
      }),
    ])
  })

  it('flags raw legacy API fallback errors that can reach users', () => {
    const cwd = fixture({
      'src/app/shared/api/legacy/AgentAPI.ts': `
export async function saveThing() {
  return { ok: false, error: 'Network error' }
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'raw-error-copy',
        location: 'src/app/shared/api/legacy/AgentAPI.ts:3',
      }),
    ])
  })

  it('flags validation copy that does not explain the next change', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  agents: {
    invalidProjectPath: 'Invalid project path',
  },
  groups: {
    invalidType: 'Invalid file type. Allowed types are: {{types}}',
  },
  a11y: {
    invalid: 'This field is invalid',
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'validation-next-action',
        location: 'src/app/shared/i18n/locales/en.ts:4',
      }),
      expect.objectContaining({
        type: 'validation-next-action',
        location: 'src/app/shared/i18n/locales/en.ts:7',
      }),
      expect.objectContaining({
        type: 'validation-next-action',
        location: 'src/app/shared/i18n/locales/en.ts:10',
      }),
    ])
  })

  it('flags Chinese validation copy that does not explain the next change', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  agents: {
    invalidProjectPath: '无效的项目路径',
  },
  groups: {
    invalidType: '无效的文件类型，允许的类型：{{types}}',
  },
  a11y: {
    invalid: '此字段无效',
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'validation-next-action',
        location: 'src/app/shared/i18n/locales/zh.ts:4',
      }),
      expect.objectContaining({
        type: 'validation-next-action',
        location: 'src/app/shared/i18n/locales/zh.ts:7',
      }),
      expect.objectContaining({
        type: 'validation-next-action',
        location: 'src/app/shared/i18n/locales/zh.ts:10',
      }),
    ])
  })

  it('flags confirmation copy that hides the impact from beginners', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  agents: {
    confirmDelete: 'Are you sure you want to delete this agent?',
  },
  settings: {
    resetConfirm: 'Are you sure you want to reset all settings?',
  },
  confirm: {
    unsavedChanges: 'You have unsaved changes. Are you sure you want to leave?',
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'confirmation-impact',
        location: 'src/app/shared/i18n/locales/en.ts:4',
      }),
      expect.objectContaining({
        type: 'confirmation-impact',
        location: 'src/app/shared/i18n/locales/en.ts:7',
      }),
      expect.objectContaining({
        type: 'confirmation-impact',
        location: 'src/app/shared/i18n/locales/en.ts:10',
      }),
    ])
  })

  it('flags Chinese confirmation copy that hides the impact from beginners', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  agents: {
    confirmDelete: '确定要删除此 Agent 吗？',
  },
  settings: {
    resetConfirm: '确定要恢复所有设置吗？',
  },
  confirm: {
    unsavedChanges: '您有未保存的更改，确定要离开吗？',
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'confirmation-impact',
        location: 'src/app/shared/i18n/locales/zh.ts:4',
      }),
      expect.objectContaining({
        type: 'confirmation-impact',
        location: 'src/app/shared/i18n/locales/zh.ts:7',
      }),
      expect.objectContaining({
        type: 'confirmation-impact',
        location: 'src/app/shared/i18n/locales/zh.ts:10',
      }),
    ])
  })

  it('accepts confirmation copy when it names the impact', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  agents: {
    confirmDelete: 'Delete this agent? This removes its setup and stops assigning new work to it.',
  },
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags limit and conflict copy that does not explain what to change next', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  auth: {
    passwordTooShort: 'Password must be at least {{min}} characters',
    emailInUse: 'This email is already in use',
    usernameInUse: 'This username is already taken',
    emailDomainRestricted: 'Registration restricted to authorized email domains',
  },
  agents: {
    maxAgentsReached: 'Maximum number of agents reached',
  },
  files: {
    uploadFailed: 'File upload failed',
    tooLarge: 'File is too large. Maximum size is {{size}}.',
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'limit-conflict-next-action',
        location: 'src/app/shared/i18n/locales/en.ts:4',
      }),
      expect.objectContaining({
        type: 'limit-conflict-next-action',
        location: 'src/app/shared/i18n/locales/en.ts:5',
      }),
      expect.objectContaining({
        type: 'limit-conflict-next-action',
        location: 'src/app/shared/i18n/locales/en.ts:6',
      }),
      expect.objectContaining({
        type: 'limit-conflict-next-action',
        location: 'src/app/shared/i18n/locales/en.ts:7',
      }),
      expect.objectContaining({
        type: 'limit-conflict-next-action',
        location: 'src/app/shared/i18n/locales/en.ts:10',
      }),
      expect.objectContaining({
        type: 'limit-conflict-next-action',
        location: 'src/app/shared/i18n/locales/en.ts:13',
      }),
      expect.objectContaining({
        type: 'limit-conflict-next-action',
        location: 'src/app/shared/i18n/locales/en.ts:14',
      }),
    ])
  })

  it('flags Chinese limit and conflict copy that does not explain what to change next', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  auth: {
    passwordTooShort: '密码至少需要 {{min}} 个字符',
    emailInUse: '该邮箱已被使用',
    usernameInUse: '该用户名已被使用',
    emailDomainRestricted: '仅允许使用授权邮箱域名注册',
  },
  agents: {
    maxAgentsReached: '已达到最大 Agent 数量',
  },
  files: {
    uploadFailed: '文件上传失败',
    tooLarge: '文件过大，最大允许 {{size}}',
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'limit-conflict-next-action',
        location: 'src/app/shared/i18n/locales/zh.ts:4',
      }),
      expect.objectContaining({
        type: 'limit-conflict-next-action',
        location: 'src/app/shared/i18n/locales/zh.ts:5',
      }),
      expect.objectContaining({
        type: 'limit-conflict-next-action',
        location: 'src/app/shared/i18n/locales/zh.ts:6',
      }),
      expect.objectContaining({
        type: 'limit-conflict-next-action',
        location: 'src/app/shared/i18n/locales/zh.ts:7',
      }),
      expect.objectContaining({
        type: 'limit-conflict-next-action',
        location: 'src/app/shared/i18n/locales/zh.ts:10',
      }),
      expect.objectContaining({
        type: 'limit-conflict-next-action',
        location: 'src/app/shared/i18n/locales/zh.ts:13',
      }),
      expect.objectContaining({
        type: 'limit-conflict-next-action',
        location: 'src/app/shared/i18n/locales/zh.ts:14',
      }),
    ])
  })

  it('accepts limit and conflict copy when it tells beginners what to change', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  auth: {
    usernameInUse: 'Choose a different username; this one is already taken.',
  },
  files: {
    tooLarge: 'Choose a file under {{size}}, then upload it again.',
  },
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags activity feed labels that expose internal event names', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  feed: {
    eventTypes: {
      tool_use: 'Tool Use',
      tool_result: 'Tool Result',
    },
    tools: {
      Task: 'Subagent Task',
    },
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'activity-jargon-copy',
        location: 'src/app/shared/i18n/locales/en.ts:5',
      }),
      expect.objectContaining({
        type: 'activity-jargon-copy',
        location: 'src/app/shared/i18n/locales/en.ts:6',
      }),
      expect.objectContaining({
        type: 'activity-jargon-copy',
        location: 'src/app/shared/i18n/locales/en.ts:9',
      }),
    ])
  })

  it('flags Chinese activity feed labels that expose internal event names', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  feed: {
    eventTypes: {
      tool_use: '工具调用',
      tool_result: '工具结果',
    },
    tools: {
      Task: '子任务',
    },
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'activity-jargon-copy',
        location: 'src/app/shared/i18n/locales/zh.ts:5',
      }),
      expect.objectContaining({
        type: 'activity-jargon-copy',
        location: 'src/app/shared/i18n/locales/zh.ts:6',
      }),
      expect.objectContaining({
        type: 'activity-jargon-copy',
        location: 'src/app/shared/i18n/locales/zh.ts:9',
      }),
    ])
  })

  it('accepts activity feed labels that describe what happened', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  feed: {
    eventTypes: {
      tool_use: 'Agent used a tool',
    },
    tools: {
      Task: 'Asked another agent',
    },
  },
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags agent status labels that do not explain readiness', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  agents: {
    status: {
      idle: 'Idle',
      offline: 'Offline',
      error: 'Error',
    },
  },
}
`,
      'src/app/features/agents/AgentListView.tsx': `
const STATUS_FILTERS = [
  { value: 'idle', label: 'Idle' },
  { value: 'offline', label: 'Offline' },
]
`,
      'src/app/features/admin/AgentsPanel.tsx': `
function agentStatusLabel() {
  return 'Offline'
}
`,
      'src/app/features/analytics/StatusStats.tsx': `
export function StatusStats() {
  return <StatCard title="Offline" />
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'agent-status-copy',
          location: 'src/app/shared/i18n/locales/en.ts:5',
        }),
        expect.objectContaining({
          type: 'agent-status-copy',
          location: 'src/app/shared/i18n/locales/en.ts:6',
        }),
        expect.objectContaining({
          type: 'agent-status-copy',
          location: 'src/app/shared/i18n/locales/en.ts:7',
        }),
        expect.objectContaining({
          type: 'agent-status-copy',
          location: 'src/app/features/agents/AgentListView.tsx:3',
        }),
        expect.objectContaining({
          type: 'agent-status-copy',
          location: 'src/app/features/admin/AgentsPanel.tsx:3',
        }),
        expect.objectContaining({
          type: 'agent-status-copy',
          location: 'src/app/features/analytics/StatusStats.tsx:3',
        }),
      ])
    )
  })

  it('flags Chinese agent status labels that do not explain readiness', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  agents: {
    status: {
      idle: '空闲',
      offline: '离线',
      error: '错误',
    },
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'agent-status-copy',
        location: 'src/app/shared/i18n/locales/zh.ts:5',
      }),
      expect.objectContaining({
        type: 'agent-status-copy',
        location: 'src/app/shared/i18n/locales/zh.ts:6',
      }),
      expect.objectContaining({
        type: 'agent-status-copy',
        location: 'src/app/shared/i18n/locales/zh.ts:7',
      }),
    ])
  })

  it('accepts agent status labels that explain readiness', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  agents: {
    status: {
      idle: 'Ready',
      working: 'Working now',
      offline: 'Not connected',
      error: 'Needs attention',
    },
  },
}
`,
      'src/app/features/agents/AgentListView.tsx': `
const STATUS_FILTERS = [
  { value: 'idle', label: 'Ready' },
  { value: 'offline', label: 'Not connected' },
]
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('ignores raw legacy API parser regexes', () => {
    const cwd = fixture({
      'src/app/shared/api/legacy/AgentAPI.ts': `
const RAW_LEGACY_ERROR_PATTERN =
  /^(?:Network error|Server error\\s*\\(\\d{3}\\)|HTTP\\s+\\d{3})$/i
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('does not treat status badges as empty states', () => {
    const cwd = fixture({
      'src/app/features/admin/StatusBadge.tsx': `
export function StatusBadge() {
  return <span>Unavailable</span>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('does not treat summary values outside empty-state UI as empty states', () => {
    const cwd = fixture({
      'src/app/features/skills/skillSummary.ts': `
export function skillSummary(totalCount: number) {
  if (totalCount === 0) return 'No saved instructions yet'
  return 'Saved instructions are ready'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags profile summary rows that leave beginners without a next step', () => {
    const cwd = fixture({
      'src/app/widgets/agents/AgentSummary.tsx': `
function ProfileSummaryRow(_props: { label: string; value: string }) {
  return null
}

export function AgentSummary() {
  return <ProfileSummaryRow label="Saved instructions" value="No saved instructions used in recent work yet" />
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'empty-state-next-action',
        location: 'src/app/widgets/agents/AgentSummary.tsx:7',
      }),
    ])
  })

  it('accepts profile summary rows when they include a next action', () => {
    const cwd = fixture({
      'src/app/widgets/agents/AgentSummary.tsx': `
function ProfileSummaryRow(_props: { label: string; value: string }) {
  return null
}

export function AgentSummary() {
  return <ProfileSummaryRow label="Saved instructions" value="No saved instructions yet. Save useful steps after a task." />
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags i18n empty-state keys without a next action', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  groups: {
    noGroups: 'No groups yet',
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'empty-state-next-action',
        location: 'src/app/shared/i18n/locales/en.ts:4',
      }),
    ])
  })

  it('accepts i18n empty-state keys with a clear next action', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  users: {
    noUsers: 'No users match this view. Clear search or invite a user first.',
  },
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('ignores raw failure strings inside error parsers', () => {
    const cwd = fixture({
      'src/app/features/chat/chatErrorMessage.ts': `
export function isNetworkError(message: string) {
  return message.includes('Failed to fetch')
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('scans user-facing error message helpers for raw fallback copy', () => {
    const cwd = fixture({
      'src/app/features/chat/chatErrorMessage.ts': `
export function chatErrorMessage() {
  return 'Server error (503)'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'raw-error-copy',
        location: 'src/app/features/chat/chatErrorMessage.ts:3',
      }),
    ])
  })

  it('flags agent location jargon in user-visible copy', () => {
    const cwd = fixture({
      'src/app/features/agents/CreateAgentModal.tsx': `
export function CreateAgentModal() {
  return <p>Connect a local agent with the Forge CLI.</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'beginner-jargon-copy',
        location: 'src/app/features/agents/CreateAgentModal.tsx:3',
      }),
    ])
  })

  it('flags managed workspace agent noun stacks in user-visible copy', () => {
    const cwd = fixture({
      'src/app/features/agents/CreateAgentModal.tsx': `
export function CreateAgentModal() {
  return <p>Create managed workspace agents before assigning file work.</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'beginner-jargon-copy',
        location: 'src/app/features/agents/CreateAgentModal.tsx:3',
      }),
    ])
  })

  it('flags raw CLI tool id lists in user-visible copy', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  agent: {
    detail: 'Choose claude, codex, gemini, or opencode before creating an agent.',
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'beginner-jargon-copy',
        location: 'src/app/shared/i18n/locales/en.ts:4',
      }),
    ])
  })

  it('flags raw CLI tool id lists in Chinese user-visible copy', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  agent: {
    detail: '请先选择 claude、codex、gemini 或 opencode。',
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'beginner-jargon-copy',
        location: 'src/app/shared/i18n/locales/zh.ts:4',
      }),
    ])
  })

  it('flags placeholder copy that leaves beginners guessing', () => {
    const cwd = fixture({
      'src/app/features/tasks/TaskStatus.tsx': `
export function TaskStatus() {
  return <p>Status: Unknown</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'placeholder-copy',
        location: 'src/app/features/tasks/TaskStatus.tsx:3',
      }),
    ])
  })

  it('accepts missing information when the copy explains what happens next', () => {
    const cwd = fixture({
      'src/app/features/tasks/TaskStatus.tsx': `
export function TaskStatus() {
  return <p>Status not reported yet. Refresh the task after the agent updates.</p>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('ignores placeholder-like internal status values', () => {
    const cwd = fixture({
      'src/app/features/admin/SystemHealth.tsx': `
type ServiceStatus = 'ready' | 'unknown'

export function SystemHealth(props: { status?: ServiceStatus }) {
  const status = props.status ?? 'unknown'
  return <p>{status === 'unknown' ? 'Not checked yet' : 'Ready'}</p>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags recoverable failure copy without a next action', () => {
    const cwd = fixture({
      'src/app/features/tasks/TaskFailure.tsx': `
export function TaskFailure() {
  return <p>The task could not be started.</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'error-next-action',
        location: 'src/app/features/tasks/TaskFailure.tsx:3',
      }),
    ])
  })

  it('accepts recoverable failure copy with a nearby next action', () => {
    const cwd = fixture({
      'src/app/features/tasks/TaskFailure.tsx': `
export function TaskFailure() {
  return (
    <section>
      <p>The task could not be started.</p>
      <p>Open task setup, check the selected agent, then try again.</p>
    </section>
  )
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('ignores parser regexes and cleanup regexes inside error message helpers', () => {
    const cwd = fixture({
      'src/app/features/chat/chatErrorMessage.ts': `
const GENERIC_BODY_TEXT = /^(Unauthorized|Forbidden|Not Found|Internal Server Error)$/i

export function cleanMessage(value: string) {
  return value.replace(/\\s+Details?:\\s*(Unauthorized|Forbidden|Not Found|Internal Server Error)\\.?$/i, '')
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })
})
