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
        '无法加载工作设置。请刷新这个设置页。如果仍然失败，请找 owner 或 admin 检查 Agent 工作设置。',
    },
  },
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
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
    ])
  })

  it('accepts admin agent activity copy that says work must start first', () => {
    const cwd = fixture({
      'src/app/features/admin/AgentsPanel.tsx': `
function formatLastActivity(epochMs) {
  if (!epochMs) return 'Activity appears after work starts'
}
`,
    })

    expect(checkBeginnerUxCopy({ cwd })).toEqual({ ok: true, findings: [] })
  })

  it('flags generic compact work-location labels in the runtime label helper', () => {
    const cwd = fixture({
      'src/app/entities/agent/model/runtime-kind.ts': `
export function runtimeKindShortLabel(kind) {
  if (!kind) return 'Not reported'
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
        location: 'src/app/entities/agent/model/runtime-kind.ts:4',
      }),
    ])
  })

  it('accepts compact work-location labels that name the missing location', () => {
    const cwd = fixture({
      'src/app/entities/agent/model/runtime-kind.ts': `
export function runtimeKindShortLabel(kind) {
  if (!kind) return 'Location missing'
  return 'Review location'
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

  it('flags app health status copy that does not tell users to check now', () => {
    const cwd = fixture({
      'src/app/features/admin/SystemHealth.tsx': `
type ServiceStatus = 'ready' | 'unknown'

function serviceStatusText(status: ServiceStatus): string {
  if (status === 'ready') return 'Ready'
  return 'Not checked yet'
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
