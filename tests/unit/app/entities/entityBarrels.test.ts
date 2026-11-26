import { describe, expect, it } from 'vitest'
import * as context from '@app/entities/context'
import * as navigation from '@app/entities/navigation'

// Unmocked: an import cycle through a barrel shows up as an undefined export.
describe('entity barrels', () => {
  it.each([
    ['context', context, ['useContextFeaturesStore', 'FeedbackControls', 'InjectionPreviewModal']],
    [
      'navigation',
      navigation,
      [
        'navigationActionErrorMessage',
        'useNavigationStore',
        'useBoardStore',
        'agentGroupApi',
        'waitingPlaceDisplayName',
        'projectApi',
        'teamApi',
      ],
    ],
  ] as const)('entities/%s exports every runtime name', (_slice, barrel, names) => {
    for (const name of names) {
      expect(barrel[name as keyof typeof barrel], name).toBeDefined()
    }
  })
})
