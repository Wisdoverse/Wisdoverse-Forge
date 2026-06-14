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

  it('flags work setup load failures that do not tell beginners how to recover', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  settings: {
    runtime: {
      couldNotLoad: '无法加载工作设置',
    },
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'work-setup-load-next-action',
        location: 'src/app/shared/i18n/locales/zh.ts:5',
      }),
    ])
  })

  it('accepts work setup load failures when they include a recovery step', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  settings: {
    runtime: {
      couldNotLoad:
        '请刷新这个设置页来加载 Agent 工作设置。如果仍然无法加载，请找 owner 或 admin 检查 Agent 工作设置。',
    },
  },
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags work setup load failures that include recovery but start with the failure', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  settings: {
    runtime: {
      couldNotLoad:
        'Agent Work Setup could not load. Refresh this settings page. If it still fails, ask an owner or admin to check agent setup.',
    },
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'work-setup-load-next-action',
        location: 'src/app/shared/i18n/locales/en.ts:5',
      }),
    ])
  })

  it('flags AI service setup summaries that reuse the Check button label as grammar', () => {
    const cwd = fixture({
      'src/app/features/settings/ProvidersSection.tsx': `
export function providerReadinessSummary() {
  return '1 AI service still needs Check. none need Check.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'provider-check-copy',
        location: 'src/app/features/settings/ProvidersSection.tsx:3',
      }),
    ])
  })

  it('accepts AI service setup summaries that describe the connection check', () => {
    const cwd = fixture({
      'src/app/features/settings/ProvidersSection.tsx': `
export function providerReadinessSummary() {
  return '1 AI service needs a connection check. no connection checks are needed.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags AI service zero-ready summaries that do not give a setup action', () => {
    const cwd = fixture({
      'src/app/features/settings/ProvidersSection.tsx': `
export function providerReadinessSummary() {
  return 'No AI services are ready to use yet'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'provider-zero-ready-copy',
        location: 'src/app/features/settings/ProvidersSection.tsx:3',
      }),
    ])
  })

  it('accepts AI service zero-ready summaries that tell users what to do next', () => {
    const cwd = fixture({
      'src/app/features/settings/ProvidersSection.tsx': `
export function providerReadinessSummary() {
  return 'Enable or add an AI service before agents can use one'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags user management empty states that do not point to inviting people', () => {
    const cwd = fixture({
      'src/app/features/admin/UserManagement.tsx': `
function userEmptyState() {
  return { title: 'No one is listed yet' }
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'admin-users-empty-copy',
          location: 'src/app/features/admin/UserManagement.tsx:3',
        }),
      ])
    )
  })

  it('accepts user management empty states that tell users to invite people', () => {
    const cwd = fixture({
      'src/app/features/admin/UserManagement.tsx': `
function userEmptyState() {
  return { title: 'Invite people to list them here' }
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags team space empty states that do not point to creating or syncing first', () => {
    const cwd = fixture({
      'src/app/features/admin/OrganizationsPanel.tsx': `
function OrganizationsEmptyState() {
  return <p>No team spaces are visible yet</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'admin-orgs-empty-copy',
          location: 'src/app/features/admin/OrganizationsPanel.tsx:3',
        }),
      ])
    )
  })

  it('accepts team space empty states that tell users to create or sync first', () => {
    const cwd = fixture({
      'src/app/features/admin/OrganizationsPanel.tsx': `
function OrganizationsEmptyState() {
  return <p>Create or sync a team space first</p>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags admin agent activity copy that does not explain when activity appears', () => {
    const cwd = fixture({
      'src/app/features/admin/AgentsPanel.tsx': `
function formatLastActivity(epochMs) {
  if (!epochMs) return 'No activity yet'
  return 'Activity time needs review'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'admin-agent-activity-copy',
        location: 'src/app/features/admin/AgentsPanel.tsx:3',
      }),
      expect.objectContaining({
        type: 'admin-agent-activity-copy',
        location: 'src/app/features/admin/AgentsPanel.tsx:4',
      }),
    ])
  })

  it('accepts admin agent activity copy that says work must start first', () => {
    const cwd = fixture({
      'src/app/features/admin/AgentsPanel.tsx': `
function formatLastActivity(epochMs) {
  if (!epochMs) return 'Activity appears after work starts'
  return 'Check activity time'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags admin agent status fallback copy that does not name the status field', () => {
    const cwd = fixture({
      'src/app/features/admin/AgentsPanel.tsx': `
function agentStatusLabel(status) {
  return status.trim() ? 'Needs review' : 'Refresh agents to confirm status'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'admin-agent-status-fallback-copy',
        location: 'src/app/features/admin/AgentsPanel.tsx:3',
      }),
    ])
  })

  it('accepts admin agent status fallback copy that tells users what to check', () => {
    const cwd = fixture({
      'src/app/features/admin/AgentsPanel.tsx': `
function agentStatusLabel(status) {
  return status.trim() ? 'Check agent status' : 'Refresh agents to confirm status'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags admin load error titles that do not tell users what to refresh or check', () => {
    const cwd = fixture({
      'src/app/features/admin/adminErrorCopy.ts': `
export function adminPanelLoadErrorMessage(error, label) {
  return 'The admin agents could not load.'
}

export function cliImageStatusErrorMessage(error) {
  return 'The agent tool update status could not load.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'admin-load-error-copy',
          location: 'src/app/features/admin/adminErrorCopy.ts:3',
        }),
        expect.objectContaining({
          type: 'admin-load-error-copy',
          location: 'src/app/features/admin/adminErrorCopy.ts:7',
        }),
      ])
    )
  })

  it('accepts admin load error titles that tell users the next action', () => {
    const cwd = fixture({
      'src/app/features/admin/adminErrorCopy.ts': `
export function adminPanelLoadErrorMessage(error, label) {
  return 'Refresh Admin to reload the agents.'
}

export function cliImageStatusErrorMessage(error) {
  return 'Choose Check now to load tool update status.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags Settings load error titles that do not tell users to refresh Settings', () => {
    const cwd = fixture({
      'src/app/features/settings/providerSettingsErrorMessage.ts': `
function baseMessage(action) {
  return 'AI service settings could not be loaded.'
}
`,
      'src/app/features/settings/gitCredentialsErrorMessage.ts': `
function baseMessage(action) {
  return 'Repository access could not be loaded.'
}
`,
      'src/app/features/settings/sshKeysErrorMessage.ts': `
function baseMessage(action) {
  return 'Repository SSH access could not be loaded.'
}
`,
      'src/app/features/settings/platformKeyErrorMessage.ts': `
function baseMessage(action) {
  return 'Outside tool access keys could not be loaded.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'settings-load-error-copy',
          location: 'src/app/features/settings/providerSettingsErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'settings-load-error-copy',
          location: 'src/app/features/settings/gitCredentialsErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'settings-load-error-copy',
          location: 'src/app/features/settings/sshKeysErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'settings-load-error-copy',
          location: 'src/app/features/settings/platformKeyErrorMessage.ts:3',
        }),
      ])
    )
  })

  it('accepts Settings load error titles that tell users to refresh Settings', () => {
    const cwd = fixture({
      'src/app/features/settings/providerSettingsErrorMessage.ts': `
function baseMessage(action) {
  return 'Refresh Settings to load AI service settings.'
}
`,
      'src/app/features/settings/gitCredentialsErrorMessage.ts': `
function baseMessage(action) {
  return 'Refresh Settings to load repository access.'
}
`,
      'src/app/features/settings/sshKeysErrorMessage.ts': `
function baseMessage(action) {
  return 'Refresh Settings to load repository SSH access.'
}
`,
      'src/app/features/settings/platformKeyErrorMessage.ts': `
function baseMessage(action) {
  return 'Refresh Settings to load outside tool access keys.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags load error titles that do not tell users which view to retry or refresh', () => {
    const cwd = fixture({
      'src/app/shared/model/chat.errors.ts': `
function baseMessage(action) {
  return 'Conversation history could not be loaded.'
}
`,
      'src/app/shared/model/billing.store.ts': `
function billingErrorMessage(area) {
  return 'Invoices could not be loaded.'
}
`,
      'src/app/shared/model/agents.store.ts': `
function loadAgentsError(error) {
  return 'Agents could not be loaded.'
}
`,
      'src/app/entities/agent/model/agents.store.ts': `
function agentServerMessage(error) {
  return 'Forge could not update Agents right now.'
}
`,
      'src/app/features/agents/model/pluginErrorMessage.ts': `
function prefix(action) {
  return 'Agent tools could not be loaded.'
}
`,
      'src/app/features/agents/model/taskErrorMessage.ts': `
function agentTasksErrorMessage(error) {
  return "This agent's work list could not be loaded."
}
`,
      'src/app/features/settings/runtimeErrorMessages.ts': `
function runtimeSettingsErrorMessage(error) {
  return 'Agent Work Setup could not be loaded.'
}
`,
      'src/app/features/manage-members/model/resourceMemberErrorMessages.ts': `
function resourceMemberErrorMessage(error) {
  return 'Members could not load for this team.'
}
`,
      'src/app/pages/settings/model/workspaceSettingsErrorMessage.ts': `
function baseMessage(resource, action) {
  return 'Workspace teams could not be loaded.'
}
`,
      'src/app/features/settings/ResourcesSection.tsx': `
function ResourceProfilesError() {
  return <p>Agent sizes could not be loaded.</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'load-error-title-copy',
          location: 'src/app/shared/model/chat.errors.ts:3',
        }),
        expect.objectContaining({
          type: 'load-error-title-copy',
          location: 'src/app/shared/model/billing.store.ts:3',
        }),
        expect.objectContaining({
          type: 'load-error-title-copy',
          location: 'src/app/shared/model/agents.store.ts:3',
        }),
        expect.objectContaining({
          type: 'load-error-title-copy',
          location: 'src/app/entities/agent/model/agents.store.ts:3',
        }),
        expect.objectContaining({
          type: 'load-error-title-copy',
          location: 'src/app/features/agents/model/pluginErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'load-error-title-copy',
          location: 'src/app/features/agents/model/taskErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'load-error-title-copy',
          location: 'src/app/features/settings/runtimeErrorMessages.ts:3',
        }),
        expect.objectContaining({
          type: 'load-error-title-copy',
          location: 'src/app/features/manage-members/model/resourceMemberErrorMessages.ts:3',
        }),
        expect.objectContaining({
          type: 'load-error-title-copy',
          location: 'src/app/pages/settings/model/workspaceSettingsErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'load-error-title-copy',
          location: 'src/app/features/settings/ResourcesSection.tsx:3',
        }),
      ])
    )
  })

  it('accepts load error titles that tell users which view to retry or refresh', () => {
    const cwd = fixture({
      'src/app/shared/model/chat.errors.ts': `
function baseMessage(action) {
  return 'Retry conversation to load conversation history.'
}
`,
      'src/app/shared/model/billing.store.ts': `
function billingErrorMessage(area) {
  return 'Refresh Billing to load invoices.'
}
`,
      'src/app/shared/model/agents.store.ts': `
function loadAgentsError(error) {
  return 'Refresh Agents to load agents.'
}
`,
      'src/app/entities/agent/model/agents.store.ts': `
function agentServerMessage(error) {
  return 'Refresh Agents to load agents.'
}
`,
      'src/app/features/agents/model/pluginErrorMessage.ts': `
function prefix(action) {
  return 'Refresh this agent page to load tools.'
}
`,
      'src/app/features/agents/model/taskErrorMessage.ts': `
function agentTasksErrorMessage(error) {
  return "Refresh this agent to load its work list."
}
`,
      'src/app/features/settings/runtimeErrorMessages.ts': `
function runtimeSettingsErrorMessage(error) {
  return 'Refresh Settings to load Agent Work Setup.'
}
`,
      'src/app/features/manage-members/model/resourceMemberErrorMessages.ts': `
function resourceMemberErrorMessage(error) {
  return 'Refresh members to load people for this team.'
}
`,
      'src/app/pages/settings/model/workspaceSettingsErrorMessage.ts': `
function baseMessage(resource, action) {
  return 'Refresh Settings to load workspace teams.'
}
`,
      'src/app/features/settings/ResourcesSection.tsx': `
function ResourceProfilesError() {
  return <p>Reload sizes to load agent sizes.</p>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags admin agent missing-field copy that does not tell users to refresh', () => {
    const cwd = fixture({
      'src/app/features/admin/AgentsPanel.tsx': `
function agentOwnerLabel(agent) {
  return agent.ownerEmail || 'Owner not reported yet'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'admin-agent-field-copy',
        location: 'src/app/features/admin/AgentsPanel.tsx:3',
      }),
    ])
  })

  it('accepts admin agent missing-field copy that tells users to refresh', () => {
    const cwd = fixture({
      'src/app/features/admin/AgentsPanel.tsx': `
function agentOwnerLabel(agent) {
  return agent.ownerEmail || 'Refresh agents to load owner'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags work-location labels that leave beginners without a refresh step', () => {
    const cwd = fixture({
      'src/app/entities/agent/model/runtime-kind.ts': `
export function runtimeKindLabel(kind) {
  if (!kind) return 'Work location not reported'
}

export function runtimeKindShortLabel(kind) {
  if (!kind) return 'Location missing'
  return 'Needs review'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'runtime-short-label-copy',
        location: 'src/app/entities/agent/model/runtime-kind.ts:3',
      }),
      expect.objectContaining({
        type: 'runtime-short-label-copy',
        location: 'src/app/entities/agent/model/runtime-kind.ts:7',
      }),
      expect.objectContaining({
        type: 'runtime-short-label-copy',
        location: 'src/app/entities/agent/model/runtime-kind.ts:8',
      }),
    ])
  })

  it('accepts work-location fallback labels that tell users to refresh', () => {
    const cwd = fixture({
      'src/app/entities/agent/model/runtime-kind.ts': `
export function runtimeKindLabel(kind) {
  if (!kind) return 'Refresh work location'
}

export function runtimeKindShortLabel(kind) {
  if (!kind) return 'Refresh location'
  return 'Review location'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags agent setup fallback copy that leaves beginners guessing', () => {
    const cwd = fixture({
      'src/app/entities/agent/model/display-labels.ts': `
export function agentAiServiceLabel(provider) {
  return 'AI service needs review'
}
`,
      'src/app/entities/agent/model/agents.store.ts': `
export function cliToolLabel(tool) {
  return 'Work tool needs review'
}
`,
      'src/app/entities/context/ui/InjectionPreviewModal.tsx': `
function runtimeLabel(runtime) {
  return runtime ? 'Work location needs review' : 'Work location not listed'
}
`,
      'src/app/features/detail/HistoryTab.tsx': `
function runSourceLabel(run) {
  return run.provider ? 'an AI service that needs review' : 'a work tool that needs review'
}
`,
      'src/app/features/settings/RuntimeSection.tsx': `
function fallbackRuntimeLabel(runtime) {
  return runtime ? 'Agent location needs review' : 'Agent location not listed'
}
function fallbackCliToolLabel(tool) {
  return tool ? 'Work tool needs review' : 'Work tool not listed'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'agent-setup-fallback-copy',
          location: 'src/app/entities/agent/model/display-labels.ts:3',
        }),
        expect.objectContaining({
          type: 'agent-setup-fallback-copy',
          location: 'src/app/entities/agent/model/agents.store.ts:3',
        }),
        expect.objectContaining({
          type: 'agent-setup-fallback-copy',
          location: 'src/app/entities/context/ui/InjectionPreviewModal.tsx:3',
        }),
        expect.objectContaining({
          type: 'agent-setup-fallback-copy',
          location: 'src/app/features/detail/HistoryTab.tsx:3',
        }),
        expect.objectContaining({
          type: 'agent-setup-fallback-copy',
          location: 'src/app/features/settings/RuntimeSection.tsx:3',
        }),
        expect.objectContaining({
          type: 'agent-setup-fallback-copy',
          location: 'src/app/features/settings/RuntimeSection.tsx:6',
        }),
      ])
    )
  })

  it('accepts agent setup fallback copy that tells beginners what to check', () => {
    const cwd = fixture({
      'src/app/entities/agent/model/display-labels.ts': `
export function agentAiServiceLabel(provider) {
  return provider ? 'Check AI service' : 'Refresh AI service'
}
`,
      'src/app/entities/agent/model/agents.store.ts': `
export function cliToolLabel(tool) {
  return tool ? 'Check work tool' : 'Refresh AI service'
}
`,
      'src/app/entities/context/ui/InjectionPreviewModal.tsx': `
function runtimeLabel(runtime) {
  return runtime ? 'Check work location' : 'Refresh work location'
}
`,
      'src/app/features/detail/HistoryTab.tsx': `
function runSourceLabel(run) {
  return run.provider ? 'an AI service you should check' : 'a work tool you should check'
}
`,
      'src/app/features/settings/RuntimeSection.tsx': `
function fallbackRuntimeLabel(runtime) {
  return runtime ? 'Check agent location' : 'Refresh agent location'
}
function fallbackCliToolLabel(tool) {
  return tool ? 'Check work tool setup' : 'Refresh work tool setup'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags clipboard failure copy that names browser clipboard access', () => {
    const cwd = fixture({
      'src/app/features/agents/CreateAgentModal.tsx': `
const CLIPBOARD_UNAVAILABLE =
  'Copy is unavailable here (no clipboard access) - select the command text and copy it manually.'
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'clipboard-copy',
        location: 'src/app/features/agents/CreateAgentModal.tsx:3',
      }),
    ])
  })

  it('accepts clipboard failure copy that tells people how to copy manually', () => {
    const cwd = fixture({
      'src/app/features/agents/CreateAgentModal.tsx': `
const CLIPBOARD_UNAVAILABLE =
  'Forge cannot copy from this browser. Select the setup command in the box, then copy it manually.'
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags billing checkpoint invoice copy that does not explain when invoices appear', () => {
    const cwd = fixture({
      'src/app/features/billing/BillingPage.tsx': `
function BillingCheckpoint() {
  return {
    label: 'Invoices',
    value: invoicesCount > 0 ? \`\${invoicesCount} invoices shown\` : 'No invoices yet',
  }
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'billing-checkpoint-copy',
        location: 'src/app/features/billing/BillingPage.tsx:5',
      }),
    ])
  })

  it('accepts billing checkpoint invoice copy that tells people when invoices appear', () => {
    const cwd = fixture({
      'src/app/features/billing/BillingPage.tsx': `
function BillingCheckpoint() {
  return {
    label: 'Invoices',
    value: invoicesCount > 0 ? \`\${invoicesCount} invoices shown\` : 'Invoices appear after a charge',
  }
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags billing usage copy that does not explain what creates usage', () => {
    const cwd = fixture({
      'src/app/features/billing/BillingPage.tsx': `
function BillingCheckpoint() {
  return {
    label: 'Usage',
    value: usageCount > 0 ? \`\${usageCount} usage areas shown\` : 'No usage reported yet',
  }
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'billing-usage-copy',
        location: 'src/app/features/billing/BillingPage.tsx:5',
      }),
    ])
  })

  it('accepts billing usage copy that explains what creates usage', () => {
    const cwd = fixture({
      'src/app/features/billing/BillingPage.tsx': `
function BillingCheckpoint() {
  return {
    label: 'Usage',
    value: usageCount > 0 ? \`\${usageCount} usage areas shown\` : 'Usage appears after agents run billable work',
  }
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags invoice receipt copy that does not explain when the link appears', () => {
    const cwd = fixture({
      'src/app/features/billing/InvoiceList.tsx': `
export function InvoiceList() {
  return <span>No link</span>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'billing-receipt-link-copy',
        location: 'src/app/features/billing/InvoiceList.tsx:3',
      }),
    ])
  })

  it('accepts invoice receipt copy that tells people when the link appears', () => {
    const cwd = fixture({
      'src/app/features/billing/InvoiceList.tsx': `
export function InvoiceList() {
  return <span>Receipt appears after payment finishes</span>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags analytics chart empty states that do not explain what creates data', () => {
    const cwd = fixture({
      'src/app/features/analytics/AnalyticsDashboard.tsx': `
export function AnalyticsDashboard() {
  return (
    <div>
      <p>No activity data</p>
      <p>No tool usage data</p>
    </div>
  )
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'analytics-chart-empty-copy',
        location: 'src/app/features/analytics/AnalyticsDashboard.tsx:5',
      }),
      expect.objectContaining({
        type: 'analytics-chart-empty-copy',
        location: 'src/app/features/analytics/AnalyticsDashboard.tsx:6',
      }),
    ])
  })

  it('accepts analytics chart empty states that explain how to create data', () => {
    const cwd = fixture({
      'src/app/features/analytics/AnalyticsDashboard.tsx': `
export function AnalyticsDashboard() {
  return (
    <div>
      <p>Run a task to fill this chart</p>
      <p>Tool use appears after an agent runs a task</p>
    </div>
  )
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags saved item useful empty copy that does not explain how to rank items', () => {
    const cwd = fixture({
      'src/app/features/analytics/ContextUsageDashboard.tsx': `
const EMPTY_TOP_USEFUL = {
  title: 'No useful saved items yet',
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'analytics-useful-empty-copy',
          location: 'src/app/features/analytics/ContextUsageDashboard.tsx:3',
        }),
      ])
    )
  })

  it('accepts saved item useful empty copy that tells users to mark items useful', () => {
    const cwd = fixture({
      'src/app/features/analytics/ContextUsageDashboard.tsx': `
const EMPTY_TOP_USEFUL = {
  title: 'Mark useful saved items to rank them here',
  detail:
    'After a task uses a saved note or instruction, choose Useful in the task result to place it in this list.',
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags analytics updated-time copy that does not tell users to refresh', () => {
    const cwd = fixture({
      'src/app/features/analytics/ContextUsageDashboard.tsx': `
export function updatedAtLabel() {
  return 'time not available'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'analytics-updated-time-copy',
        location: 'src/app/features/analytics/ContextUsageDashboard.tsx:3',
      }),
    ])
  })

  it('accepts analytics updated-time copy that tells users to refresh', () => {
    const cwd = fixture({
      'src/app/features/analytics/ContextUsageDashboard.tsx': `
export function updatedAtLabel() {
  return 'Refresh analytics to update time'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags optional saved item empty copy that does not explain how more items appear', () => {
    const cwd = fixture({
      'src/app/entities/context/ui/InjectionPreviewModal.tsx': `
export function InjectionPreviewModal() {
  return <PreviewSection empty="No other saved items were found." />
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'saved-item-optional-empty-copy',
          location: 'src/app/entities/context/ui/InjectionPreviewModal.tsx:3',
        }),
      ])
    )
  })

  it('accepts optional saved item empty copy that explains what creates more items', () => {
    const cwd = fixture({
      'src/app/entities/context/ui/InjectionPreviewModal.tsx': `
export function InjectionPreviewModal() {
  return <PreviewSection empty="More saved items appear here after tasks save helpful notes or instructions." />
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task assignment status copy that does not tell users to choose an agent', () => {
    const cwd = fixture({
      'src/app/features/detail/HistoryTab.tsx': `
function taskCheckIn() {
  return { title: 'No agent assigned yet' }
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-agent-assignment-copy',
          location: 'src/app/features/detail/HistoryTab.tsx:3',
        }),
      ])
    )
  })

  it('flags task list agent fallback copy that does not tell users to refresh', () => {
    const cwd = fixture({
      'src/app/features/list/ListView.tsx': `
function taskAgentLabel(task) {
  return 'Agent not reported yet'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-agent-assignment-copy',
          location: 'src/app/features/list/ListView.tsx:3',
        }),
      ])
    )
  })

  it('flags task form agent status copy that does not tell users to refresh agent status', () => {
    const cwd = fixture({
      'src/app/features/board/TaskFormModal.tsx': `
function agentStatusLabel() {
  return 'status not reported'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'task-form-agent-status-copy',
        location: 'src/app/features/board/TaskFormModal.tsx:3',
      }),
    ])
  })

  it('accepts task form agent status copy that tells users to refresh agent status', () => {
    const cwd = fixture({
      'src/app/features/board/TaskFormModal.tsx': `
function agentStatusLabel() {
  return 'refresh agent status'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task form queue load copy that starts with the failure instead of the next step', () => {
    const cwd = fixture({
      'src/app/features/board/TaskFormModal.tsx': `
function handleProjectChange() {
  return 'Task queues could not load for this project. Select the project again to retry.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'task-form-queue-load-copy',
        location: 'src/app/features/board/TaskFormModal.tsx:3',
      }),
    ])
  })

  it('accepts task form queue load copy that starts with the next step', () => {
    const cwd = fixture({
      'src/app/features/board/TaskFormModal.tsx': `
function handleProjectChange() {
  return 'Select the project again to load task queues. If it still does not load, refresh the board or ask an owner to check task queue setup.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task support reference copy that does not tell users to refresh task details', () => {
    const cwd = fixture({
      'src/app/features/detail/TaskDetailPanel.tsx': `
function taskSupportReference() {
  return 'Support reference not reported'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'task-support-reference-copy',
        location: 'src/app/features/detail/TaskDetailPanel.tsx:3',
      }),
    ])
  })

  it('accepts task support reference copy that tells users to refresh task details', () => {
    const cwd = fixture({
      'src/app/features/detail/TaskDetailPanel.tsx': `
function taskSupportReference() {
  return 'Refresh task details'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags agent configuration detail copy that does not tell users what to refresh', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentConfigTab.tsx': `
function modelLabel() {
  return 'AI model not reported'
}

function cliToolLabel() {
  return 'Work tool not reported'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'agent-config-detail-copy',
        location: 'src/app/features/agents/AgentConfigTab.tsx:3',
      }),
      expect.objectContaining({
        type: 'agent-config-detail-copy',
        location: 'src/app/features/agents/AgentConfigTab.tsx:7',
      }),
    ])
  })

  it('accepts agent configuration detail copy that names what to refresh', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentConfigTab.tsx': `
function modelLabel() {
  return 'Refresh AI model'
}

function cliToolLabel() {
  return 'Refresh work tool setup'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags agent model fallback copy that does not tell users what to refresh', () => {
    const cwd = fixture({
      'src/app/entities/agent/model/agents.store.ts': `
const info = {
  model: 'Model not reported',
}
`,
      'src/app/shared/model/agents.store.ts': `
const info = {
  model: agent.model ?? agent.cliTool ?? 'unknown',
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'agent-model-copy',
        location: 'src/app/entities/agent/model/agents.store.ts:3',
      }),
      expect.objectContaining({
        type: 'agent-model-copy',
        location: 'src/app/shared/model/agents.store.ts:3',
      }),
    ])
  })

  it('accepts agent model fallback copy that tells users to refresh model data', () => {
    const cwd = fixture({
      'src/app/entities/agent/model/agents.store.ts': `
const info = {
  model: 'Refresh AI model',
}
`,
      'src/app/shared/model/agents.store.ts': `
const info = {
  model: agent.model ?? agent.cliTool ?? 'Refresh AI model',
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags agent AI service fallback copy that does not tell users what to refresh', () => {
    const cwd = fixture({
      'src/app/entities/agent/model/display-labels.ts': `
function agentAiServiceLabel() {
  return 'AI service not reported'
}
`,
      'src/app/entities/agent/model/agents.store.ts': `
function managedToAgentInfo() {
  return { provider: 'AI service not reported' }
}
`,
      'src/app/shared/model/agents.store.ts': `
function cliToolToProvider() {
  return 'AI service not reported'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'agent-ai-service-copy',
          location: 'src/app/entities/agent/model/display-labels.ts:3',
        }),
        expect.objectContaining({
          type: 'agent-ai-service-copy',
          location: 'src/app/entities/agent/model/agents.store.ts:3',
        }),
        expect.objectContaining({
          type: 'agent-ai-service-copy',
          location: 'src/app/shared/model/agents.store.ts:3',
        }),
      ])
    )
    expect(result.findings).toHaveLength(3)
  })

  it('accepts agent AI service fallback copy that tells users to refresh service data', () => {
    const cwd = fixture({
      'src/app/entities/agent/model/display-labels.ts': `
function agentAiServiceLabel() {
  return 'Refresh AI service'
}
`,
      'src/app/entities/agent/model/agents.store.ts': `
function managedToAgentInfo() {
  return { provider: 'Refresh AI service' }
}
`,
      'src/app/shared/model/agents.store.ts': `
function cliToolToProvider() {
  return 'Refresh AI service'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags access level fallback copy that does not tell users what to refresh', () => {
    const cwd = fixture({
      'src/app/entities/user/model/roleLabels.ts': `
function userRoleLabel() {
  return 'Access level not reported'
  return 'Access level needs review'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'access-level-copy',
          location: 'src/app/entities/user/model/roleLabels.ts:3',
        }),
        expect.objectContaining({
          type: 'access-level-copy',
          location: 'src/app/entities/user/model/roleLabels.ts:4',
        }),
      ])
    )
  })

  it('accepts access level fallback copy that tells users to refresh role data', () => {
    const cwd = fixture({
      'src/app/entities/user/model/roleLabels.ts': `
function userRoleLabel() {
  return 'Refresh access level'
  return 'Check access level'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('accepts task assignment status copy that tells users to choose an agent', () => {
    const cwd = fixture({
      'src/app/features/detail/HistoryTab.tsx': `
function taskCheckIn() {
  return { title: 'Choose an agent to start this task' }
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags timeline empty titles that do not tell users how to begin', () => {
    const cwd = fixture({
      'src/app/widgets/views/TimelineView.tsx': `
export function TimelineView() {
  return <p>No timeline events yet</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'timeline-empty-copy',
        location: 'src/app/widgets/views/TimelineView.tsx:3',
      }),
    ])
  })

  it('accepts timeline empty titles that tell users how to begin', () => {
    const cwd = fixture({
      'src/app/widgets/views/TimelineView.tsx': `
export function TimelineView() {
  return <p>Start a task to build the timeline</p>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags visual map empty titles that do not tell users to open Agents', () => {
    const cwd = fixture({
      'src/app/widgets/views/Workshop3DView.tsx': `
export function Workshop3DEmptyState() {
  return <p>No agents on the visual map yet</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'workshop-3d-empty-copy',
          location: 'src/app/widgets/views/Workshop3DView.tsx:3',
        }),
      ])
    )
  })

  it('accepts visual map empty titles that tell users to open Agents', () => {
    const cwd = fixture({
      'src/app/widgets/views/Workshop3DView.tsx': `
export function Workshop3DEmptyState() {
  return <p>Open Agents to build the visual map</p>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags agent detail activity copy that does not tell users to open Tasks', () => {
    const cwd = fixture({
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
function agentNextStep() {
  return { detail: 'No task activity has been loaded yet. Open Tasks to see this agent history.' }
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'agent-detail-activity-copy',
        location: 'src/app/widgets/agent-detail/AgentDetailView.tsx:3',
      }),
    ])
  })

  it('accepts agent detail activity copy that tells users to open Tasks first', () => {
    const cwd = fixture({
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
function agentNextStep() {
  return { detail: "Open Tasks to load this agent's work history and decide what to send next." }
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags agent tool update status copy that does not tell users to check now', () => {
    const cwd = fixture({
      'src/app/features/admin/CliImagesPanel.tsx': `
function ToolRow() {
  return <p>Latest tool found: Not checked yet</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'cli-image-status-copy',
        location: 'src/app/features/admin/CliImagesPanel.tsx:3',
      }),
    ])
  })

  it('flags agent tool version copy that leaves beginners waiting', () => {
    const cwd = fixture({
      'src/app/features/admin/CliImagesPanel.tsx': `
function ToolRow() {
  return <p>Current version: Version not reported yet</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'cli-image-status-copy',
        location: 'src/app/features/admin/CliImagesPanel.tsx:3',
      }),
    ])
  })

  it('accepts agent tool update status copy that tells users to check now', () => {
    const cwd = fixture({
      'src/app/features/admin/CliImagesPanel.tsx': `
function ToolRow() {
  return <p>Latest tool found: Check now to find latest package</p>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags work setup summaries that do not tell users to sign in first', () => {
    const cwd = fixture({
      'src/app/features/settings/RuntimeSection.tsx': `
function runtimeReadinessSummary() {
  return 'No work tool sign-ins are connected yet'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'runtime-sign-in-copy',
        location: 'src/app/features/settings/RuntimeSection.tsx:3',
      }),
    ])
  })

  it('accepts work setup summaries that tell users to sign in before starting agents', () => {
    const cwd = fixture({
      'src/app/features/settings/RuntimeSection.tsx': `
function runtimeReadinessSummary() {
  return 'Sign in to a work tool before starting agents that need one'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags default agent location copy that does not explain how to recover', () => {
    const cwd = fixture({
      'src/app/features/settings/RuntimeSection.tsx': `
export function RuntimeSection() {
  return <RuntimeReadinessMetric label="Default agent location" value="Not set yet" />
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'runtime-default-location-copy',
        location: 'src/app/features/settings/RuntimeSection.tsx:3',
      }),
    ])
  })

  it('accepts default agent location copy that tells users to load setup first', () => {
    const cwd = fixture({
      'src/app/features/settings/RuntimeSection.tsx': `
export function RuntimeSection() {
  return <RuntimeReadinessMetric label="Default agent location" value="Load setup to choose a location" />
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
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

  it('flags shared agent status fallbacks that do not tell users what to refresh', () => {
    const cwd = fixture({
      'src/app/entities/agent/model/status-labels.ts': `
export function agentStatusLabel(status) {
  if (!status) return 'Status not reported'
  return 'Status needs review'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'agent-shared-status-copy',
        location: 'src/app/entities/agent/model/status-labels.ts:3',
      }),
      expect.objectContaining({
        type: 'agent-shared-status-copy',
        location: 'src/app/entities/agent/model/status-labels.ts:4',
      }),
    ])
  })

  it('accepts shared agent status fallbacks that give users a next step', () => {
    const cwd = fixture({
      'src/app/entities/agent/model/status-labels.ts': `
export function agentStatusLabel(status) {
  if (!status) return 'Refresh agent status'
  return 'Check agent status'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags saved-item review copy that exposes approval workflow jargon', () => {
    const cwd = fixture({
      'src/app/features/context/ApprovalQueueView.tsx': `
const STATE_FILTERS = [
  { value: 'pending', label: 'Pending' },
]

function StatusPill({ state }) {
  return <span>{titleCase(state)}</span>
}

export function DecisionCopy({ approving }) {
  return (
    <section aria-label={approving ? \`Approve item\` : \`Reject item\`}>
      <p>{approving ? 'Approve only when' : 'Reject when'}</p>
      <button title="Approve and save this item"><span>Approve</span></button>
      <button><span>Reject</span></button>
      <Field label="Reject reason" />
      <p>Next: switch back to Pending when you only want items waiting for a decision.</p>
    </section>
  )
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'review-decision-copy',
          location: 'src/app/features/context/ApprovalQueueView.tsx:3',
        }),
        expect.objectContaining({
          type: 'review-decision-copy',
          location: 'src/app/features/context/ApprovalQueueView.tsx:7',
        }),
        expect.objectContaining({
          type: 'review-decision-copy',
          location: 'src/app/features/context/ApprovalQueueView.tsx:13',
        }),
        expect.objectContaining({
          type: 'review-decision-copy',
          location: 'src/app/features/context/ApprovalQueueView.tsx:14',
        }),
        expect.objectContaining({
          type: 'review-decision-copy',
          location: 'src/app/features/context/ApprovalQueueView.tsx:15',
        }),
        expect.objectContaining({
          type: 'review-decision-copy',
          location: 'src/app/features/context/ApprovalQueueView.tsx:16',
        }),
        expect.objectContaining({
          type: 'review-decision-copy',
          location: 'src/app/features/context/ApprovalQueueView.tsx:17',
        }),
      ])
    )
  })

  it('accepts saved-item review copy that says what will be saved', () => {
    const cwd = fixture({
      'src/app/features/context/ApprovalQueueView.tsx': `
const STATE_FILTERS = [
  { value: 'pending', label: 'Waiting for review' },
]

const STATUS_LABELS = {
  pending: 'Waiting for review',
  approved: 'Saved',
  rejected: 'Not saved',
}

export function DecisionCopy({ approving }) {
  return (
    <section aria-label={approving ? \`Save item\` : \`Do not save item\`}>
      <p>{approving ? 'Save only when' : 'Do not save when'}</p>
      <button title="Save this item for future work"><span>Save</span></button>
      <button><span>Do not save</span></button>
      <Field label="Why not save it?" />
      <p>Next: switch back to Waiting for review when you only want items waiting for a decision.</p>
    </section>
  )
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags saved-item history empty copy that does not explain how history starts', () => {
    const cwd = fixture({
      'src/app/features/context/ApprovalQueueView.tsx': `
const EMPTY_HISTORY = {
  title: 'No saved item history yet',
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'review-history-empty-copy',
        location: 'src/app/features/context/ApprovalQueueView.tsx:3',
      }),
    ])
  })

  it('accepts saved-item history empty copy that points to the first review', () => {
    const cwd = fixture({
      'src/app/features/context/ApprovalQueueView.tsx': `
const EMPTY_HISTORY = {
  title: 'Review the first saved item to start history',
  detail:
    'Saved and not-saved notes or instructions appear here after someone reviews the first suggestion.',
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags saved-note capacity copy that exposes unit counts', () => {
    const cwd = fixture({
      'src/app/entities/context/ui/InjectionPreviewModal.tsx': `
export function InjectionPreviewModal() {
  return (
    <section>
      <p>Fits in this agent's note space (4,000 units available)</p>
      <p>Uses about 120 units of note space</p>
    </section>
  )
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'note-space-copy',
        location: 'src/app/entities/context/ui/InjectionPreviewModal.tsx:5',
      }),
      expect.objectContaining({
        type: 'note-space-copy',
        location: 'src/app/entities/context/ui/InjectionPreviewModal.tsx:6',
      }),
    ])
  })

  it('accepts saved-note capacity copy that uses plain size language', () => {
    const cwd = fixture({
      'src/app/entities/context/ui/InjectionPreviewModal.tsx': `
export function InjectionPreviewModal() {
  return (
    <section>
      <p>Plenty of room for saved notes</p>
      <p>Small saved item</p>
    </section>
  )
}
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

  it('flags live work status copy that does not tell users to refresh', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentTerminalTab.tsx': `
export function liveWorkStatusLabel() {
  return 'Status not reported'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'live-work-status-copy',
        location: 'src/app/features/agents/AgentTerminalTab.tsx:3',
      }),
    ])
  })

  it('accepts live work status copy that tells users to refresh', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentTerminalTab.tsx': `
export function liveWorkStatusLabel() {
  return 'Refresh to load status'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task detail run status copy that does not tell users to refresh task status', () => {
    const cwd = fixture({
      'src/app/features/detail/HistoryTab.tsx': `
export function readableRunStatus() {
  return 'Status not reported'
}
`,
      'src/app/features/detail/ContextTab.tsx': `
export function runStatusLabel() {
  return 'Status not reported'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-detail-run-status-copy',
          location: 'src/app/features/detail/HistoryTab.tsx:3',
        }),
        expect.objectContaining({
          type: 'task-detail-run-status-copy',
          location: 'src/app/features/detail/ContextTab.tsx:3',
        }),
      ])
    )
  })

  it('accepts task detail run status copy that tells users to refresh task status', () => {
    const cwd = fixture({
      'src/app/features/detail/HistoryTab.tsx': `
export function readableRunStatus() {
  return 'Refresh task status'
}
`,
      'src/app/features/detail/ContextTab.tsx': `
export function runStatusLabel() {
  return 'Refresh task status'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task status and priority fallbacks that leave beginners guessing', () => {
    const cwd = fixture({
      'src/app/entities/task/model/taskLabels.ts': `
export function taskStateLabel(status) {
  if (!status) return 'Status not listed'
  return 'Status needs review'
}

export function taskPriorityLabel(priority) {
  if (!priority) return 'Priority not listed'
  return 'Priority needs review'
}
`,
      'src/app/features/detail/HistoryTab.tsx': `
export function readableRunStatus(status) {
  return status ? 'Status needs review' : 'Refresh task status'
}
`,
      'src/app/features/detail/ContextTab.tsx': `
export function runStatusLabel(status) {
  return status ? 'Status needs review' : 'Refresh task status'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-status-fallback-copy',
          location: 'src/app/entities/task/model/taskLabels.ts:3',
        }),
        expect.objectContaining({
          type: 'task-status-fallback-copy',
          location: 'src/app/entities/task/model/taskLabels.ts:4',
        }),
        expect.objectContaining({
          type: 'task-status-fallback-copy',
          location: 'src/app/entities/task/model/taskLabels.ts:8',
        }),
        expect.objectContaining({
          type: 'task-status-fallback-copy',
          location: 'src/app/entities/task/model/taskLabels.ts:9',
        }),
        expect.objectContaining({
          type: 'task-status-fallback-copy',
          location: 'src/app/features/detail/HistoryTab.tsx:3',
        }),
        expect.objectContaining({
          type: 'task-status-fallback-copy',
          location: 'src/app/features/detail/ContextTab.tsx:3',
        }),
      ])
    )
  })

  it('accepts task status and priority fallbacks that give users a next step', () => {
    const cwd = fixture({
      'src/app/entities/task/model/taskLabels.ts': `
export function taskStateLabel(status) {
  if (!status) return 'Refresh task status'
  return 'Check task status'
}

export function taskPriorityLabel(priority) {
  if (!priority) return 'Refresh task priority'
  return 'Check task priority'
}
`,
      'src/app/features/detail/HistoryTab.tsx': `
export function readableRunStatus(status) {
  return status ? 'Check task status' : 'Refresh task status'
}
`,
      'src/app/features/detail/ContextTab.tsx': `
export function runStatusLabel(status) {
  return status ? 'Check task status' : 'Refresh task status'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags failed task status copy that does not point users to recovery', () => {
    const cwd = fixture({
      'src/app/entities/task/model/taskLabels.ts': `
const TASK_STATE_LABELS = {
  failed: 'Needs review',
}
`,
      'src/app/features/board/KanbanColumn.tsx': `
const COLUMN_COPY = {
  failed: { label: 'Needs review' },
}
`,
      'src/app/features/detail/HistoryTab.tsx': `
export function readableRunStatus(status) {
  return 'Needs review'
}
`,
      'src/app/features/inbox/InboxItem.tsx': `
const TYPE_CONFIG = {
  failed: { label: 'Needs review' },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-recovery-status-copy',
          location: 'src/app/entities/task/model/taskLabels.ts:3',
        }),
        expect.objectContaining({
          type: 'task-recovery-status-copy',
          location: 'src/app/features/board/KanbanColumn.tsx:3',
        }),
        expect.objectContaining({
          type: 'task-recovery-status-copy',
          location: 'src/app/features/detail/HistoryTab.tsx:3',
        }),
        expect.objectContaining({
          type: 'task-recovery-status-copy',
          location: 'src/app/features/inbox/InboxItem.tsx:3',
        }),
      ])
    )
  })

  it('accepts failed task status copy that points users to recovery', () => {
    const cwd = fixture({
      'src/app/entities/task/model/taskLabels.ts': `
const TASK_STATE_LABELS = {
  failed: 'Review recovery',
}
`,
      'src/app/features/board/KanbanColumn.tsx': `
const COLUMN_COPY = {
  failed: { label: 'Review recovery' },
}
`,
      'src/app/features/detail/HistoryTab.tsx': `
export function readableRunStatus(status) {
  return 'Review recovery'
}
`,
      'src/app/features/inbox/InboxItem.tsx': `
const TYPE_CONFIG = {
  failed: { label: 'Recovery needed' },
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags saved item and task type fallbacks that leave beginners guessing', () => {
    const cwd = fixture({
      'src/app/entities/context/ui/InjectionPreviewModal.tsx': `
function itemKindLabel(kind) {
  if (kind === 'memory') return 'Saved note'
  return 'Saved item needs review'
}
function scopeKindLabel(scope) {
  return 'Sharing setting needs review'
}
function sensitivityLabel(sensitivity) {
  return 'Safety label needs review'
}
function degradationLabel(reason) {
  return 'Some note limits need review'
}
`,
      'src/app/features/detail/ContextCandidatesList.tsx': `
function candidateTitle(candidate) {
  return 'Suggested item needs review'
}
`,
      'src/app/features/analytics/ContextUsageDashboard.tsx': `
function taskKindLabel(kind) {
  return kind ? 'Task type needs review' : 'Task type not listed'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'context-fallback-copy',
          location: 'src/app/entities/context/ui/InjectionPreviewModal.tsx:4',
        }),
        expect.objectContaining({
          type: 'context-fallback-copy',
          location: 'src/app/entities/context/ui/InjectionPreviewModal.tsx:7',
        }),
        expect.objectContaining({
          type: 'context-fallback-copy',
          location: 'src/app/entities/context/ui/InjectionPreviewModal.tsx:10',
        }),
        expect.objectContaining({
          type: 'context-fallback-copy',
          location: 'src/app/entities/context/ui/InjectionPreviewModal.tsx:13',
        }),
        expect.objectContaining({
          type: 'context-fallback-copy',
          location: 'src/app/features/detail/ContextCandidatesList.tsx:3',
        }),
        expect.objectContaining({
          type: 'context-fallback-copy',
          location: 'src/app/features/analytics/ContextUsageDashboard.tsx:3',
        }),
      ])
    )
  })

  it('accepts saved item and task type fallbacks that tell beginners what to do', () => {
    const cwd = fixture({
      'src/app/entities/context/ui/InjectionPreviewModal.tsx': `
function itemKindLabel(kind) {
  if (kind === 'memory') return 'Saved note'
  return 'Check saved item'
}
function scopeKindLabel(scope) {
  return 'Check sharing setting'
}
function sensitivityLabel(sensitivity) {
  return 'Check safety label'
}
function degradationLabel(reason) {
  return 'Check note limits'
}
`,
      'src/app/features/detail/ContextCandidatesList.tsx': `
function candidateTitle(candidate) {
  return 'Check suggested item'
}
`,
      'src/app/features/analytics/ContextUsageDashboard.tsx': `
function taskKindLabel(kind) {
  return kind ? 'Check task type' : 'Refresh task type'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags chat sender fallback copy that leaves beginners guessing', () => {
    const cwd = fixture({
      'src/app/features/chat/ChatView.tsx': `
function messageRoleLabel(role) {
  return role.trim() ? 'Message needs review' : 'Message sender not reported'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'chat-message-fallback-copy',
        location: 'src/app/features/chat/ChatView.tsx:3',
      }),
    ])
  })

  it('accepts chat sender fallback copy that tells users what to do', () => {
    const cwd = fixture({
      'src/app/features/chat/ChatView.tsx': `
function messageRoleLabel(role) {
  return role.trim() ? 'Check message sender' : 'Refresh chat to load sender'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags chat tool step fallback copy that does not tell users how to use the result', () => {
    const cwd = fixture({
      'src/app/features/chat/ToolCallDetail.tsx': `
function toolDataSummary(data) {
  return data.ok ? 'This step finished successfully.' : 'This step needs review.'
}

function toolOutcome() {
  return { label: 'Needs review' }
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'chat-tool-step-copy',
          location: 'src/app/features/chat/ToolCallDetail.tsx:3',
        }),
        expect.objectContaining({
          type: 'chat-tool-step-copy',
          location: 'src/app/features/chat/ToolCallDetail.tsx:7',
        }),
      ])
    )
  })

  it('accepts chat tool step fallback copy that tells users to check before relying on it', () => {
    const cwd = fixture({
      'src/app/features/chat/ToolCallDetail.tsx': `
function toolDataSummary(data) {
  return data.ok ? 'This step finished successfully.' : 'Check this step before relying on the answer.'
}

function toolOutcome() {
  return { label: 'Check step' }
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags app health status copy that does not tell users to check now', () => {
    const cwd = fixture({
      'src/app/features/admin/SystemHealth.tsx': `
type ServiceStatus = 'ready' | 'unknown'

function serviceStatusText(status: ServiceStatus): string {
  if (status === 'ready') return 'Ready'
  return 'Not checked'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'system-health-status-copy',
        location: 'src/app/features/admin/SystemHealth.tsx:6',
      }),
    ])
  })

  it('accepts app health status copy that tells users to check now', () => {
    const cwd = fixture({
      'src/app/features/admin/SystemHealth.tsx': `
type ServiceStatus = 'ready' | 'unknown'

function serviceStatusText(status: ServiceStatus): string {
  if (status === 'ready') return 'Ready'
  return 'Choose Check now to confirm'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags saved instruction load copy that hides the retry action', () => {
    const cwd = fixture({
      'src/app/features/skills/SkillsView.tsx': `
function savedInstructionsLoadErrorMessage(error) {
  return RAW_LOAD_ERROR_PATTERN.test(error) ? 'Saved instructions could not load.' : error
}
`,
      'src/app/shared/model/skills.store.ts': `
function skillResponseErrorMessage(action) {
  return action === 'create'
    ? 'The instruction could not be created. Review the fields and try again.'
    : 'Forge could not load Saved instructions right now. Refresh Saved instructions, then try again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toHaveLength(2)
    expect(result.findings).toEqual(expect.arrayContaining([
      expect.objectContaining({
        type: 'saved-instructions-load-copy',
        location: 'src/app/features/skills/SkillsView.tsx:3',
      }),
      expect.objectContaining({
        type: 'saved-instructions-load-copy',
        location: 'src/app/shared/model/skills.store.ts:5',
      }),
    ]))
  })

  it('accepts saved instruction load copy that points to retry', () => {
    const cwd = fixture({
      'src/app/features/skills/SkillsView.tsx': `
function savedInstructionsLoadErrorMessage(error) {
  return RAW_LOAD_ERROR_PATTERN.test(error) ? 'Saved instructions need a refresh.' : error
}
`,
      'src/app/shared/model/skills.store.ts': `
function skillResponseErrorMessage(action) {
  return action === 'create'
    ? 'The instruction could not be created. Review the fields and try again.'
    : 'Refresh Saved instructions to load the list.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags access key last-used copy that does not explain tool use', () => {
    const cwd = fixture({
      'src/app/features/settings/KeysSection.tsx': `
function KeyRow() {
  return <span>Not used yet</span>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'access-key-last-used-copy',
        location: 'src/app/features/settings/KeysSection.tsx:3',
      }),
    ])
  })

  it('accepts access key last-used copy that explains trusted tool use', () => {
    const cwd = fixture({
      'src/app/features/settings/KeysSection.tsx': `
function KeyRow() {
  return <span>Use this key from a trusted tool first</span>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags date fallback copy that does not tell users which list to refresh', () => {
    const cwd = fixture({
      'src/app/features/settings/GitCredentialsSection.tsx': `
function CredentialRow() {
  return 'Added date not reported'
}
`,
      'src/app/features/settings/SshKeysSection.tsx': `
function SshKeyRow() {
  return 'Added date needs review'
}
`,
      'src/app/features/settings/KeysSection.tsx': `
function KeyRow() {
  return 'Created date needs review'
}
`,
      'src/app/features/admin/UserManagement.tsx': `
function UserRow() {
  return 'Sign-in date needs review'
}
`,
      'src/app/features/admin/OrganizationsPanel.tsx': `
function OrgRow() {
  return 'Created date needs review'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'date-fallback-copy',
          location: 'src/app/features/settings/GitCredentialsSection.tsx:3',
        }),
        expect.objectContaining({
          type: 'date-fallback-copy',
          location: 'src/app/features/settings/SshKeysSection.tsx:3',
        }),
        expect.objectContaining({
          type: 'date-fallback-copy',
          location: 'src/app/features/settings/KeysSection.tsx:3',
        }),
        expect.objectContaining({
          type: 'date-fallback-copy',
          location: 'src/app/features/admin/UserManagement.tsx:3',
        }),
        expect.objectContaining({
          type: 'date-fallback-copy',
          location: 'src/app/features/admin/OrganizationsPanel.tsx:3',
        }),
      ])
    )
  })

  it('accepts date fallback copy that points users to the right list', () => {
    const cwd = fixture({
      'src/app/features/settings/GitCredentialsSection.tsx': `
function CredentialRow() {
  return 'Refresh repository access to load added date'
}
`,
      'src/app/features/settings/SshKeysSection.tsx': `
function SshKeyRow() {
  return 'Refresh SSH access to check added date'
}
`,
      'src/app/features/settings/KeysSection.tsx': `
function KeyRow() {
  return 'Refresh access keys to check created date'
}
`,
      'src/app/features/admin/UserManagement.tsx': `
function UserRow() {
  return 'Refresh users to check sign-in date'
}
`,
      'src/app/features/admin/OrganizationsPanel.tsx': `
function OrgRow() {
  return 'Refresh team spaces to check created date'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags account profile copy that does not tell users to refresh', () => {
    const cwd = fixture({
      'src/app/features/settings/AccountSection.tsx': `
function ProfileRow() {
  return <span>Username not reported yet</span>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'account-profile-copy',
        location: 'src/app/features/settings/AccountSection.tsx:3',
      }),
    ])
  })

  it('accepts account profile copy that tells users to refresh', () => {
    const cwd = fixture({
      'src/app/features/settings/AccountSection.tsx': `
function ProfileRow() {
  return <span>Refresh this page to load username</span>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags saved instruction maintainer fallback copy that does not tell users to refresh', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  skills: {
    detail: {
      unknownAuthor: 'Maintainer not listed yet',
    },
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  skills: {
    detail: {
      unknownAuthor: '暂未列出维护者',
    },
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'skill-maintainer-fallback-copy',
          location: 'src/app/shared/i18n/locales/en.ts:5',
        }),
        expect.objectContaining({
          type: 'skill-maintainer-fallback-copy',
          location: 'src/app/shared/i18n/locales/zh.ts:5',
        }),
      ])
    )
  })

  it('accepts saved instruction maintainer fallback copy that tells users to refresh', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  skills: {
    detail: {
      unknownAuthor: 'Refresh saved instructions to load maintainer',
    },
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  skills: {
    detail: {
      unknownAuthor: '刷新保存的说明以加载维护者',
    },
  },
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags governance audit fallbacks that leave beginners without a field to check', () => {
    const cwd = fixture({
      'src/app/features/governance/AuditLogView.tsx': `
function auditEventLabel(eventType) {
  return readableCodeLabel(eventType, { fallback: 'Change not listed' })
}

function shortEventType(eventType) {
  return eventType.trim() || 'not listed'
}

function resourceTypeLabel(value) {
  return readableCodeLabel(value, { fallback: 'Resource not listed' })
}

function tamperStatusLabel() {
  return { label: 'Not checked' }
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'governance-audit-fallback-copy',
          location: 'src/app/features/governance/AuditLogView.tsx:3',
        }),
        expect.objectContaining({
          type: 'governance-audit-fallback-copy',
          location: 'src/app/features/governance/AuditLogView.tsx:7',
        }),
        expect.objectContaining({
          type: 'governance-audit-fallback-copy',
          location: 'src/app/features/governance/AuditLogView.tsx:11',
        }),
        expect.objectContaining({
          type: 'governance-audit-fallback-copy',
          location: 'src/app/features/governance/AuditLogView.tsx:15',
        }),
      ])
    )
  })

  it('accepts governance audit fallbacks that tell beginners which field to check', () => {
    const cwd = fixture({
      'src/app/features/governance/AuditLogView.tsx': `
function auditEventLabel(eventType) {
  return readableCodeLabel(eventType, { fallback: 'Check audit change' })
}

function shortEventType(eventType) {
  return eventType.trim() || 'Check support event'
}

function resourceTypeLabel(value) {
  return readableCodeLabel(value, { fallback: 'Check record type' })
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags recovery copy that repeats the same refresh step', () => {
    const cwd = fixture({
      'src/app/features/board/boardErrorMessages.ts': `
function serviceRecoveryMessage(action) {
  return 'The task board could not load. Refresh the board, then try again. Forge could not load the board right now. Refresh the board, then try again. If it still fails, ask an owner or admin to check task board setup.'
}
`,
      'src/app/features/detail/taskDetailErrorMessages.ts': `
function serviceRecoveryMessage(action) {
  return 'Saved notes and run details could not load. Refresh the detail panel, then try again. Forge could not load task details right now. Refresh the task, then try again. If it still fails, ask an owner or admin to check task setup.'
}
`,
      'src/app/features/context/approvalQueueErrorMessages.ts': `
function serviceRecoveryMessage(action) {
  return 'The saved item review list could not load. Refresh the list so you see the latest items. Forge could not load saved items right now. Refresh the list, then try again. If it still fails, ask an owner or admin to check saved item setup.'
}
`,
      'src/app/entities/navigation/model/navigation.store.ts': `
function navigationActionErrorMessage(actionPhrase) {
  return 'Navigation could not load task queues. Forge could not connect while loading the sidebar. Check your connection, then refresh the page.'
}
function serviceRecoveryMessage() {
  return 'Forge could not load workspace navigation right now. Refresh the sidebar, then try again. If it still fails, ask an owner or admin to check workspace navigation.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'duplicate-recovery-copy',
          location: 'src/app/features/board/boardErrorMessages.ts:3',
        }),
        expect.objectContaining({
          type: 'duplicate-recovery-copy',
          location: 'src/app/features/detail/taskDetailErrorMessages.ts:3',
        }),
        expect.objectContaining({
          type: 'duplicate-recovery-copy',
          location: 'src/app/features/context/approvalQueueErrorMessages.ts:3',
        }),
        expect.objectContaining({
          type: 'duplicate-recovery-copy',
          location: 'src/app/entities/navigation/model/navigation.store.ts:3',
        }),
        expect.objectContaining({
          type: 'duplicate-recovery-copy',
          location: 'src/app/entities/navigation/model/navigation.store.ts:6',
        }),
      ])
    )
  })

  it('flags task detail load copy that starts with the failure instead of the next step', () => {
    const cwd = fixture({
      'src/app/features/detail/taskDetailErrorMessages.ts': `
const ACTION_FALLBACKS = {
  loadAgents: 'Available agents could not load. Refresh this task before assigning it.',
  loadContext: 'Saved notes and run details could not load. Refresh the detail panel, then try again.',
  loadRuns: 'Agent work history could not load. Refresh Updates before deciding whether to retry this task.',
  previewContext: 'The saved item review could not load. Choose an available agent, then try again.',
}
function networkRecoveryMessage() {
  return 'Forge could not connect while loading this task. Check your connection, then refresh the page.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-detail-load-copy',
          location: 'src/app/features/detail/taskDetailErrorMessages.ts:3',
        }),
        expect.objectContaining({
          type: 'task-detail-load-copy',
          location: 'src/app/features/detail/taskDetailErrorMessages.ts:4',
        }),
        expect.objectContaining({
          type: 'task-detail-load-copy',
          location: 'src/app/features/detail/taskDetailErrorMessages.ts:5',
        }),
        expect.objectContaining({
          type: 'task-detail-load-copy',
          location: 'src/app/features/detail/taskDetailErrorMessages.ts:6',
        }),
        expect.objectContaining({
          type: 'task-detail-load-copy',
          location: 'src/app/features/detail/taskDetailErrorMessages.ts:9',
        }),
      ])
    )
  })

  it('accepts task detail load copy that starts with the next step', () => {
    const cwd = fixture({
      'src/app/features/detail/taskDetailErrorMessages.ts': `
const ACTION_FALLBACKS = {
  loadAgents: 'Refresh this task before assigning an agent.',
  loadContext: 'Refresh the detail panel to load saved notes and run details.',
  loadRuns: 'Refresh Updates before deciding whether to retry this task.',
  previewContext: 'Choose an available agent, then open saved item review again.',
}
function networkRecoveryMessage() {
  return 'If it still does not load, check your connection and refresh the page.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags board load copy that starts with the failure instead of the next step', () => {
    const cwd = fixture({
      'src/app/features/board/boardErrorMessages.ts': `
const ACTION_FALLBACKS = {
  loadReadiness: 'Agent status could not load. Refresh the board before sending work.',
  loadTasks: 'The task board could not load. Refresh the board, then try again.',
  previewContext: 'The saved item preview could not load. Choose an available agent, then try again.',
}
function notFoundMessage() {
  return 'This board item was not found. Refresh the board, then choose the current task again.'
}
function networkRecoveryMessage() {
  return 'Forge could not connect while loading the board. Check your connection, then refresh the page.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'board-load-copy',
          location: 'src/app/features/board/boardErrorMessages.ts:3',
        }),
        expect.objectContaining({
          type: 'board-load-copy',
          location: 'src/app/features/board/boardErrorMessages.ts:4',
        }),
        expect.objectContaining({
          type: 'board-load-copy',
          location: 'src/app/features/board/boardErrorMessages.ts:5',
        }),
        expect.objectContaining({
          type: 'board-load-copy',
          location: 'src/app/features/board/boardErrorMessages.ts:8',
        }),
        expect.objectContaining({
          type: 'board-load-copy',
          location: 'src/app/features/board/boardErrorMessages.ts:11',
        }),
      ])
    )
  })

  it('accepts board load copy that starts with the next step', () => {
    const cwd = fixture({
      'src/app/features/board/boardErrorMessages.ts': `
const ACTION_FALLBACKS = {
  loadReadiness: 'Refresh the board to load agent status before sending work.',
  loadTasks: 'Refresh the board to load tasks.',
  previewContext: 'Choose an available agent, then open the saved item preview again.',
}
function notFoundMessage() {
  return 'Refresh the board, then choose the current task again.'
}
function networkRecoveryMessage() {
  return 'If it still does not load, check your connection and refresh the page.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags authentication network copy that starts with the failure instead of the next step', () => {
    const cwd = fixture({
      'src/app/features/auth/AuthPage.ts': `
function authLoginErrorMessage() {
  return 'Sign-in could not finish. Forge could not connect while signing you in. Check your connection, then try again.'
}
function authRegisterErrorMessage() {
  return 'Account could not be created. Forge could not connect while creating it. Check your connection, then try again.'
}
function authRecoveryErrorMessage() {
  return 'Verification email could not be sent. Forge could not connect while sending it. Check your connection, then try again.'
}
`,
      'src/app/shared/auth/AuthManager.ts': `
const AUTH_NETWORK_ERROR = 'Forge could not connect. Check your connection, then try again.'
`,
      'src/app/shared/api/legacy/AgentAPI.ts': `
const LEGACY_API_NETWORK_ERROR = 'Forge could not connect. Check your connection, then try again.'
`,
      'src/app/features/agents/AgentControlPanel.tsx': `
function agentControlErrorMessage() {
  return 'Forge could not connect while changing this agent. Check your connection, refresh this agent, then try again.'
}
`,
      'src/app/features/chat/useChatStream.ts': `
function chatStreamRequestErrorMessage() {
  return 'Forge could not connect while sending this message. Check your connection, then resend it.'
}
`,
      'src/app/shared/model/chat.errors.ts': `
function networkRecoveryMessage(action) {
  return action === 'load'
    ? 'Forge could not connect while loading this conversation. Check your connection, then try again.'
    : 'Forge could not connect while clearing this chat. Check your connection, then try again.'
}
`,
      'src/app/entities/context/model/feedbackErrorMessage.ts': `
function feedbackErrorMessage() {
  return 'Feedback could not be saved. Forge could not connect while saving it. Check your connection, then try again.'
}
`,
      'src/app/layouts/sidebar/ProjectTree.tsx': `
function renameErrorMessage() {
  return 'Team name could not be saved. Forge could not connect while saving it. Check your connection, then save again.'
}
`,
      'src/app/pages/settings/model/workspaceSettingsErrorMessage.ts': `
function connectionMessage() {
  return 'The project was not created. Forge could not connect while creating this project. Check your connection, then try again.'
}
`,
      'src/app/shared/model/settings.store.ts': `
function settingsConnectionMessage() {
  return 'Settings could not load AI service settings. Forge could not connect while loading Settings. Check your connection, then try again.'
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  errors: {
    network: 'Forge 暂时连不上。请检查网络后重试。',
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'network-copy',
          location: 'src/app/features/auth/AuthPage.ts:3',
        }),
        expect.objectContaining({
          type: 'network-copy',
          location: 'src/app/features/auth/AuthPage.ts:6',
        }),
        expect.objectContaining({
          type: 'network-copy',
          location: 'src/app/features/auth/AuthPage.ts:9',
        }),
        expect.objectContaining({
          type: 'network-copy',
          location: 'src/app/shared/auth/AuthManager.ts:2',
        }),
        expect.objectContaining({
          type: 'network-copy',
          location: 'src/app/shared/api/legacy/AgentAPI.ts:2',
        }),
        expect.objectContaining({
          type: 'network-copy',
          location: 'src/app/features/agents/AgentControlPanel.tsx:3',
        }),
        expect.objectContaining({
          type: 'network-copy',
          location: 'src/app/features/chat/useChatStream.ts:3',
        }),
        expect.objectContaining({
          type: 'network-copy',
          location: 'src/app/shared/model/chat.errors.ts:4',
        }),
        expect.objectContaining({
          type: 'network-copy',
          location: 'src/app/shared/model/chat.errors.ts:5',
        }),
        expect.objectContaining({
          type: 'network-copy',
          location: 'src/app/entities/context/model/feedbackErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'network-copy',
          location: 'src/app/layouts/sidebar/ProjectTree.tsx:3',
        }),
        expect.objectContaining({
          type: 'network-copy',
          location: 'src/app/pages/settings/model/workspaceSettingsErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'network-copy',
          location: 'src/app/shared/model/settings.store.ts:3',
        }),
        expect.objectContaining({
          type: 'network-copy',
          location: 'src/app/shared/i18n/locales/zh.ts:4',
        }),
      ])
    )
  })

  it('accepts authentication network copy that starts with the next step', () => {
    const cwd = fixture({
      'src/app/features/auth/AuthPage.ts': `
function authLoginErrorMessage() {
  return 'Check your connection, then try signing in again. Forge could not reach sign-in.'
}
function authRecoveryErrorMessage() {
  return 'Check your connection, then send the verification email again. Forge could not reach email delivery.'
}
`,
      'src/app/shared/auth/AuthManager.ts': `
const AUTH_NETWORK_ERROR = 'Check your connection, then try again. Forge could not connect.'
`,
      'src/app/shared/api/legacy/AgentAPI.ts': `
const LEGACY_API_NETWORK_ERROR = 'Check your connection, then try again. Forge could not connect.'
`,
      'src/app/features/agents/AgentControlPanel.tsx': `
function agentControlErrorMessage() {
  return 'Check your connection, refresh this agent, then try again. Forge could not connect while changing this agent.'
}
`,
      'src/app/features/chat/useChatStream.ts': `
function chatStreamRequestErrorMessage() {
  return 'Check your connection, then resend the message. Forge could not connect while sending this message.'
}
`,
      'src/app/shared/model/chat.errors.ts': `
function networkRecoveryMessage(action) {
  return action === 'load'
    ? 'Check your connection, then choose Retry conversation again. Forge could not connect while loading this conversation.'
    : 'Check your connection, then clear chat again. Forge could not connect while clearing this chat.'
}
`,
      'src/app/entities/context/model/feedbackErrorMessage.ts': `
function feedbackErrorMessage() {
  return 'Check your connection, then save this feedback again. Forge could not connect while saving it.'
}
`,
      'src/app/layouts/sidebar/ProjectTree.tsx': `
function renameErrorMessage() {
  return 'Check your connection, then save this team name again. Forge could not connect while saving it.'
}
`,
      'src/app/pages/settings/model/workspaceSettingsErrorMessage.ts': `
function connectionMessage() {
  return 'Check your connection, then create this project again. Forge could not connect while creating it.'
}
`,
      'src/app/shared/model/settings.store.ts': `
function settingsConnectionMessage() {
  return 'Check your connection, then refresh Settings to load AI service settings. Forge could not connect while loading Settings.'
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  errors: {
    network: '请检查网络，然后重试。Forge 暂时无法连接。',
  },
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags team and project setting errors that start with the failure', () => {
    const cwd = fixture({
      'src/app/shared/lib/workspaceResourceErrorMessage.ts': `
function workspaceResourceConnectionMessage() {
  return 'Team could not be saved. Forge could not connect while saving workspace settings. Check your connection, then try again.'
}
function workspaceResourceUnavailableMessage() {
  return 'Forge could not save workspace settings right now. Refresh Settings, then save the project again. If it still fails, ask an owner or admin to check workspace setup.'
}
function notFoundMessage() {
  return 'This project could not be found. Refresh Settings, then choose an existing project.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'workspace-resource-copy',
          location: 'src/app/shared/lib/workspaceResourceErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'workspace-resource-copy',
          location: 'src/app/shared/lib/workspaceResourceErrorMessage.ts:6',
        }),
        expect.objectContaining({
          type: 'workspace-resource-copy',
          location: 'src/app/shared/lib/workspaceResourceErrorMessage.ts:9',
        }),
      ])
    )
  })

  it('accepts team and project setting errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/shared/lib/workspaceResourceErrorMessage.ts': `
function workspaceResourceConnectionMessage() {
  return 'Check your connection, then save the team again in Settings.'
}
function workspaceResourceUnavailableMessage() {
  return 'Refresh Settings, then save the project again. If it still fails, ask an owner or admin to check workspace setup.'
}
function notFoundMessage() {
  return 'Refresh Settings, then choose an existing project.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('accepts recovery copy that gives one clear refresh step', () => {
    const cwd = fixture({
      'src/app/features/board/boardErrorMessages.ts': `
function serviceRecoveryMessage(action) {
  return 'Refresh the board to load tasks. If it still fails, ask an owner or admin to check task board setup.'
}
`,
      'src/app/features/detail/taskDetailErrorMessages.ts': `
function serviceRecoveryMessage(action) {
  return 'Refresh the detail panel to load saved notes and run details. If it still fails, ask an owner or admin to check task setup.'
}
`,
      'src/app/features/context/approvalQueueErrorMessages.ts': `
function serviceRecoveryMessage(action) {
  return 'The saved item review list could not load. Refresh the list so you see the latest items. If it still fails, ask an owner or admin to check saved item setup.'
}
`,
      'src/app/entities/navigation/model/navigation.store.ts': `
function navigationActionErrorMessage(actionPhrase) {
  return 'Check your connection, then refresh the sidebar to load task queues.'
}
function serviceRecoveryMessage() {
  return 'Refresh the sidebar to load workspace navigation. If it still fails, ask an owner or admin to check workspace navigation.'
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
