import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'

import {
  checkFsdBoundaries as runChecker,
  sharedFileInventory,
} from '../../../scripts/check-fsd-boundaries.mjs'

let tempRoots: string[] = []

type SharedInventory = Map<string, 'generic' | 'frozen-domain'>

// Fixtures that are not about the shared inventory list their own shared/api
// and shared/lib files as generic, so only the rule under test reports.
function fixtureInventory(cwd: string): SharedInventory {
  const sharedDir = path.join(cwd, 'src/app/shared')
  const inventory: SharedInventory = new Map()
  for (const dir of ['api', 'lib']) {
    if (!fs.existsSync(path.join(sharedDir, dir))) continue
    for (const file of fs.readdirSync(path.join(sharedDir, dir), { recursive: true })) {
      const entry = `${dir}/${String(file).split(path.sep).join('/')}`
      if (fs.statSync(path.join(sharedDir, entry)).isFile()) inventory.set(entry, 'generic')
    }
  }
  return inventory
}

function checkFsdBoundaries({
  cwd,
  sharedInventory = fixtureInventory(cwd),
}: {
  cwd: string
  sharedInventory?: SharedInventory
}) {
  return runChecker({ cwd, sharedInventory })
}

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

    it('flags inline type specifiers that are not page entrypoints (codex P2)', () => {
      const cwd = fixture({
        'src/app/routes/tasks.tsx': `
import { TasksPage, type TaskViewLoadingFallback } from '@app/pages/tasks'

export function TasksRoute(): TaskViewLoadingFallback {
  return <TasksPage />
}
`,
        'src/app/pages/tasks/index.tsx': `
export type TaskViewLoadingFallback = unknown

export function TasksPage() {
  return null
}
`,
      })

      const result = checkFsdBoundaries({ cwd })

      expect(result.ok).toBe(false)
      expect(result.errors).toEqual([
        expect.stringContaining(
          'imports TaskViewLoadingFallback from @app/pages/tasks; route files must import Page entrypoints from @app/pages/*'
        ),
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

    it('allows the downward direction but flags a deep entity import (public-api)', () => {
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

      expect(result.ok).toBe(false)
      expect(result.errors).toEqual([
        expect.stringContaining(
          '[public-api] src/app/features/board/model/useBoard.ts -> @app/entities/task/api/taskApi'
        ),
      ])
    })

    it('tolerates ambient .d.ts files but still boundary-checks their references (codex round 2)', () => {
      const cwd = fixture({
        'src/app/shared/types/global.d.ts': `
declare global {
  interface Window {
    agentforge: import('@app/features/board/BoardView').BoardApi
  }
}

export {}
`,
        'src/app/features/board/BoardView.tsx': `
export interface BoardApi {
  id: string
}
export function BoardView() {
  return null
}
`,
      })

      const result = checkFsdBoundaries({ cwd })

      // No parse crash — the only finding is the real boundary violation
      // (shared -> features) referenced through the type-position import().
      expect(result.errors).toEqual([
        expect.stringContaining(
          '(shared/types) imports @app/features/board/BoardView (features/board)'
        ),
      ])
    })

    it('flags type-position import() references erased by transpilation (codex round 2)', () => {
      const cwd = fixture({
        'src/app/entities/task/model/types.ts': `
export type BoardProps = import('@app/features/board/BoardView').BoardProps
`,
        'src/app/features/board/BoardView.tsx': `
export interface BoardProps {
  id: string
}
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

    it('rejects an unrecognised src/app dir even when it imports only from shared', () => {
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
      expect(result.ok).toBe(false)
      expect(result.errors).toEqual([
        expect.stringContaining('[unknown-file] src/app/scratch/util.ts'),
      ])
    })

    it('rejects unknown files without imports', () => {
      const cwd = fixture({
        'src/app/scratch/standalone.ts': 'export const value = true\n',
      })

      const result = checkFsdBoundaries({ cwd })

      expect(result.ok).toBe(false)
      expect(result.errors).toEqual([
        expect.stringContaining('[unknown-file] src/app/scratch/standalone.ts'),
      ])
    })

    it.each(['features', 'entities', 'widgets', 'pages'])(
      'rejects a file directly under the %s layer instead of a slice',
      (layer) => {
        const cwd = fixture({
          [`src/app/${layer}/flat.ts`]: 'export const value = true\n',
        })

        const result = checkFsdBoundaries({ cwd })

        expect(result.ok).toBe(false)
        expect(result.errors).toEqual([
          expect.stringContaining(`[unknown-file] src/app/${layer}/flat.ts`),
        ])
      }
    )

    it('accepts valid app and shared files', () => {
      const cwd = fixture({
        'src/app/routes/index.ts': 'export const route = true\n',
        'src/app/root.ts': 'export const root = true\n',
        'src/app/shared/index.ts': 'export const shared = true\n',
        'src/app/shared/lib/helper.ts': 'export const helper = true\n',
      })

      const result = checkFsdBoundaries({ cwd })

      expect(result.ok).toBe(true)
      expect(result.errors).toEqual([])
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

  describe('public-api (error)', () => {
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

      expect(result.ok).toBe(false)
      expect(result.errors).toEqual([
        expect.stringContaining(
          '[public-api] src/app/pages/tasks/index.tsx -> @app/features/board/model/helpers (deep import into features/board'
        ),
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

      expect(result.ok).toBe(false)
      expect(result.errors).toEqual([expect.stringContaining('[public-api]')])
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
      expect(result.errors).toEqual([])
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
      expect(result.errors).toEqual([])
    })
  })

  describe('public-api for entities (error)', () => {
    it('flags a deep import into an entity slice from outside it', () => {
      const cwd = fixture({
        'src/app/features/board/BoardView.tsx': `
import { useBoardStore } from '@app/entities/navigation/model/board.store'
export const x = useBoardStore
`,
        'src/app/entities/navigation/index.ts': `
export { useBoardStore } from './model/board.store'
`,
        'src/app/entities/navigation/model/board.store.ts': `
export const useBoardStore = 1
`,
      })

      const result = checkFsdBoundaries({ cwd })

      expect(result.ok).toBe(false)
      expect(result.errors).toEqual([
        expect.stringContaining(
          '[public-api] src/app/features/board/BoardView.tsx -> @app/entities/navigation/model/board.store (deep import into entities/navigation; import the slice barrel @app/entities/navigation)'
        ),
      ])
    })

    it.each([
      ['relative', `import { useBoardStore } from '../../entities/navigation/model/board.store'`],
      ['type-only', `import type { BoardState } from '@app/entities/navigation/model/board.store'`],
      ['re-export', `export { useBoardStore } from '@app/entities/navigation/model/board.store'`],
      [
        'dynamic import()',
        `export const load = () => import('@app/entities/navigation/model/board.store')`,
      ],
    ])('flags a %s deep import into an entity slice', (_form, statement) => {
      const cwd = fixture({
        'src/app/features/board/BoardView.tsx': `${statement}\nexport const marker = 1\n`,
        'src/app/entities/navigation/model/board.store.ts': `
export type BoardState = { id: string }
export const useBoardStore = 1
`,
      })

      const result = checkFsdBoundaries({ cwd })

      expect(result.ok).toBe(false)
      expect(result.errors).toEqual([
        expect.stringContaining('[public-api] src/app/features/board/BoardView.tsx -> '),
      ])
      expect(result.errors[0]).toContain('(deep import into entities/navigation;')
    })

    it('flags a deep import into a nested directory of an entity slice', () => {
      const cwd = fixture({
        'src/app/pages/login/index.tsx': `
import { teamApi } from '@app/entities/navigation/team'
export function LoginPage() {
  return <span>{String(teamApi)}</span>
}
`,
        'src/app/entities/navigation/team/index.ts': `
export const teamApi = 1
`,
      })

      const result = checkFsdBoundaries({ cwd })

      expect(result.ok).toBe(false)
      expect(result.errors).toEqual([
        expect.stringContaining(
          '[public-api] src/app/pages/login/index.tsx -> @app/entities/navigation/team'
        ),
      ])
    })

    it('accepts an import of the entity slice barrel', () => {
      const cwd = fixture({
        'src/app/features/board/BoardView.tsx': `
import { useBoardStore } from '@app/entities/navigation'
export const x = useBoardStore
`,
        'src/app/entities/navigation/index.ts': `
export { useBoardStore } from './model/board.store'
`,
        'src/app/entities/navigation/model/board.store.ts': `
export const useBoardStore = 1
`,
      })

      const result = checkFsdBoundaries({ cwd })
      expect(result.ok).toBe(true)
      expect(result.errors).toEqual([])
    })

    it('leaves deep imports inside the same entity slice alone', () => {
      const cwd = fixture({
        'src/app/entities/navigation/model/navigation.store.ts': `
import { teamApi } from '../team/api/teamApi'
import { projectApi } from '@app/entities/navigation/project/api/projectApi'
export const x = [teamApi, projectApi]
`,
        'src/app/entities/navigation/team/api/teamApi.ts': `
export const teamApi = 1
`,
        'src/app/entities/navigation/project/api/projectApi.ts': `
export const projectApi = 1
`,
      })

      const result = checkFsdBoundaries({ cwd })
      expect(result.ok).toBe(true)
      expect(result.errors).toEqual([])
    })
  })

  describe('cross-entity (error)', () => {
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

      expect(result.ok).toBe(false)
      expect(result.errors).toEqual([
        expect.stringContaining(
          '[cross-entity] src/app/entities/task/model/task.ts -> @app/entities/agent/model/agent'
        ),
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
      expect(result.errors).toEqual([])
    })
  })

  describe('same-layer-isolation (error)', () => {
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

      expect(result.ok).toBe(false)
      // Exactly one error: sibling-slice isolation wins over public-api for
      // the same import.
      expect(result.errors).toEqual([
        expect.stringContaining(
          '[same-layer-isolation] src/app/widgets/agent-detail/AgentDetailView.tsx'
        ),
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

      expect(result.ok).toBe(false)
      expect(result.errors).toEqual([expect.stringContaining('[same-layer-isolation]')])
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
      expect(result.errors).toEqual([])
    })
  })

  describe('shared-purity (error)', () => {
    it('flags domain stores parked under shared/model', () => {
      const cwd = fixture({
        'src/app/shared/model/feed.store.ts': `
export const useFeedStore = () => null
`,
      })

      const result = checkFsdBoundaries({ cwd })

      expect(result.ok).toBe(false)
      expect(result.errors).toEqual([
        expect.stringContaining(
          '[shared-purity] src/app/shared/model/feed.store.ts (domain store in shared'
        ),
      ])
    })

    it.each(['api', 'lib'])('flags an unlisted new file in shared/%s', (dir) => {
      const cwd = fixture({
        'src/app/shared/lib/utils.ts': 'export const cn = 1\n',
        [`src/app/shared/${dir}/agentThing.ts`]: 'export const agentThing = 1\n',
      })

      const result = checkFsdBoundaries({
        cwd,
        sharedInventory: new Map([['lib/utils.ts', 'generic']]),
      })

      expect(result.ok).toBe(false)
      expect(result.errors).toEqual([
        expect.stringContaining(
          `[shared-purity] src/app/shared/${dir}/agentThing.ts (unlisted file in shared/${dir} — domain code belongs in entities/<slice>`
        ),
      ])
    })

    it('covers sub-directories, index files and declaration files', () => {
      const cwd = fixture({
        'src/app/shared/api/index.ts': 'export {}\n',
        'src/app/shared/api/legacy/teamApi.ts': 'export const teamApi = 1\n',
        'src/app/shared/lib/globals.d.ts': 'export {}\n',
      })

      const result = checkFsdBoundaries({ cwd, sharedInventory: new Map() })

      expect(result.errors).toEqual([
        expect.stringContaining('[shared-purity] src/app/shared/api/index.ts (unlisted'),
        expect.stringContaining('[shared-purity] src/app/shared/api/legacy/teamApi.ts (unlisted'),
        expect.stringContaining('[shared-purity] src/app/shared/lib/globals.d.ts (unlisted'),
      ])
    })

    it('accepts listed generic and frozen-domain files', () => {
      const cwd = fixture({
        'src/app/shared/lib/utils.ts': 'export const cn = 1\n',
        'src/app/shared/api/legacy/billingApi.ts': 'export const billingApi = 1\n',
      })

      const result = checkFsdBoundaries({
        cwd,
        sharedInventory: new Map([
          ['lib/utils.ts', 'generic'],
          ['api/legacy/billingApi.ts', 'frozen-domain'],
        ]),
      })

      expect(result.ok).toBe(true)
      expect(result.errors).toEqual([])
    })

    it('flags a stale inventory entry whose file does not exist', () => {
      const cwd = fixture({
        'src/app/shared/lib/utils.ts': 'export const cn = 1\n',
      })

      const result = checkFsdBoundaries({
        cwd,
        sharedInventory: new Map([
          ['lib/utils.ts', 'generic'],
          ['api/removed.ts', 'frozen-domain'],
        ]),
      })

      expect(result.ok).toBe(false)
      expect(result.errors).toEqual([
        expect.stringContaining(
          '[shared-purity] src/app/shared/api/removed.ts (stale inventory entry'
        ),
      ])
    })

    it('does not let the frozen-domain list grow', () => {
      const frozen = [...sharedFileInventory.values()].filter((kind) => kind === 'frozen-domain')
      // This number may only go down: move domain code to entities/<slice>
      // instead of adding frozen-domain entries.
      expect(frozen.length).toBeLessThanOrEqual(18)
    })

    it('leaves non-store shared/model files alone', () => {
      const cwd = fixture({
        'src/app/shared/model/board.types.ts': `
export type ColumnId = string
`,
      })

      const result = checkFsdBoundaries({ cwd })
      expect(result.ok).toBe(true)
      expect(result.errors).toEqual([])
    })
  })

  describe('import forms the checker cannot resolve (error)', () => {
    it('flags an alias import that climbs out with ..', () => {
      const cwd = fixture({
        'src/app/features/board/BoardView.tsx': `
import { useBoardStore } from '@shared/../src/app/entities/navigation/model/board.store'
export const x = useBoardStore
`,
        'src/app/entities/navigation/model/board.store.ts': 'export const useBoardStore = 1\n',
      })

      const result = checkFsdBoundaries({ cwd })

      expect(result.ok).toBe(false)
      expect(result.errors).toEqual([
        expect.stringContaining(
          "src/app/features/board/BoardView.tsx imports @shared/../src/app/entities/navigation/model/board.store; an alias import must not contain '..'"
        ),
      ])
    })

    it('flags a computed import()', () => {
      const cwd = fixture({
        'src/app/features/board/BoardView.tsx': `
const target = '@app/entities/navigation/model/board.store'
export const load = () => import(target)
`,
      })

      const result = checkFsdBoundaries({ cwd })

      expect(result.ok).toBe(false)
      expect(result.errors).toEqual([
        expect.stringContaining('[computed-import] src/app/features/board/BoardView.tsx'),
      ])
    })
  })

  describe('repository', () => {
    // Parses all of src/app, which takes longer than the 10 s default on a busy runner.
    it('passes on the real source tree with the real shared inventory', { timeout: 60_000 }, () => {
      const result = runChecker({ cwd: path.resolve(__dirname, '../../..') })

      expect(result.errors).toEqual([])
      expect(result.ok).toBe(true)
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
