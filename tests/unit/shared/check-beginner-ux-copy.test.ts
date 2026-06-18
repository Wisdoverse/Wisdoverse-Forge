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

  it('flags saved-item review copy that stops at unavailable preview wording', () => {
    const cwd = fixture({
      'src/app/features/context/ApprovalQueueView.tsx': `
export function ApprovalQueueView() {
  return <p>This cannot be saved because the original task preview is unavailable.</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'review-decision-copy',
        location: 'src/app/features/context/ApprovalQueueView.tsx:3',
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

  it('flags shared API fallbacks that explain the failure before recovery', () => {
    const cwd = fixture({
      'src/app/shared/api/legacy/AgentAPI.ts': `
const LEGACY_API_REQUEST_ERROR =
  'Forge could not finish this request. Wait a moment, then try again.'
`,
      'src/app/shared/api/agent-api-types.ts': `
export function extractApiError(
  fallback = 'Forge did not return a clear error. Refresh, then try again.'
) {
  return fallback
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'api-fallback-error-copy',
          location: 'src/app/shared/api/legacy/AgentAPI.ts:3',
        }),
        expect.objectContaining({
          type: 'api-fallback-error-copy',
          location: 'src/app/shared/api/agent-api-types.ts:3',
        }),
      ])
    )
  })

  it('accepts shared API fallbacks that start with recovery', () => {
    const cwd = fixture({
      'src/app/shared/api/legacy/AgentAPI.ts': `
const LEGACY_API_REQUEST_ERROR =
  'Wait a moment, then try again. Forge could not finish this request.'
`,
      'src/app/shared/api/agent-api-types.ts': `
export function extractApiError(
  fallback = 'Refresh, then try again. Forge did not return a clear error.'
) {
  return fallback
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
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

  it('flags AI service connection check errors that start with the failure', () => {
    const cwd = fixture({
      'src/app/features/settings/providerTestErrorMessage.ts': `
function providerTestErrorMessage() {
  return 'OpenAI Production connection check needs attention. Forge could not check this AI service right now. Try again in a few minutes.'
}
function networkErrorMessage() {
  return 'Local Lab connection check needs attention. Forge could not connect to this AI service. Check the service address and your connection, then check again.'
}
function rateLimitMessage() {
  return 'OpenAI Production connection check needs attention. This AI service is receiving too many checks right now. Wait a minute, then check again.'
}
function unknownMessage() {
  return 'AI service connection check needs attention. Review the AI service settings, then check again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'provider-test-error-copy',
          location: 'src/app/features/settings/providerTestErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'provider-test-error-copy',
          location: 'src/app/features/settings/providerTestErrorMessage.ts:6',
        }),
        expect.objectContaining({
          type: 'provider-test-error-copy',
          location: 'src/app/features/settings/providerTestErrorMessage.ts:9',
        }),
        expect.objectContaining({
          type: 'provider-test-error-copy',
          location: 'src/app/features/settings/providerTestErrorMessage.ts:12',
        }),
      ])
    )
  })

  it('accepts AI service connection check errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/features/settings/providerTestErrorMessage.ts': `
function providerTestErrorMessage() {
  return 'Try checking OpenAI Production again in a few minutes. If it still cannot be checked, ask an owner or admin to check AI service settings.'
}
function networkErrorMessage() {
  return 'Check the service address and your connection, then check Local Lab again. Forge could not connect to this AI service.'
}
function rateLimitMessage() {
  return 'Wait a minute, then check OpenAI Production again. This AI service is receiving too many checks right now.'
}
function unknownMessage() {
  return 'Review the AI service settings, then check this AI service again.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags AI service settings errors that start with the failure instead of the next step', () => {
    const cwd = fixture({
      'src/app/features/settings/providerSettingsErrorMessage.ts': `
function saveMessage() {
  return 'AI service could not be saved. Paste the service access key from the selected AI service, then save again.'
}
function removeMessage() {
  return 'AI service could not be removed. Refresh Settings, then try again.'
}
function rateLimitMessage() {
  return 'Refresh Settings to load AI service settings. Forge is receiving too many AI service requests right now. Wait a minute, then try again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'provider-settings-error-copy',
          location: 'src/app/features/settings/providerSettingsErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'provider-settings-error-copy',
          location: 'src/app/features/settings/providerSettingsErrorMessage.ts:6',
        }),
        expect.objectContaining({
          type: 'provider-settings-error-copy',
          location: 'src/app/features/settings/providerSettingsErrorMessage.ts:9',
        }),
      ])
    )
  })

  it('accepts AI service settings errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/features/settings/providerSettingsErrorMessage.ts': `
function providerSettingsErrorMessage(action) {
  if (action === 'save') return 'Paste the service access key from the selected AI service, then save again.'
  if (action === 'remove') return 'Refresh Settings, then remove this AI service again.'
  return 'Wait a minute, then try again. Forge is receiving too many AI service requests right now.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags AI service address copy that exposes endpoint setup mechanics', () => {
    const cwd = fixture({
      'src/app/features/settings/ProvidersSection.tsx': `
function serviceAddressHelp() {
  return 'Leave blank to use the China address. If your team uses the global address, paste this: https://api.example.com/v1'
}
function baseUrlPlaceholder() {
  return 'https://api.example.com'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'provider-address-copy',
          location: 'src/app/features/settings/ProvidersSection.tsx:3',
        }),
        expect.objectContaining({
          type: 'provider-address-copy',
          location: 'src/app/features/settings/ProvidersSection.tsx:6',
        }),
      ])
    )
  })

  it('accepts AI service address copy that starts from the safe default', () => {
    const cwd = fixture({
      'src/app/features/settings/ProvidersSection.tsx': `
function serviceAddressHelp(selectedProvider) {
  return 'Leave blank to use the default regional address. Fill it only when your service guide or owner gives you a global address.'
}
function serviceAddressPlaceholder(needsBaseUrl) {
  return needsBaseUrl ? 'Paste the service address from your guide' : 'Usually leave this blank'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags AI service setup copy that uses terse check and region jargon', () => {
    const cwd = fixture({
      'src/app/features/settings/ProvidersSection.tsx': `
const PROVIDER_SETUP_STEPS = [
  { label: 'Paste service access key', value: 'Open that account, copy its access key, and paste it here.' },
  { label: 'Save and check', value: 'Click Check after saving. Ready means agents can use this service.' },
]
function CatalogPanel() {
  return 'Service address and model are filled in for you. After saving, click Check.'
}
function RegionToggle() {
  return 'Service address region'
}
function CatalogGrid() {
  return 'Standard setup · Coding plan · China/Global address'
}
`,
      'src/app/features/agents/CreateAgentModal.tsx': `
function CreateAgentModal() {
  return 'Open Settings > AI services, add a service, save it, then click Check until it says Ready.'
}
`,
      'src/app/features/agents/AgentControlPanel.tsx': `
function AgentControlPanel() {
  return 'Open Settings > AI services, click Check on this service, refresh Agents, then send messages after it shows Ready.'
}
`,
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
function AgentDetailView() {
  return 'Open AI service settings, click Check for this connection, then refresh Agents before sending chat work.'
}
`,
      'src/app/features/chat/ChatView.tsx': `
function ChatView() {
  return 'Open AI service settings, check this connection, then refresh Agents before sending a message.'
}
`,
      'src/app/features/settings/providerTestErrorMessage.ts': `
function providerTestErrorMessage() {
  return 'Check the service access key, model, and service address, then save and check again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'provider-setup-copy',
          location: 'src/app/features/settings/ProvidersSection.tsx:3',
        }),
        expect.objectContaining({
          type: 'provider-setup-copy',
          location: 'src/app/features/settings/ProvidersSection.tsx:4',
        }),
        expect.objectContaining({
          type: 'provider-setup-copy',
          location: 'src/app/features/settings/ProvidersSection.tsx:7',
        }),
        expect.objectContaining({
          type: 'provider-setup-copy',
          location: 'src/app/features/settings/ProvidersSection.tsx:10',
        }),
        expect.objectContaining({
          type: 'provider-setup-copy',
          location: 'src/app/features/settings/ProvidersSection.tsx:13',
        }),
        expect.objectContaining({
          type: 'provider-setup-copy',
          location: 'src/app/features/agents/CreateAgentModal.tsx:3',
        }),
        expect.objectContaining({
          type: 'provider-setup-copy',
          location: 'src/app/features/agents/AgentControlPanel.tsx:3',
        }),
        expect.objectContaining({
          type: 'provider-setup-copy',
          location: 'src/app/widgets/agent-detail/AgentDetailView.tsx:3',
        }),
        expect.objectContaining({
          type: 'provider-setup-copy',
          location: 'src/app/features/chat/ChatView.tsx:3',
        }),
        expect.objectContaining({
          type: 'provider-setup-copy',
          location: 'src/app/features/settings/providerTestErrorMessage.ts:3',
        }),
      ])
    )
  })

  it('accepts AI service setup copy that names the next beginner action', () => {
    const cwd = fixture({
      'src/app/features/settings/ProvidersSection.tsx': `
const PROVIDER_SETUP_STEPS = [
  { label: 'Paste the service access key', value: 'Open that account, copy the service access key, and paste it here.' },
  { label: 'Save, then check connection', value: 'After saving, choose Check connection. Ready means agents can use this service.' },
]
function CatalogPanel() {
  return 'Forge fills in the service website address and model for you. After saving, choose Check connection.'
}
function RegionToggle() {
  return 'Service website region'
}
function CatalogGrid() {
  return 'Standard setup · Coding plan · China or global website address'
}
`,
      'src/app/features/agents/CreateAgentModal.tsx': `
function CreateAgentModal() {
  return 'Open Settings > AI services, add a service, save it, then choose Check connection until it says Ready.'
}
`,
      'src/app/features/agents/AgentControlPanel.tsx': `
function AgentControlPanel() {
  return 'Open Settings > AI services, choose Check connection for this service, refresh Agents, then send messages after it shows Ready.'
}
`,
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
function AgentDetailView() {
  return 'Open AI service settings, choose Check connection for this service, then refresh Agents before sending chat work.'
}
`,
      'src/app/features/chat/ChatView.tsx': `
function ChatView() {
  return 'Open AI service settings, choose Check connection, then refresh Agents before sending a message.'
}
`,
      'src/app/features/settings/providerTestErrorMessage.ts': `
function providerTestErrorMessage() {
  return 'Check the service access key, model, and service address, then save and choose Check connection again.'
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

  it('flags admin role copy that exposes system configuration jargon', () => {
    const cwd = fixture({
      'src/app/features/admin/UserManagement.tsx': `
const ROLE_DETAILS = {
  admin: { description: 'Can manage users, settings, and system configuration.' },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'admin-user-role-copy',
          location: 'src/app/features/admin/UserManagement.tsx:3',
        }),
      ])
    )
  })

  it('accepts admin role copy that names people and safety controls', () => {
    const cwd = fixture({
      'src/app/features/admin/UserManagement.tsx': `
const ROLE_DETAILS = {
  admin: { description: 'Can manage people, team settings, and safety controls.' },
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

function PeopleGuide() {
  return 'A sudden jump can mean onboarding succeeded or access needs review.'
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
        expect.objectContaining({
          type: 'admin-orgs-empty-copy',
          location: 'src/app/features/admin/OrganizationsPanel.tsx:7',
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

  it('flags admin agent empty titles that do not tell users what to do next', () => {
    const cwd = fixture({
      'src/app/features/admin/AgentsPanel.tsx': `
function AgentsEmptyState() {
  return <p>No agents to show</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'admin-agent-empty-copy',
          location: 'src/app/features/admin/AgentsPanel.tsx:3',
        }),
      ])
    )
  })

  it('accepts admin agent empty titles that name the next setup action', () => {
    const cwd = fixture({
      'src/app/features/admin/AgentsPanel.tsx': `
function AgentsEmptyState() {
  return <p>Create or connect an agent first</p>
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

  it('flags admin store error copy that explains failure before the next step', () => {
    const cwd = fixture({
      'src/app/shared/model/admin.store.ts': `
export function adminHttpErrorMessage(label) {
  return \`You do not have access to the admin \${label}. Ask an owner or admin to give you Admin access, then reload Admin.\`
}

export function adminLoadErrorMessage(label) {
  return \`Forge could not load the admin \${label} right now. Reload the \${label}, then try again.\`
}

function adminNetworkErrorMessage(resource) {
  return \`Forge could not connect while loading the admin \${adminResourceLabel(resource)}. Check your connection, then refresh Admin.\`
}

function adminUserActionNetworkMessage(action) {
  return \`The \${adminUserActionLabel(action)} could not reach the server. Check your connection and try again.\`
}

function adminUserActionErrorMessage(action) {
  return action === 'change-role'
    ? 'You do not have access to change user access. Ask an owner or admin to give you Admin access, then save again.'
    : 'You do not have access to remove user accounts. Ask an owner or admin to give you Admin access, then try again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'admin-store-error-copy',
          location: 'src/app/shared/model/admin.store.ts:3',
        }),
        expect.objectContaining({
          type: 'admin-store-error-copy',
          location: 'src/app/shared/model/admin.store.ts:7',
        }),
        expect.objectContaining({
          type: 'admin-store-error-copy',
          location: 'src/app/shared/model/admin.store.ts:11',
        }),
        expect.objectContaining({
          type: 'admin-store-error-copy',
          location: 'src/app/shared/model/admin.store.ts:15',
        }),
        expect.objectContaining({
          type: 'admin-store-error-copy',
          location: 'src/app/shared/model/admin.store.ts:20',
        }),
        expect.objectContaining({
          type: 'admin-store-error-copy',
          location: 'src/app/shared/model/admin.store.ts:21',
        }),
      ])
    )
  })

  it('accepts admin store error copy that starts with the recovery step', () => {
    const cwd = fixture({
      'src/app/shared/model/admin.store.ts': `
export function adminHttpErrorMessage(label) {
  return \`Ask an owner or admin to give you Admin access, then reload Admin. You do not have access to the admin \${label}.\`
}

export function adminLoadErrorMessage(label) {
  return \`Reload the \${label}, then try again. Forge could not load the admin \${label} right now.\`
}

function adminNetworkErrorMessage(resource) {
  return \`Check your connection, then refresh Admin. Forge could not connect while loading the admin \${adminResourceLabel(resource)}.\`
}

function adminUserActionNetworkMessage(action) {
  return \`Check your connection, then try again. The \${adminUserActionLabel(action)} could not reach the server.\`
}

function adminUserActionErrorMessage(action) {
  return action === 'change-role'
    ? 'Ask an owner or admin to give you Admin access, then save again. You do not have access to change user access.'
    : 'Ask an owner or admin to give you Admin access, then try again. You do not have access to remove user accounts.'
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

  it('flags code access errors that start with the failure instead of the next step', () => {
    const cwd = fixture({
      'src/app/features/settings/gitCredentialsErrorMessage.ts': `
function saveMessage() {
  return 'Code access could not be saved. Paste a new code access key from GitHub or GitLab, then save again.'
}
function removeMessage() {
  return 'Code access could not be removed. Refresh Settings, then try again.'
}
function rateLimitMessage() {
  return 'Refresh Settings to load code access. Forge is receiving too many code access requests right now. Wait a minute, then try again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'code-access-error-copy',
          location: 'src/app/features/settings/gitCredentialsErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'code-access-error-copy',
          location: 'src/app/features/settings/gitCredentialsErrorMessage.ts:6',
        }),
        expect.objectContaining({
          type: 'code-access-error-copy',
          location: 'src/app/features/settings/gitCredentialsErrorMessage.ts:9',
        }),
      ])
    )
  })

  it('accepts code access errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/features/settings/gitCredentialsErrorMessage.ts': `
function gitCredentialsErrorMessage(action) {
  if (action === 'save') return 'Paste a new code access key from GitHub or GitLab, then save again.'
  if (action === 'remove') return 'Refresh Settings, then remove code access again.'
  return 'Wait a minute, then try again. Forge is receiving too many code access requests right now.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags SSH code access errors that start with the failure instead of the next step', () => {
    const cwd = fixture({
      'src/app/features/settings/sshKeysErrorMessage.ts': `
function saveMessage() {
  return 'SSH code access could not be saved. Add a name for this access, then save again.'
}
function removeMessage() {
  return 'SSH code access could not be removed. Refresh Settings, then try again.'
}
function rateLimitMessage() {
  return 'Refresh Settings to load SSH code access. Forge is receiving too many SSH code access requests right now. Wait a minute, then try again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'ssh-code-access-error-copy',
          location: 'src/app/features/settings/sshKeysErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'ssh-code-access-error-copy',
          location: 'src/app/features/settings/sshKeysErrorMessage.ts:6',
        }),
        expect.objectContaining({
          type: 'ssh-code-access-error-copy',
          location: 'src/app/features/settings/sshKeysErrorMessage.ts:9',
        }),
      ])
    )
  })

  it('accepts SSH code access errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/features/settings/sshKeysErrorMessage.ts': `
function sshKeysErrorMessage(action) {
  if (action === 'save') return 'Add a name for this access, then save again.'
  if (action === 'remove') return 'Refresh Settings, then remove this SSH code access again.'
  return 'Wait a minute, then try again. Forge is receiving too many SSH code access requests right now.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags outside tool access key errors that start with the failure instead of the next step', () => {
    const cwd = fixture({
      'src/app/features/settings/platformKeyErrorMessage.ts': `
function createMessage() {
  return 'Outside tool access key could not be created. Enter the tool or job name, then try again.'
}
function removeMessage() {
  return 'Outside tool access key could not be removed. Refresh Settings, then try again.'
}
function rateLimitMessage() {
  return 'Refresh Settings to load outside tool access keys. Forge is receiving too many outside tool access requests right now. Wait a minute, then try again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'platform-key-error-copy',
          location: 'src/app/features/settings/platformKeyErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'platform-key-error-copy',
          location: 'src/app/features/settings/platformKeyErrorMessage.ts:6',
        }),
        expect.objectContaining({
          type: 'platform-key-error-copy',
          location: 'src/app/features/settings/platformKeyErrorMessage.ts:9',
        }),
      ])
    )
  })

  it('accepts outside tool access key errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/features/settings/platformKeyErrorMessage.ts': `
function platformKeyErrorMessage(action) {
  if (action === 'create') return 'Enter the tool or job name, then try again.'
  if (action === 'remove') return 'Refresh Settings, then remove this outside tool access key again.'
  return 'Wait a minute, then try again. Forge is receiving too many outside tool access requests right now.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags account settings errors that start with the failure instead of the next step', () => {
    const cwd = fixture({
      'src/app/features/settings/accountErrorMessages.ts': `
function expiredMessage() {
  return 'Your sign-in expired. Sign in again, then change your password again.'
}
function permissionMessage() {
  return 'You do not have permission to rename this team space. Ask an owner or admin to update your role.'
}
function validationMessage() {
  return 'The current password did not match this account. Re-enter the current password, then try again.'
}
function rateLimitMessage() {
  return 'Forge is receiving too many account settings requests right now. Wait a moment, then change your password again.'
}
function serverMessage() {
  return 'Team space name could not be saved. Refresh Settings, then try again.'
}
function unknownMessage() {
  return 'Account settings could not rename the team space. Refresh Settings, then try again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'account-settings-error-copy',
          location: 'src/app/features/settings/accountErrorMessages.ts:3',
        }),
        expect.objectContaining({
          type: 'account-settings-error-copy',
          location: 'src/app/features/settings/accountErrorMessages.ts:6',
        }),
        expect.objectContaining({
          type: 'account-settings-error-copy',
          location: 'src/app/features/settings/accountErrorMessages.ts:9',
        }),
        expect.objectContaining({
          type: 'account-settings-error-copy',
          location: 'src/app/features/settings/accountErrorMessages.ts:12',
        }),
        expect.objectContaining({
          type: 'account-settings-error-copy',
          location: 'src/app/features/settings/accountErrorMessages.ts:15',
        }),
        expect.objectContaining({
          type: 'account-settings-error-copy',
          location: 'src/app/features/settings/accountErrorMessages.ts:18',
        }),
      ])
    )
  })

  it('accepts account settings errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/features/settings/accountErrorMessages.ts': `
function accountErrorMessage(action) {
  if (action === 'expired') return 'Sign in again, then change your password again. Your sign-in expired.'
  if (action === 'permission') return 'Ask an owner or admin to update your role. You do not have permission to rename this team space.'
  if (action === 'validation') return 'Re-enter the current password, then try again. The current password did not match this account.'
  if (action === 'rateLimit') return 'Wait a moment, then change your password again. Forge is receiving too many account settings requests right now.'
  return 'Refresh Settings, then rename the team space again. If it still fails, ask an owner or admin to check account settings.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags common English error translations that start with the failure', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  errors: {
    generic: 'Something went wrong. Try again, then ask an owner to check the system if it repeats.',
    notFound: '{{resource}} was not found. Refresh the page, then try again.',
    serverError: 'Forge could not finish this right now. Wait a moment, then try again.',
    uploadError: 'The upload did not finish. Check the file and connection, then try again.',
    uploadFailed: 'Upload did not finish. Check the file, then try again.',
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'common-error-copy',
          location: 'src/app/shared/i18n/locales/en.ts:4',
        }),
        expect.objectContaining({
          type: 'common-error-copy',
          location: 'src/app/shared/i18n/locales/en.ts:5',
        }),
        expect.objectContaining({
          type: 'common-error-copy',
          location: 'src/app/shared/i18n/locales/en.ts:6',
        }),
        expect.objectContaining({
          type: 'common-error-copy',
          location: 'src/app/shared/i18n/locales/en.ts:7',
        }),
        expect.objectContaining({
          type: 'common-error-copy',
          location: 'src/app/shared/i18n/locales/en.ts:8',
        }),
      ])
    )
  })

  it('flags common Chinese error translations that start with the failure', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  errors: {
    generic: '出现了问题。请重试；如果反复发生，请让管理员检查系统。',
    notFound: '未找到 {{resource}}。请刷新页面后重试。',
    serverError: 'Forge 暂时无法完成这个操作。请稍等片刻后重试。',
    uploadError: '上传没有完成。请检查文件和网络后重试。',
    uploadFailed: '上传没有完成。请检查文件，然后重试。',
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'common-error-copy',
          location: 'src/app/shared/i18n/locales/zh.ts:4',
        }),
        expect.objectContaining({
          type: 'common-error-copy',
          location: 'src/app/shared/i18n/locales/zh.ts:5',
        }),
        expect.objectContaining({
          type: 'common-error-copy',
          location: 'src/app/shared/i18n/locales/zh.ts:6',
        }),
        expect.objectContaining({
          type: 'common-error-copy',
          location: 'src/app/shared/i18n/locales/zh.ts:7',
        }),
        expect.objectContaining({
          type: 'common-error-copy',
          location: 'src/app/shared/i18n/locales/zh.ts:8',
        }),
      ])
    )
  })

  it('accepts common error translations that start with recovery actions', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  errors: {
    generic: 'Try again. If it repeats, ask an owner to check the system.',
    notFound: 'Refresh the page, then try again. {{resource}} was not found.',
    serverError: 'Wait a moment, then try again. Forge could not finish this right now.',
    uploadError: 'Check the file and connection, then upload again. The upload did not finish.',
    uploadFailed: 'Check the file, then upload again. The upload did not finish.',
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  errors: {
    generic: '请重试；如果反复发生，请让管理员检查系统。',
    notFound: '请刷新页面后重试。未找到 {{resource}}。',
    serverError: '请稍等片刻后重试。Forge 暂时无法完成这个操作。',
    uploadError: '请检查文件和网络后重新上传。上传没有完成。',
    uploadFailed: '请检查文件后重新上传。上传没有完成。',
  },
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags localized user access copy that exposes role jargon', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  admin: {
    users: {
      role: 'Role',
      roles: { operator: 'Operator' },
    },
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  errors: {
    forbidden: '你当前没有权限执行这个操作。请让所有者或管理员更新你的角色。',
  },
  admin: {
    users: {
      role: '角色',
      roles: { operator: '操作员' },
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
          type: 'locale-access-role-copy',
          location: 'src/app/shared/i18n/locales/en.ts:5',
        }),
        expect.objectContaining({
          type: 'locale-access-role-copy',
          location: 'src/app/shared/i18n/locales/zh.ts:4',
        }),
        expect.objectContaining({
          type: 'locale-access-role-copy',
          location: 'src/app/shared/i18n/locales/en.ts:6',
        }),
        expect.objectContaining({
          type: 'locale-access-role-copy',
          location: 'src/app/shared/i18n/locales/zh.ts:8',
        }),
        expect.objectContaining({
          type: 'locale-access-role-copy',
          location: 'src/app/shared/i18n/locales/zh.ts:9',
        }),
      ])
    )
  })

  it('accepts localized user access copy that names access level and recovery', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  admin: {
    users: { role: 'Access level', roles: { operator: 'Member' } },
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  errors: {
    forbidden: '你当前无法执行这个操作。请让所有者或管理员检查你的团队空间访问权限。',
  },
  admin: {
    users: { role: '访问级别', roles: { operator: '成员' } },
  },
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags vague localized error and status labels', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  common: {
    error: 'Needs attention',
  },
  agents: {
    status: {
      error: 'Needs attention',
    },
  },
  feed: {
    eventTypes: {
      error: 'Needs attention',
    },
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  common: {
    error: '有内容需要处理。请查看提示信息，然后重试。',
  },
  agents: {
    status: {
      error: '需要处理',
    },
  },
  feed: {
    eventTypes: {
      error: '需要处理',
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
          type: 'locale-vague-error-label-copy',
          sample: expect.stringContaining('Needs attention'),
        }),
        expect.objectContaining({
          type: 'locale-vague-error-label-copy',
          sample: expect.stringContaining('有内容需要处理'),
        }),
        expect.objectContaining({
          type: 'locale-vague-error-label-copy',
          sample: expect.stringContaining('需要处理'),
        }),
      ])
    )
  })

  it('flags workspace settings errors that start with the failure instead of the next step', () => {
    const cwd = fixture({
      'src/app/pages/settings/model/workspaceSettingsErrorMessage.ts': `
function createMessage() {
  return 'The project was not created. Enter a project name, then try again.'
}
function authMessage() {
  return 'Refresh Settings to load workspace teams. Sign in again, then return to Settings.'
}
function permissionMessage() {
  return 'Refresh Settings to load workspace projects. Ask an owner or admin to update your workspace access.'
}
function permissionNoNextStepMessage() {
  return 'Ask an owner or admin to update your team space access.'
}
function networkMessage() {
  return 'Refresh Settings to load workspace projects. Check your connection, then refresh Settings again.'
}
function busyMessage() {
  return 'Refresh Settings to load workspace teams. Too many setup changes are happening right now. Wait a minute, then refresh Settings again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'workspace-settings-error-copy',
          location: 'src/app/pages/settings/model/workspaceSettingsErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'workspace-settings-error-copy',
          location: 'src/app/pages/settings/model/workspaceSettingsErrorMessage.ts:6',
        }),
        expect.objectContaining({
          type: 'workspace-settings-error-copy',
          location: 'src/app/pages/settings/model/workspaceSettingsErrorMessage.ts:9',
        }),
        expect.objectContaining({
          type: 'workspace-settings-error-copy',
          location: 'src/app/pages/settings/model/workspaceSettingsErrorMessage.ts:12',
        }),
        expect.objectContaining({
          type: 'workspace-settings-error-copy',
          location: 'src/app/pages/settings/model/workspaceSettingsErrorMessage.ts:15',
        }),
        expect.objectContaining({
          type: 'workspace-settings-error-copy',
          location: 'src/app/pages/settings/model/workspaceSettingsErrorMessage.ts:18',
        }),
      ])
    )
  })

  it('accepts workspace settings errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/pages/settings/model/workspaceSettingsErrorMessage.ts': `
function workspaceSettingsErrorMessage(action) {
  if (action === 'auth') return 'Sign in again, then refresh Settings to load workspace teams.'
  if (action === 'permission') return 'Ask an owner or admin to update your team space access, then refresh Settings to load projects.'
  if (action === 'network') return 'Check your connection, then refresh Settings to load workspace projects.'
  if (action === 'busy') return 'Wait a minute, then refresh Settings to load workspace teams. Too many setup changes are happening right now.'
  if (action === 'server') return 'Refresh Settings to load workspace projects. If it still fails, ask an owner or admin to check workspace setup.'
  return 'Enter a project name, then try again.'
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

  it('flags agent tool errors that start with the failure instead of the next step', () => {
    const cwd = fixture({
      'src/app/features/agents/model/pluginErrorMessage.ts': `
function prefix() {
  return 'Tool change was not saved. The switch was returned to its previous setting.'
}
function serverMessage() {
  return 'Forge could not finish this tool request right now. Wait a few minutes, then try again.'
}
function listMessage() {
  return "Forge could not read this agent's tool list. Refresh the page."
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'agent-plugin-error-copy',
          location: 'src/app/features/agents/model/pluginErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'agent-plugin-error-copy',
          location: 'src/app/features/agents/model/pluginErrorMessage.ts:6',
        }),
        expect.objectContaining({
          type: 'agent-plugin-error-copy',
          location: 'src/app/features/agents/model/pluginErrorMessage.ts:9',
        }),
      ])
    )
  })

  it('accepts agent tool errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/features/agents/model/pluginErrorMessage.ts': `
function prefix() {
  return 'Refresh this agent page, then try the tool change again. The switch was returned to its previous setting.'
}
function serverMessage() {
  return 'Wait a few minutes, then try the tool change again. The switch was returned to its previous setting. Forge could not finish this tool request right now.'
}
function listMessage() {
  return "Refresh this agent page, then try the tool change again. The switch was returned to its previous setting. Forge could not read this agent's tool list."
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags member access errors that start with the failure instead of the next step', () => {
    const cwd = fixture({
      'src/app/features/manage-members/model/resourceMemberErrorMessages.ts': `
function resourceMemberErrorMessage() {
  return 'You do not have permission to manage people for this team. Ask an owner or admin to give you access.'
}

function memberLoadError() {
  return 'People access is busy. Wait a moment, then try again.'
}

function memberUnavailableMessage() {
  return 'Forge could not update people access right now. Refresh members, then try again.'
}

function validationMessage() {
  return 'This person could not be removed. Check whether they are the last owner.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'resource-member-error-copy',
          location: 'src/app/features/manage-members/model/resourceMemberErrorMessages.ts:3',
        }),
        expect.objectContaining({
          type: 'resource-member-error-copy',
          location: 'src/app/features/manage-members/model/resourceMemberErrorMessages.ts:7',
        }),
        expect.objectContaining({
          type: 'resource-member-error-copy',
          location: 'src/app/features/manage-members/model/resourceMemberErrorMessages.ts:11',
        }),
        expect.objectContaining({
          type: 'resource-member-error-copy',
          location: 'src/app/features/manage-members/model/resourceMemberErrorMessages.ts:15',
        }),
      ])
    )
  })

  it('accepts member access errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/features/manage-members/model/resourceMemberErrorMessages.ts': `
function resourceMemberErrorMessage() {
  return 'Ask an owner or admin to give you access, then reopen members for this team.'
}

function memberLoadError() {
  return 'Wait a moment, then try again. People access is busy right now.'
}

function memberUnavailableMessage() {
  return 'Refresh members, then remove the person again. Forge could not update people access right now.'
}

function validationMessage() {
  return 'Check whether this person is the last owner, then try removing them again.'
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

  it('flags chat-only lifecycle copy that sends beginners to a missing workspace', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  errors: {
    agent: {
      lifecycle: {
        start_api: {
          title: 'No workspace to start',
        },
      },
    },
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'agent-api-lifecycle-copy',
        location: 'src/app/shared/i18n/locales/en.ts:7',
      }),
    ])
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
  return runtime ? 'Check file work place' : 'Refresh file work place'
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
  'Forge cannot copy from this browser. Select the setup text in the box, then copy it manually.'
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
    label: 'Capacity',
    value: usageCount > 0 ? \`\${usageCount} capacity checks shown\` : 'Capacity details appear after agents run billable work',
  }
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags billing event usage copy that exposes audit-record jargon', () => {
    const cwd = fixture({
      'src/app/features/billing/UsageMeter.tsx': `
function metricCopy(metric) {
  return { description: 'Run updates, audit records, and timeline messages.' }
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'billing-usage-audit-copy',
        location: 'src/app/features/billing/UsageMeter.tsx:3',
      }),
    ])
  })

  it('accepts billing event usage copy that uses change-history wording', () => {
    const cwd = fixture({
      'src/app/features/billing/UsageMeter.tsx': `
function metricCopy(metric) {
  return { description: 'Work updates, change history, and timeline messages.' }
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags billing event usage labels that expose activity-event wording', () => {
    const cwd = fixture({
      'src/app/features/billing/UsageMeter.tsx': `
function metricCopy(metric) {
  return { label: 'Activity events' }
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'billing-usage-event-copy',
        location: 'src/app/features/billing/UsageMeter.tsx:3',
      }),
    ])
  })

  it('accepts billing event usage labels that describe work update history', () => {
    const cwd = fixture({
      'src/app/features/billing/UsageMeter.tsx': `
function metricCopy(metric) {
  return { label: 'Work update history' }
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags billing setup copy that uses setup-path or workspace wording', () => {
    const cwd = fixture({
      'src/app/features/billing/BillingPage.tsx': `
function BillingNotConfigured() {
  return (
    <div>
      <p>Billing setup path</p>
      <p>Billing setup steps</p>
      <p>Ask an owner or admin to turn on billing for this workspace.</p>
      <p>Do not paste secret payment settings here.</p>
    </div>
  )
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'billing-setup-copy',
          location: 'src/app/features/billing/BillingPage.tsx:5',
        }),
        expect.objectContaining({
          type: 'billing-setup-copy',
          location: 'src/app/features/billing/BillingPage.tsx:6',
        }),
        expect.objectContaining({
          type: 'billing-setup-copy',
          location: 'src/app/features/billing/BillingPage.tsx:7',
        }),
        expect.objectContaining({
          type: 'billing-setup-copy',
          location: 'src/app/features/billing/BillingPage.tsx:8',
        }),
      ])
    )
  })

  it('accepts billing setup copy that gives a plain next step', () => {
    const cwd = fixture({
      'src/app/features/billing/BillingPage.tsx': `
function BillingNotConfigured() {
  return (
    <div>
      <p>What to do next</p>
      <p>Ask an owner or admin to turn on billing for this team.</p>
      <p>Do not enter payment account passwords or keys on this page.</p>
    </div>
  )
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

  it('flags billing errors that explain the failure before the next step', () => {
    const cwd = fixture({
      'src/app/shared/model/billing.store.ts': `
function billingErrorMessage() {
  return 'Refresh Billing to load usage. Forge could not connect while loading billing. Check your connection, then refresh Billing again.'
}
`,
      'src/app/features/billing/BillingPage.tsx': `
function billingActionErrors() {
  setActionError('The secure payment page did not open. Try again or ask an owner or admin to check billing.')
  setActionError('The billing management page did not open. Try again or ask an owner or admin to check access.')
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'billing-error-copy',
          location: 'src/app/shared/model/billing.store.ts:3',
        }),
        expect.objectContaining({
          type: 'billing-error-copy',
          location: 'src/app/features/billing/BillingPage.tsx:3',
        }),
        expect.objectContaining({
          type: 'billing-error-copy',
          location: 'src/app/features/billing/BillingPage.tsx:4',
        }),
      ])
    )
  })

  it('accepts billing errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/shared/model/billing.store.ts': `
function billingErrorMessage() {
  return 'Refresh Billing to load usage. Check your connection, then refresh Billing again. Forge could not connect while loading billing.'
}
`,
      'src/app/features/billing/BillingPage.tsx': `
function billingActionErrors() {
  setActionError('Try opening the secure payment page again. If it still does not open, ask an owner or admin to check billing.')
  setActionError('Try opening the billing management page again. If it still does not open, ask an owner or admin to check access.')
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
      <p>Tool use appears after an agent finishes a task</p>
    </div>
  )
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags analytics errors that start with the failure instead of the next step', () => {
    const cwd = fixture({
      'src/app/shared/model/analytics.store.ts': `
export function analyticsUnavailableMessage() {
  return 'Analytics could not load live activity. Refresh the dashboard.'
}

export function analyticsNetworkErrorMessage() {
  return 'Analytics could not reach the service. Check your connection, then refresh the dashboard.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'analytics-error-copy',
        location: 'src/app/shared/model/analytics.store.ts:3',
      }),
      expect.objectContaining({
        type: 'analytics-error-copy',
        location: 'src/app/shared/model/analytics.store.ts:7',
      }),
    ])
  })

  it('accepts analytics errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/shared/model/analytics.store.ts': `
export function analyticsUnavailableMessage() {
  return 'Refresh the dashboard. If this is a new workspace, run an agent task first.'
}

export function analyticsNetworkErrorMessage() {
  return 'Check your connection, then refresh the dashboard. Analytics could not connect.'
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

  it('flags saved item review empty copy that says nothing instead of what is clear', () => {
    const cwd = fixture({
      'src/app/features/analytics/ContextUsageDashboard.tsx': `
const EMPTY_NEEDS_REVIEW = {
  title: 'Nothing to check right now',
}

const EMPTY_STALE = {
  title: 'Nothing looks outdated',
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'analytics-review-empty-copy',
          location: 'src/app/features/analytics/ContextUsageDashboard.tsx:3',
        }),
        expect.objectContaining({
          type: 'analytics-review-empty-copy',
          location: 'src/app/features/analytics/ContextUsageDashboard.tsx:7',
        }),
      ])
    )
  })

  it('accepts saved item review empty copy that names the clear state', () => {
    const cwd = fixture({
      'src/app/features/analytics/ContextUsageDashboard.tsx': `
const EMPTY_NEEDS_REVIEW = {
  title: 'No saved items need checking',
}

const EMPTY_STALE = {
  title: 'No saved items look outdated',
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

  it('flags saved item selection empty copy that says nothing instead of naming the list', () => {
    const cwd = fixture({
      'src/app/entities/context/ui/InjectionPreviewModal.tsx': `
export function InjectionPreviewModal() {
  return (
    <div>
      <PreviewSection empty="Nothing will be shared yet." />
      <PreviewSection empty="Nothing is kept yet. Choose the pin button on a saved item to keep it easy to reuse." />
      <PreviewSection empty="No saved items are selected yet." />
    </div>
  )
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'saved-item-selection-empty-copy',
          location: 'src/app/entities/context/ui/InjectionPreviewModal.tsx:5',
        }),
        expect.objectContaining({
          type: 'saved-item-selection-empty-copy',
          location: 'src/app/entities/context/ui/InjectionPreviewModal.tsx:6',
        }),
        expect.objectContaining({
          type: 'saved-item-selection-empty-copy',
          location: 'src/app/entities/context/ui/InjectionPreviewModal.tsx:7',
        }),
      ])
    )
  })

  it('accepts saved item selection empty copy that names selected and pinned items', () => {
    const cwd = fixture({
      'src/app/entities/context/ui/InjectionPreviewModal.tsx': `
export function InjectionPreviewModal() {
  return (
    <div>
      <PreviewSection empty="No saved items will be included yet. Add one below, or send without notes if none fit." />
      <PreviewSection empty="No saved items are pinned yet. Choose the pin button on a saved item to keep it easy to reuse." />
    </div>
  )
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

  it('flags task list empty copy that does not point to the board action', () => {
    const cwd = fixture({
      'src/app/features/list/ListView.tsx': `
function EmptyList() {
  return 'Create one small task from the board first'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-list-empty-copy',
          location: 'src/app/features/list/ListView.tsx:3',
        }),
      ])
    )
  })

  it('flags task list empty copy that asks beginners for expected proof', () => {
    const cwd = fixture({
      'src/app/features/list/ListView.tsx': `
function EmptyList() {
  return [
    'Use the board to give an agent one clear outcome and expected proof.',
    'Start with the outcome you want, then add the proof you expect the agent to return.',
  ]
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-list-empty-copy',
          location: 'src/app/features/list/ListView.tsx:4',
        }),
        expect.objectContaining({
          type: 'task-list-empty-copy',
          location: 'src/app/features/list/ListView.tsx:5',
        }),
      ])
    )
  })

  it('accepts task list empty copy that points to opening the board', () => {
    const cwd = fixture({
      'src/app/features/list/ListView.tsx': `
function EmptyList() {
  return 'Use the board to create one small task first. Tell the agent what to send back. Open board to create task.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
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
      'src/app/features/detail/HistoryTab.tsx': `
function supportRunReference(id) {
  return 'not listed'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-support-reference-copy',
          location: 'src/app/features/detail/TaskDetailPanel.tsx:3',
        }),
        expect.objectContaining({
          type: 'task-support-reference-copy',
          location: 'src/app/features/detail/HistoryTab.tsx:3',
        }),
      ])
    )
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

  it('flags task detail no-agent copy that does not point users to agent setup', () => {
    const cwd = fixture({
      'src/app/features/detail/TaskDetailPanel.tsx': `
function emptyAgents() {
  return 'No available agent can take this task right now.'
}
`,
      'src/app/features/detail/taskDetailErrorMessages.ts': `
function noAgentError() {
  return 'No agent is available for this task. Start an agent or wait for one to finish, then try again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-detail-agent-setup-copy',
          location: 'src/app/features/detail/TaskDetailPanel.tsx:3',
        }),
        expect.objectContaining({
          type: 'task-detail-agent-setup-copy',
          location: 'src/app/features/detail/taskDetailErrorMessages.ts:3',
        }),
      ])
    )
  })

  it('accepts task detail no-agent copy that points users to agent setup', () => {
    const cwd = fixture({
      'src/app/features/detail/TaskDetailPanel.tsx': `
function emptyAgents() {
  return 'Open Agents to start or connect an agent, then return here and refresh this task.'
}
`,
      'src/app/features/detail/taskDetailErrorMessages.ts': `
function noAgentError() {
  return 'No agent can take this task right now. Open Agents to start or connect an agent, then refresh this task and try again.'
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

function emptyInstructionBadge() {
  return 'No instructions'
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
      expect.objectContaining({
        type: 'agent-config-detail-copy',
        location: 'src/app/features/agents/AgentConfigTab.tsx:11',
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

function emptyInstructionBadge() {
  return 'Add instructions'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags agent instruction save errors that hide the next step behind the failure', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentConfigTab.tsx': `
function promptProfileSaveErrorMessage() {
  return 'Agent instructions were not saved. Refresh this agent, confirm it is still a chat-only agent, then save again. Ask an admin to check your agent access if it keeps failing.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'agent-config-save-copy',
          location: 'src/app/features/agents/AgentConfigTab.tsx:3',
        }),
      ])
    )
  })

  it('accepts agent instruction save errors that start with the next action', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentConfigTab.tsx': `
function promptProfileSaveErrorMessage() {
  return 'Refresh this agent, confirm it is still a chat-only agent, then save again. If it keeps failing, ask an owner or admin to check your agent access. Agent instructions were not saved.'
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

  it('flags create-agent work-style labels that do not say where the agent works', () => {
    const cwd = fixture({
      'src/app/features/agents/CreateAgentModal.tsx': `
function createReviewItems() {
  return [{ label: 'Work style', value: 'Claude in a managed workspace' }]
}
function WorkStylePicker() {
  return <label>Choose work style</label>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'create-agent-work-location-copy',
          location: 'src/app/features/agents/CreateAgentModal.tsx:3',
        }),
        expect.objectContaining({
          type: 'create-agent-work-location-copy',
          location: 'src/app/features/agents/CreateAgentModal.tsx:6',
        }),
      ])
    )
  })

  it('accepts create-agent labels that ask where the agent works', () => {
    const cwd = fixture({
      'src/app/features/agents/CreateAgentModal.tsx': `
function createReviewItems() {
  return [{ label: 'Where it works', value: 'Claude with project files' }]
}
function WorkLocationPicker() {
  return <label>Where should this agent work?</label>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags create-agent work-area copy that exposes workspace internals', () => {
    const cwd = fixture({
      'src/app/features/agents/CreateAgentModal.tsx': `
function runtimeFitFor() {
  return [{ label: 'Agent location', value: 'Managed workspace' }]
}
function HelpText() {
  return <><p>Uses a ready workspace managed by Forge for file work.</p><p>Forge prepares this project workspace for the agent.</p></>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'create-agent-work-area-copy',
          sample: expect.stringContaining('Agent location'),
        }),
        expect.objectContaining({
          type: 'create-agent-work-area-copy',
          sample: expect.stringContaining('ready workspace managed by Forge'),
        }),
        expect.objectContaining({
          type: 'create-agent-work-area-copy',
          sample: expect.stringContaining('project workspace'),
        }),
      ])
    )
  })

  it('accepts create-agent work-area copy that explains project files plainly', () => {
    const cwd = fixture({
      'src/app/features/agents/CreateAgentModal.tsx': `
function runtimeFitFor() {
  return [{ label: 'Where it works', value: 'Forge project area' }]
}
function HelpText() {
  return <><p>Forge prepares a safe project area for file work.</p><p>Forge prepares this project area for the agent.</p></>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags agent display copy that exposes managed-workspace internals', () => {
    const cwd = fixture({
      'src/app/entities/agent/model/display-labels.ts': `
function agentRuntimeLabel() {
  return 'OpenCode in a managed workspace'
}
`,
      'src/app/entities/agent/model/runtime-kind.ts': `
export const RUNTIME_KIND_LABELS = { container: 'Managed workspace' }
`,
      'src/app/entities/context/ui/InjectionPreviewModal.tsx': `
function runtimeLabel() {
  return 'Managed workspace'
}
`,
      'src/app/features/admin/AgentsPanel.tsx': `
const AGENT_GUIDANCE = [{ title: 'Managed workspace' }]
`,
      'src/app/features/admin/SystemHealth.tsx': `
const action = 'Ask an owner or admin to check managed workspace setup.'
`,
      'src/app/features/analytics/ContextUsageDashboard.tsx': `
const RUNTIME_LABELS = { container: 'Managed workspace' }
`,
      'src/app/features/agents/AgentCard.tsx': `
function AgentCard() {
  return <p>Managed workspace</p>
}
`,
      'src/app/features/agents/AgentListView.tsx': `
function AgentChoiceGuide() {
  return <p>Managed workspace</p>
}
`,
      'src/app/features/agents/AgentKindBadge.tsx': `
export function AgentKindBadge() {
  return <span title="Uses a Forge-managed project workspace.">Managed workspace</span>
}
`,
      'src/app/features/agents/AgentConfigTab.tsx': `
function Connection() {
  return <p>Ready in managed workspace</p>
}
`,
      'src/app/features/detail/HistoryTab.tsx': `
function runSourceLabel() {
  return 'a managed workspace'
}
`,
      'src/app/features/settings/ResourcesSection.tsx': `
function ResourceProfilesEmptyState() {
  return <p>Return here before creating agents in managed workspaces.</p>
}
`,
      'src/app/features/settings/RuntimeSection.tsx': `
function fallbackRuntimeLabel() {
  return 'Managed workspace'
}
`,
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  runtimeLabels: { container: 'Managed workspace' },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  runtimeLabels: { container: '托管工作区' },
}
`,
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
function agentFolderLabel() {
  return 'Workspace project folder'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'agent-work-area-display-copy',
          sample: expect.stringContaining('in a managed workspace'),
        }),
        expect.objectContaining({
          type: 'agent-work-area-display-copy',
          sample: expect.stringContaining('Managed workspace'),
        }),
        expect.objectContaining({
          type: 'agent-work-area-display-copy',
          sample: expect.stringContaining('managed workspace setup'),
        }),
        expect.objectContaining({
          type: 'agent-work-area-display-copy',
          sample: expect.stringContaining('Forge-managed project workspace'),
        }),
        expect.objectContaining({
          type: 'agent-work-area-display-copy',
          sample: expect.stringContaining('Ready in managed workspace'),
        }),
        expect.objectContaining({
          type: 'agent-work-area-display-copy',
          sample: expect.stringContaining('托管工作区'),
        }),
        expect.objectContaining({
          type: 'agent-work-area-display-copy',
          sample: expect.stringContaining('Workspace project folder'),
        }),
      ])
    )
  })

  it('accepts agent display copy that explains project files plainly', () => {
    const cwd = fixture({
      'src/app/entities/agent/model/display-labels.ts': `
function agentRuntimeLabel() {
  return 'OpenCode with project files'
}
`,
      'src/app/entities/context/ui/InjectionPreviewModal.tsx': `
function runtimeLabel() {
  return 'Project files'
}
`,
      'src/app/features/analytics/ContextUsageDashboard.tsx': `
const RUNTIME_LABELS = { container: 'Project files' }
`,
      'src/app/features/agents/AgentKindBadge.tsx': `
export function AgentKindBadge() {
  return <span title="Works in a Forge project area. It can change files.">Project files</span>
}
`,
      'src/app/features/agents/AgentConfigTab.tsx': `
function Connection() {
  return <p>Ready with project files</p>
}
`,
      'src/app/features/detail/HistoryTab.tsx': `
function runSourceLabel() {
  return 'project files'
}
`,
      'src/app/features/settings/ResourcesSection.tsx': `
function ResourceProfilesEmptyState() {
  return <p>Return here before creating agents that edit project files.</p>
}
`,
      'src/app/features/settings/RuntimeSection.tsx': `
function fallbackRuntimeLabel() {
  return 'Project files'
}
`,
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  runtimeLabels: { container: 'Project files' },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  runtimeLabels: { container: '项目文件' },
}
`,
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
function agentFolderLabel() {
  return 'Default project folder'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags create-agent project labels that do not explain where new tasks start', () => {
    const cwd = fixture({
      'src/app/features/agents/CreateAgentModal.tsx': `
function createReviewItems() {
  return [{ label: 'Primary project', value: 'Platform' }]
}
function ProjectReadiness() {
  return <p>New tasks start from the Primary Project selected above.</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'create-agent-project-copy',
          location: 'src/app/features/agents/CreateAgentModal.tsx:3',
        }),
        expect.objectContaining({
          type: 'create-agent-project-copy',
          location: 'src/app/features/agents/CreateAgentModal.tsx:6',
        }),
      ])
    )
  })

  it('accepts create-agent project labels that explain where new tasks start', () => {
    const cwd = fixture({
      'src/app/features/agents/CreateAgentModal.tsx': `
function createReviewItems() {
  return [{ label: 'Project for new tasks', value: 'Platform' }]
}
function ProjectReadiness() {
  return <p>New tasks start from the project shown above.</p>
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
  return { operator: 'Operator' }
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
        expect.objectContaining({
          type: 'access-level-copy',
          location: 'src/app/entities/user/model/roleLabels.ts:5',
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

function drawTimeline(ctx) {
  ctx.fillText('Waiting for run events')
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
      expect.objectContaining({
        type: 'timeline-empty-copy',
        location: 'src/app/widgets/views/TimelineView.tsx:7',
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
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'agent-detail-activity-copy',
          location: 'src/app/widgets/agent-detail/AgentDetailView.tsx:3',
        }),
      ])
    )
  })

  it('accepts agent detail activity copy that tells users to open Tasks first', () => {
    const cwd = fixture({
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
function agentNextStep() {
  return { detail: "Go to Tasks to load this agent's work history and decide what to send next." }
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags chat-only agent file access copy that sounds like nothing is needed', () => {
    const cwd = fixture({
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
function agentFolderLabel() {
  return 'No file access needed'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'agent-detail-file-access-copy',
          location: 'src/app/widgets/agent-detail/AgentDetailView.tsx:3',
        }),
      ])
    )
  })

  it('accepts chat-only agent file access copy that points users to another agent', () => {
    const cwd = fixture({
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
function agentFolderLabel() {
  return 'Use another agent for file work'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags agent detail availability copy that does not tell users where to recover', () => {
    const cwd = fixture({
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
function agentAvailabilityLabel() {
  return 'Unavailable until restarted or reconnected'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'agent-detail-availability-copy',
          location: 'src/app/widgets/agent-detail/AgentDetailView.tsx:3',
        }),
      ])
    )
  })

  it('accepts agent detail availability copy that names the recovery surface', () => {
    const cwd = fixture({
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
function agentAvailabilityLabel() {
  return 'Open Live work and start file work'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags agent detail start failure copy that starts with the failure result', () => {
    const cwd = fixture({
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
function PendingTerminal() {
  return 'Start did not finish. Check the agent status, then try once more.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'agent-detail-start-failure-copy',
          location: 'src/app/widgets/agent-detail/AgentDetailView.tsx:3',
        }),
      ])
    )
  })

  it('accepts agent detail start failure copy that starts with the recovery action', () => {
    const cwd = fixture({
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
function PendingTerminal() {
  return 'Check the agent status, then choose Start file work again.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags workspace wording in agent file-work controls', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentControlPanel.tsx': `
function StartCard() {
  return 'Workspace needs to start. Choose Start workspace again.'
}
`,
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
function PendingTerminal() {
  return 'Open Live work, choose Start workspace, and wait until this agent shows Ready.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'agent-file-work-control-copy',
          location: 'src/app/features/agents/AgentControlPanel.tsx:3',
        }),
        expect.objectContaining({
          type: 'agent-file-work-control-copy',
          location: 'src/app/widgets/agent-detail/AgentDetailView.tsx:3',
        }),
      ])
    )
  })

  it('accepts file-work wording in agent start controls', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentControlPanel.tsx': `
function StartCard() {
  return 'File work needs to start. Choose Start file work again.'
}
`,
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
function PendingTerminal() {
  return 'Open Live work, choose Start file work, and wait until this agent shows Ready.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags title-style beginner guidance that sounds like a menu label', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentTasksTab.tsx': `
export function AgentTasksEmptyState() {
  return <h3>Open Tasks to send this agent work</h3>
}
`,
      'src/app/features/agents/AgentListView.tsx': `
const SORT_OPTIONS = [{ value: 'success', label: 'Success Rate' }]
const LOWER_SORT_OPTIONS = [{ value: 'success', label: 'Success rate' }]
const nextStep = { title: 'Review current work' }
const boardTitle = 'Create a Task Queue First'
const projectTitle = 'Pick a Project to Start'
const providerHeading = 'AI Services'
const providerPlaceholder = 'My AI Service...'
const providerSave = 'Save AI Service'
const sections = [
  { group: 'AI Setup', label: 'Outside Tool Access' },
  { group: 'Work Setup', label: 'Code Access' },
  { group: 'Product Info', label: 'SSH Code Access' },
  { label: 'Work Capacity' },
  { label: 'Agent Work Setup' },
  { label: 'Team Members' },
]
const teamNextStep = 'Open Team Members after creation to invite people.'
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'title-style-guidance-copy',
          location: 'src/app/features/agents/AgentTasksTab.tsx:3',
        }),
        expect.objectContaining({
          type: 'title-style-guidance-copy',
          location: 'src/app/features/agents/AgentListView.tsx:2',
        }),
        expect.objectContaining({
          type: 'title-style-guidance-copy',
          location: 'src/app/features/agents/AgentListView.tsx:3',
        }),
        expect.objectContaining({
          type: 'title-style-guidance-copy',
          location: 'src/app/features/agents/AgentListView.tsx:4',
        }),
        expect.objectContaining({
          type: 'title-style-guidance-copy',
          location: 'src/app/features/agents/AgentListView.tsx:5',
        }),
        expect.objectContaining({
          type: 'title-style-guidance-copy',
          location: 'src/app/features/agents/AgentListView.tsx:6',
        }),
        expect.objectContaining({
          type: 'title-style-guidance-copy',
          location: 'src/app/features/agents/AgentListView.tsx:7',
        }),
        expect.objectContaining({
          type: 'title-style-guidance-copy',
          location: 'src/app/features/agents/AgentListView.tsx:8',
        }),
        expect.objectContaining({
          type: 'title-style-guidance-copy',
          location: 'src/app/features/agents/AgentListView.tsx:9',
        }),
        expect.objectContaining({
          type: 'title-style-guidance-copy',
          location: 'src/app/features/agents/AgentListView.tsx:11',
        }),
        expect.objectContaining({
          type: 'title-style-guidance-copy',
          location: 'src/app/features/agents/AgentListView.tsx:12',
        }),
        expect.objectContaining({
          type: 'title-style-guidance-copy',
          location: 'src/app/features/agents/AgentListView.tsx:13',
        }),
        expect.objectContaining({
          type: 'title-style-guidance-copy',
          location: 'src/app/features/agents/AgentListView.tsx:14',
        }),
        expect.objectContaining({
          type: 'title-style-guidance-copy',
          location: 'src/app/features/agents/AgentListView.tsx:15',
        }),
        expect.objectContaining({
          type: 'title-style-guidance-copy',
          location: 'src/app/features/agents/AgentListView.tsx:16',
        }),
        expect.objectContaining({
          type: 'title-style-guidance-copy',
          location: 'src/app/features/agents/AgentListView.tsx:18',
        }),
      ])
    )
  })

  it('accepts action-first beginner guidance', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentTasksTab.tsx': `
export function AgentTasksEmptyState() {
  return <h3>Go to Tasks to send this agent work</h3>
}
`,
      'src/app/features/agents/AgentListView.tsx': `
const SORT_OPTIONS = [{ value: 'success', label: 'Best finish rate' }]
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags agent list empty summaries that do not point to creating the first agent', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentListView.tsx': `
function AgentSummary() {
  return agents.length === 0 ? 'No agents' : '2/4 agents'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'agent-list-summary-copy',
        location: 'src/app/features/agents/AgentListView.tsx:3',
      }),
    ])
  })

  it('accepts agent list empty summaries that start with the first action', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentListView.tsx': `
function AgentSummary() {
  return agents.length === 0 ? 'Create first agent' : '2/4 agents'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags completed task notifications that stop at a missing summary', () => {
    const cwd = fixture({
      'src/app/hooks/useWsDispatch.ts': `
function completionSummary() {
  return 'No completion summary was provided'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'task-completion-summary-copy',
        location: 'src/app/hooks/useWsDispatch.ts:3',
      }),
    ])
  })

  it('flags completed task notifications that say open details without naming task details', () => {
    const cwd = fixture({
      'src/app/hooks/useWsDispatch.ts': `
function completionSummary() {
  return 'Finished with a text result. Open details to review it.'
}
function safeCompletionMessage() {
  return 'Finished with a summary you should check. Open details before using the result.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-completion-details-copy',
          location: 'src/app/hooks/useWsDispatch.ts:3',
        }),
        expect.objectContaining({
          type: 'task-completion-details-copy',
          location: 'src/app/hooks/useWsDispatch.ts:6',
        }),
      ])
    )
  })

  it('accepts completed task notifications that point users to task details', () => {
    const cwd = fixture({
      'src/app/hooks/useWsDispatch.ts': `
function completionSummary() {
  return 'Open the task details to confirm what changed before using the result.'
}
function safeCompletionMessage() {
  return 'Finished with a summary you should check. Open the task details before using the result.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task owner guidance that exposes owner-input jargon', () => {
    const cwd = fixture({
      'src/app/hooks/useWsDispatch.ts': `
function taskNotificationMessage(actor, detail) {
  return \`\${actor} is blocked and needs owner input: \${detail}\`
}
`,
      'src/app/features/detail/HistoryTab.tsx': `
function taskStatusSnapshot(agentName) {
  return \`\${agentName} needs owner input\`
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-owner-input-copy',
          location: 'src/app/hooks/useWsDispatch.ts:3',
        }),
        expect.objectContaining({
          type: 'task-owner-input-copy',
          location: 'src/app/features/detail/HistoryTab.tsx:3',
        }),
      ])
    )
  })

  it('accepts task owner guidance that asks for the user answer', () => {
    const cwd = fixture({
      'src/app/hooks/useWsDispatch.ts': `
function taskNotificationMessage(actor, detail) {
  return \`\${actor} needs your answer before work can continue: \${detail}\`
}
`,
      'src/app/features/detail/HistoryTab.tsx': `
function taskStatusSnapshot(agentName) {
  return \`\${agentName} needs your answer\`
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags missing tool summary copy that assumes the tool should be turned on', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentPluginsTab.tsx': `
function toolDescription() {
  return 'No tool summary yet. Ask an owner what this tool lets the agent do before turning it on.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'agent-tool-summary-copy',
        location: 'src/app/features/agents/AgentPluginsTab.tsx:3',
      }),
    ])
  })

  it('accepts missing tool summary copy that tells users to keep the team setting', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentPluginsTab.tsx': `
function toolDescription() {
  return 'Tool summary is missing. Keep the team setting until an owner explains what this tool lets the agent do.'
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

  it('flags agent tool update status copy that only says a check failed', () => {
    const cwd = fixture({
      'src/app/features/admin/CliImagesPanel.tsx': `
function ToolRow() {
  return <p>Check failed</p>
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

  it('flags agent tool action errors that start with the failure', () => {
    const cwd = fixture({
      'src/app/features/admin/CliImagesPanel.tsx': `
function ToolUpdateError() {
  return <p>The restart could not be started.</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'cli-image-action-copy',
          location: 'src/app/features/admin/CliImagesPanel.tsx:3',
        }),
      ])
    )
  })

  it('flags agent tool update empty copy that gives no setup path', () => {
    const cwd = fixture({
      'src/app/features/admin/CliImagesPanel.tsx': `
function ToolUpdateEmpty() {
  return <p>No agent tools are configured for update checks.</p>
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

  it('flags agent tool package copy that exposes build-server wording', () => {
    const cwd = fixture({
      'src/app/features/admin/CliImagesPanel.tsx': `
function ToolRow() {
  return <><p>Built here</p><p>Building on this server — usually a few minutes.</p><p>Builds automatically — new versions build themselves</p></>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'cli-image-status-copy',
          location: 'src/app/features/admin/CliImagesPanel.tsx:3',
        }),
      ])
    )
  })

  it('accepts agent tool update status copy that tells users to check now', () => {
    const cwd = fixture({
      'src/app/features/admin/CliImagesPanel.tsx': `
function ToolRow() {
  return <p>Latest check: Choose Check now to check for updates</p>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags agent tool update result copy that uses failed or skipped jargon', () => {
    const cwd = fixture({
      'src/app/features/admin/CliImagesPanel.tsx': `
function RollResultBlock({ result, prune }) {
  return (
    <div>
      <p>{result.failed > 0 ? \` · \${result.failed} failed\` : ''}</p>
      <p>{result.skippedBusy > 0 ? \` · \${result.skippedBusy} skipped (busy)\` : ''}</p>
      <p>{prune.errors > 0 ? \` · \${prune.errors} errors\` : ''}</p>
      <p>The last cleanup hit 1 error.</p>
    </div>
  )
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'cli-image-result-copy',
          location: 'src/app/features/admin/CliImagesPanel.tsx:5',
        }),
        expect.objectContaining({
          type: 'cli-image-result-copy',
          location: 'src/app/features/admin/CliImagesPanel.tsx:6',
        }),
        expect.objectContaining({
          type: 'cli-image-result-copy',
          location: 'src/app/features/admin/CliImagesPanel.tsx:7',
        }),
        expect.objectContaining({
          type: 'cli-image-result-copy',
          location: 'src/app/features/admin/CliImagesPanel.tsx:8',
        }),
      ])
    )
  })

  it('accepts agent tool update result copy that explains next status', () => {
    const cwd = fixture({
      'src/app/features/admin/CliImagesPanel.tsx': `
function RollResultBlock({ result, prune }) {
  return (
    <div>
      <p>{result.failed > 0 ? \` · \${result.failed} need a retry\` : ''}</p>
      <p>{result.skippedBusy > 0 ? \` · \${result.skippedBusy} still working\` : ''}</p>
      <p>{prune.errors > 0 ? \` · \${prune.errors} need a check\` : ''}</p>
      <p>The last cleanup needs a check for 1 package.</p>
    </div>
  )
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

function CredentialStatusRow() {
  return <p>No work tool sign-in saved</p>
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
      expect.objectContaining({
        type: 'runtime-sign-in-copy',
        location: 'src/app/features/settings/RuntimeSection.tsx:7',
      }),
    ])
  })

  it('accepts work setup summaries that tell users to sign in before starting agents', () => {
    const cwd = fixture({
      'src/app/features/settings/RuntimeSection.tsx': `
function runtimeReadinessSummary() {
  return 'Sign in to a tool for file work before starting agents that need one'
}

function CredentialStatusRow() {
  return <p>Sign in before starting agents that use this tool</p>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags default file work place copy that does not explain how to recover', () => {
    const cwd = fixture({
      'src/app/features/settings/RuntimeSection.tsx': `
export function RuntimeSection() {
  return <RuntimeReadinessMetric label="Default file work place" value="Not set yet" />
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

  it('accepts default file work place copy that tells users to load setup first', () => {
    const cwd = fixture({
      'src/app/features/settings/RuntimeSection.tsx': `
export function RuntimeSection() {
  return <RuntimeReadinessMetric label="Default file work place" value="Load setup to choose where files open" />
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags runtime setup errors that start with the failure summary', () => {
    const cwd = fixture({
      'src/app/features/settings/runtimeErrorMessages.ts': `
function runtimeErrorMessage() {
  return 'Agent connection status could not load. Start or wake an agent, then refresh this page.'
}

function runtimeCliErrorMessage() {
  return 'Work tool sign-in did not start. Check the connected AI service, then reconnect the account.'
}

function runtimeSettingsErrorMessage() {
  return 'Where agents run could not be saved. Choose an available agent location and work tool, then save again.'
}

function runtimeSettingsFallback() {
  return 'Try again. Where agents run could not be saved. If it still fails, ask an owner or admin to check Where agents run.'
}
`,
      'src/app/features/settings/RuntimeSection.tsx': `
function runtimeChecklistCopy() {
  return 'The Where agents run settings have not loaded yet. Check setup. If they still do not load, ask an owner or admin to check Where agents run.'
}

function credentialStatusCopy() {
  return 'Work tool sign-ins could not be checked. Check setup. If they still cannot be checked, ask an owner or admin to check work tool sign-ins.'
}

function heartbeatStatusCopy() {
  return 'Agent online status could not be checked. Check setup. If it still cannot be checked, ask an owner or admin to check Where agents run.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'runtime-error-copy',
          location: 'src/app/features/settings/runtimeErrorMessages.ts:3',
        }),
        expect.objectContaining({
          type: 'runtime-error-copy',
          location: 'src/app/features/settings/runtimeErrorMessages.ts:7',
        }),
        expect.objectContaining({
          type: 'runtime-error-copy',
          location: 'src/app/features/settings/runtimeErrorMessages.ts:11',
        }),
        expect.objectContaining({
          type: 'runtime-error-copy',
          location: 'src/app/features/settings/runtimeErrorMessages.ts:15',
        }),
        expect.objectContaining({
          type: 'runtime-error-copy',
          location: 'src/app/features/settings/RuntimeSection.tsx:3',
        }),
        expect.objectContaining({
          type: 'runtime-error-copy',
          location: 'src/app/features/settings/RuntimeSection.tsx:7',
        }),
        expect.objectContaining({
          type: 'runtime-error-copy',
          location: 'src/app/features/settings/RuntimeSection.tsx:11',
        }),
      ])
    )
  })

  it('accepts runtime setup errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/features/settings/runtimeErrorMessages.ts': `
function runtimeErrorMessage() {
  return 'Start or wake an agent, then refresh this page. Agent connection status could not load.'
}

function runtimeCliErrorMessage() {
  return 'Check the connected AI service, then reconnect the account. Work tool sign-in did not start.'
}

function runtimeSettingsErrorMessage() {
  return 'Choose an available file work place and work tool, then save again. Agent work setup could not be saved.'
}

function runtimeSettingsFallback() {
  return 'Check the file work place and work tool choices, then save Agent work setup again. If it still fails, ask an owner or admin to check Agent work setup in Settings.'
}
`,
      'src/app/features/settings/RuntimeSection.tsx': `
function runtimeChecklistCopy() {
  return 'Refresh this settings page to load Agent work setup. If it still does not load, ask an owner or admin to check Agent work setup in Settings.'
}

function credentialStatusCopy() {
  return 'Choose Check again to refresh work tool sign-ins. If they still cannot be checked, ask an owner or admin to check work tool sign-ins.'
}

function heartbeatStatusCopy() {
  return 'Choose Check again to refresh agent online status. If it still cannot be checked, ask an owner or admin to check Agent work setup in Settings.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags settings runtime navigation copy that explains internal run and tool choices', () => {
    const cwd = fixture({
      'src/app/pages/settings/ui/SettingsLayout.tsx': `
const SECTIONS = [
  {
    label: 'Where agents run',
    description: 'Choose where agents run and which work tool they use.',
  },
]
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'settings-runtime-nav-copy',
        location: 'src/app/pages/settings/ui/SettingsLayout.tsx:5',
      }),
    ])
  })

  it('accepts settings runtime navigation copy that explains file-work setup', () => {
    const cwd = fixture({
      'src/app/pages/settings/ui/SettingsLayout.tsx': `
const SECTIONS = [
  {
    label: 'Agent work setup',
    description: 'Choose where agents edit files and which tool opens the work.',
  },
]
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags agent work setup labels that use agent location wording', () => {
    const cwd = fixture({
      'src/app/features/settings/RuntimeSection.tsx': `
export function RuntimeSection() {
  return (
    <>
      <RuntimeReadinessMetric label="Default agent location" value="Project files" />
      <SettingRow label="Agent locations available" />
      <RuntimeChecklistRow title="Default agent location and work tool" />
      <p>Choose where new agents run and which tool, such as Claude or Codex, they use.</p>
    </>
  )
}
`,
      'src/app/features/settings/runtimeErrorMessages.ts': `
export function message() {
  return 'Check the agent location and work tool choices, then save Agent work setup again.'
}
`,
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  settings: {
    runtime: {
      defaultRuntimeLabel: 'Default agent location',
      availableRuntimesLabel: 'Agent locations available',
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
          type: 'settings-runtime-location-copy',
          location: 'src/app/features/settings/RuntimeSection.tsx:5',
        }),
        expect.objectContaining({
          type: 'settings-runtime-location-copy',
          location: 'src/app/features/settings/RuntimeSection.tsx:6',
        }),
        expect.objectContaining({
          type: 'settings-runtime-location-copy',
          location: 'src/app/features/settings/RuntimeSection.tsx:7',
        }),
        expect.objectContaining({
          type: 'settings-runtime-location-copy',
          location: 'src/app/features/settings/RuntimeSection.tsx:8',
        }),
        expect.objectContaining({
          type: 'settings-runtime-location-copy',
          location: 'src/app/features/settings/runtimeErrorMessages.ts:3',
        }),
        expect.objectContaining({
          type: 'settings-runtime-location-copy',
          location: 'src/app/shared/i18n/locales/en.ts:5',
        }),
      ])
    )
  })

  it('accepts agent work setup labels that explain where files open', () => {
    const cwd = fixture({
      'src/app/features/settings/RuntimeSection.tsx': `
export function RuntimeSection() {
  return (
    <>
      <RuntimeReadinessMetric label="Default file work place" value="Project files" />
      <SettingRow label="Places agents can edit files" />
      <RuntimeChecklistRow title="Default file work place and tool" />
      <p>Choose where new agents edit files and which tool, such as Claude or Codex, opens the work.</p>
    </>
  )
}
`,
      'src/app/features/settings/runtimeErrorMessages.ts': `
export function message() {
  return 'Check the file work place and work tool choices, then save Agent work setup again.'
}
`,
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  settings: {
    runtime: {
      defaultRuntimeLabel: 'Default file work place',
      availableRuntimesLabel: 'Places agents can edit files',
    },
  },
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags runtime setup status copy that only says what is missing', () => {
    const cwd = fixture({
      'src/app/features/settings/RuntimeSection.tsx': `
function checklistCopy() {
  return 'No work tool setup status yet. Check again after the tools finish setting up.'
}

function heartbeatCopy() {
  return 'No agent has been seen online yet. Start or wake an agent, then check again.'
}

function runtimeReadinessSummary() {
  return 'Setup has 1 agent location and 1 work tool like Claude or Codex. No extra work tool sign-ins are needed, and no agents are online yet.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'runtime-setup-status-copy',
          location: 'src/app/features/settings/RuntimeSection.tsx:3',
        }),
        expect.objectContaining({
          type: 'runtime-setup-status-copy',
          location: 'src/app/features/settings/RuntimeSection.tsx:7',
        }),
        expect.objectContaining({
          type: 'runtime-setup-status-copy',
          location: 'src/app/features/settings/RuntimeSection.tsx:11',
        }),
      ])
    )
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

  it('flags agent project folder copy that uses path wording', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  agents: {
    projectPath: 'Project Path',
    searchProjects: 'Search projects or enter a folder path...',
    invalidProjectPath: 'Enter a project folder path, then try again.',
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  agents: {
    projectPath: '项目路径',
    enterFolderPath: '输入项目文件夹路径...',
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'agent-project-location-copy',
          sample: expect.stringContaining('Project Path'),
        }),
        expect.objectContaining({
          type: 'agent-project-location-copy',
          sample: expect.stringContaining('folder path'),
        }),
        expect.objectContaining({
          type: 'agent-project-location-copy',
          sample: expect.stringContaining('文件夹路径'),
        }),
      ])
    )
  })

  it('accepts agent project folder copy that uses location wording', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  agents: {
    projectPath: 'Project folder location',
    searchProjects: 'Search projects or enter a folder location...',
    invalidProjectPath: 'Enter the project folder location, then try again.',
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  agents: {
    projectPath: '项目文件夹位置',
    enterFolderPath: '输入项目文件夹位置...',
  },
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
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
    delete: 'Delete this item? It will be removed from this workspace.',
  },
  admin: {
    users: {
      confirmDelete: 'Delete this user? They will lose access to this workspace.',
    },
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
      expect.objectContaining({
        type: 'confirmation-impact',
        location: 'src/app/shared/i18n/locales/en.ts:11',
      }),
      expect.objectContaining({
        type: 'confirmation-impact',
        location: 'src/app/shared/i18n/locales/en.ts:15',
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
    delete: '要删除这一项吗？它会从当前工作区移除。',
  },
  admin: {
    users: {
      confirmDelete: '要删除这个用户吗？该用户将失去当前工作区访问权限。',
    },
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
      expect.objectContaining({
        type: 'confirmation-impact',
        location: 'src/app/shared/i18n/locales/zh.ts:11',
      }),
      expect.objectContaining({
        type: 'confirmation-impact',
        location: 'src/app/shared/i18n/locales/zh.ts:15',
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
  confirm: {
    delete: 'Delete this item? It will be removed from this team space.',
  },
  admin: {
    users: {
      confirmDelete: 'Delete this user? They will lose access to this team space.',
    },
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  confirm: {
    delete: '要删除这一项吗？它会从当前团队空间移除。',
  },
  admin: {
    users: {
      confirmDelete: '要删除这个用户吗？该用户将失去当前团队空间访问权限。',
    },
  },
}
`,
      'src/app/layouts/sidebar/ProjectTree.tsx': `
function deleteDetail() {
  return 'The project is removed from this team space, and agents are moved out instead of deleted.'
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
      error: 'Check update',
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
      error: 'Check agent status',
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
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'beginner-jargon-copy',
          location: 'src/app/features/agents/CreateAgentModal.tsx:3',
        }),
      ])
    )
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
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'beginner-jargon-copy',
          location: 'src/app/features/agents/CreateAgentModal.tsx:3',
        }),
      ])
    )
  })

  it('flags this-computer setup copy that uses command-window jargon', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentListView.tsx': `
export function HostCliEnrollmentPanel() {
  return (
    <section>
      <p>Use this backup if your browser cannot open the setup window or your team asks you to run a command.</p>
      <p>Then the setup command appears here.</p>
      <p>Keep the command window open while it works.</p>
      <p>Keep Terminal or PowerShell open while it works.</p>
      <button>Copy setup command</button>
    </section>
  )
}
`,
      'src/app/features/agents/CreateAgentModal.tsx': `
export function CreateAgentModal() {
  return (
    <section>
      <p>Run setup command on this computer</p>
      <p>Forge creates the agent, then shows a setup command for this computer.</p>
      <p>One-line Windows setup command is not ready for this agent.</p>
      <p>One-line Windows setup text is not ready for this agent.</p>
      <p>Paste it into Terminal or PowerShell on the computer that will do the work.</p>
      <p>Copy these backup setup values into the same Terminal or PowerShell window.</p>
      <p>Paste it into the terminal app on the computer that will do the work.</p>
      <p>Leave blank to use the folder where you run the setup command.</p>
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
          type: 'this-computer-setup-copy',
          location: 'src/app/features/agents/AgentListView.tsx:5',
        }),
        expect.objectContaining({
          type: 'this-computer-setup-copy',
          location: 'src/app/features/agents/AgentListView.tsx:6',
        }),
        expect.objectContaining({
          type: 'this-computer-setup-copy',
          location: 'src/app/features/agents/AgentListView.tsx:7',
        }),
        expect.objectContaining({
          type: 'this-computer-setup-copy',
          location: 'src/app/features/agents/AgentListView.tsx:8',
        }),
        expect.objectContaining({
          type: 'this-computer-setup-copy',
          location: 'src/app/features/agents/AgentListView.tsx:9',
        }),
        expect.objectContaining({
          type: 'this-computer-setup-copy',
          location: 'src/app/features/agents/CreateAgentModal.tsx:6',
        }),
        expect.objectContaining({
          type: 'this-computer-setup-copy',
          location: 'src/app/features/agents/CreateAgentModal.tsx:7',
        }),
        expect.objectContaining({
          type: 'this-computer-setup-copy',
          location: 'src/app/features/agents/CreateAgentModal.tsx:8',
        }),
        expect.objectContaining({
          type: 'this-computer-setup-copy',
          location: 'src/app/features/agents/CreateAgentModal.tsx:9',
        }),
        expect.objectContaining({
          type: 'this-computer-setup-copy',
          location: 'src/app/features/agents/CreateAgentModal.tsx:10',
        }),
        expect.objectContaining({
          type: 'this-computer-setup-copy',
          location: 'src/app/features/agents/CreateAgentModal.tsx:11',
        }),
        expect.objectContaining({
          type: 'this-computer-setup-copy',
          location: 'src/app/features/agents/CreateAgentModal.tsx:12',
        }),
      ])
    )
  })

  it('accepts this-computer setup copy that uses setup text and clear app names', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentListView.tsx': `
export function HostCliEnrollmentPanel() {
  return (
    <section>
      <p>Use this backup if the guided setup does not open.</p>
      <p>Then the setup text appears here.</p>
      <p>Open your computer's command app: Terminal on macOS/Linux, or PowerShell on Windows.</p>
      <p>Keep that command app open while it works.</p>
      <button>Copy setup text</button>
    </section>
  )
}
`,
      'src/app/features/agents/CreateAgentModal.tsx': `
export function CreateAgentModal() {
  return (
    <section>
      <p>Paste setup text in this computer's command app</p>
      <p>Forge creates the agent, then shows setup steps for this computer.</p>
      <p>Windows setup needs the backup values for this agent.</p>
      <p>Leave blank to use the folder where you paste the setup text.</p>
    </section>
  )
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags this-computer setup command wording in agent status and error copy', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentControlPanel.tsx': `
export function AgentControlPanel() {
  return <p>Run the setup command on that computer again.</p>
}
`,
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
export function AgentDetailView() {
  return <p>Folder where you ran the setup command</p>
}
`,
      'src/app/entities/agent/model/agents.store.ts': `
export function agentError() {
  return 'Forge could not prepare the setup command for this computer.'
}
`,
      'src/app/shared/model/agents.store.ts': `
export const THIS_COMPUTER_SETUP_ERROR =
  'This computer setup command could not be prepared.'
`,
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  detail: 'Setup command needs to be run again',
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'this-computer-setup-copy',
          location: 'src/app/features/agents/AgentControlPanel.tsx:3',
        }),
        expect.objectContaining({
          type: 'this-computer-setup-copy',
          location: 'src/app/widgets/agent-detail/AgentDetailView.tsx:3',
        }),
        expect.objectContaining({
          type: 'this-computer-setup-copy',
          location: 'src/app/entities/agent/model/agents.store.ts:3',
        }),
        expect.objectContaining({
          type: 'this-computer-setup-copy',
          location: 'src/app/shared/model/agents.store.ts:3',
        }),
        expect.objectContaining({
          type: 'this-computer-setup-copy',
          location: 'src/app/shared/i18n/locales/en.ts:3',
        }),
      ])
    )
  })

  it('accepts this-computer setup text wording in agent status and error copy', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentControlPanel.tsx': `
export function AgentControlPanel() {
  return <p>Paste the setup text on that computer again.</p>
}
`,
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
export function AgentDetailView() {
  return <p>Folder where you pasted the setup text</p>
}
`,
      'src/app/entities/agent/model/agents.store.ts': `
export function agentError() {
  return 'Check your connection, then choose Create Agent again. Forge could not prepare the setup text for this computer.'
}
`,
      'src/app/shared/model/agents.store.ts': `
export const THIS_COMPUTER_SETUP_ERROR =
  'This computer setup text could not be prepared. Check the agent name and work tool, then choose Create Agent again.'
`,
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  detail: 'Setup text needs to be pasted again',
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags agent store errors that explain failure before the next step', () => {
    const cwd = fixture({
      'src/app/entities/agent/model/agents.store.ts': `
function agentConnectionMessage(actionPhrase) {
  return \`Forge could not \${actionPhrase}. It could not connect while updating Agents. Check your connection, then refresh Agents.\`
}

function agentPermissionMessage(actionPhrase) {
  return \`You do not have permission to \${actionPhrase}. Ask an owner or admin to update access.\`
}

function agentBusyMessage(actionPhrase) {
  return \`The Agents page is busy. Wait a moment, then try to \${actionPhrase} again.\`
}

function agentConflictMessage() {
  return 'This agent is already working. Wait for the current work to finish.'
}

function agentChangedMessage() {
  return 'This agent changed while you were working. Refresh the Agents page, then try again.'
}

function agentServerMessage() {
  return 'Forge could not prepare the setup text for this computer right now. Wait a moment, then choose Create Agent again.'
}

function agentRuntimeMessage() {
  return 'The place where this agent runs is not ready. Ask an owner or admin to check Where agents run.'
}

function agentUnknownMessage(actionPhrase) {
  return \`Forge could not \${actionPhrase}. Refresh the Agents page, then try again.\`
}

function agentCreatedStartFailureMessage() {
  return 'Agent was created, but it could not start yet. It will stay in the list. Ask an owner or admin to check Where agents run.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'agent-store-error-copy',
          location: 'src/app/entities/agent/model/agents.store.ts:3',
        }),
        expect.objectContaining({
          type: 'agent-store-error-copy',
          location: 'src/app/entities/agent/model/agents.store.ts:7',
        }),
        expect.objectContaining({
          type: 'agent-store-error-copy',
          location: 'src/app/entities/agent/model/agents.store.ts:11',
        }),
        expect.objectContaining({
          type: 'agent-store-error-copy',
          location: 'src/app/entities/agent/model/agents.store.ts:15',
        }),
        expect.objectContaining({
          type: 'agent-store-error-copy',
          location: 'src/app/entities/agent/model/agents.store.ts:19',
        }),
        expect.objectContaining({
          type: 'agent-store-error-copy',
          location: 'src/app/entities/agent/model/agents.store.ts:23',
        }),
        expect.objectContaining({
          type: 'agent-store-error-copy',
          location: 'src/app/entities/agent/model/agents.store.ts:27',
        }),
        expect.objectContaining({
          type: 'agent-store-error-copy',
          location: 'src/app/entities/agent/model/agents.store.ts:31',
        }),
        expect.objectContaining({
          type: 'agent-store-error-copy',
          location: 'src/app/entities/agent/model/agents.store.ts:35',
        }),
      ])
    )
  })

  it('accepts agent store errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/entities/agent/model/agents.store.ts': `
function agentConnectionMessage(actionPhrase) {
  return \`Check your connection, then refresh Agents. Forge could not \${actionPhrase} while updating Agents.\`
}

function agentPermissionMessage(actionPhrase) {
  return \`Ask an owner or admin to update access, then try to \${actionPhrase} again. You do not have permission to \${actionPhrase}.\`
}

function agentBusyMessage(actionPhrase) {
  return \`Wait a moment, then try to \${actionPhrase} again. The Agents page is busy.\`
}

function agentConflictMessage() {
  return 'Wait for the current work to finish, refresh the Agents page, then try again. This agent is already working.'
}

function agentChangedMessage() {
  return 'Refresh the Agents page, review its current status, then try again. This agent changed while you were working.'
}

function agentServerMessage() {
  return 'Wait a moment, then choose Create Agent again. Forge could not prepare the setup text for this computer right now.'
}

function agentRuntimeMessage() {
  return 'Ask an owner or admin to check Agent work setup in Settings, then start this agent from the card. The place where this agent runs is not ready.'
}

function agentUnknownMessage(actionPhrase) {
  return \`Refresh the Agents page, then try to \${actionPhrase} again. Forge could not \${actionPhrase}.\`
}

function agentCreatedStartFailureMessage() {
  return 'Ask an owner or admin to check Agent work setup in Settings, then start this agent from the card. Agent was created, but it could not start yet.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
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
      'src/app/features/agents/AgentTasksTab.tsx': `
const STATE_LABELS = {
  failed: 'Stopped with an error',
}

const STATE_HELP = {
  failed: 'These tasks stopped before finishing.',
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
          location: 'src/app/features/agents/AgentTasksTab.tsx:3',
        }),
        expect.objectContaining({
          type: 'task-recovery-status-copy',
          location: 'src/app/features/agents/AgentTasksTab.tsx:7',
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
      'src/app/features/agents/AgentTasksTab.tsx': `
const STATE_LABELS = {
  failed: 'Review recovery',
}

const STATE_HELP = {
  failed: 'Open the task, review the latest update, then retry when ready.',
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
      'src/app/features/detail/ContextAppliedList.tsx': `
function contentLoadError() {
  return 'The full saved note could not load. Choose Show full saved note again before relying on it.'
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
          location: 'src/app/features/detail/ContextAppliedList.tsx:3',
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
      'src/app/features/detail/ContextAppliedList.tsx': `
function contentLoadError() {
  return 'Choose Show complete saved note again before relying on it. The complete saved note could not load.'
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

  it('flags chat filters that explain You with operator jargon', () => {
    const cwd = fixture({
      'src/app/features/chat/ChatView.tsx': `
function conversationFilterEmptyCopy() {
  return 'The You filter only shows requests sent by an operator.'
}
function chatOnlyBanner() {
  return 'This agent can answer in chat, but it does not work on workspace files.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'chat-operator-copy',
        location: 'src/app/features/chat/ChatView.tsx:3',
      }),
      expect.objectContaining({
        type: 'chat-operator-copy',
        location: 'src/app/features/chat/ChatView.tsx:6',
      }),
    ])
  })

  it('accepts chat filters that explain You as the current user', () => {
    const cwd = fixture({
      'src/app/features/chat/ChatView.tsx': `
function conversationFilterEmptyCopy() {
  return 'The You filter only shows requests you sent.'
}
function chatOnlyBanner() {
  return 'This agent can answer in chat, but it does not open project files.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags chat filter empty copy that uses reported-work jargon', () => {
    const cwd = fixture({
      'src/app/features/chat/ChatView.tsx': `
function toolEmptyTitle() {
  return 'No work steps have been reported yet'
}

function toolEmptyDetail() {
  return 'Work steps appear when a workspace agent reports commands or tool runs.'
}

function toolEmptyNextStep() {
  return 'Next: use All to see chat updates, or assign a workspace task to create work steps.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'chat-filter-empty-copy',
          location: 'src/app/features/chat/ChatView.tsx:3',
        }),
        expect.objectContaining({
          type: 'chat-filter-empty-copy',
          location: 'src/app/features/chat/ChatView.tsx:7',
        }),
        expect.objectContaining({
          type: 'chat-filter-empty-copy',
          location: 'src/app/features/chat/ChatView.tsx:11',
        }),
      ])
    )
  })

  it('flags feed and analytics guidance that uses reported-work jargon', () => {
    const cwd = fixture({
      'src/app/features/feed/ActivityFeed.tsx': `
function emptyFeed() {
  return 'No work has reported progress yet. Start a task or wait for an assigned agent.'
}

function filteredEmpty() {
  return 'No completed updates in this view'
}

function filteredDetail() {
  return 'No recent activity matches this view.'
}
`,
      'src/app/features/analytics/AnalyticsDashboard.tsx': `
function nextStep() {
  return 'Review Command line failures first. Open recent task results and check the steps that ended in error before assigning more work.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'activity-feed-empty-copy',
          location: 'src/app/features/feed/ActivityFeed.tsx:3',
        }),
        expect.objectContaining({
          type: 'activity-feed-empty-copy',
          location: 'src/app/features/feed/ActivityFeed.tsx:7',
        }),
        expect.objectContaining({
          type: 'activity-feed-empty-copy',
          location: 'src/app/features/feed/ActivityFeed.tsx:11',
        }),
        expect.objectContaining({
          type: 'analytics-guidance-copy',
          location: 'src/app/features/analytics/AnalyticsDashboard.tsx:3',
        }),
      ])
    )
  })

  it('accepts analytics low-success guidance that points to recovery notes', () => {
    const cwd = fixture({
      'src/app/features/analytics/AnalyticsDashboard.tsx': `
function nextStep() {
  return 'Review Command line recovery first. Open recent task results, review the recovery notes, then pause new work until the next step is clear.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags analytics chart labels that expose event wording', () => {
    const cwd = fixture({
      'src/app/features/analytics/AnalyticsDashboard.tsx': `
function ActivityBarChart() {
  return <div aria-label="Hourly event activity">{activeBar.value} events<span>{activePct}% of window</span><button aria-label={\`\${bar.label}: \${bar.value} events\`} /></div>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'analytics-event-label-copy',
          sample: expect.stringContaining('Hourly event activity'),
        }),
        expect.objectContaining({
          type: 'analytics-event-label-copy',
          sample: expect.stringContaining('events'),
        }),
        expect.objectContaining({
          type: 'analytics-event-label-copy',
          sample: expect.stringContaining('of window'),
        }),
      ])
    )
  })

  it('accepts analytics chart labels that describe work updates', () => {
    const cwd = fixture({
      'src/app/features/analytics/AnalyticsDashboard.tsx': `
function ActivityBarChart() {
  return <div aria-label="Hourly work updates">{activeBar.value} updates<span>{activePct}% of shown hours</span><button aria-label={\`\${bar.label}: \${bar.value} updates\`} /></div>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags suggested saved-item preview copy that stops at no preview', () => {
    const cwd = fixture({
      'src/app/features/detail/ContextCandidatesList.tsx': `
function candidatePreview() {
  return 'No preview yet. Open saved item review to read the full suggestion.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'context-candidate-preview-copy',
          location: 'src/app/features/detail/ContextCandidatesList.tsx:3',
        }),
      ])
    )
  })

  it('flags chat tool step fallback copy that does not tell users how to use the result', () => {
    const cwd = fixture({
      'src/app/features/chat/ToolCallDetail.tsx': `
function toolDataSummary(data) {
  return data.ok ? 'This step finished successfully.' : 'This step needs review.'
}

function emptyResult() {
  return 'This step has not reported a result yet.'
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
        expect.objectContaining({
          type: 'chat-tool-step-copy',
          location: 'src/app/features/chat/ToolCallDetail.tsx:11',
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

  it('flags vague needs-review copy in user-visible recovery messages', () => {
    const cwd = fixture({
      'src/app/hooks/useWsDispatch.ts': `
export function safeCompletionMessage() {
  return 'Finished with a summary that needs review. Open the task details before using the result.'
}
`,
      'src/app/features/context/ApprovalQueueView.tsx': `
export function approvalQueueEmptyState() {
  return 'Clear filters before assuming nothing needs review.'
}
`,
      'src/app/features/settings/AccountSection.tsx': `
	export function AccountSection() {
	  return <span>Start is back in the left menu. Open it when setup needs review.</span>
	}
	`,
      'src/app/features/chat/ChatView.tsx': `
	export function attentionEmptyCopy() {
	  return 'This conversation is needing review.'
	}
	`,
      'src/app/entities/context/ui/FeedbackControls.tsx': `
	export function feedbackConfirmation() {
	  return 'future tasks will treat this item as needing review.'
	}
	`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toHaveLength(5)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'vague-needs-review-copy',
          location: 'src/app/hooks/useWsDispatch.ts:3',
        }),
        expect.objectContaining({
          type: 'vague-needs-review-copy',
          location: 'src/app/features/context/ApprovalQueueView.tsx:3',
        }),
        expect.objectContaining({
          type: 'vague-needs-review-copy',
          location: 'src/app/features/settings/AccountSection.tsx:3',
        }),
        expect.objectContaining({
          type: 'vague-needs-review-copy',
          location: 'src/app/features/chat/ChatView.tsx:3',
        }),
        expect.objectContaining({
          type: 'vague-needs-review-copy',
          location: 'src/app/entities/context/ui/FeedbackControls.tsx:3',
        }),
      ])
    )
  })

  it('accepts needs-review replacements that tell beginners what to check', () => {
    const cwd = fixture({
      'src/app/hooks/useWsDispatch.ts': `
export function safeCompletionMessage() {
  return 'Finished with a summary you should check. Open the task details before using the result.'
}
`,
      'src/app/features/context/ApprovalQueueView.tsx': `
export function approvalQueueEmptyState() {
  return 'Clear filters before assuming there is nothing to check.'
}
`,
      'src/app/features/settings/AccountSection.tsx': `
export function AccountSection() {
  return <span>Start is back in the left menu. Open the setup checklist whenever you want to check setup again.</span>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags vague needs-attention titles in user-visible messages', () => {
    const cwd = fixture({
      'src/app/features/analytics/AnalyticsDashboard.tsx': `
export function AnalyticsDashboard() {
  return <p>Analytics needs attention</p>
}
`,
      'src/app/features/chat/ChatView.tsx': `
export function ChatView() {
  return <span>Conversation needs attention</span>
}
`,
      'src/app/features/agents/AgentTasksTab.tsx': `
export function AgentTasksTab() {
  return <p>This agent's work list needs attention.</p>
}
`,
      'src/app/features/settings/RuntimeSection.tsx': `
export function RuntimeSection() {
  return <h3>Agent work setup needs attention</h3>
}
`,
      'src/app/features/settings/ProvidersSection.tsx': `
export function ProvidersSection() {
  return <h3>AI service setup needs attention</h3>
}
`,
      'src/app/features/admin/CliImagesPanel.tsx': `
export function stateLabel() {
  return 'Check needs attention'
}
`,
      'src/app/features/detail/ContextEvidenceList.tsx': `
export function payloadSummary() {
  return 'The recorded result needs attention.'
}
`,
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  common: {
    error: 'Something needs attention. Review the message, then try again.',
  },
}
`,
      'src/app/shared/lib/taskFailureCopy.ts': `
export function taskFailurePreview() {
  return 'Stopped because sign-in or service access needs attention.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toHaveLength(9)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'vague-needs-attention-copy',
          location: 'src/app/features/analytics/AnalyticsDashboard.tsx:3',
        }),
        expect.objectContaining({
          type: 'vague-needs-attention-copy',
          location: 'src/app/features/chat/ChatView.tsx:3',
        }),
        expect.objectContaining({
          type: 'vague-needs-attention-copy',
          location: 'src/app/features/agents/AgentTasksTab.tsx:3',
        }),
        expect.objectContaining({
          type: 'vague-needs-attention-copy',
          location: 'src/app/features/settings/RuntimeSection.tsx:3',
        }),
        expect.objectContaining({
          type: 'vague-needs-attention-copy',
          location: 'src/app/features/settings/ProvidersSection.tsx:3',
        }),
        expect.objectContaining({
          type: 'vague-needs-attention-copy',
          location: 'src/app/features/admin/CliImagesPanel.tsx:3',
        }),
        expect.objectContaining({
          type: 'vague-needs-attention-copy',
          location: 'src/app/features/detail/ContextEvidenceList.tsx:3',
        }),
        expect.objectContaining({
          type: 'vague-needs-attention-copy',
          location: 'src/app/shared/i18n/locales/en.ts:4',
        }),
        expect.objectContaining({
          type: 'vague-needs-attention-copy',
          location: 'src/app/shared/lib/taskFailureCopy.ts:3',
        }),
      ])
    )
  })

  it('accepts needs-attention replacements that tell beginners the next action', () => {
    const cwd = fixture({
      'src/app/features/analytics/AnalyticsDashboard.tsx': `
export function AnalyticsDashboard() {
  return <p>Refresh analytics data</p>
}
`,
      'src/app/features/chat/ChatView.tsx': `
export function ChatView() {
  return <span>Check this conversation</span>
}
`,
      'src/app/features/agents/AgentTasksTab.tsx': `
export function AgentTasksTab() {
  return <p>Refresh this agent's work list.</p>
}
`,
      'src/app/features/settings/RuntimeSection.tsx': `
export function RuntimeSection() {
  return <h3>Finish agent work setup</h3>
}
`,
      'src/app/features/settings/ProvidersSection.tsx': `
export function ProvidersSection() {
  return <h3>Finish AI service setup</h3>
}
`,
      'src/app/features/admin/CliImagesPanel.tsx': `
export function stateLabel() {
  return 'Choose Check now'
}
`,
      'src/app/features/detail/ContextEvidenceList.tsx': `
export function payloadSummary() {
  return 'Check the recorded result before reusing it.'
}
`,
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  common: {
    error: 'Check the message, then try again.',
  },
}
`,
      'src/app/shared/lib/taskFailureCopy.ts': `
export function taskFailurePreview() {
  return 'Reconnect sign-in or service access, then retry.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags remaining needs-attention copy in setup and progress surfaces', () => {
    const cwd = fixture({
      'src/app/features/chat/ToolCallDetail.tsx': `
function toolDataSummary(issue) {
  return \`Needs attention: \${issue}\`
}
`,
      'src/app/widgets/views/TimelineView.tsx': `
export function TimelineView() {
  return <p>Open a task when the timeline shows something that needs attention</p>
}
`,
      'src/app/features/settings/RuntimeSection.tsx': `
function toolVersion(detail) {
  return detail.version ?? 'Needs attention'
}
function versionSourceLabel(imagePresent) {
  return imagePresent ? 'ready' : 'needs attention'
}
`,
      'src/app/features/settings/providerTestErrorMessage.ts': `
function providerTestErrorMessage() {
  return 'Try checking OpenAI Production again in a few minutes. If it still needs attention, ask an owner or admin to check AI service settings.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'vague-needs-attention-copy',
          location: 'src/app/features/chat/ToolCallDetail.tsx:3',
        }),
        expect.objectContaining({
          type: 'vague-needs-attention-copy',
          location: 'src/app/widgets/views/TimelineView.tsx:3',
        }),
        expect.objectContaining({
          type: 'vague-needs-attention-copy',
          location: 'src/app/features/settings/RuntimeSection.tsx:3',
        }),
        expect.objectContaining({
          type: 'vague-needs-attention-copy',
          location: 'src/app/features/settings/RuntimeSection.tsx:6',
        }),
        expect.objectContaining({
          type: 'vague-needs-attention-copy',
          location: 'src/app/features/settings/providerTestErrorMessage.ts:3',
        }),
      ])
    )
  })

  it('flags tool and saved-item problem copy that exposes technical-problem jargon', () => {
    const cwd = fixture({
      'src/app/features/chat/ToolCallDetail.tsx': `
const TECHNICAL_PROBLEM_MESSAGE =
  'This step reported a technical problem. Ask the agent to explain it in plain language, then retry if the task still matters.'
`,
      'src/app/features/detail/ContextEvidenceList.tsx': `
const TECHNICAL_EVIDENCE_MESSAGE =
  'This record reported a technical problem. Ask the agent to explain it in plain language, then retry if the task still matters.'
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'technical-problem-copy',
          location: 'src/app/features/chat/ToolCallDetail.tsx:3',
        }),
        expect.objectContaining({
          type: 'technical-problem-copy',
          location: 'src/app/features/detail/ContextEvidenceList.tsx:3',
        }),
      ])
    )
  })

  it('accepts tool and saved-item problem copy that tells users what happened next', () => {
    const cwd = fixture({
      'src/app/features/chat/ToolCallDetail.tsx': `
const PROBLEM_MESSAGE =
  'This step hit a problem. Ask the agent to explain what happened, then retry if the task still matters.'
`,
      'src/app/features/detail/ContextEvidenceList.tsx': `
const PROBLEM_MESSAGE =
  'This record hit a problem. Ask the agent to explain what happened, then retry if the task still matters.'
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags saved detail toggles that expose full-record jargon', () => {
    const cwd = fixture({
      'src/app/features/detail/ContextEvidenceList.tsx': `
export function ContextEvidenceList() {
  return <details><summary>Show full record</summary><p>Open the full record only when checking an unexpected result.</p><p>Full record details were saved but could not be shown safely.</p></details>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'context-evidence-full-record-copy',
          location: 'src/app/features/detail/ContextEvidenceList.tsx:3',
        }),
      ])
    )
  })

  it('accepts saved detail toggles that use beginner-facing wording', () => {
    const cwd = fixture({
      'src/app/features/detail/ContextEvidenceList.tsx': `
export function ContextEvidenceList() {
  return <details><summary>Show saved details</summary><p>Open saved details only when checking an unexpected result.</p></details>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags chat offline copy that tells users to start an agent without a setup path', () => {
    const cwd = fixture({
      'src/app/features/chat/ChatView.tsx': `
function ChatView() {
  return 'This agent is offline. Start it before sending a message.'
}
`,
      'src/app/features/chat/ChatComposer.tsx': `
function ChatComposer() {
  return 'Start it before sending a message'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'chat-offline-copy',
          location: 'src/app/features/chat/ChatView.tsx:3',
        }),
        expect.objectContaining({
          type: 'chat-offline-copy',
          location: 'src/app/features/chat/ChatComposer.tsx:3',
        }),
      ])
    )
  })

  it('accepts chat offline copy that points to the right setup area', () => {
    const cwd = fixture({
      'src/app/features/chat/ChatView.tsx': `
function ChatView() {
  return 'Open AI service settings, choose Check connection, then refresh Agents before sending a message.'
}
`,
      'src/app/features/chat/ChatComposer.tsx': `
function ChatComposer() {
  return 'This agent is not ready. Open Agents, start or reconnect it, then return here when it shows Ready.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags chat stream errors that start with the failure before the resend action', () => {
    const cwd = fixture({
      'src/app/features/chat/useChatStream.ts': `
function chatStreamEventErrorMessage() {
  return 'The agent could not finish this reply. Resend the message. If it still fails, ask an owner or admin to check chat setup.'
}
function chatStreamHttpErrorMessage() {
  return 'This message was not sent. Refresh this agent, then resend the message.'
}
function chatStreamConflictMessage() {
  return 'This agent is already working. Wait for the current reply to finish, then resend the message.'
}
function chatStreamReadErrorMessage() {
  return 'The reply stopped before it finished. Check that the agent is still online, then resend the message.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'chat-stream-error-copy',
          location: 'src/app/features/chat/useChatStream.ts:3',
        }),
        expect.objectContaining({
          type: 'chat-stream-error-copy',
          location: 'src/app/features/chat/useChatStream.ts:6',
        }),
        expect.objectContaining({
          type: 'chat-stream-error-copy',
          location: 'src/app/features/chat/useChatStream.ts:9',
        }),
        expect.objectContaining({
          type: 'chat-stream-error-copy',
          location: 'src/app/features/chat/useChatStream.ts:12',
        }),
      ])
    )
  })

  it('accepts chat stream errors that start with the resend action', () => {
    const cwd = fixture({
      'src/app/features/chat/useChatStream.ts': `
function chatStreamEventErrorMessage() {
  return 'Resend the message. The agent could not finish this reply. If it still fails, ask an owner or admin to check chat setup.'
}
function chatStreamHttpErrorMessage() {
  return 'Refresh this agent, then resend the message. This message was not sent.'
}
function chatStreamConflictMessage() {
  return 'Wait for the current reply to finish, then resend the message. This agent is already working.'
}
function chatStreamReadErrorMessage() {
  return 'Check that the agent is still online, then resend the message. The reply stopped before it finished.'
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

  it('flags app health live update copy that exposes event wording', () => {
    const cwd = fixture({
      'src/app/features/admin/SystemHealth.tsx': `
const SERVICE_DEFINITIONS = [
  { description: 'Moves events from running agents into the browser in near real time.' },
]
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'system-health-live-update-copy',
        location: 'src/app/features/admin/SystemHealth.tsx:3',
      }),
    ])
  })

  it('accepts app health live update copy that describes visible progress', () => {
    const cwd = fixture({
      'src/app/features/admin/SystemHealth.tsx': `
const SERVICE_DEFINITIONS = [
  { description: 'Shows progress from running agents in the browser in near real time.' },
]
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags app health error copy that starts with the failure summary', () => {
    const cwd = fixture({
      'src/app/features/admin/systemHealthErrorMessage.ts': `
export function systemHealthErrorMessage() {
  const base = 'Forge could not check app health.'
  return \`\${base} Refresh Admin, then choose Check now.\`
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'system-health-error-copy',
          location: 'src/app/features/admin/systemHealthErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'system-health-error-copy',
          location: 'src/app/features/admin/systemHealthErrorMessage.ts:4',
        }),
      ])
    )
  })

  it('accepts app health error copy that starts with the next step', () => {
    const cwd = fixture({
      'src/app/features/admin/systemHealthErrorMessage.ts': `
export function systemHealthErrorMessage() {
  return 'Refresh Admin, then choose Check now. Forge could not check app health.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags saved instruction source labels that still say workspace', () => {
    const cwd = fixture({
      'src/app/features/skills/SkillCard.tsx': `
export function SkillCard() {
  return 'Saved in Workspace saved instructions by Platform team'
}
`,
      'src/app/features/skills/SkillDetailModal.tsx': `
export function SkillDetailModal() {
  return 'Workspace saved instructions'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'saved-instruction-source-label-copy',
          location: 'src/app/features/skills/SkillCard.tsx:3',
        }),
        expect.objectContaining({
          type: 'saved-instruction-source-label-copy',
          location: 'src/app/features/skills/SkillDetailModal.tsx:3',
        }),
      ])
    )
  })

  it('accepts saved instruction source labels that say team space', () => {
    const cwd = fixture({
      'src/app/features/skills/SkillCard.tsx': `
export function SkillCard() {
  return 'Saved in Team space saved instructions by Platform team'
}
`,
      'src/app/features/skills/SkillDetailModal.tsx': `
export function SkillDetailModal() {
  return 'Team space saved instructions'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags saved instruction publishing copy that still says workspace', () => {
    const cwd = fixture({
      'src/app/features/detail/SkillDraftModal.tsx': `
export function SkillDraftModal() {
  return 'Review what should repeat before saving it for the workspace.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'saved-instruction-workspace-copy',
          location: 'src/app/features/detail/SkillDraftModal.tsx:3',
        }),
      ])
    )
  })

  it('accepts saved instruction publishing copy that says team space', () => {
    const cwd = fixture({
      'src/app/features/detail/SkillDraftModal.tsx': `
export function SkillDraftModal() {
  return 'Review what should repeat before saving it for your team space.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags saved instruction draft copy that does not tell beginners the next action', () => {
    const cwd = fixture({
      'src/app/features/detail/SkillDraftModal.tsx': `
export function SkillDraftModal() {
  const error = 'Keep or rewrite the reusable instructions before publishing.'
  return 'Review the reusable instructions.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'saved-instruction-draft-copy',
          location: 'src/app/features/detail/SkillDraftModal.tsx:3',
        }),
        expect.objectContaining({
          type: 'saved-instruction-draft-copy',
          location: 'src/app/features/detail/SkillDraftModal.tsx:4',
        }),
      ])
    )
  })

  it('accepts saved instruction draft copy that names the field and review step', () => {
    const cwd = fixture({
      'src/app/features/detail/SkillDraftModal.tsx': `
export function SkillDraftModal() {
  const error = 'Add the repeatable steps, or keep the suggested steps, before publishing.'
  return 'Find this instruction, then review the reusable steps before agents use them.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags saved instruction availability labels that still say workspace', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  skills: { detail: { availabilityWorkspace: 'This workspace' } }
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  skills: { detail: { availabilityWorkspace: '当前工作区' } }
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'saved-instruction-availability-copy',
          location: 'src/app/shared/i18n/locales/en.ts:3',
        }),
        expect.objectContaining({
          type: 'saved-instruction-availability-copy',
          location: 'src/app/shared/i18n/locales/zh.ts:3',
        }),
      ])
    )
  })

  it('accepts saved instruction availability labels that say team space', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  skills: { detail: { availabilityWorkspace: 'This team space' } }
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  skills: { detail: { availabilityWorkspace: '当前团队空间' } }
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags vague app health attention copy', () => {
    const cwd = fixture({
      'src/app/features/admin/SystemHealth.tsx': `
function StatusBadge({ status }) {
  return status === 'degraded' ? 'Needs attention' : 'Ready'
}

function OverallBanner() {
  return 'Some areas need attention'
}

function SystemHealth() {
  return 'Start with anything marked Fix first, then items marked Needs attention.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'system-health-status-copy',
          sample: expect.stringContaining('Needs attention'),
        }),
        expect.objectContaining({
          type: 'system-health-status-copy',
          sample: expect.stringContaining('Some areas need attention'),
        }),
        expect.objectContaining({
          type: 'system-health-status-copy',
          sample: expect.stringContaining('items marked Needs attention'),
        }),
      ])
    )
  })

  it('flags saved-data health copy that exposes run and evidence jargon', () => {
    const cwd = fixture({
      'src/app/features/admin/SystemHealth.tsx': `
const SERVICE_DEFINITIONS = [
  {
    key: 'database',
    description: 'Keeps accounts, tasks, runs, evidence, and settings available.',
  },
]
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'system-health-status-copy',
        sample: expect.stringContaining('runs, evidence'),
      }),
    ])
  })

  it('flags app health helper text that exposes page visibility mechanics', () => {
    const cwd = fixture({
      'src/app/features/admin/SystemHealth.tsx': `
function SystemHealth() {
  return (
    <section>
      <p>Checks when opened, then every 30 seconds while this page is visible. Hidden tabs pause checks.</p>
      <button>{loading ? 'Checking...' : 'Check now'}</button>
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
          type: 'system-health-status-copy',
          sample: expect.stringContaining('while this page is visible'),
        }),
        expect.objectContaining({
          type: 'system-health-status-copy',
          sample: expect.stringContaining('Checking...'),
        }),
      ])
    )
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
    ? 'Review the fields, then create the instruction again.'
    : 'Forge could not load Saved instructions right now. Refresh Saved instructions, then try again.'
}
function skillAccessErrorMessage() {
  return 'You do not have access to saved instructions for this team space. Ask an owner or admin to update your team space access.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toHaveLength(3)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'saved-instructions-load-copy',
          location: 'src/app/features/skills/SkillsView.tsx:3',
        }),
        expect.objectContaining({
          type: 'saved-instructions-load-copy',
          location: 'src/app/shared/model/skills.store.ts:5',
        }),
        expect.objectContaining({
          type: 'saved-instructions-load-copy',
          location: 'src/app/shared/model/skills.store.ts:8',
        }),
      ])
    )
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
    ? 'Review the fields, then create the instruction again.'
    : 'Refresh Saved instructions to load the list.'
}
function skillAccessErrorMessage() {
  return 'Ask an owner or admin to update your team space access, then refresh Saved instructions. You do not have access to saved instructions for this team space.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags saved instruction creation errors that start with the failure instead of the next step', () => {
    const cwd = fixture({
      'src/app/features/skills/model/createSkillErrorMessage.ts': `
function network() {
  return 'Forge could not connect while creating this instruction. Check your connection, then try again.'
}
function permission() {
  return 'You do not have permission to create workspace instructions. Ask an owner or admin to let you create saved instructions.'
}
function missingRoute() {
  return 'Saved instructions could not be opened from this page. Refresh Saved instructions, then try again.'
}
function conflict() {
  return 'An instruction with this name or trigger may already exist. Review the existing instructions, then try again.'
}
`,
      'src/app/features/detail/model/skillDraftErrorMessage.ts': `
function publish() {
  return 'Instruction was not published. Review the draft and try again.'
}
function network() {
  return 'Forge could not connect while publishing this instruction. Check your connection, then publish again.'
}
function service() {
  return 'Forge could not publish this instruction right now. Wait a few minutes, then publish again.'
}
`,
      'src/app/shared/model/skills.store.ts': `
function busy() {
  return 'Instruction setup is busy. Wait a moment, then create the instruction.'
}
function service() {
  return 'Forge could not create the instruction right now. Refresh Saved instructions, then try again.'
}
function fallback() {
  return 'The instruction could not be created. Review the fields and try again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'saved-instruction-create-copy',
          location: 'src/app/features/skills/model/createSkillErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'saved-instruction-create-copy',
          location: 'src/app/features/skills/model/createSkillErrorMessage.ts:6',
        }),
        expect.objectContaining({
          type: 'saved-instruction-create-copy',
          location: 'src/app/features/skills/model/createSkillErrorMessage.ts:9',
        }),
        expect.objectContaining({
          type: 'saved-instruction-create-copy',
          location: 'src/app/features/skills/model/createSkillErrorMessage.ts:12',
        }),
        expect.objectContaining({
          type: 'saved-instruction-create-copy',
          location: 'src/app/features/detail/model/skillDraftErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'saved-instruction-create-copy',
          location: 'src/app/features/detail/model/skillDraftErrorMessage.ts:6',
        }),
        expect.objectContaining({
          type: 'saved-instruction-create-copy',
          location: 'src/app/features/detail/model/skillDraftErrorMessage.ts:9',
        }),
        expect.objectContaining({
          type: 'saved-instruction-create-copy',
          location: 'src/app/shared/model/skills.store.ts:3',
        }),
        expect.objectContaining({
          type: 'saved-instruction-create-copy',
          location: 'src/app/shared/model/skills.store.ts:6',
        }),
        expect.objectContaining({
          type: 'saved-instruction-create-copy',
          location: 'src/app/shared/model/skills.store.ts:9',
        }),
      ])
    )
  })

  it('accepts saved instruction creation errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/features/skills/model/createSkillErrorMessage.ts': `
function network() {
  return 'Check your connection, then create the instruction again. Forge could not connect while creating it.'
}
function permission() {
  return 'Ask an owner or admin to let you create saved instructions. Your account cannot create workspace instructions yet.'
}
function service() {
  return 'Refresh Saved instructions, then create the instruction again. If it still fails, ask an owner or admin to check instruction setup.'
}
`,
      'src/app/features/detail/model/skillDraftErrorMessage.ts': `
function publish() {
  return 'Review the draft, then publish again. Instruction was not published.'
}
function network() {
  return 'Check your connection, then publish again. Forge could not connect while publishing this instruction.'
}
function service() {
  return 'Wait a few minutes, then publish again. Forge could not publish this instruction right now.'
}
`,
      'src/app/shared/model/skills.store.ts': `
function fallback() {
  return 'Review the fields, then create the instruction again.'
}
function busy() {
  return 'Wait a moment, then create the instruction again. Instruction setup is busy right now.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags saved instruction templates that expose PR and CI status jargon', () => {
    const cwd = fixture({
      'src/app/features/skills/CreateSkillModal.tsx': `
const skillTemplates = [{
  content: 'Check GitHub or GitLab once and summarize a recent PR or CI summary. Classify the result as ACTION, WAIT, or DONE. For ACTION, inspect only the failed check or job details. For WAIT, stop monitoring in chat and suggest a background monitor. For DONE, report final status.'
}]
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'saved-instruction-template-copy',
          location: 'src/app/features/skills/CreateSkillModal.tsx:3',
        }),
      ])
    )
  })

  it('flags saved instruction templates that tell users to link evidence', () => {
    const cwd = fixture({
      'src/app/features/skills/CreateSkillModal.tsx': `
const skillTemplates = [{
  content: 'Check what changed. Keep the answer short and link evidence when available.'
}]
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'saved-instruction-template-copy',
        sample: expect.stringContaining('link evidence'),
      }),
    ])
  })

  it('accepts saved instruction templates that use plain status result language', () => {
    const cwd = fixture({
      'src/app/features/skills/CreateSkillModal.tsx': `
const skillTemplates = [{
  content: 'Check the code review page once and summarize review result, merge readiness, and build result. Start with one plain result: Needs a fix, Waiting, or Done. For Needs a fix, open only the failed build or review item. For Waiting, stop checking in chat.'
}]
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags agent instruction templates that expose evidence or root-cause jargon', () => {
    const cwd = fixture({
      'src/app/features/agents/CreateAgentModal.tsx': `
const AGENT_ROLE_TEMPLATES = [{
  systemPrompt: 'You investigate unclear failures by gathering evidence first.'
}]
`,
      'src/app/features/agents/AgentConfigTab.tsx': `
const PROMPT_TEMPLATES = [{
  value: 'Separate symptoms from root cause and leave a next action when more evidence is needed.'
}]
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'agent-instruction-template-copy',
          sample: expect.stringContaining('gathering evidence first'),
        }),
        expect.objectContaining({
          type: 'agent-instruction-template-copy',
          sample: expect.stringContaining('root cause'),
        }),
      ])
    )
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

  it('flags outside tool access value copy that hides the one-time save action behind key jargon', () => {
    const cwd = fixture({
      'src/app/features/settings/KeysSection.tsx': `
function NewKeyBanner() {
  return (
    <section>
      <p>This is the only time the full key is shown. Copy it into a password manager before choosing I saved it.</p>
      <button>Copy key</button>
      <p>Forge cannot copy from this browser. Select the key text, then copy it manually before choosing I saved it.</p>
      <th>Key preview</th>
    </section>
  )
}
const ACCESS_KEY_EMPTY_STEPS = ['Copy the new key into a password manager before closing this message.']
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'access-key-secret-value-copy',
          sample: expect.stringContaining('full key'),
        }),
        expect.objectContaining({
          type: 'access-key-secret-value-copy',
          sample: expect.stringContaining('Copy key'),
        }),
        expect.objectContaining({
          type: 'access-key-secret-value-copy',
          sample: expect.stringContaining('Select the key text'),
        }),
        expect.objectContaining({
          type: 'access-key-secret-value-copy',
          sample: expect.stringContaining('Key preview'),
        }),
        expect.objectContaining({
          type: 'access-key-secret-value-copy',
          sample: expect.stringContaining('Copy the new key'),
        }),
      ])
    )
  })

  it('accepts outside tool access value copy that tells beginners what to save', () => {
    const cwd = fixture({
      'src/app/features/settings/KeysSection.tsx': `
function NewKeyBanner() {
  return (
    <section>
      <p>This full access value is shown only once. Save it in a password manager before choosing I saved this value.</p>
      <button>Copy access value</button>
      <p>Forge cannot copy from this browser. Select the access value text, then copy it manually before choosing I saved this value.</p>
      <th>Saved key starts with</th>
    </section>
  )
}
const ACCESS_KEY_EMPTY_STEPS = ['Save the new access value in a password manager before closing this message.']
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags code access setup copy that starts with a vague key', () => {
    const cwd = fixture({
      'src/app/features/settings/GitCredentialsSection.tsx': `
function AddCredentialForm() {
  return <p>Paste the key from GitHub or GitLab. Those sites may call it a personal access token.</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'code-access-key-copy',
        location: 'src/app/features/settings/GitCredentialsSection.tsx:3',
      }),
    ])
  })

  it('accepts code access setup copy that names the key before provider token wording', () => {
    const cwd = fixture({
      'src/app/features/settings/GitCredentialsSection.tsx': `
function AddCredentialForm() {
  return <p>Paste the code access key from GitHub or GitLab. If that page says personal access token, use that value here.</p>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags code access address copy that uses cloud or Git address jargon', () => {
    const cwd = fixture({
      'src/app/features/settings/GitCredentialsSection.tsx': `
const GIT_CREDENTIAL_SETUP_STEPS = [
  { label: 'Leave address blank for cloud', value: 'Only enter an address when your company hosts its own GitHub or GitLab.' },
]
function CredentialRow() {
  return <span>Default cloud address</span>
}
function AddCredentialForm() {
  return <><label>Git service</label><p>For a company-hosted Git service, enter the address.</p><label>GitHub or GitLab address</label></>
}
const tableHeaders = [{ label: 'Git address' }]
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'code-access-address-copy',
          sample: expect.stringContaining('Leave address blank for cloud'),
        }),
        expect.objectContaining({
          type: 'code-access-address-copy',
          sample: expect.stringContaining('Only enter an address'),
        }),
        expect.objectContaining({
          type: 'code-access-address-copy',
          sample: expect.stringContaining('Default cloud address'),
        }),
        expect.objectContaining({
          type: 'code-access-address-copy',
          sample: expect.stringContaining('Git service'),
        }),
        expect.objectContaining({
          type: 'code-access-address-copy',
          sample: expect.stringContaining('company-hosted Git service'),
        }),
        expect.objectContaining({
          type: 'code-access-address-copy',
          sample: expect.stringContaining('GitHub or GitLab address'),
        }),
        expect.objectContaining({
          type: 'code-access-address-copy',
          sample: expect.stringContaining('Git address'),
        }),
      ])
    )
  })

  it('accepts code access address copy that tells beginners when to leave it empty', () => {
    const cwd = fixture({
      'src/app/features/settings/GitCredentialsSection.tsx': `
const GIT_CREDENTIAL_SETUP_STEPS = [
  { label: 'Use the normal website by default', value: 'Leave the website address empty for github.com or gitlab.com. Add one only if your company uses its own GitHub or GitLab website.' },
]
function CredentialRow({ provider }) {
  return <span>{provider === 'github' ? 'github.com' : 'gitlab.com'}</span>
}
function AddCredentialForm() {
  return <label>Company GitHub or GitLab website</label>
}
const tableHeaders = [{ label: 'Code website' }, { label: 'Website address' }]
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags HTTPS code access setup copy that falls back to repository wording', () => {
    const cwd = fixture({
      'src/app/features/settings/GitCredentialsSection.tsx': `
function AddCredentialForm() {
  return <p>Choose the site that owns the repository.</p>
}
function savedMessage() {
  return 'Code access saved. Create a small task with a private repository link to confirm agents can open it. If it cannot read the repository, come back here and replace this key.'
}
function EmptyState() {
  return <p>Use this for links such as https://github.com/team/repo.git.</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'code-access-repository-copy',
          sample: expect.stringContaining('owns the repository'),
        }),
        expect.objectContaining({
          type: 'code-access-repository-copy',
          sample: expect.stringContaining('private repository link'),
        }),
        expect.objectContaining({
          type: 'code-access-repository-copy',
          sample: expect.stringContaining('team/repo.git'),
        }),
      ])
    )
  })

  it('accepts HTTPS code access setup copy that talks about code links', () => {
    const cwd = fixture({
      'src/app/features/settings/GitCredentialsSection.tsx': `
function AddCredentialForm() {
  return <p>Choose where this code lives.</p>
}
function savedMessage() {
  return 'Code access saved. Create a small task with a private code link to confirm agents can open it. If it cannot open the code, come back here and replace this key.'
}
function EmptyState() {
  return <p>Use this for links such as https://github.com/team/project.git.</p>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags SSH code access setup copy that uses public-line and key-type jargon', () => {
    const cwd = fixture({
      'src/app/features/settings/SshKeysSection.tsx': `
function describeKeyType(keyType) {
  if (keyType === 'ssh-ed25519') return 'Modern key type'
  if (keyType === 'ssh-rsa') return 'RSA key type'
}
const SSH_KEY_SETUP_STEPS = [
  { label: 'Paste the public line', value: 'Copy only the one-line .pub key that starts with ssh-ed25519 or ssh-rsa.' },
]
function AddSshKeyForm() {
  return <label>Public key line</label>
}
function validation() {
  return 'Paste the public key line before saving.'
}
function savedMessage() {
  return 'SSH code access saved. Create a small task with a git@ code link to confirm agents can open it. If it cannot read the repository, come back here and replace this key.'
}
const tableHeaders = [{ label: 'Safety check' }, { label: 'Key type' }]
`,
      'src/app/features/settings/sshKeysErrorMessage.ts': `
export function sshKeysErrorMessage() {
  return 'Paste the public key line that starts with ssh-ed25519 or ssh-rsa, then save again.'
}
export function duplicateMessage() {
  return 'Choose the saved access or remove the old one first. This public key line is already saved.'
}
export function requiredMessage() {
  return 'Check the access name and public key line, then try again.'
}
`,
      'src/app/shared/model/settings.store.ts': `
export function settingsActionErrorMessage() {
  return 'Add a name for this access, paste the public key line, then save again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'ssh-code-access-jargon-copy',
          sample: expect.stringContaining('Modern key type'),
        }),
        expect.objectContaining({
          type: 'ssh-code-access-jargon-copy',
          sample: expect.stringContaining('RSA key type'),
        }),
        expect.objectContaining({
          type: 'ssh-code-access-jargon-copy',
          sample: expect.stringContaining('Paste the public line'),
        }),
        expect.objectContaining({
          type: 'ssh-code-access-jargon-copy',
          sample: expect.stringContaining('one-line .pub key'),
        }),
        expect.objectContaining({
          type: 'ssh-code-access-jargon-copy',
          sample: expect.stringContaining('Public key line'),
        }),
        expect.objectContaining({
          type: 'ssh-code-access-jargon-copy',
          sample: expect.stringContaining('Paste the public key line'),
        }),
        expect.objectContaining({
          type: 'ssh-code-access-jargon-copy',
          sample: expect.stringContaining('Safety check'),
        }),
        expect.objectContaining({
          type: 'ssh-code-access-jargon-copy',
          sample: expect.stringContaining('Key type'),
        }),
        expect.objectContaining({
          type: 'ssh-code-access-jargon-copy',
          sample: expect.stringContaining('read the repository'),
        }),
        expect.objectContaining({
          type: 'ssh-code-access-jargon-copy',
          sample: expect.stringContaining('Paste the public key line that starts'),
        }),
        expect.objectContaining({
          type: 'ssh-code-access-jargon-copy',
          sample: expect.stringContaining('This public key line'),
        }),
        expect.objectContaining({
          type: 'ssh-code-access-jargon-copy',
          sample: expect.stringContaining('access name and public key line'),
        }),
        expect.objectContaining({
          type: 'ssh-code-access-jargon-copy',
          sample: expect.stringContaining('paste the public key line'),
        }),
      ])
    )
  })

  it('accepts SSH code access setup copy that explains the shareable public key line', () => {
    const cwd = fixture({
      'src/app/features/settings/SshKeysSection.tsx': `
function describeKeyType(keyType) {
  if (keyType === 'ssh-ed25519') return 'Recommended SSH key'
  if (keyType === 'ssh-rsa') return 'Older SSH key'
  return 'Ask an admin to check this SSH key'
}
const SSH_KEY_SETUP_STEPS = [
  { label: 'Paste the shareable public key line', value: 'Copy only the shareable one-line public key from the .pub file. It starts with ssh-ed25519 or ssh-rsa.' },
]
function AddSshKeyForm() {
  return <label>Shareable public key line</label>
}
function validation() {
  return 'Paste the shareable public key line before saving.'
}
function savedMessage() {
  return 'SSH code access saved. Create a small task with a git@ code link to confirm agents can open it. If agents cannot open the code, come back here and replace this key.'
}
const tableHeaders = [{ label: 'Saved key check code' }, { label: 'Accepted by Forge' }]
`,
      'src/app/features/settings/sshKeysErrorMessage.ts': `
export function sshKeysErrorMessage() {
  return 'Paste the shareable public key line that starts with ssh-ed25519 or ssh-rsa, then save again.'
}
export function duplicateMessage() {
  return 'Choose the saved access or remove the old one first. This shareable public key line is already saved.'
}
export function requiredMessage() {
  return 'Check the access name and shareable public key line, then try again.'
}
`,
      'src/app/shared/model/settings.store.ts': `
export function settingsActionErrorMessage() {
  return 'Add a name for this access, paste the shareable public key line, then save again.'
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

  it('flags Start guide reset and onboarding copy that hide the visible result', () => {
    const cwd = fixture({
      'src/app/features/settings/AccountSection.tsx': `
function GettingStartedGuideRow() {
  return <section><h3>Onboarding</h3><p>Start is already visible in the left menu, so there is nothing to restore.</p><button>Reset Start guide</button></section>
}
`,
      'src/app/pages/settings/ui/SettingsLayout.tsx': `
export const item = {
  description: 'Update profile, password, and the Start guide reset.',
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'start-guide-reset-copy',
          location: 'src/app/features/settings/AccountSection.tsx:3',
        }),
        expect.objectContaining({
          type: 'start-guide-reset-copy',
          location: 'src/app/pages/settings/ui/SettingsLayout.tsx:3',
        }),
      ])
    )
  })

  it('accepts setup checklist restore copy that explains the visible result', () => {
    const cwd = fixture({
      'src/app/features/settings/AccountSection.tsx': `
function GettingStartedGuideRow() {
  return <button>Show setup checklist</button>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags Start and setup checklist errors that start with the failure', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  gettingStarted: {
    skipError: 'Start could not be hidden. Check your connection, then try Skip again.',
  },
}
`,
      'src/app/features/settings/AccountSection.tsx': `
function GettingStartedGuideRow() {
  return 'The setup checklist could not be shown. Check your connection, then try again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'start-guide-error-copy',
          location: 'src/app/shared/i18n/locales/en.ts:4',
        }),
        expect.objectContaining({
          type: 'start-guide-error-copy',
          location: 'src/app/features/settings/AccountSection.tsx:3',
        }),
      ])
    )
  })

  it('accepts setup checklist errors that start with the next action', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  gettingStarted: {
    skipError: 'Check your connection, then choose Skip again. The setup checklist could not be hidden.',
  },
}
`,
      'src/app/features/settings/AccountSection.tsx': `
function GettingStartedGuideRow() {
  return 'Check your connection, then choose Show setup checklist again. The setup checklist could not be shown.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags Start navigation copy that sounds like a launch button', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  nav: { start: 'Start' },
  gettingStarted: {
    skipHint: 'This only hides Start from the sidebar. You can show Start again from Settings.',
    skipError: 'Check your connection, then choose Skip again. Start could not be hidden.',
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  nav: { start: '开始' },
  gettingStarted: {
    skipHint: '这只会隐藏侧栏里的 Start，也可以在设置里重新显示 Start。',
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'start-nav-copy',
          sample: expect.stringContaining("start: 'Start'"),
        }),
        expect.objectContaining({
          type: 'start-nav-copy',
          sample: expect.stringContaining('hides Start'),
        }),
        expect.objectContaining({
          type: 'start-nav-copy',
          sample: expect.stringContaining("start: '开始'"),
        }),
        expect.objectContaining({
          type: 'start-nav-copy',
          sample: expect.stringContaining('隐藏侧栏里的 Start'),
        }),
      ])
    )
  })

  it('accepts setup checklist navigation copy', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  nav: { start: 'Setup checklist' },
  gettingStarted: {
    skipHint: 'This only hides the setup checklist from the left menu. You can show it again from Settings.',
    skipError: 'Check your connection, then choose Skip again. The setup checklist could not be hidden.',
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  nav: { start: '设置清单' },
  gettingStarted: {
    skipHint: '这只会隐藏左侧菜单里的设置清单，也可以在设置里重新显示它。',
  },
}
`,
      'src/app/layouts/sidebar/SidebarNav.tsx': `
const item = { description: 'follow the setup checklist' }
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags sidebar layout wording in user-visible left-menu copy', () => {
    const cwd = fixture({
      'src/app/layouts/AppLayout.tsx': `
export function AppLayout() {
  return <button aria-label="Close sidebar" />
}
`,
      'src/app/layouts/sidebar/SidebarHeader.tsx': `
export function SidebarHeader() {
  return <button title="Expand sidebar">Open</button>
}
`,
      'src/app/layouts/sidebar/ProjectTree.tsx': `
function renameErrorMessage() {
  return 'Refresh the sidebar, then save this project name again.'
}
`,
      'src/app/features/manage-team/ui/EditableTeamRow.tsx': `
export function EditableTeamRow() {
  return <p>Projects in this team will also disappear from the sidebar.</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'left-menu-copy',
          location: 'src/app/layouts/AppLayout.tsx:3',
        }),
        expect.objectContaining({
          type: 'left-menu-copy',
          location: 'src/app/layouts/sidebar/SidebarHeader.tsx:3',
        }),
        expect.objectContaining({
          type: 'left-menu-copy',
          location: 'src/app/layouts/sidebar/ProjectTree.tsx:3',
        }),
        expect.objectContaining({
          type: 'left-menu-copy',
          location: 'src/app/features/manage-team/ui/EditableTeamRow.tsx:3',
        }),
      ])
    )
  })

  it('flags workspace setup wording in beginner-facing recovery copy', () => {
    const cwd = fixture({
      'src/app/routes/context.tsx': `
export const detail = 'Saved notes review is available for this workspace. Ask an owner to check workspace setup.'
`,
      'src/app/routes/context-audit.tsx': `
export const detail = 'Audit is enabled for this workspace. Ask an owner to check workspace setup.'
`,
      'src/app/features/settings/ResourcesSection.tsx': `
export function ResourcesSection() {
  return <p>Ask an owner or admin to add agent sizes in workspace settings.</p>
}
`,
      'src/app/layouts/sidebar/ProjectTree.tsx': `
function renameErrorMessage() {
  return 'Refresh the left menu, then save this project name again. If it still fails, ask an owner or admin to check workspace setup.'
}
`,
      'src/app/layouts/sidebar/SidebarNav.tsx': `
export const settings = { description: 'manage workspace, agents, and access' }
export const logout = { title: 'Logout: sign out of this workspace' }
`,
      'src/app/routes/__root.tsx': `
export function AuthShellLoadingState() {
  return 'We are confirming your session before opening the workspace.'
}
`,
      'src/app/shared/lib/taskFailureCopy.ts': `
export function taskBlockedPreview() {
  return 'The workspace is busy. Retry later or ask an owner for help.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'workspace-setup-copy',
          location: 'src/app/routes/context.tsx:2',
        }),
        expect.objectContaining({
          type: 'workspace-setup-copy',
          location: 'src/app/routes/context-audit.tsx:2',
        }),
        expect.objectContaining({
          type: 'workspace-setup-copy',
          location: 'src/app/features/settings/ResourcesSection.tsx:3',
        }),
        expect.objectContaining({
          type: 'workspace-setup-copy',
          location: 'src/app/layouts/sidebar/ProjectTree.tsx:3',
        }),
        expect.objectContaining({
          type: 'workspace-setup-copy',
          location: 'src/app/layouts/sidebar/SidebarNav.tsx:2',
        }),
        expect.objectContaining({
          type: 'workspace-setup-copy',
          location: 'src/app/layouts/sidebar/SidebarNav.tsx:3',
        }),
        expect.objectContaining({
          type: 'workspace-setup-copy',
          location: 'src/app/routes/__root.tsx:3',
        }),
        expect.objectContaining({
          type: 'workspace-setup-copy',
          location: 'src/app/shared/lib/taskFailureCopy.ts:3',
        }),
      ])
    )
  })

  it('accepts concrete setup wording for beginner-facing recovery copy', () => {
    const cwd = fixture({
      'src/app/routes/context.tsx': `
export const detail = 'Saved notes review is available here. Ask an owner to check saved items setup.'
`,
      'src/app/routes/context-audit.tsx': `
export const detail = 'Audit history is available here. Ask an owner to check audit setup.'
`,
      'src/app/features/settings/ResourcesSection.tsx': `
export function ResourcesSection() {
  return <p>Ask an owner or admin to add agent sizes in Work limits.</p>
}
`,
      'src/app/layouts/sidebar/ProjectTree.tsx': `
function renameErrorMessage() {
  return 'Refresh the left menu, then save this project name again. If it still fails, ask an owner or admin to check team and project setup.'
}
`,
      'src/app/layouts/sidebar/SidebarNav.tsx': `
export const settings = { description: 'manage teams, agents, and access' }
export const logout = { title: 'Logout: sign out of Forge' }
`,
      'src/app/routes/__root.tsx': `
export function AuthShellLoadingState() {
  return 'We are making sure you are signed in before opening your team space.'
}
`,
      'src/app/shared/lib/taskFailureCopy.ts': `
export function taskBlockedPreview() {
  return 'Too much work is running right now. Wait a bit, then retry or ask an owner for help.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags first-run guide copy that describes a path instead of the setup checklist', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  gettingStarted: {
    title: 'Start with one safe path',
    description: 'Follow one step at a time. Finish this path to create an agent.',
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  gettingStarted: {
    title: '先按一条安全路径跑通',
    description: '一次只做一步。先完成这条最小路径。',
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'start-guide-path-copy',
          sample: expect.stringContaining('Start with one safe path'),
        }),
        expect.objectContaining({
          type: 'start-guide-path-copy',
          sample: expect.stringContaining('安全路径'),
        }),
      ])
    )
  })

  it('accepts first-run guide copy that uses setup checklist wording', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  gettingStarted: {
    title: 'Set up your first agent safely',
    description: 'Follow one step at a time. Finish this checklist to create an agent.',
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  gettingStarted: {
    title: '按清单安全设置第一个 Agent',
    description: '一次只做一步。按这份设置清单创建 Agent。',
  },
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags bare 3D task view labels in the top bar', () => {
    const cwd = fixture({
      'src/app/layouts/TopBar.tsx': `
const VIEW_OPTIONS = [
  { id: '3d', label: '3D' },
]
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'task-view-label-copy',
        location: 'src/app/layouts/TopBar.tsx:3',
      }),
    ])
  })

  it('accepts map as the beginner-facing task view label', () => {
    const cwd = fixture({
      'src/app/layouts/TopBar.tsx': `
const VIEW_OPTIONS = [
  { id: '3d', label: 'Map' },
]
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags manual plus task labels in the top bar', () => {
    const cwd = fixture({
      'src/app/layouts/TopBar.tsx': `
export function TopBar() {
  return <button>+ Task</button>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'top-bar-create-task-copy',
        location: 'src/app/layouts/TopBar.tsx:3',
      }),
    ])
  })

  it('accepts a clear new task label in the top bar', () => {
    const cwd = fixture({
      'src/app/layouts/TopBar.tsx': `
export function TopBar() {
  return <button><span>New task</span></button>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags unclear command palette task action copy', () => {
    const cwd = fixture({
      'src/app/features/cmdk/CommandPalette.tsx': `
const ACTION_COMMANDS = [
  { id: 'action:create-task', label: 'Create task', description: 'Start a new piece of work.' },
]
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'command-palette-create-task-copy',
          location: 'src/app/features/cmdk/CommandPalette.tsx:3',
        }),
      ])
    )
  })

  it('accepts command palette task action copy that matches the top bar', () => {
    const cwd = fixture({
      'src/app/features/cmdk/CommandPalette.tsx': `
const ACTION_COMMANDS = [
  { id: 'action:create-task', label: 'New task', description: 'Create a task for an agent to finish.' },
]
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags project settings controls that use configuration wording', () => {
    const cwd = fixture({
      'src/app/layouts/sidebar/ProjectTree.tsx': `
function ProjectTree() {
  return <button aria-label="Close project configuration" />
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'project-settings-copy',
          location: 'src/app/layouts/sidebar/ProjectTree.tsx:3',
        }),
      ])
    )
  })

  it('accepts project settings controls that use plain settings wording', () => {
    const cwd = fixture({
      'src/app/layouts/sidebar/ProjectTree.tsx': `
function ProjectTree() {
  return <button aria-label="Close project settings" />
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags unclear project menu task action copy', () => {
    const cwd = fixture({
      'src/app/layouts/sidebar/ProjectTree.tsx': `
function ProjectTree() {
  return <ProjectMenuItem label="Create task here" detail="Start work in this project" />
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'project-menu-create-task-copy',
          location: 'src/app/layouts/sidebar/ProjectTree.tsx:3',
        }),
        expect.objectContaining({
          type: 'project-menu-create-task-copy',
          location: 'src/app/layouts/sidebar/ProjectTree.tsx:3',
        }),
      ])
    )
  })

  it('accepts project menu task action copy that explains the result', () => {
    const cwd = fixture({
      'src/app/layouts/sidebar/ProjectTree.tsx': `
function ProjectTree() {
  return <ProjectMenuItem label="New task for this project" detail="Open the task form with this project selected" />
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags unclear task form submit labels', () => {
    const cwd = fixture({
      'src/app/features/board/TaskFormModal.tsx': `
function TaskFormModal() {
  return <button>{selectingProject ? 'Preparing Project...' : confirmIncompleteBrief ? 'Create Anyway' : 'Create Task'}</button>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-form-submit-label-copy',
          location: 'src/app/features/board/TaskFormModal.tsx:3',
        }),
      ])
    )
  })

  it('accepts task form submit labels that keep the action explicit', () => {
    const cwd = fixture({
      'src/app/features/board/TaskFormModal.tsx': `
function TaskFormModal() {
  return <button>{selectingProject ? 'Preparing project...' : confirmIncompleteBrief ? 'Create task anyway' : 'Create task'}</button>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task form no-project copy that leaves users in a dead end', () => {
    const cwd = fixture({
      'src/app/features/board/TaskFormModal.tsx': `
function TaskFormModal() {
  return <span>No projects available. Create a project in Settings before creating tasks.</span>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'task-form-no-project-copy',
        location: 'src/app/features/board/TaskFormModal.tsx:3',
      }),
    ])
  })

  it('accepts task form no-project copy that explains the next setup action', () => {
    const cwd = fixture({
      'src/app/features/board/TaskFormModal.tsx': `
function TaskFormModal() {
  return <div><p>Create a project before sending tasks</p><p>Projects keep each task, its files, and its activity history in one place.</p><button>Open project settings</button></div>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task form no-agent copy that leaves setup disconnected', () => {
    const cwd = fixture({
      'src/app/features/board/TaskFormModal.tsx': `
function TaskFormModal() {
  return (
    <div>
      <span>No agents are online. You can create the task now; it will wait here until an agent comes online.</span>
      <span>Create the task now, or open agent setup to connect an agent first.</span>
    </div>
  )
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-form-no-agent-copy',
          location: 'src/app/features/board/TaskFormModal.tsx:5',
        }),
        expect.objectContaining({
          type: 'task-form-no-agent-copy',
          location: 'src/app/features/board/TaskFormModal.tsx:6',
        }),
      ])
    )
  })

  it('flags task form unavailable-agent copy that leaves setup disconnected', () => {
    const cwd = fixture({
      'src/app/features/board/TaskFormModal.tsx': `
function TaskFormModal() {
  return (
    <div>
      <span>No agents are available right now. Keep the default choice so the next available agent can pick it up.</span>
      <span>Create the task now, or open agent setup to start or connect an agent first.</span>
    </div>
  )
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-form-no-agent-copy',
          location: 'src/app/features/board/TaskFormModal.tsx:5',
        }),
        expect.objectContaining({
          type: 'task-form-no-agent-copy',
          location: 'src/app/features/board/TaskFormModal.tsx:6',
        }),
      ])
    )
  })

  it('accepts task form no-agent copy that links to setup', () => {
    const cwd = fixture({
      'src/app/features/board/TaskFormModal.tsx': `
function TaskFormModal() {
  return <div><p>Connect an agent before this task can start</p><p>Save the task now. It will wait until an agent is Ready, or you can open agent setup first.</p><button>Open agent setup</button></div>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task form readiness and agent choice copy that uses internal status words', () => {
    const cwd = fixture({
      'src/app/features/board/TaskFormModal.tsx': `
function TaskFormModal() {
  return (
    <div>
      <p>Preparing This Project</p>
      <p>Ready to Send</p>
      <option>Let the next available agent pick it up</option>
      <p>Keep this choice when any available agent can do the work.</p>
      <span>2 available</span>
    </div>
  )
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-form-ready-state-copy',
          location: 'src/app/features/board/TaskFormModal.tsx:5',
        }),
        expect.objectContaining({
          type: 'task-form-ready-state-copy',
          location: 'src/app/features/board/TaskFormModal.tsx:6',
        }),
        expect.objectContaining({
          type: 'task-form-agent-choice-copy',
          location: 'src/app/features/board/TaskFormModal.tsx:7',
        }),
        expect.objectContaining({
          type: 'task-form-agent-choice-copy',
          location: 'src/app/features/board/TaskFormModal.tsx:8',
        }),
        expect.objectContaining({
          type: 'task-form-agent-choice-copy',
          location: 'src/app/features/board/TaskFormModal.tsx:9',
        }),
      ])
    )
  })

  it('accepts task form readiness and agent choice copy that uses visible Ready wording', () => {
    const cwd = fixture({
      'src/app/features/board/TaskFormModal.tsx': `
function TaskFormModal() {
  return (
    <div>
      <p>Preparing this project</p>
      <p>Ready to send</p>
      <option>Let the next ready agent pick it up</option>
      <p>Leave automatic selection on when any ready agent can do the work.</p>
      <span>2 ready</span>
    </div>
  )
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags quick task creation copy that uses draft-task jargon', () => {
    const cwd = fixture({
      'src/app/features/board/QuickCreate.tsx': `
function QuickCreate() {
  return <button>Add Draft Task</button>
}
`,
      'src/app/features/board/KanbanColumn.tsx': `
function KanbanColumn() {
  return <p>Add a draft task below with the result you want.</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'quick-create-draft-task-copy',
          location: 'src/app/features/board/QuickCreate.tsx:3',
        }),
        expect.objectContaining({
          type: 'quick-create-draft-task-copy',
          location: 'src/app/features/board/KanbanColumn.tsx:3',
        }),
      ])
    )
  })

  it('accepts quick task creation copy that explains the unsent state', () => {
    const cwd = fixture({
      'src/app/features/board/QuickCreate.tsx': `
function QuickCreate() {
  return <div><button>Add Task</button><button>Save Task</button><p>This saves the task in Not sent yet.</p></div>
}
`,
      'src/app/features/board/KanbanColumn.tsx': `
function KanbanColumn() {
  return <p>Add a task below with the result you want.</p>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags title-case task queue submit labels', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentGroupsPanel.tsx': `
function AgentGroupsPanel() {
  return <button>{saving ? 'Creating…' : 'Create Task Queue'}</button>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'agent-task-queue-submit-label-copy',
        location: 'src/app/features/agents/AgentGroupsPanel.tsx:3',
      }),
    ])
  })

  it('accepts sentence-case task queue submit labels', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentGroupsPanel.tsx': `
function AgentGroupsPanel() {
  return <button>{saving ? 'Creating…' : 'Create task queue'}</button>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task queue overview copy that describes implementation behavior first', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentGroupsPanel.tsx': `
function AgentGroupsPanel() {
  return <p>Task queues are simple places agents check for tasks. Create a queue, add agents, then send tasks to it.</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'agent-task-queue-overview-copy',
        location: 'src/app/features/agents/AgentGroupsPanel.tsx:3',
      }),
    ])
  })

  it('accepts task queue overview copy that explains where new tasks wait', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentGroupsPanel.tsx': `
function AgentGroupsPanel() {
  return <p>Task queues are shared lists where new tasks wait for an available agent.</p>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task queue empty states that start with no-results copy', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentGroupsPanel.tsx': `
function AgentGroupsPanel() {
  return (
    <section>
      <p>No task queues yet. Create one below so agents can receive tasks.</p>
      <p>No tasks are in this task queue yet. Create a task and choose this task queue.</p>
    </section>
  )
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'agent-task-queue-empty-copy',
        location: 'src/app/features/agents/AgentGroupsPanel.tsx:5',
      }),
      expect.objectContaining({
        type: 'agent-task-queue-empty-copy',
        location: 'src/app/features/agents/AgentGroupsPanel.tsx:6',
      }),
    ])
  })

  it('accepts task queue empty states that start with the next action', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentGroupsPanel.tsx': `
function AgentGroupsPanel() {
  return (
    <section>
      <p>Create the first task queue so agents know where to receive tasks.</p>
      <p>Create the first task for this queue, then choose this task queue so agents know where to pick it up.</p>
    </section>
  )
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task queue errors that start with the failure before the next action', () => {
    const cwd = fixture({
      'src/app/features/agents/model/agentGroupErrorMessage.ts': `
export function agentGroupErrorMessage() {
  return 'Task queue was not created. Ask an owner or admin to let you create and manage task queues in this project.'
}
`,
      'src/app/features/agents/model/createAgentWorkLaneErrorMessage.ts': `
export function createAgentWorkLaneErrorMessage() {
  return 'Task queue was not created. Forge could not connect while creating the task queue. Check your connection, then try again.'
}
`,
      'src/app/entities/agent-group/api/agentGroupApi.ts': `
export function missingGroupMessage() {
  throw new Error(
    'Task queue was not created. Check the task queue name and project, then try again.'
  )
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'agent-task-queue-error-copy',
          location: 'src/app/features/agents/model/agentGroupErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'agent-task-queue-error-copy',
          location: 'src/app/features/agents/model/createAgentWorkLaneErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'agent-task-queue-error-copy',
          location: 'src/app/entities/agent-group/api/agentGroupApi.ts:4',
        }),
      ])
    )
  })

  it('accepts task queue errors that start with the next action', () => {
    const cwd = fixture({
      'src/app/features/agents/model/agentGroupErrorMessage.ts': `
export function agentGroupErrorMessage() {
  return 'Ask an owner or admin to let you create and manage task queues in this project. Task queue was not created.'
}
`,
      'src/app/features/agents/model/createAgentWorkLaneErrorMessage.ts': `
export function createAgentWorkLaneErrorMessage() {
  return 'Check your connection, then try creating the task queue again. Forge could not connect while creating the task queue.'
}
`,
      'src/app/entities/agent-group/api/agentGroupApi.ts': `
export function missingGroupMessage() {
  throw new Error(
    'Check the task queue name and project, then create the queue again. Task queue was not created.'
  )
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

  it('flags create-agent optional context labels that sound like missing setup', () => {
    const cwd = fixture({
      'src/app/features/agents/CreateAgentModal.tsx': `
export function CreateAgentModal() {
  return (
    <section>
      <p>No primary project</p>
      <option>No task queue</option>
      <p>No task queue selected yet</p>
    </section>
  )
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'create-agent-optional-context-copy',
        location: 'src/app/features/agents/CreateAgentModal.tsx:5',
      }),
      expect.objectContaining({
        type: 'create-agent-optional-context-copy',
        location: 'src/app/features/agents/CreateAgentModal.tsx:6',
      }),
      expect.objectContaining({
        type: 'create-agent-optional-context-copy',
        location: 'src/app/features/agents/CreateAgentModal.tsx:7',
      }),
    ])
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

  it('flags saved instruction summary fallbacks that only say a summary is missing', () => {
    const cwd = fixture({
      'src/app/features/skills/SkillCard.tsx': `
export function SkillCard() {
  return 'No summary yet. Open details before using this saved instruction.'
}
`,
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  skills: {
    detail: {
      noDescription:
        'No summary yet. Review the instructions below before using this saved instruction.',
    },
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  skills: {
    detail: {
      noDescription: '还没有简介。使用这条保存的说明前，请先查看下面的说明。',
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
          type: 'saved-instruction-summary-fallback-copy',
          location: 'src/app/features/skills/SkillCard.tsx:3',
        }),
        expect.objectContaining({
          type: 'saved-instruction-summary-fallback-copy',
          location: 'src/app/shared/i18n/locales/en.ts:6',
        }),
        expect.objectContaining({
          type: 'saved-instruction-summary-fallback-copy',
          location: 'src/app/shared/i18n/locales/zh.ts:5',
        }),
      ])
    )
  })

  it('flags saved instruction card fallbacks that say open details without naming saved instruction details', () => {
    const cwd = fixture({
      'src/app/features/skills/SkillCard.tsx': `
export function SkillCard() {
  return 'Open details to check the reusable instructions before using this saved instruction.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'saved-instruction-summary-fallback-copy',
        location: 'src/app/features/skills/SkillCard.tsx:3',
      }),
    ])
  })

  it('flags saved instruction work-tool tooltips that do not say where to check setup', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  skills: {
    detail: {
      unknownToolTooltip: 'Work tool setup needs review.',
    },
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  skills: {
    detail: {
      unknownToolTooltip: '工作工具设置需要检查。',
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
          type: 'saved-instruction-tool-tooltip-copy',
          location: 'src/app/shared/i18n/locales/en.ts:5',
        }),
        expect.objectContaining({
          type: 'saved-instruction-tool-tooltip-copy',
          location: 'src/app/shared/i18n/locales/zh.ts:5',
        }),
      ])
    )
  })

  it('accepts saved instruction fallback copy that starts with the check action', () => {
    const cwd = fixture({
      'src/app/features/skills/SkillCard.tsx': `
export function SkillCard() {
  return 'Open saved instruction details to review the reusable instructions before using it.'
}
`,
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  skills: {
    detail: {
      noDescription:
        'Check the reusable instructions below before using this saved instruction.',
      unknownToolTooltip:
        'Open Settings and check the work tool before using this saved instruction.',
    },
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  skills: {
    detail: {
      noDescription: '使用这条保存的说明前，请先查看下面的可复用说明。',
      unknownToolTooltip: '打开设置检查工作工具，再使用这条保存的说明。',
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
  return readableCodeLabel(eventType, { fallback: 'Check change' })
}

function shortEventType(eventType) {
  return eventType.trim() || 'Saved change name missing'
}

function resourceTypeLabel(value) {
  return readableCodeLabel(value, { fallback: 'Check record type' })
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags governance audit errors that start with the failure instead of the next step', () => {
    const cwd = fixture({
      'src/app/features/governance/governanceAuditErrorMessages.ts': `
const ACTION_FALLBACKS = {
  exportAudit: 'The audit export did not finish. Keep secrets hidden, refresh the audit view, then try the export again.',
  loadAudit: 'Governance audit history could not load. Refresh the audit view, then apply the filters again.',
}
function notFound() {
  return 'Governance audit is not available from this view. Open the Admin audit view again, then retry.'
}
function conflict() {
  return 'The audit data changed while export was running. Refresh the audit view, then export again.'
}
function rateLimit() {
  return 'Governance audit is handling too many requests right now. Wait a moment, then try again.'
}
function service() {
  return 'Forge could not load governance audit history right now. Refresh the audit view, then try again.'
}
function permission() {
  return 'You do not have permission to view or export audit history. Ask an owner or admin to update your team space access.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'governance-audit-error-copy',
          location: 'src/app/features/governance/governanceAuditErrorMessages.ts:3',
        }),
        expect.objectContaining({
          type: 'governance-audit-error-copy',
          location: 'src/app/features/governance/governanceAuditErrorMessages.ts:4',
        }),
        expect.objectContaining({
          type: 'governance-audit-error-copy',
          location: 'src/app/features/governance/governanceAuditErrorMessages.ts:7',
        }),
        expect.objectContaining({
          type: 'governance-audit-error-copy',
          location: 'src/app/features/governance/governanceAuditErrorMessages.ts:10',
        }),
        expect.objectContaining({
          type: 'governance-audit-error-copy',
          location: 'src/app/features/governance/governanceAuditErrorMessages.ts:13',
        }),
        expect.objectContaining({
          type: 'governance-audit-error-copy',
          location: 'src/app/features/governance/governanceAuditErrorMessages.ts:16',
        }),
        expect.objectContaining({
          type: 'governance-audit-error-copy',
          location: 'src/app/features/governance/governanceAuditErrorMessages.ts:19',
        }),
      ])
    )
  })

  it('accepts governance audit errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/features/governance/governanceAuditErrorMessages.ts': `
const ACTION_FALLBACKS = {
  exportAudit: 'Keep secrets hidden, refresh change history, then try the export again.',
  loadAudit: 'Refresh change history, then apply the filters again.',
}
function notFound() {
  return 'Open Admin change history again, then retry.'
}
function conflict() {
  return 'Refresh change history, then export again because the change list changed while export was running.'
}
function rateLimit() {
  return 'Wait a moment, then try again. Change history is handling too many requests right now.'
}
function service() {
  return 'Refresh change history, then apply the filters again. If it still fails, ask an owner or admin to check change history setup.'
}
function permission() {
  return 'Ask an owner or admin to update your team space access, then retry this change-history action. You do not have permission to view or export change history.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags governance history copy that exposes audit and event jargon', () => {
    const cwd = fixture({
      'src/app/features/governance/AuditLogView.tsx': `
function AuditLogView() {
  return <section aria-label="Common audit views">
    <p>Pick a common audit view, then narrow it.</p>
    <label>Exact event name</label>
    <input placeholder="Paste an event category only when needed" />
    <button aria-label="Refresh audit history">Refresh</button>
    <button>Show event details</button>
    <button>Show change details</button>
    <p>Check change details</p>
  </section>
}
`,
      'src/app/features/governance/governanceAuditErrorMessages.ts': `
function message() {
  return 'Your sign-in expired. Sign in again, then retry this audit action.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'governance-audit-jargon-copy',
          location: 'src/app/features/governance/AuditLogView.tsx:3',
        }),
        expect.objectContaining({
          type: 'governance-audit-jargon-copy',
          location: 'src/app/features/governance/AuditLogView.tsx:4',
        }),
        expect.objectContaining({
          type: 'governance-audit-jargon-copy',
          location: 'src/app/features/governance/AuditLogView.tsx:5',
        }),
        expect.objectContaining({
          type: 'governance-audit-jargon-copy',
          location: 'src/app/features/governance/AuditLogView.tsx:6',
        }),
        expect.objectContaining({
          type: 'governance-audit-jargon-copy',
          location: 'src/app/features/governance/AuditLogView.tsx:7',
        }),
        expect.objectContaining({
          type: 'governance-audit-jargon-copy',
          location: 'src/app/features/governance/AuditLogView.tsx:8',
        }),
        expect.objectContaining({
          type: 'governance-audit-jargon-copy',
          location: 'src/app/features/governance/AuditLogView.tsx:9',
        }),
        expect.objectContaining({
          type: 'governance-audit-jargon-copy',
          location: 'src/app/features/governance/governanceAuditErrorMessages.ts:3',
        }),
      ])
    )
  })

  it('accepts governance history copy that uses change-history wording', () => {
    const cwd = fixture({
      'src/app/features/governance/AuditLogView.tsx': `
function AuditLogView() {
  return <section aria-label="Common change views">
    <p>Pick a common change view, then narrow it.</p>
    <label>Specific change name</label>
    <input placeholder="Paste an exact change area only when needed" />
    <button aria-label="Refresh change history">Refresh</button>
    <button>Show saved change name</button>
  </section>
}
`,
      'src/app/features/governance/governanceAuditErrorMessages.ts': `
function message() {
  return 'Your sign-in expired. Sign in again, then retry this change-history action.'
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

  it('flags navigation permission errors that start with the failure', () => {
    const cwd = fixture({
      'src/app/entities/navigation/model/navigation.store.ts': `
function navigationActionErrorMessage(actionPhrase) {
  return \`You do not have permission to \${actionPhrase}. Ask an owner or admin to update your team space access.\`
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'navigation-error-copy',
        location: 'src/app/entities/navigation/model/navigation.store.ts:3',
      }),
    ])
  })

  it('accepts navigation permission errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/entities/navigation/model/navigation.store.ts': `
function navigationActionErrorMessage(actionPhrase) {
  return \`Ask an owner or admin to update your team space access, then refresh the left menu to \${actionPhrase}. You do not have permission to \${actionPhrase}.\`
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
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
function permissionMessage() {
  return 'You do not have permission to view this task. Ask an owner or admin to give you access to this task.'
}
function networkRecoveryMessage() {
  return 'Forge could not connect while loading this task. Check your connection, then refresh the page.'
}
function notFoundMessage() {
  return 'This task was not found. Refresh the board, then open the task again.'
}
function conflictMessage() {
  return 'This task changed while you were working. Refresh the detail panel, then try again.'
}
function busyMessage() {
  return 'Task actions are busy. Wait a moment, then try again.'
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
        expect.objectContaining({
          type: 'task-detail-load-copy',
          location: 'src/app/features/detail/taskDetailErrorMessages.ts:12',
        }),
        expect.objectContaining({
          type: 'task-detail-load-copy',
          location: 'src/app/features/detail/taskDetailErrorMessages.ts:15',
        }),
        expect.objectContaining({
          type: 'task-detail-load-copy',
          location: 'src/app/features/detail/taskDetailErrorMessages.ts:18',
        }),
        expect.objectContaining({
          type: 'task-detail-load-copy',
          location: 'src/app/features/detail/taskDetailErrorMessages.ts:21',
        }),
      ])
    )
  })

  it('accepts task detail load copy that starts with the next step', () => {
    const cwd = fixture({
      'src/app/features/detail/taskDetailErrorMessages.ts': `
const ACTION_FALLBACKS = {
  loadAgents: 'Refresh this task before assigning an agent.',
  loadContext: 'Refresh the detail panel to load saved notes and work history.',
  loadRuns: 'Refresh Updates before deciding whether to retry this task.',
  previewContext: 'Choose an available agent, then open saved item review again.',
}
function permissionMessage() {
  return 'Ask an owner or admin to give you access to this task, then refresh the task detail panel. You do not have permission to view this task.'
}
function networkRecoveryMessage() {
  return 'If it still does not load, check your connection and refresh the page.'
}
function notFoundMessage() {
  return 'Refresh the board, then open the task again. This task was not found.'
}
function conflictMessage() {
  return 'Refresh the detail panel, then try again. This task changed while you were working.'
}
function busyMessage() {
  return 'Wait a moment, then try again. Task actions are busy.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task detail action copy that starts with the failure instead of the next step', () => {
    const cwd = fixture({
      'src/app/features/detail/taskDetailErrorMessages.ts': `
const ACTION_FALLBACKS = {
  approveTask: 'The task was not approved. Check that the task is still waiting for approval, then try again.',
  blockTask: 'The task was not marked as needing help. Refresh the task, then choose Needs help again.',
  cancelTask: 'The task was not canceled. Refresh the task, then choose Cancel again.',
  publishTask: 'The task was not sent with selected notes. Review the saved notes, then try again.',
  retryTask: 'The task was not retried. Refresh the task, then try Retry task again.',
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-detail-action-copy',
          location: 'src/app/features/detail/taskDetailErrorMessages.ts:3',
        }),
        expect.objectContaining({
          type: 'task-detail-action-copy',
          location: 'src/app/features/detail/taskDetailErrorMessages.ts:4',
        }),
        expect.objectContaining({
          type: 'task-detail-action-copy',
          location: 'src/app/features/detail/taskDetailErrorMessages.ts:5',
        }),
        expect.objectContaining({
          type: 'task-detail-action-copy',
          location: 'src/app/features/detail/taskDetailErrorMessages.ts:6',
        }),
        expect.objectContaining({
          type: 'task-detail-action-copy',
          location: 'src/app/features/detail/taskDetailErrorMessages.ts:7',
        }),
      ])
    )
  })

  it('accepts task detail action copy that starts with the next step', () => {
    const cwd = fixture({
      'src/app/features/detail/taskDetailErrorMessages.ts': `
const ACTION_FALLBACKS = {
  approveTask: 'Check that the task is still waiting for approval, then choose Approve again. The task was not approved.',
  blockTask: 'Refresh the task, then choose Needs help again. The task was not marked as needing help.',
  cancelTask: 'Refresh the task, then choose Cancel again. The task was not canceled.',
  publishTask: 'Review the selected saved notes, then send the task again. The task was not sent with selected notes.',
  retryTask: 'Refresh the task, then choose Retry task again. The task was not retried.',
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task handoff agent capability copy that exposes raw capability labels', () => {
    const cwd = fixture({
      'src/app/features/detail/TaskDetailPanel.tsx': `
function AgentChoice({ participant }) {
  return <span>{participant.capabilities.join(', ')}</span>
}
`,
      'src/app/features/board/AssignmentReadinessPanel.tsx': `
function ParticipantChip({ participant }) {
  return <span>{participant.capabilities.join(', ')}</span>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-agent-capability-copy',
          location: 'src/app/features/detail/TaskDetailPanel.tsx:3',
        }),
        expect.objectContaining({
          type: 'task-agent-capability-copy',
          location: 'src/app/features/board/AssignmentReadinessPanel.tsx:3',
        }),
      ])
    )
  })

  it('accepts task handoff agent capability copy that explains the action', () => {
    const cwd = fixture({
      'src/app/features/detail/TaskDetailPanel.tsx': `
function AgentChoice() {
  return <span>Can build the task and review the result</span>
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
function permissionMessage() {
  return 'You do not have permission to change this board. Ask an owner or admin to give you access to this board.'
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
        expect.objectContaining({
          type: 'board-load-copy',
          location: 'src/app/features/board/boardErrorMessages.ts:14',
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
function permissionMessage() {
  return 'Ask an owner or admin to give you access to this board, then refresh the board and try again. You do not have permission to change this board.'
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

  it('flags board action copy that starts with the failure instead of the next step', () => {
    const cwd = fixture({
      'src/app/features/board/boardErrorMessages.ts': `
const ACTION_FALLBACKS = {
  createTask: 'The task was not created. Check the project, task queue, and result, then try again.',
  moveTask: 'The task was moved back because the board change was not saved.',
  publishTask: 'The task was not sent with selected saved items. Review the saved item preview, then try again.',
  selectProject: 'The project was not selected. Choose the project again, then create the task.',
}
`,
      'src/app/features/board/QuickCreate.tsx': `
function QuickCreate() {
  return 'The task was not saved. Check the board message, then try again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'board-action-error-copy',
          location: 'src/app/features/board/boardErrorMessages.ts:3',
        }),
        expect.objectContaining({
          type: 'board-action-error-copy',
          location: 'src/app/features/board/boardErrorMessages.ts:4',
        }),
        expect.objectContaining({
          type: 'board-action-error-copy',
          location: 'src/app/features/board/boardErrorMessages.ts:5',
        }),
        expect.objectContaining({
          type: 'board-action-error-copy',
          location: 'src/app/features/board/boardErrorMessages.ts:6',
        }),
        expect.objectContaining({
          type: 'board-action-error-copy',
          location: 'src/app/features/board/QuickCreate.tsx:3',
        }),
      ])
    )
  })

  it('accepts board action copy that starts with the next step', () => {
    const cwd = fixture({
      'src/app/features/board/boardErrorMessages.ts': `
const ACTION_FALLBACKS = {
  createTask: 'Check the project, task queue, and result, then create the task again. The task was not created.',
  moveTask: 'Refresh the board, then move the task again. The task was moved back because the board change was not saved.',
  publishTask: 'Review the saved item preview, then send the task with selected saved items again. The task was not sent.',
  selectProject: 'Choose the project again, then create the task. The project was not selected.',
}
`,
      'src/app/features/board/QuickCreate.tsx': `
function QuickCreate() {
  return 'Check the board message, then save the task again. The task was not saved.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags board no-agent preview copy that does not point users to agent setup', () => {
    const cwd = fixture({
      'src/app/features/board/boardErrorMessages.ts': `
function noAgentPreview() {
  return 'No agent is available for saved item preview. Start an agent or wait for one to finish, then try again.'
}
`,
      'src/app/features/board/AssignmentReadinessPanel.tsx': `
function AssignmentReadinessPanel() {
  return 'No agent can take work right now.'
}
function summarizeHandoff() {
  return '1 task needs an agent. Connect or free up an agent before it can start.'
}
function ParticipantChip() {
  return 'No recent activity'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'board-agent-setup-copy',
          location: 'src/app/features/board/boardErrorMessages.ts:3',
        }),
        expect.objectContaining({
          type: 'board-agent-setup-copy',
          location: 'src/app/features/board/AssignmentReadinessPanel.tsx:3',
        }),
        expect.objectContaining({
          type: 'board-agent-setup-copy',
          location: 'src/app/features/board/AssignmentReadinessPanel.tsx:6',
        }),
        expect.objectContaining({
          type: 'board-agent-setup-copy',
          location: 'src/app/features/board/AssignmentReadinessPanel.tsx:9',
        }),
      ])
    )
  })

  it('accepts board no-agent preview copy that points users to agent setup', () => {
    const cwd = fixture({
      'src/app/features/board/boardErrorMessages.ts': `
function noAgentPreview() {
  return 'No agent can prepare the saved item preview right now. Open Agents to start or connect an agent, then return to the board and refresh.'
}
`,
      'src/app/features/board/AssignmentReadinessPanel.tsx': `
function AssignmentReadinessPanel() {
  return 'Open Agents to start or connect an agent, or wait for one to finish.'
}
function summarizeHandoff() {
  return '1 task needs an agent. Open Agents to start or connect an agent, or wait for one to finish.'
}
function ParticipantChip() {
  return 'Open Agents to reconnect'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags board clear-state copy that does not tell users what to do next', () => {
    const cwd = fixture({
      'src/app/features/board/AssignmentReadinessPanel.tsx': `
function summarizeHandoff() {
  return 'Task queue is clear.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'board-clear-copy',
          location: 'src/app/features/board/AssignmentReadinessPanel.tsx:3',
        }),
      ])
    )
  })

  it('accepts board clear-state copy that tells users to create a task', () => {
    const cwd = fixture({
      'src/app/features/board/AssignmentReadinessPanel.tsx': `
function summarizeHandoff() {
  return 'Create a task when you have work to send.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags authentication errors that start with the failure instead of recovery', () => {
    const cwd = fixture({
      'src/app/features/auth/AuthPage.ts': `
function authLoginErrorMessage() {
  return 'We could not sign you in right now. Try again in a minute. If it still fails, ask an owner or admin to check sign-in setup.'
}
function authRegisterErrorMessage() {
  return 'An account may already exist for this email. Sign in instead, or reset the password if you cannot access it.'
}
function authSignInErrorMessage() {
  return 'This sign-in link expired or could not be verified. Start sign-in again from this page.'
}
function authRecoveryErrorMessage(action) {
  if (action === 'reset-password') {
    return 'Password could not be updated. Check the password rules, then try again.'
  }
  if (action === 'forgot-password') {
    return 'Reset email could not be requested. Check the email address, wait a moment, then try again.'
  }
  return 'Verification email could not be sent. Check that this is the email you used to create the account, then try again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'auth-error-copy',
          location: 'src/app/features/auth/AuthPage.ts:3',
        }),
        expect.objectContaining({
          type: 'auth-error-copy',
          location: 'src/app/features/auth/AuthPage.ts:6',
        }),
        expect.objectContaining({
          type: 'auth-error-copy',
          location: 'src/app/features/auth/AuthPage.ts:9',
        }),
        expect.objectContaining({
          type: 'auth-error-copy',
          location: 'src/app/features/auth/AuthPage.ts:13',
        }),
        expect.objectContaining({
          type: 'auth-error-copy',
          location: 'src/app/features/auth/AuthPage.ts:16',
        }),
        expect.objectContaining({
          type: 'auth-error-copy',
          location: 'src/app/features/auth/AuthPage.ts:18',
        }),
      ])
    )
  })

  it('accepts authentication errors that start with recovery', () => {
    const cwd = fixture({
      'src/app/features/auth/AuthPage.ts': `
function authLoginErrorMessage() {
  return 'Try signing in again in a minute. If it still fails, ask an owner or admin to check sign-in setup.'
}
function authRegisterErrorMessage() {
  return 'Sign in instead, or reset the password if you cannot access it. An account may already exist for this email.'
}
function authSignInErrorMessage() {
  return 'Start sign-in again from this page. This sign-in link expired or could not be verified.'
}
function authRecoveryErrorMessage(action) {
  if (action === 'reset-password') {
    return 'Check the password rules, then try again. Password could not be updated.'
  }
  if (action === 'forgot-password') {
    return 'Check the email address, wait a moment, then request the reset email again.'
  }
  return 'Check that this is the email you used to create the account, then send the verification email again.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags sign-in orientation that exposes evidence jargon', () => {
    const cwd = fixture({
      'src/app/features/auth/AuthPage.ts': `
function render() {
  return 'Sign in to manage agents, tasks, evidence, and team settings from one team space.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'auth-intro-copy',
        location: 'src/app/features/auth/AuthPage.ts:3',
      }),
    ])
  })

  it('flags sign-in orientation that mentions workspace-admin invitations', () => {
    const cwd = fixture({
      'src/app/features/auth/AuthPage.ts': `
function renderLoginForm() {
  return 'Use the email your workspace admin invited. After sign in, you will land on your task board.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'auth-intro-copy',
        location: 'src/app/features/auth/AuthPage.ts:3',
      }),
    ])
  })

  it('accepts sign-in orientation that explains saved work records and invitation email', () => {
    const cwd = fixture({
      'src/app/features/auth/AuthPage.ts': `
function render() {
  return 'Sign in to manage agents, tasks, saved work records, and team settings from one team space.'
}
function renderLoginForm() {
  return 'Use the email address from your invitation. After sign in, you will land on your task board.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags getting started setup labels that still say workspace', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  gettingStarted: {
    steps: {
      workspace: {
        title: 'Workspace',
        create: 'Create workspace',
        review: 'Review workspace',
      },
    },
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  gettingStarted: {
    steps: {
      workspace: {
        title: '工作区',
        create: '创建工作区',
        review: '查看工作区',
      },
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
          type: 'getting-started-team-project-copy',
          sample: expect.stringContaining('Workspace'),
        }),
        expect.objectContaining({
          type: 'getting-started-team-project-copy',
          sample: expect.stringContaining('Create workspace'),
        }),
        expect.objectContaining({
          type: 'getting-started-team-project-copy',
          sample: expect.stringContaining('Review workspace'),
        }),
        expect.objectContaining({
          type: 'getting-started-team-project-copy',
          sample: expect.stringContaining('工作区'),
        }),
        expect.objectContaining({
          type: 'getting-started-team-project-copy',
          sample: expect.stringContaining('创建工作区'),
        }),
        expect.objectContaining({
          type: 'getting-started-team-project-copy',
          sample: expect.stringContaining('查看工作区'),
        }),
      ])
    )
  })

  it('accepts getting started setup labels that name team and project', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  gettingStarted: {
    steps: {
      workspace: {
        title: 'Team and project',
        create: 'Create team and project',
        review: 'Review team and project',
      },
    },
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  gettingStarted: {
    steps: {
      workspace: {
        title: '团队和项目',
        create: '创建团队和项目',
        review: '查看团队和项目',
      },
    },
  },
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags legal privacy copy that exposes evidence jargon', () => {
    const cwd = fixture({
      'src/app/shared/ui/legal/LegalPage.ts': `
function renderPrivacy() {
  return [
    '<li>To show live task, agent, and evidence updates in the product interface</li>',
    '<li>IP address used for rate limiting and audit logging</li>',
    '<li>To maintain audit logs for security monitoring and compliance purposes</li>',
    '<li>The export includes event history and configuration settings</li>',
    '<p>Review what you agree to and how your workspace data is handled.</p>',
    '<li>Visual workspace preferences, such as saved view settings</li>',
  ].join('')
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'legal-privacy-copy',
          sample: expect.stringContaining('evidence updates'),
        }),
        expect.objectContaining({
          type: 'legal-privacy-copy',
          sample: expect.stringContaining('rate limiting'),
        }),
        expect.objectContaining({
          type: 'legal-privacy-copy',
          sample: expect.stringContaining('audit logs'),
        }),
        expect.objectContaining({
          type: 'legal-privacy-copy',
          sample: expect.stringContaining('event history'),
        }),
        expect.objectContaining({
          type: 'legal-privacy-copy',
          sample: expect.stringContaining('configuration settings'),
        }),
        expect.objectContaining({
          type: 'legal-privacy-copy',
          sample: expect.stringContaining('workspace data'),
        }),
        expect.objectContaining({
          type: 'legal-privacy-copy',
          sample: expect.stringContaining('Visual workspace preferences'),
        }),
      ])
    )
  })

  it('accepts legal privacy copy that explains saved work updates plainly', () => {
    const cwd = fixture({
      'src/app/shared/ui/legal/LegalPage.ts': `
function renderPrivacy() {
  return [
    '<li>To show live task, agent, and saved work updates in the product interface</li>',
    '<li>IP address used to protect the Service, slow abusive requests, and record security-relevant activity</li>',
    '<li>To keep security history records for safety reviews and legal requirements</li>',
    '<li>The export includes change history and settings choices</li>',
    '<p>Review what you agree to and how your team space data is handled.</p>',
    '<li>Saved view choices, such as layout and display settings</li>',
  ].join('')
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags inbox and agent setup copy that exposes triage jargon', () => {
    const cwd = fixture({
      'src/app/features/inbox/InboxView.tsx': `
function renderInboxPath() {
  return 'Inbox triage path'
}
`,
      'src/app/features/agents/AgentConfigTab.tsx': `
function promptTemplate() {
  return 'You are a triage agent. Reproduce the reported behavior.'
}
`,
      'src/app/features/agents/AgentGroupsPanel.tsx': `
function groupTemplate() {
  return 'Triage Queue'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'beginner-sorting-copy',
          sample: expect.stringContaining('Inbox triage path'),
        }),
        expect.objectContaining({
          type: 'beginner-sorting-copy',
          sample: expect.stringContaining('triage agent'),
        }),
        expect.objectContaining({
          type: 'beginner-sorting-copy',
          sample: expect.stringContaining('Triage Queue'),
        }),
      ])
    )
  })

  it('accepts inbox and agent setup copy that describes sorting work plainly', () => {
    const cwd = fixture({
      'src/app/features/inbox/InboxView.tsx': `
function renderInboxPath() {
  return 'Inbox action order'
}
`,
      'src/app/features/agents/AgentConfigTab.tsx': `
function promptTemplate() {
  return 'You help sort incoming work. Recreate the reported behavior.'
}
`,
      'src/app/features/agents/AgentGroupsPanel.tsx': `
function groupTemplate() {
  return 'Intake Queue'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags Getting Started review copy that exposes evidence jargon', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  gettingStarted: {
    steps: { review: { why: 'Reviewing the result confirms the agent returned useful work and evidence.' } },
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  gettingStarted: {
    steps: { review: { success: '任务已经完成，并且能看到输出或证据。' } },
  },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'getting-started-review-copy',
          location: 'src/app/shared/i18n/locales/en.ts:4',
        }),
        expect.objectContaining({
          type: 'getting-started-review-copy',
          location: 'src/app/shared/i18n/locales/zh.ts:4',
        }),
      ])
    )
  })

  it('accepts Getting Started review copy that explains result files plainly', () => {
    const cwd = fixture({
      'src/app/shared/i18n/locales/en.ts': `
export const en = {
  gettingStarted: {
    steps: { review: { success: 'A task has completed output or result files you can open.' } },
  },
}
`,
      'src/app/shared/i18n/locales/zh.ts': `
export const zh = {
  gettingStarted: {
    steps: { review: { success: '任务已经完成，并且能看到输出或结果文件。' } },
  },
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task result review copy that exposes evidence jargon', () => {
    const cwd = fixture({
      'src/app/features/detail/DescriptionTab.tsx': `
function DescriptionTab() {
  return <ReviewSection title="Result files and evidence" />
}
`,
      'src/app/features/detail/TaskDetailPanel.tsx': `
function ResultReviewGuide() {
  return [
    'Use this result as evidence for the task outcome.',
    'Check the evidence',
  ]
}
`,
      'src/app/features/detail/HistoryTab.tsx': `
function taskCheckIn() {
  return '1 result item ready to review.'
}
`,
      'src/app/features/list/ListView.tsx': `
function listNextStep() {
  return {
    detail: 'Open completed tasks to check the result, evidence, and anything worth reusing.',
    action: 'Open it to review the result and evidence.',
  }
}
`,
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
function agentNextStep() {
  return 'You can decide whether to reuse the agent, review evidence, or assign another task.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-detail-result-review-copy',
          location: 'src/app/features/detail/DescriptionTab.tsx:3',
        }),
        expect.objectContaining({
          type: 'task-detail-result-review-copy',
          location: 'src/app/features/detail/TaskDetailPanel.tsx:4',
        }),
        expect.objectContaining({
          type: 'task-detail-result-review-copy',
          location: 'src/app/features/detail/TaskDetailPanel.tsx:5',
        }),
        expect.objectContaining({
          type: 'task-detail-result-review-copy',
          location: 'src/app/features/detail/HistoryTab.tsx:3',
        }),
        expect.objectContaining({
          type: 'task-detail-result-review-copy',
          location: 'src/app/features/list/ListView.tsx:4',
        }),
        expect.objectContaining({
          type: 'task-detail-result-review-copy',
          location: 'src/app/features/list/ListView.tsx:5',
        }),
        expect.objectContaining({
          type: 'task-detail-result-review-copy',
          location: 'src/app/widgets/agent-detail/AgentDetailView.tsx:3',
        }),
      ])
    )
  })

  it('accepts task result review copy that describes result files plainly', () => {
    const cwd = fixture({
      'src/app/features/detail/DescriptionTab.tsx': `
function DescriptionTab() {
  return <ReviewSection title="Result files" />
}
`,
      'src/app/features/detail/TaskDetailPanel.tsx': `
function ResultReviewGuide() {
  return [
    'Use this result to decide whether the task is done.',
    'Check result files',
  ]
}
`,
      'src/app/features/list/ListView.tsx': `
function listNextStep() {
  return {
    detail: 'Open completed tasks to check the result, result files, and anything worth reusing.',
    action: 'Open it to review the result and result files.',
  }
}
`,
      'src/app/widgets/agent-detail/AgentDetailView.tsx': `
function agentNextStep() {
  return 'You can decide whether to reuse the agent, review result files, or assign another task.'
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
      'src/app/features/settings/providerSettingsErrorMessage.ts': `
function providerSettingsErrorMessage() {
  return 'AI service could not be saved. Forge could not connect while opening AI service settings. Check your connection, then try again.'
}
`,
      'src/app/features/settings/platformKeyErrorMessage.ts': `
function platformKeyErrorMessage() {
  return 'Outside tool access key could not be created. Forge could not connect while opening outside tool access settings. Check your connection, then try again.'
}
`,
      'src/app/features/settings/gitCredentialsErrorMessage.ts': `
function gitCredentialsErrorMessage() {
  return 'Repository access could not be saved. Forge could not connect while opening repository access. Check your connection, then try again.'
}
`,
      'src/app/features/settings/sshKeysErrorMessage.ts': `
function sshKeysErrorMessage() {
  return 'Repository SSH access could not be saved. Forge could not connect while opening repository SSH access. Check your connection, then try again.'
}
`,
      'src/app/features/settings/accountErrorMessages.ts': `
function accountErrorMessage() {
  return 'Password could not be changed. Forge could not connect while opening password settings. Check your connection, then try again.'
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
          location: 'src/app/features/settings/providerSettingsErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'network-copy',
          location: 'src/app/features/settings/platformKeyErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'network-copy',
          location: 'src/app/features/settings/gitCredentialsErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'network-copy',
          location: 'src/app/features/settings/sshKeysErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'network-copy',
          location: 'src/app/features/settings/accountErrorMessages.ts:3',
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
      'src/app/features/settings/providerSettingsErrorMessage.ts': `
function providerSettingsErrorMessage() {
  return 'Check your connection, then save this AI service again. Forge could not connect while opening AI service settings.'
}
`,
      'src/app/features/settings/platformKeyErrorMessage.ts': `
function platformKeyErrorMessage() {
  return 'Check your connection, then create this outside tool access key again. Forge could not connect while opening outside tool access settings.'
}
`,
      'src/app/features/settings/gitCredentialsErrorMessage.ts': `
function gitCredentialsErrorMessage() {
  return 'Check your connection, then save repository access again. Forge could not connect while opening repository access.'
}
`,
      'src/app/features/settings/sshKeysErrorMessage.ts': `
function sshKeysErrorMessage() {
  return 'Check your connection, then save this repository SSH access again. Forge could not connect while opening repository SSH access.'
}
`,
      'src/app/features/settings/accountErrorMessages.ts': `
function accountErrorMessage() {
  return 'Check your connection, then change your password again. Forge could not connect while opening password settings.'
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

  it('flags agent control errors that start with the failure or expose internal fallback copy', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentControlPanel.tsx': `
function agentControlErrorMessage() {
  return 'You do not have permission to change this agent. Ask an owner or admin to let you manage this agent, then try again.'
}
function localActionError() {
  return 'agent control action failed'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'agent-control-error-copy',
          location: 'src/app/features/agents/AgentControlPanel.tsx:3',
        }),
        expect.objectContaining({
          type: 'agent-control-error-copy',
          location: 'src/app/features/agents/AgentControlPanel.tsx:6',
        }),
      ])
    )
  })

  it('accepts agent control permission errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/features/agents/AgentControlPanel.tsx': `
function agentControlErrorMessage() {
  return 'Ask an owner or admin to let you manage this agent, then try again. You do not have permission to change this agent.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags saved item feedback errors that start with the failure before the next step', () => {
    const cwd = fixture({
      'src/app/entities/context/model/feedbackErrorMessage.ts': `
function permission() {
  return 'You do not have permission to save feedback for this saved item. Ask an owner or admin to give you access to the saved item.'
}
function missing() {
  return 'This saved item could not be found. Refresh the task, then choose it again.'
}
function changed() {
  return 'This saved item changed while you were giving feedback. Refresh the task, review it, then try again.'
}
function busy() {
  return 'Feedback is busy. Wait a moment, then save this feedback again.'
}
function service() {
  return 'Forge could not save feedback right now. Refresh the task, then try again.'
}
function fallback() {
  return 'Feedback could not be saved. Refresh the task and try again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'feedback-error-copy',
          location: 'src/app/entities/context/model/feedbackErrorMessage.ts:3',
        }),
        expect.objectContaining({
          type: 'feedback-error-copy',
          location: 'src/app/entities/context/model/feedbackErrorMessage.ts:6',
        }),
        expect.objectContaining({
          type: 'feedback-error-copy',
          location: 'src/app/entities/context/model/feedbackErrorMessage.ts:9',
        }),
        expect.objectContaining({
          type: 'feedback-error-copy',
          location: 'src/app/entities/context/model/feedbackErrorMessage.ts:12',
        }),
        expect.objectContaining({
          type: 'feedback-error-copy',
          location: 'src/app/entities/context/model/feedbackErrorMessage.ts:15',
        }),
        expect.objectContaining({
          type: 'feedback-error-copy',
          location: 'src/app/entities/context/model/feedbackErrorMessage.ts:18',
        }),
      ])
    )
  })

  it('accepts saved item feedback errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/entities/context/model/feedbackErrorMessage.ts': `
function permission() {
  return 'Ask an owner or admin to give you access to this saved item, then save feedback again. You do not have permission to save feedback for this saved item.'
}
function missing() {
  return 'Refresh the task, then choose this saved item again. This saved item could not be found.'
}
function changed() {
  return 'Refresh the task, review this saved item, then save feedback again. This saved item changed while you were giving feedback.'
}
function busy() {
  return 'Wait a moment, then save this feedback again. Feedback is busy.'
}
function service() {
  return 'Refresh the task, then save feedback again. Forge could not save feedback right now.'
}
function fallback() {
  return 'Refresh the task, then save feedback again. Feedback could not be saved.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags chat errors that explain failure before the next step', () => {
    const cwd = fixture({
      'src/app/shared/model/chat.errors.ts': `
function serviceRecoveryMessage(action) {
  return action === 'load'
    ? 'Forge could not load this conversation right now. Wait a few minutes, then try again.'
    : 'Forge could not update this chat right now. Wait a few minutes, then try again.'
}

function chatErrorMessage(base) {
  return \`\${base} Forge could not read this conversation. Refresh the chat, then try again.\`
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'chat-error-copy',
          location: 'src/app/shared/model/chat.errors.ts:4',
        }),
        expect.objectContaining({
          type: 'chat-error-copy',
          location: 'src/app/shared/model/chat.errors.ts:5',
        }),
        expect.objectContaining({
          type: 'chat-error-copy',
          location: 'src/app/shared/model/chat.errors.ts:9',
        }),
      ])
    )
  })

  it('accepts chat errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/shared/model/chat.errors.ts': `
function serviceRecoveryMessage(action) {
  return action === 'load'
    ? 'Wait a few minutes, then choose Retry conversation again. Forge could not load this conversation right now.'
    : 'Wait a few minutes, then clear chat again if you still want to remove the messages. Forge could not update this chat right now.'
}

function chatErrorMessage(base) {
  return \`\${base} Refresh the chat, then try again. Forge could not read this conversation.\`
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags Settings store errors that explain failure before the next step', () => {
    const cwd = fixture({
      'src/app/shared/model/settings.store.ts': `
function settingsUnavailableMessage(operation, actionPhrase) {
  return \`Forge could not \${operation} right now. Refresh Settings, then try to \${actionPhrase} again.\`
}

function settingsDefaultMessage(actionPhrase) {
  return \`Settings could not \${actionPhrase}. Refresh Settings, then try again.\`
}

function settingsPermissionMessage(actionPhrase) {
  return \`You do not have permission to \${actionPhrase}. Ask an owner or admin to manage Settings.\`
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'settings-store-error-copy',
          location: 'src/app/shared/model/settings.store.ts:3',
        }),
        expect.objectContaining({
          type: 'settings-store-error-copy',
          location: 'src/app/shared/model/settings.store.ts:7',
        }),
        expect.objectContaining({
          type: 'settings-store-error-copy',
          location: 'src/app/shared/model/settings.store.ts:11',
        }),
      ])
    )
  })

  it('accepts Settings store errors that start with the recovery step', () => {
    const cwd = fixture({
      'src/app/shared/model/settings.store.ts': `
function settingsUnavailableMessage(operation, actionPhrase) {
  return \`Refresh Settings, then try to \${actionPhrase} again. Forge could not \${operation} right now.\`
}

function settingsDefaultMessage(actionPhrase) {
  return \`Refresh Settings, then try to \${actionPhrase} again. Settings could not \${actionPhrase}.\`
}

function settingsPermissionMessage(actionPhrase) {
  return \`Ask an owner or admin to manage Settings, then try to \${actionPhrase} again. You do not have permission to \${actionPhrase}.\`
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
function permissionMessage() {
  return 'You do not have permission to save this team. Ask an owner or admin to update your team space access.'
}
`,
      'src/app/layouts/sidebar/ProjectTree.tsx': `
function renameErrorMessage() {
  return 'Project name could not be saved. Refresh the sidebar and try again.'
}
function projectMissingMessage() {
  return 'This project could not be found. Refresh the sidebar, then choose the current project again.'
}
function projectChangedMessage() {
  return 'This project changed while you were editing. Refresh the sidebar, review the current name, then try again.'
}
function renameBusyMessage() {
  return 'The sidebar is busy. Wait a moment, then save this project name again.'
}
function renameServiceMessage() {
  return 'Forge could not save this project name right now. Refresh the sidebar, then save again.'
}
function deleteServiceMessage() {
  return 'Forge could not delete this project right now. Refresh the sidebar, then try again.'
}
function permissionRenameMessage() {
  return 'You do not have permission to rename this project. Ask an owner or admin to let you edit this project.'
}
function permissionDeleteMessage() {
  return 'You do not have permission to delete this team. Ask an owner or admin to let you delete this team.'
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
        expect.objectContaining({
          type: 'workspace-resource-copy',
          location: 'src/app/shared/lib/workspaceResourceErrorMessage.ts:12',
        }),
        expect.objectContaining({
          type: 'workspace-resource-copy',
          location: 'src/app/layouts/sidebar/ProjectTree.tsx:3',
        }),
        expect.objectContaining({
          type: 'workspace-resource-copy',
          location: 'src/app/layouts/sidebar/ProjectTree.tsx:6',
        }),
        expect.objectContaining({
          type: 'workspace-resource-copy',
          location: 'src/app/layouts/sidebar/ProjectTree.tsx:9',
        }),
        expect.objectContaining({
          type: 'workspace-resource-copy',
          location: 'src/app/layouts/sidebar/ProjectTree.tsx:12',
        }),
        expect.objectContaining({
          type: 'workspace-resource-copy',
          location: 'src/app/layouts/sidebar/ProjectTree.tsx:15',
        }),
        expect.objectContaining({
          type: 'workspace-resource-copy',
          location: 'src/app/layouts/sidebar/ProjectTree.tsx:18',
        }),
        expect.objectContaining({
          type: 'workspace-resource-copy',
          location: 'src/app/layouts/sidebar/ProjectTree.tsx:21',
        }),
        expect.objectContaining({
          type: 'workspace-resource-copy',
          location: 'src/app/layouts/sidebar/ProjectTree.tsx:24',
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
function permissionMessage() {
  return 'Ask an owner or admin to update your team space access, then save the team again in Settings. You do not have permission to save this team.'
}
`,
      'src/app/layouts/sidebar/ProjectTree.tsx': `
function renameErrorMessage() {
  return 'Refresh the left menu, then save this project name again. The project name was not saved.'
}
function projectMissingMessage() {
  return 'Refresh the left menu, then choose the current project again. This project could not be found.'
}
function projectChangedMessage() {
  return 'Refresh the left menu, review the current name, then save this project name again. This project changed while you were editing.'
}
function renameBusyMessage() {
  return 'Wait a moment, then save this project name again. The left menu is busy.'
}
function renameServiceMessage() {
  return 'Refresh the left menu, then save this project name again. Forge could not save it right now.'
}
function deleteServiceMessage() {
  return 'Refresh the left menu, then delete this project again. Forge could not delete it right now.'
}
function permissionRenameMessage() {
  return 'Ask an owner or admin to let you edit this project, then save this project name again from the left menu. You do not have permission to rename this project.'
}
function permissionDeleteMessage() {
  return 'Ask an owner or admin to let you delete this team, then delete it again from the left menu. You do not have permission to delete this team.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags AuthManager fallbacks that stop at a failure label', () => {
    const cwd = fixture({
      'src/app/shared/auth/AuthManager.ts': `
const LOGIN_FALLBACK = 'Login failed'
const REGISTER_FALLBACK = 'Registration failed'
const SSO_FALLBACK = 'Auth code exchange failed'
const RESEND_FALLBACK = 'Failed to resend'
const FORGOT_FALLBACK = 'Failed to send reset email'
const RESET_FALLBACK = 'Failed to reset password'
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'auth-manager-copy',
          location: 'src/app/shared/auth/AuthManager.ts:2',
        }),
        expect.objectContaining({
          type: 'auth-manager-copy',
          location: 'src/app/shared/auth/AuthManager.ts:3',
        }),
        expect.objectContaining({
          type: 'auth-manager-copy',
          location: 'src/app/shared/auth/AuthManager.ts:4',
        }),
        expect.objectContaining({
          type: 'auth-manager-copy',
          location: 'src/app/shared/auth/AuthManager.ts:5',
        }),
        expect.objectContaining({
          type: 'auth-manager-copy',
          location: 'src/app/shared/auth/AuthManager.ts:6',
        }),
        expect.objectContaining({
          type: 'auth-manager-copy',
          location: 'src/app/shared/auth/AuthManager.ts:7',
        }),
      ])
    )
  })

  it('accepts AuthManager fallbacks that start with recovery steps', () => {
    const cwd = fixture({
      'src/app/shared/auth/AuthManager.ts': `
const LOGIN_FALLBACK = 'Check your email and password, then try signing in again. Forge could not finish sign-in.'
const REGISTER_FALLBACK = 'Check the account details, then create the account again. Forge could not finish account setup.'
const SSO_FALLBACK = 'Start sign-in again from this page. Forge could not finish this sign-in link.'
const RESEND_FALLBACK = 'Check the email address, then send the verification email again. Forge could not finish sending it.'
const FORGOT_FALLBACK = 'Check the email address, then request the reset email again. Forge could not finish sending it.'
const RESET_FALLBACK = 'Check the password rules, then save the new password again. Forge could not finish password reset.'
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags project creation errors that start with the failure', () => {
    const cwd = fixture({
      'src/app/features/manage-project/ui/CreateProjectForm.tsx': `
function createProjectErrorMessage(code) {
  return 'Too many project changes are happening right now. Wait a minute, then create this project again.'
}
function serverProjectErrorMessage() {
  return 'Forge could not create the project right now. Wait a few minutes, then try again. If it still fails, ask an owner or admin to check project setup.'
}
function fallbackProjectErrorMessage() {
  return 'Could not create the project. Check the project name and team, then try again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'project-create-error-copy',
          location: 'src/app/features/manage-project/ui/CreateProjectForm.tsx:3',
        }),
        expect.objectContaining({
          type: 'project-create-error-copy',
          location: 'src/app/features/manage-project/ui/CreateProjectForm.tsx:6',
        }),
        expect.objectContaining({
          type: 'project-create-error-copy',
          location: 'src/app/features/manage-project/ui/CreateProjectForm.tsx:9',
        }),
      ])
    )
  })

  it('accepts project creation errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/features/manage-project/ui/CreateProjectForm.tsx': `
function createProjectErrorMessage(code) {
  if (code === 429) return 'Wait a minute, then create this project again. Too many project changes are happening right now.'
  if (code >= 500) return 'Wait a few minutes, then create this project again. Forge could not create the project right now. If it still fails, ask an owner or admin to check project setup.'
  return 'Check the project name and team, then create this project again. Forge could not create the project.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags project setup overview copy that exposes evidence jargon', () => {
    const cwd = fixture({
      'src/app/features/manage-project/ui/CreateProjectForm.tsx': `
function ProjectSetupPath() {
  return <p>Use projects for the work areas where agents receive tasks and evidence.</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'project-create-overview-copy',
        location: 'src/app/features/manage-project/ui/CreateProjectForm.tsx:3',
      }),
    ])
  })

  it('accepts project setup overview copy that describes saved work records plainly', () => {
    const cwd = fixture({
      'src/app/features/manage-project/ui/CreateProjectForm.tsx': `
function ProjectSetupPath() {
  return <p>Use projects to keep one work area&apos;s tasks, files, and saved work records together.</p>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags project creation code-link copy that falls back to repository URL wording', () => {
    const cwd = fixture({
      'src/app/features/manage-project/ui/CreateProjectForm.tsx': `
function CreateProjectForm() {
  return <><label>Git repository URL</label><input placeholder="https://github.com/org/repo.git" /><p>Clone an existing repo into this project.</p></>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'project-create-code-link-copy',
          sample: expect.stringContaining('Git repository URL'),
        }),
        expect.objectContaining({
          type: 'project-create-code-link-copy',
          sample: expect.stringContaining('org/repo.git'),
        }),
        expect.objectContaining({
          type: 'project-create-code-link-copy',
          sample: expect.stringContaining('repo'),
        }),
      ])
    )
  })

  it('accepts project creation code-link copy that uses code-link wording', () => {
    const cwd = fixture({
      'src/app/features/manage-project/ui/CreateProjectForm.tsx': `
function CreateProjectForm() {
  return <><label>Code link</label><input placeholder="https://github.com/team/project.git" /><p>Forge copies that code into this project.</p></>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags team and project creation copy that exposes setup path or address preview wording', () => {
    const cwd = fixture({
      'src/app/features/manage-team/ui/CreateTeamForm.tsx': `
function CreateTeamForm() {
  return <><p>Team setup path</p><p>Address preview: platform-ops.</p></>
}
`,
      'src/app/features/manage-project/ui/CreateProjectForm.tsx': `
function CreateProjectForm() {
  return <><p>Project setup path</p><p>Work folder preview: /workspace/app</p></>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'team-project-create-copy',
          location: 'src/app/features/manage-team/ui/CreateTeamForm.tsx:3',
          sample: expect.stringContaining('Team setup path'),
        }),
        expect.objectContaining({
          type: 'team-project-create-copy',
          location: 'src/app/features/manage-project/ui/CreateProjectForm.tsx:3',
          sample: expect.stringContaining('Project setup path'),
        }),
      ])
    )
  })

  it('accepts team and project creation copy that uses steps and short-name wording', () => {
    const cwd = fixture({
      'src/app/features/manage-team/ui/CreateTeamForm.tsx': `
function CreateTeamForm() {
  return <><p>Team creation steps</p><p>Team short name: platform-ops.</p></>
}
`,
      'src/app/features/manage-project/ui/CreateProjectForm.tsx': `
function CreateProjectForm() {
  return <><p>Project creation steps</p><p>Project short name: app.</p><p>Agent work folder: /workspace/app</p></>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags team and project row copy that labels generated names as addresses', () => {
    const cwd = fixture({
      'src/app/features/manage-team/ui/EditableTeamRow.tsx': `
function EditableTeamRow({ team }) {
  return <p>Address: {team.slug}</p>
}
`,
      'src/app/features/manage-project/ui/EditableProjectRow.tsx': `
function EditableProjectRow({ project }) {
  return <span>Address: {project.slug}</span>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'team-project-row-address-copy',
          location: 'src/app/features/manage-team/ui/EditableTeamRow.tsx:3',
        }),
        expect.objectContaining({
          type: 'team-project-row-address-copy',
          location: 'src/app/features/manage-project/ui/EditableProjectRow.tsx:3',
        }),
      ])
    )
  })

  it('accepts team and project row copy that labels generated names as short names', () => {
    const cwd = fixture({
      'src/app/features/manage-team/ui/EditableTeamRow.tsx': `
function EditableTeamRow({ team }) {
  return <p>Team short name: {team.slug}</p>
}
`,
      'src/app/features/manage-project/ui/EditableProjectRow.tsx': `
function EditableProjectRow({ project }) {
  return <span>Project short name: {project.slug}</span>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags sidebar and admin labels that expose generated names as link or URL names', () => {
    const cwd = fixture({
      'src/app/layouts/sidebar/ProjectTree.tsx': `
function ProjectTree({ projectMenu }) {
  return <p>{projectMenu.team.name} team · link name {projectMenu.project.slug}</p>
}
`,
      'src/app/features/admin/OrganizationsPanel.tsx': `
function OrganizationsPanel({ org }) {
  return <p>URL name: {org.slug}</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'team-project-short-name-copy',
          location: 'src/app/layouts/sidebar/ProjectTree.tsx:3',
        }),
        expect.objectContaining({
          type: 'team-project-short-name-copy',
          location: 'src/app/features/admin/OrganizationsPanel.tsx:3',
        }),
      ])
    )
  })

  it('accepts sidebar and admin labels that explain generated names as short names', () => {
    const cwd = fixture({
      'src/app/layouts/sidebar/ProjectTree.tsx': `
function ProjectTree({ projectMenu }) {
  return <p>{projectMenu.team.name} team · project short name {projectMenu.project.slug}</p>
}
`,
      'src/app/features/admin/OrganizationsPanel.tsx': `
function OrganizationsPanel({ org }) {
  return <p>Team space short name: {org.slug}</p>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags code import retry errors that start with the failure', () => {
    const cwd = fixture({
      'src/app/features/manage-project/ui/CloneStatusBadge.tsx': `
function cloneRetryErrorMessage(code) {
  return 'Too many code import retries are happening right now. Wait a minute, then try again.'
}
function serverCloneErrorMessage() {
  return 'Forge could not copy code right now. Wait a few minutes, then try again.'
}
function fallbackCloneErrorMessage() {
  return 'Could not copy code into the project. Check the code link and saved code access, then try again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'clone-retry-error-copy',
          location: 'src/app/features/manage-project/ui/CloneStatusBadge.tsx:3',
        }),
        expect.objectContaining({
          type: 'clone-retry-error-copy',
          location: 'src/app/features/manage-project/ui/CloneStatusBadge.tsx:6',
        }),
        expect.objectContaining({
          type: 'clone-retry-error-copy',
          location: 'src/app/features/manage-project/ui/CloneStatusBadge.tsx:9',
        }),
      ])
    )
  })

  it('accepts code import retry errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/features/manage-project/ui/CloneStatusBadge.tsx': `
function permissionCloneRetryErrorMessage() {
  return 'Ask an owner or admin to let you copy code into this project, then try again. You do not have permission right now.'
}
function busyCloneRetryErrorMessage() {
  return 'Wait a minute, then try copying code again. Too many copy retries are happening right now.'
}
function fallbackCloneRetryErrorMessage() {
  return 'Check the code link and saved code access, then try copying code again. Forge could not copy code into the project.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags code import status and recovery labels that do not match copy wording', () => {
    const cwd = fixture({
      'src/app/features/manage-project/ui/CloneStatusBadge.tsx': `
const VISUALS = {
  queued: { label: 'Code import queued' },
  ready: { label: 'Code ready' },
  failed: { label: 'Code import failed' },
}
function cloneFailureMessage() {
  return 'Forge could not finish the code import.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'clone-status-label-copy',
          location: 'src/app/features/manage-project/ui/CloneStatusBadge.tsx:3',
        }),
        expect.objectContaining({
          type: 'clone-status-label-copy',
          location: 'src/app/features/manage-project/ui/CloneStatusBadge.tsx:4',
        }),
        expect.objectContaining({
          type: 'clone-status-label-copy',
          location: 'src/app/features/manage-project/ui/CloneStatusBadge.tsx:5',
        }),
        expect.objectContaining({
          type: 'clone-status-label-copy',
          location: 'src/app/features/manage-project/ui/CloneStatusBadge.tsx:8',
        }),
      ])
    )
  })

  it('accepts code copy status labels that describe the user-visible action', () => {
    const cwd = fixture({
      'src/app/features/manage-project/ui/CloneStatusBadge.tsx': `
const VISUALS = {
  queued: { label: 'Code copy waiting' },
  cloning: { label: 'Copying code…' },
  ready: { label: 'Code copied' },
  failed: { label: 'Code copy needs help' },
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags direct rendering of code import failure details', () => {
    const cwd = fixture({
      'src/app/features/manage-project/ui/CloneStatusBadge.tsx': `
export function CloneStatusBadge({ clone }) {
  return <p>{clone.errorMessage}</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'clone-failure-message-copy',
          location: 'src/app/features/manage-project/ui/CloneStatusBadge.tsx:3',
        }),
      ])
    )
  })

  it('accepts code import failure summaries that do not expose raw details', () => {
    const cwd = fixture({
      'src/app/features/manage-project/ui/CloneStatusBadge.tsx': `
function cloneFailureMessage(clone) {
  if (clone.errorClass === 'auth') {
    return 'Check saved code access for this code project, then try copying code again. The code website rejected Forge access.'
  }
  return 'Check the code link and saved code access, then try copying code again. Forge could not finish copying code.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags code copy failure summaries that fall back to repository wording', () => {
    const cwd = fixture({
      'src/app/features/manage-project/ui/CloneStatusBadge.tsx': `
function authFailureMessage() {
  return 'Check saved code access for this repository, then try copying code again.'
}
function networkFailureMessage() {
  return 'Check your connection and repository host, then try copying code again.'
}
function timeoutFailureMessage() {
  return 'The repository took too long to respond.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'clone-failure-repository-copy',
          sample: expect.stringContaining('this repository'),
        }),
        expect.objectContaining({
          type: 'clone-failure-repository-copy',
          sample: expect.stringContaining('repository host'),
        }),
        expect.objectContaining({
          type: 'clone-failure-repository-copy',
          sample: expect.stringContaining('The repository'),
        }),
      ])
    )
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
  return 'Refresh the detail panel to load saved notes and work history. If it still fails, ask an owner or admin to check task setup.'
}
`,
      'src/app/features/context/approvalQueueErrorMessages.ts': `
function serviceRecoveryMessage(action) {
  return 'Refresh the list so you see the latest saved items. The saved item review list could not load. If it still fails, ask an owner or admin to check saved item setup.'
}
`,
      'src/app/entities/navigation/model/navigation.store.ts': `
function navigationActionErrorMessage(actionPhrase) {
  return 'Check your connection, then refresh the left menu to load task queues.'
}
function serviceRecoveryMessage() {
  return 'Refresh the left menu to load workspace navigation. If it still fails, ask an owner or admin to check workspace navigation.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags saved item review errors that start with the failure instead of the next step', () => {
    const cwd = fixture({
      'src/app/features/context/approvalQueueErrorMessages.ts': `
const ACTION_FALLBACKS = {
  approveCandidate: 'The item was not approved. Check who can reuse it and the original task preview, then try again.',
  loadQueue: 'The saved item review list could not load. Refresh the list so you see the latest items.',
  rejectCandidate: 'The item was not rejected. Refresh the list, then try the reject action again.',
}
function forbidden() {
  return 'You do not have permission to review saved items. Ask an owner or admin to let you approve saved notes and instructions.'
}
function missing() {
  return 'This item was not found. Refresh the list so you see the latest items.'
}
function conflict() {
  return 'This item changed while you were reviewing it. Refresh the list, then open it again.'
}
function busy() {
  return 'The saved item review list is busy. Wait a moment, then try again.'
}
function network() {
  return 'Forge could not connect while saving this review decision. Check your connection, then try again.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'approval-queue-error-copy',
          location: 'src/app/features/context/approvalQueueErrorMessages.ts:3',
        }),
        expect.objectContaining({
          type: 'approval-queue-error-copy',
          location: 'src/app/features/context/approvalQueueErrorMessages.ts:4',
        }),
        expect.objectContaining({
          type: 'approval-queue-error-copy',
          location: 'src/app/features/context/approvalQueueErrorMessages.ts:5',
        }),
        expect.objectContaining({
          type: 'approval-queue-error-copy',
          location: 'src/app/features/context/approvalQueueErrorMessages.ts:8',
        }),
        expect.objectContaining({
          type: 'approval-queue-error-copy',
          location: 'src/app/features/context/approvalQueueErrorMessages.ts:11',
        }),
        expect.objectContaining({
          type: 'approval-queue-error-copy',
          location: 'src/app/features/context/approvalQueueErrorMessages.ts:14',
        }),
        expect.objectContaining({
          type: 'approval-queue-error-copy',
          location: 'src/app/features/context/approvalQueueErrorMessages.ts:17',
        }),
        expect.objectContaining({
          type: 'approval-queue-error-copy',
          location: 'src/app/features/context/approvalQueueErrorMessages.ts:20',
        }),
      ])
    )
  })

  it('accepts saved item review errors that start with the next step', () => {
    const cwd = fixture({
      'src/app/features/context/approvalQueueErrorMessages.ts': `
const ACTION_FALLBACKS = {
  approveCandidate: 'Check who can reuse it and the original task preview, then approve the item again. The item was not approved.',
  loadQueue: 'Refresh the list so you see the latest saved items. The saved item review list could not load.',
  rejectCandidate: 'Refresh the list, then reject the item again. The item was not rejected.',
}
function forbidden() {
  return 'Ask an owner or admin to let you approve saved notes and instructions, then retry this review action. You do not have permission right now.'
}
function missing() {
  return 'Refresh the list so you see the latest saved items. This item was not found.'
}
function conflict() {
  return 'Refresh the list, then open this item again. It changed while you were reviewing it.'
}
function busy() {
  return 'Wait a moment, then try again. The saved item review list is busy.'
}
function network() {
  return 'Check your connection, then try this review action again. Forge could not connect while saving this review decision.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task saved-item detail copy that exposes run-detail jargon', () => {
    const cwd = fixture({
      'src/app/features/detail/ContextTab.tsx': `
function ContextEmptyState() {
  return 'No saved notes or run details yet. Work run 1 helped during this run.'
}
`,
      'src/app/features/detail/ContextEvidenceList.tsx': `
function evidenceTitle() {
  return 'Run details'
}
function payloadSummary() {
  return 'Additional run details with 2 pieces of information.'
}
`,
      'src/app/features/detail/ContextCandidatesList.tsx': `
function sectionDescription() {
  return 'These are suggested notes from the run.'
}
`,
      'src/app/features/chat/ToolCallDetail.tsx': `
function formatTechnicalDetails() {
  return 'Support can check the run details if needed.'
}
`,
      'src/app/features/board/KanbanColumn.tsx': `
function emptyState() {
  return 'No active runs'
}
`,
      'src/app/entities/context/ui/FeedbackControls.tsx': `
function feedbackHelp() {
  return 'Your answer helps future runs choose safer saved items.'
}
`,
      'src/app/features/inbox/InboxView.tsx': `
function nextStepDescription() {
  return 'Fixing it keeps future agent runs from failing.'
}
`,
      'src/app/features/billing/UsageMeter.tsx': `
function highAction() {
  return 'Review busy agents before more agent runs are blocked.'
}
`,
      'src/app/features/analytics/AnalyticsDashboard.tsx': `
function emptyChart() {
  return 'Tool use appears after an agent runs a task.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'context-work-history-copy',
          location: 'src/app/features/detail/ContextTab.tsx:3',
        }),
        expect.objectContaining({
          type: 'context-work-history-copy',
          location: 'src/app/features/detail/ContextEvidenceList.tsx:3',
        }),
        expect.objectContaining({
          type: 'context-work-history-copy',
          location: 'src/app/features/detail/ContextEvidenceList.tsx:6',
        }),
        expect.objectContaining({
          type: 'context-work-history-copy',
          location: 'src/app/features/detail/ContextCandidatesList.tsx:3',
        }),
        expect.objectContaining({
          type: 'context-work-history-copy',
          location: 'src/app/features/chat/ToolCallDetail.tsx:3',
        }),
        expect.objectContaining({
          type: 'context-work-history-copy',
          location: 'src/app/features/board/KanbanColumn.tsx:3',
        }),
        expect.objectContaining({
          type: 'context-work-history-copy',
          location: 'src/app/entities/context/ui/FeedbackControls.tsx:3',
        }),
        expect.objectContaining({
          type: 'context-work-history-copy',
          location: 'src/app/features/inbox/InboxView.tsx:3',
        }),
        expect.objectContaining({
          type: 'context-work-history-copy',
          location: 'src/app/features/billing/UsageMeter.tsx:3',
        }),
        expect.objectContaining({
          type: 'context-work-history-copy',
          location: 'src/app/features/analytics/AnalyticsDashboard.tsx:3',
        }),
      ])
    )
  })

  it('accepts task saved-item and work copy that uses work history wording', () => {
    const cwd = fixture({
      'src/app/features/detail/ContextTab.tsx': `
function ContextEmptyState() {
  return 'No saved notes or work history yet. Check 1 helped during this task.'
}
`,
      'src/app/features/detail/ContextEvidenceList.tsx': `
function evidenceTitle() {
  return 'Work details'
}
function payloadSummary() {
  return 'Additional work details with 2 pieces of information.'
}
`,
      'src/app/features/detail/ContextCandidatesList.tsx': `
function sectionDescription() {
  return 'These are suggested notes from this task.'
}
`,
      'src/app/features/board/KanbanColumn.tsx': `
function emptyState() {
  return 'No work in progress'
}
`,
      'src/app/entities/context/ui/FeedbackControls.tsx': `
function feedbackHelp() {
  return 'Your answer helps future tasks choose safer saved items.'
}
`,
      'src/app/features/inbox/InboxView.tsx': `
function nextStepDescription() {
  return 'Fixing it keeps future agent work from failing.'
}
`,
      'src/app/features/billing/UsageMeter.tsx': `
function highAction() {
  return 'Review busy agents before more agent work is blocked.'
}
`,
      'src/app/features/analytics/AnalyticsDashboard.tsx': `
function emptyChart() {
  return 'Tool use appears after an agent finishes a task.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task result record titles that expose source-type jargon', () => {
    const cwd = fixture({
      'src/app/features/detail/ContextEvidenceList.tsx': `
function evidenceTitle(item) {
  if (item.sourceType === 'task_result') return 'Task result'
  if (item.sourceType === 'tool_call') return 'Tool activity'
  if (item.sourceType === 'source_message') return 'Source message'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'context-evidence-source-title-copy',
          location: 'src/app/features/detail/ContextEvidenceList.tsx:3',
        }),
        expect.objectContaining({
          type: 'context-evidence-source-title-copy',
          location: 'src/app/features/detail/ContextEvidenceList.tsx:4',
        }),
        expect.objectContaining({
          type: 'context-evidence-source-title-copy',
          location: 'src/app/features/detail/ContextEvidenceList.tsx:5',
        }),
      ])
    )
  })

  it('accepts task result record titles that explain what users are checking', () => {
    const cwd = fixture({
      'src/app/features/detail/ContextEvidenceList.tsx': `
function evidenceTitle(item) {
  if (item.sourceType === 'task_result') return 'Final answer'
  if (item.sourceType === 'tool_call') return 'Step the agent took'
  if (item.sourceType === 'source_message') return 'Message used for this work'
  return 'Work details'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags inbox needs-action empty copy that says nothing instead of caught up', () => {
    const cwd = fixture({
      'src/app/features/inbox/InboxView.tsx': `
function needsActionEmptyState() {
  return 'Nothing needs action right now'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'inbox-needs-action-empty-copy',
          location: 'src/app/features/inbox/InboxView.tsx:3',
        }),
      ])
    )
  })

  it('accepts inbox needs-action empty copy that says the user is caught up', () => {
    const cwd = fixture({
      'src/app/features/inbox/InboxView.tsx': `
function needsActionEmptyState() {
  return 'You are caught up on action items'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags inbox load errors that start with the failure summary', () => {
    const cwd = fixture({
      'src/app/features/inbox/InboxView.tsx': `
function InboxLoadError() {
  return 'Saved notifications could not be loaded. New updates will still appear here. Check your connection, then reload the inbox.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'inbox-load-error-copy',
        location: 'src/app/features/inbox/InboxView.tsx:3',
      }),
    ])
  })

  it('accepts inbox load errors that start with the reload action', () => {
    const cwd = fixture({
      'src/app/features/inbox/InboxView.tsx': `
function InboxLoadError() {
  return 'Check your connection, then reload the inbox. Saved notifications could not be loaded, but new updates will still appear here.'
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

  it('flags failed task card next steps that tell beginners to fix an error', () => {
    const cwd = fixture({
      'src/app/features/board/TaskCard.tsx': `
function taskNextStep() {
  return 'Open details, fix the error, then retry.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-recovery-status-copy',
          location: 'src/app/features/board/TaskCard.tsx:3',
        }),
      ])
    )
  })

  it('accepts failed task card next steps that point to task details and the recovery note', () => {
    const cwd = fixture({
      'src/app/features/board/TaskCard.tsx': `
function taskNextStep() {
  return 'Open task details, read the recovery note, then retry.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task recovery entry points that say open details without naming task details', () => {
    const cwd = fixture({
      'src/app/features/board/TaskCard.tsx': `
function taskNextStep() {
  return 'Open details, review the recovery note, then retry.'
}
`,
      'src/app/features/feed/FeedItem.tsx': `
function displayFeedDetail() {
  return 'Open details to see the recovery note, then retry or choose another agent.'
}
`,
      'src/app/features/feed/AttentionZone.tsx': `
export function AttentionZone() {
  return <button>Open details</button>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-recovery-details-copy',
          location: 'src/app/features/board/TaskCard.tsx:3',
        }),
        expect.objectContaining({
          type: 'task-recovery-details-copy',
          location: 'src/app/features/feed/FeedItem.tsx:3',
        }),
        expect.objectContaining({
          type: 'task-recovery-details-copy',
          location: 'src/app/features/feed/AttentionZone.tsx:3',
        }),
      ])
    )
  })

  it('accepts task recovery entry points that name task details', () => {
    const cwd = fixture({
      'src/app/features/board/TaskCard.tsx': `
function taskNextStep() {
  return 'Open task details, read the recovery note, then retry.'
}
`,
      'src/app/features/feed/FeedItem.tsx': `
function displayFeedDetail() {
  return 'Open task details to read the recovery note, then retry or choose another agent.'
}
`,
      'src/app/features/feed/AttentionZone.tsx': `
export function AttentionZone() {
  return <button>Open task details</button>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task failure previews that say open details without naming the task details', () => {
    const cwd = fixture({
      'src/app/shared/lib/taskFailureCopy.ts': `
export function taskFailurePreview() {
  return 'Stopped before finishing. Open details to see what happened and retry.'
}
export function taskBlockedPreview() {
  return 'This task needs help before it can continue. Open details, review the latest update, then retry or ask an owner for help.'
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-failure-details-copy',
          location: 'src/app/shared/lib/taskFailureCopy.ts:3',
        }),
        expect.objectContaining({
          type: 'task-failure-details-copy',
          location: 'src/app/shared/lib/taskFailureCopy.ts:6',
        }),
      ])
    )
  })

  it('accepts task failure previews that name task details and latest updates', () => {
    const cwd = fixture({
      'src/app/shared/lib/taskFailureCopy.ts': `
export function taskFailurePreview() {
  return 'Stopped before finishing. Open the task details, review the latest update, then retry when ready.'
}
export function taskBlockedPreview() {
  return 'This task needs help before it can continue. Open the task details, review the latest update, then retry or ask an owner for help.'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags failed task board empty copy that uses retry-path wording', () => {
    const cwd = fixture({
      'src/app/features/board/KanbanColumn.tsx': `
const COLUMN_EMPTY_STATE = {
  failed: { title: 'Retry paths appear here after a task stops', detail: 'Review the recovery note and retry path.' },
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'task-recovery-status-copy',
        location: 'src/app/features/board/KanbanColumn.tsx:3',
      }),
    ])
  })

  it('accepts failed task board empty copy that uses retry-step wording', () => {
    const cwd = fixture({
      'src/app/features/board/KanbanColumn.tsx': `
const COLUMN_EMPTY_STATE = {
  failed: { title: 'Retry steps appear here after a task stops', detail: 'Review the recovery note and retry steps.' },
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task detail reuse copy that describes a save-for-next-time path', () => {
    const cwd = fixture({
      'src/app/features/detail/DescriptionTab.tsx': `
function DescriptionTab() {
  return <p>The save-for-next-time path becomes available once useful work is completed.</p>
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual([
      expect.objectContaining({
        type: 'task-reuse-path-copy',
        location: 'src/app/features/detail/DescriptionTab.tsx:3',
      }),
    ])
  })

  it('accepts task detail reuse copy that describes a save-for-next-time option', () => {
    const cwd = fixture({
      'src/app/features/detail/DescriptionTab.tsx': `
function DescriptionTab() {
  return <p>The save-for-next-time option becomes available once useful work is completed.</p>
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags failed task list and detail copy that still asks beginners to read failures', () => {
    const cwd = fixture({
      'src/app/features/list/ListView.tsx': `
function taskNextAction() {
  return 'Open it, read the failure, then retry only after the cause is clear.'
}
`,
      'src/app/features/detail/DescriptionTab.tsx': `
function nextActionForTask() {
  return { title: 'Triage failure' }
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-recovery-status-copy',
          location: 'src/app/features/list/ListView.tsx:3',
        }),
        expect.objectContaining({
          type: 'task-recovery-status-copy',
          location: 'src/app/features/detail/DescriptionTab.tsx:3',
        }),
      ])
    )
  })

  it('accepts task list and detail recovery copy that points to the next step', () => {
    const cwd = fixture({
      'src/app/features/list/ListView.tsx': `
function taskNextAction() {
  return 'Open it, review the recovery note, then retry only after the next step is clear.'
}
`,
      'src/app/features/detail/DescriptionTab.tsx': `
function nextActionForTask() {
  return { title: 'Review recovery' }
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags task detail empty copy that leaves beginners without a next step', () => {
    const cwd = fixture({
      'src/app/features/detail/DescriptionTab.tsx': `
function Work() {
  return (
    <>
      <p>No description provided.</p>
      <p>No result files were attached.</p>
    </>
  )
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-detail-empty-copy',
          location: 'src/app/features/detail/DescriptionTab.tsx:5',
        }),
        expect.objectContaining({
          type: 'task-detail-empty-copy',
          location: 'src/app/features/detail/DescriptionTab.tsx:6',
        }),
      ])
    )
  })

  it('flags canceled task detail titles that describe a dead end', () => {
    const cwd = fixture({
      'src/app/features/detail/DescriptionTab.tsx': `
function nextActionForTask() {
  return { title: 'No current work' }
}
`,
      'src/app/features/detail/HistoryTab.tsx': `
function taskCheckIn() {
  return { title: 'No current agent work' }
}
`,
    })

    const result = checkBeginnerUxCopy({ cwd })

    expect(result.ok).toBe(false)
    expect(result.findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'task-detail-empty-copy',
          location: 'src/app/features/detail/DescriptionTab.tsx:3',
        }),
        expect.objectContaining({
          type: 'task-detail-empty-copy',
          location: 'src/app/features/detail/HistoryTab.tsx:3',
        }),
      ])
    )
  })

  it('accepts task detail empty copy that points beginners to the next step', () => {
    const cwd = fixture({
      'src/app/features/detail/DescriptionTab.tsx': `
function Work() {
  return (
    <>
      <p>No brief was saved. Open Updates to see what was asked before accepting, retrying, or closing this task.</p>
      <p>No result files were saved. Use Next action above, then retry or create a follow-up task if files are still needed.</p>
    </>
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
