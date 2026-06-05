import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test } from 'vitest'
import { ContextCandidatesList } from '@app/features/detail/ContextCandidatesList'
import type { TaskContextCandidate } from '@shared/types/context'

const createdAt = new Date('2026-05-06T07:00:00.000Z').toISOString()

function candidate(overrides: Partial<TaskContextCandidate> = {}): TaskContextCandidate {
  return {
    id: overrides.id ?? 'candidate-1',
    itemKind: overrides.itemKind ?? 'memory',
    state: overrides.state ?? 'pending',
    ownerUserId: overrides.ownerUserId ?? 'user-1',
    sourceRunId: overrides.sourceRunId ?? 'run-12345678',
    targetSkillId: overrides.targetSkillId ?? null,
    proposedPreview: overrides.proposedPreview ?? {
      title: 'Release memory',
      content_preview: 'Remember to run the release health check.',
    },
    createdAt: overrides.createdAt ?? createdAt,
    updatedAt: overrides.updatedAt ?? createdAt,
  }
}

afterEach(cleanup)

describe('ContextCandidatesList', () => {
  test('does not render an empty candidate section', () => {
    const { container } = render(
      <ContextCandidatesList title="Suggested memory updates" kind="memory" candidates={[]} />
    )

    expect(container).toBeEmptyDOMElement()
  })

  test('explains suggested memories as review-only before reuse', () => {
    render(
      <ContextCandidatesList
        title="Suggested memory updates"
        kind="memory"
        candidates={[candidate()]}
      />
    )

    expect(screen.getByText('Suggested memory updates')).toBeInTheDocument()
    expect(
      screen.getByText(/not saved for future work until someone reviews them/i)
    ).toBeInTheDocument()
    expect(screen.getByText('Release memory')).toBeInTheDocument()
    expect(screen.getByText('Suggested memory')).toBeInTheDocument()
    expect(screen.getByText('Waiting for review')).toBeInTheDocument()
    expect(
      screen.getByText(/review the wording in Context before saving it for future tasks/i)
    ).toBeInTheDocument()
    expect(screen.getByText('Suggested from this task run')).toBeInTheDocument()
  })

  test('explains skill candidates as drafts that agents cannot use yet', () => {
    render(
      <ContextCandidatesList
        title="Suggested skills to review"
        kind="skill"
        candidates={[
          candidate({
            id: 'candidate-2',
            itemKind: 'skill',
            state: 'approved',
            proposedPreview: {
              name: 'Release operator',
              content_preview: '',
            },
          }),
        ]}
      />
    )

    expect(screen.getByText('Suggested skills to review')).toBeInTheDocument()
    expect(
      screen.getByText(/draft skills.*before agents can reuse the workflow/i)
    ).toBeInTheDocument()
    expect(screen.getByText('Release operator')).toBeInTheDocument()
    expect(screen.getByText('Draft skill')).toBeInTheDocument()
    expect(screen.getByText('Approved')).toBeInTheDocument()
    expect(
      screen.getByText(/Open the Context queue to inspect the full suggestion/i)
    ).toBeInTheDocument()
    expect(
      screen.getByText(/review the draft in Context before agents can use it/i)
    ).toBeInTheDocument()
  })
})
