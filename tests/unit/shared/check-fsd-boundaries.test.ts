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
  describe('route page-entrypoint contract (error — existing)', () => {
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

    it('accepts routes that mount pages while pages compose feature barrels', () => {
      const cwd = fixture({
        'src/app/routes/tasks.tsx': `
import { TasksPage } from '@app/pages/tasks'

export function TasksRoute() {
  return <TasksPage />
}
`,
        'src/app/pages/tasks/index.tsx': `
import { BoardView } from '@app/features/board'

export function TasksPage() {
  return <BoardView />
}
`,
        'src/app/features/board/index.ts': `
export { BoardView } from './BoardView'
`,
        'src/app/features/board/BoardView.tsx': `
export function BoardView() {
  return null
}
`,
      })

      const result = checkFsdBoundaries({ cwd })
      expect(result.ok).toBe(true)
      expect(result.errors).toEqual([])
      expect(result.warnings).toEqual([])
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

  describe('downward-layering (error)', () => {
    it('flags an upward import from entities into features', () => {
      const cwd = fixture({
        'src/app/entities/task/model/api.ts': `
import { BoardView } from '@app/features/board/BoardView'
export const x = BoardView
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
          '(entities/task) imports @app/features/board/BoardView (features/board)'
        ),
      ])
    })

    it('accepts a downward import from features into entities and shared', () => {
      const cwd = fixture({
        'src/app/features/board/model/useBoard.ts': `
import { taskApi } from '@app/entities/task/api/taskApi'
import { helper } from '@app/shared/lib/utils'
export const x = [taskApi, helper]
`,
        'src/app/entities/task/api/taskApi.ts': `
export const taskApi = 1
`,
        'src/app/shared/lib/utils.ts': `
export const helper = 1
`,
      })

      const result = checkFsdBoundaries({ cwd })
      expect(result.ok).toBe(true)
      expect(result.errors).toEqual([])
      expect(result.warnings).toEqual([])
    })

    it('sees type-only imports (AST extraction keeps them)', () => {
      const cwd = fixture({
        'src/app/entities/task/model/types.ts': `
import type { BoardProps } from '@app/widgets/board-panel/BoardPanel'
export type X = BoardProps
`,
        'src/app/widgets/board-panel/BoardPanel.tsx': `
export interface BoardProps { id: string }
export function BoardPanel() {
  return null
}
`,
      })

      const result = checkFsdBoundaries({ cwd })

      expect(result.ok).toBe(false)
      expect(result.errors).toEqual([
        expect.stringContaining(
          '(entities/task) imports @app/widgets/board-panel/BoardPanel (widgets/board-panel)'
        ),
      ])
    })
  })

  describe('unknown-dir (error, F074)', () => {
    it('flags an unrecognised src/app dir that imports above the shared layer', () => {
      const cwd = fixture({
        'src/app/scratch/util.ts': `
import { thing } from '@app/features/board/BoardView'
export const x = thing
`,
        'src/app/features/board/BoardView.tsx': `
export const thing = 1
`,
      })

      const result = checkFsdBoundaries({ cwd })

      // An unknown dir is ranked LOWEST (not app), so reaching into features is a
      // violation that surfaces it for proper classification — rather than being
      // silently allowed as if it were the top `app` layer.
      expect(result.ok).toBe(false)
      expect(result.errors.join('\n')).toContain('unknown/scratch')
    })

    it('allows an unrecognised src/app dir to import only from shared', () => {
      const cwd = fixture({
        'src/app/scratch/util.ts': `
import { helper } from '@app/shared/lib/utils'
export const x = helper
`,
        'src/app/shared/lib/utils.ts': `
export const helper = 1
`,
      })

      const result = checkFsdBoundaries({ cwd })
      expect(result.ok).toBe(true)
    })

    it('forbids importing FROM an unrecognised src/app dir', () => {
      const cwd = fixture({
        'src/app/shared/lib/a.ts': `
import { util } from '@app/scratch/util'
export const x = util
`,
        'src/app/scratch/util.ts': `
export const util = 1
`,
      })

      const result = checkFsdBoundaries({ cwd })

      // An unknown dir is not a valid module location, so even `shared` must not
      // import from it — ranking it equal to shared would have wrongly allowed this.
      expect(result.ok).toBe(false)
      expect(result.errors.join('\n')).toContain('unknown/scratch')
    })
  })

  describe('public-api (warn)', () => {
    it('warns when a page deep-imports into a feature slice instead of its barrel', () => {
      const cwd = fixture({
        'src/app/pages/tasks/index.tsx': `
import { helper } from '@app/features/board/model/helpers'

export function TasksPage() {
  return <span>{helper}</span>
}
`,
        'src/app/features/board/model/helpers.ts': `
export const helper = 1
`,
      })

      const result = checkFsdBoundaries({ cwd })

      expect(result.ok).toBe(true)
      expect(result.errors).toEqual([])
      expect(result.warnings).toEqual([
        {
          rule: 'public-api',
          file: 'src/app/pages/tasks/index.tsx',
          target: '@app/features/board/model/helpers',
          reason: expect.stringContaining('deep import into features/board'),
        },
      ])
    })

    it('warns on dynamic import() deep into a feature slice', () => {
      const cwd = fixture({
        'src/app/pages/tasks/index.tsx': `
const load = () => import('@app/features/board/model/helpers')
export function TasksPage() {
  return null
}
export const x = load
`,
        'src/app/features/board/model/helpers.ts': `
export const helper = 1
`,
      })

      const result = checkFsdBoundaries({ cwd })

      expect(result.ok).toBe(true)
      expect(result.warnings).toEqual([expect.objectContaining({ rule: 'public-api' })])
    })

    it('accepts cross-slice imports that target the slice barrel', () => {
      const cwd = fixture({
        'src/app/pages/tasks/index.tsx': `
import { BoardView } from '@app/features/board'

export function TasksPage() {
  return <BoardView />
}
`,
        'src/app/features/board/index.ts': `
export { BoardView } from './BoardView'
`,
        'src/app/features/board/BoardView.tsx': `
export function BoardView() {
  return null
}
`,
      })

      const result = checkFsdBoundaries({ cwd })
      expect(result.ok).toBe(true)
      expect(result.warnings).toEqual([])
    })

    it('leaves same-slice deep imports alone', () => {
      const cwd = fixture({
        'src/app/features/board/BoardView.tsx': `
import { helper } from './model/helpers'
export function BoardView() {
  return <span>{helper}</span>
}
`,
        'src/app/features/board/model/helpers.ts': `
export const helper = 1
`,
      })

      const result = checkFsdBoundaries({ cwd })
      expect(result.ok).toBe(true)
      expect(result.warnings).toEqual([])
    })
  })

  describe('cross-entity (warn)', () => {
    it('warns when an entity slice imports a sibling entity slice', () => {
      const cwd = fixture({
        'src/app/entities/task/model/task.ts': `
import { agent } from '@app/entities/agent/model/agent'
export const x = agent
`,
        'src/app/entities/agent/model/agent.ts': `
export const agent = 1
`,
      })

      const result = checkFsdBoundaries({ cwd })

      expect(result.ok).toBe(true)
      expect(result.errors).toEqual([])
      expect(result.warnings).toEqual([
        expect.objectContaining({
          rule: 'cross-entity',
          file: 'src/app/entities/task/model/task.ts',
          target: '@app/entities/agent/model/agent',
        }),
      ])
    })

    it('accepts entity slices that only use their own files and shared', () => {
      const cwd = fixture({
        'src/app/entities/task/model/task.ts': `
import { helper } from '@app/shared/lib/utils'
import { taskApi } from '../api/taskApi'
export const x = [helper, taskApi]
`,
        'src/app/entities/task/api/taskApi.ts': `
export const taskApi = 1
`,
        'src/app/shared/lib/utils.ts': `
export const helper = 1
`,
      })

      const result = checkFsdBoundaries({ cwd })
      expect(result.ok).toBe(true)
      expect(result.warnings).toEqual([])
    })
  })

  describe('same-layer-isolation (warn for widgets/pages, error for features)', () => {
    it('warns when a widget imports a sibling widget slice', () => {
      const cwd = fixture({
        'src/app/widgets/agent-detail/AgentDetailView.tsx': `
import { TimelineView } from '@app/widgets/views/TimelineView'
export function AgentDetailView() {
  return <TimelineView />
}
`,
        'src/app/widgets/views/TimelineView.tsx': `
export function TimelineView() {
  return null
}
`,
      })

      const result = checkFsdBoundaries({ cwd })

      expect(result.ok).toBe(true)
      expect(result.errors).toEqual([])
      // Exactly one warning: sibling-slice isolation wins over public-api for
      // the same import.
      expect(result.warnings).toEqual([
        expect.objectContaining({
          rule: 'same-layer-isolation',
          file: 'src/app/widgets/agent-detail/AgentDetailView.tsx',
        }),
      ])
    })

    it('warns when a page imports a sibling page slice, even via its barrel', () => {
      const cwd = fixture({
        'src/app/pages/inbox/index.tsx': `
import { TasksPage } from '@app/pages/tasks'
export function InboxPage() {
  return <TasksPage />
}
`,
        'src/app/pages/tasks/index.tsx': `
export function TasksPage() {
  return null
}
`,
      })

      const result = checkFsdBoundaries({ cwd })

      expect(result.ok).toBe(true)
      expect(result.warnings).toEqual([expect.objectContaining({ rule: 'same-layer-isolation' })])
    })

    it('keeps cross-feature imports as an error (enforced pre-FSD-5)', () => {
      const cwd = fixture({
        'src/app/features/board/BoardView.tsx': `
import { ChatView } from '@app/features/chat/ChatView'
export function BoardView() {
  return <ChatView />
}
`,
        'src/app/features/chat/ChatView.tsx': `
export function ChatView() {
  return null
}
`,
      })

      const result = checkFsdBoundaries({ cwd })

      expect(result.ok).toBe(false)
      expect(result.errors).toEqual([
        expect.stringContaining(
          '(features/board) imports @app/features/chat/ChatView (features/chat)'
        ),
      ])
      expect(result.warnings).toEqual([])
    })

    it('allows a slice to import its own files freely', () => {
      const cwd = fixture({
        'src/app/widgets/views/TimelineView.tsx': `
import { helper } from '@app/widgets/views/lib/helper'
export function TimelineView() {
  return <span>{helper}</span>
}
`,
        'src/app/widgets/views/lib/helper.ts': `
export const helper = 1
`,
      })

      const result = checkFsdBoundaries({ cwd })
      expect(result.ok).toBe(true)
      expect(result.warnings).toEqual([])
    })
  })

  describe('shared-purity (warn)', () => {
    it('flags domain stores parked under shared/model', () => {
      const cwd = fixture({
        'src/app/shared/model/feed.store.ts': `
export const useFeedStore = () => null
`,
      })

      const result = checkFsdBoundaries({ cwd })

      expect(result.ok).toBe(true)
      expect(result.warnings).toEqual([
        {
          rule: 'shared-purity',
          file: 'src/app/shared/model/feed.store.ts',
          target: null,
          reason: 'domain store in shared — relocate in FSD-2',
        },
      ])
    })

    it('leaves non-store shared/model files alone', () => {
      const cwd = fixture({
        'src/app/shared/model/board.types.ts': `
export type ColumnId = string
`,
      })

      const result = checkFsdBoundaries({ cwd })
      expect(result.ok).toBe(true)
      expect(result.warnings).toEqual([])
    })
  })

  describe('reporting', () => {
    it('returns per-layer conformance stats', () => {
      const cwd = fixture({
        'src/app/pages/tasks/index.tsx': `
import { helper } from '@app/features/board/model/helpers'
export function TasksPage() {
  return <span>{helper}</span>
}
`,
        'src/app/features/board/model/helpers.ts': `
export const helper = 1
`,
      })

      const result = checkFsdBoundaries({ cwd })

      expect(result.layerStats).toEqual({
        pages: { files: 1, clean: 0 },
        features: { files: 1, clean: 1 },
      })
    })
  })
})
