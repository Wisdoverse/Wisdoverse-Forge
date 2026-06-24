import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'

import { checkFsdBoundaries } from '../../../scripts/check-fsd-boundaries.mjs'

let tempRoots: string[] = []

function fixture(files: Record<string, string>) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'fsd-boundaries-'))
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

describe('check-fsd-boundaries.mjs', () => {
  it('flags route files that bypass the pages layer to mount feature views', () => {
    const cwd = fixture({
      'src/app/routes/tasks.tsx': `
import { BoardView } from '@app/features/board/BoardView'

export function TasksRoute() {
  return <BoardView />
}
`,
      'src/app/features/board/BoardView.tsx': `
export function BoardView() {
  return null
}
`,
    })

    const result = checkFsdBoundaries({ cwd })

    expect(result.ok).toBe(false)
    expect(result.errors).toEqual([
      expect.stringContaining(
        'route files must render @app/pages/* entrypoints instead of feature/widget view modules'
      ),
    ])
  })

  it('accepts routes that mount pages while pages compose features', () => {
    const cwd = fixture({
      'src/app/routes/tasks.tsx': `
import { TasksPage } from '@app/pages/tasks'

export function TasksRoute() {
  return <TasksPage />
}
`,
      'src/app/pages/tasks/index.tsx': `
import { BoardView } from '@app/features/board/BoardView'

export function TasksPage() {
  return <BoardView />
}
`,
      'src/app/features/board/BoardView.tsx': `
export function BoardView() {
  return null
}
`,
    })

    expect(checkFsdBoundaries({ cwd })).toEqual({ ok: true, errors: [] })
  })

  it('flags route files that import non-page exports from page entrypoints', () => {
    const cwd = fixture({
      'src/app/routes/settings.tsx': `
import { SettingsLayout } from '@app/pages/settings'

export function SettingsRoute() {
  return <SettingsLayout />
}
`,
      'src/app/pages/settings/index.ts': `
export { SettingsLayout } from './ui/SettingsLayout'
`,
      'src/app/pages/settings/ui/SettingsLayout.tsx': `
export function SettingsLayout() {
  return null
}
`,
    })

    const result = checkFsdBoundaries({ cwd })

    expect(result.ok).toBe(false)
    expect(result.errors).toEqual([
      expect.stringContaining('route files must import Page entrypoints from @app/pages/*'),
    ])
  })

  it('flags route files that re-export non-page components from page entrypoints', () => {
    const cwd = fixture({
      'src/app/routes/tasks.tsx': `
import { TasksPage } from '@app/pages/tasks'

export { TaskViewLoadingFallback } from '@app/pages/tasks'

export function TasksRoute() {
  return <TasksPage />
}
`,
      'src/app/pages/tasks/index.tsx': `
export function TaskViewLoadingFallback() {
  return null
}

export function TasksPage() {
  return null
}
`,
    })

    const result = checkFsdBoundaries({ cwd })

    expect(result.ok).toBe(false)
    expect(result.errors).toEqual([
      expect.stringContaining('route files must import Page entrypoints from @app/pages/*'),
    ])
  })
})
