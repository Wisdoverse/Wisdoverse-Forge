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
})
