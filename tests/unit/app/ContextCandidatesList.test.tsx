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
      screen.getByText(
        /memory suggestions are not saved for future work until someone reviews them/i
      )
    ).toBeInTheDocument()
    expect(screen.getByText('Release memory')).toBeInTheDocument()
    expect(screen.getByText('Suggested memory')).toBeInTheDocument()
    expect(screen.getByText('Waiting for review')).toBeInTheDocument()
    expect(
      screen.getByText(/review the wording before saving it for future tasks/i)
    ).toBeInTheDocument()
    expect(screen.getByText('Suggested from this task')).toBeInTheDocument()
  })

  test('explains skill suggestions as review-only before agents can use them', () => {
    render(
      <ContextCandidatesList
        title="Suggested reusable skills"
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

    expect(screen.getByText('Suggested reusable skills')).toBeInTheDocument()
    expect(
      screen.getByText(/skill suggestions.*before agents can reuse the workflow/i)
    ).toBeInTheDocument()
    expect(screen.getByText('Release operator')).toBeInTheDocument()
    expect(screen.getByText('Skill suggestion')).toBeInTheDocument()
    expect(screen.getByText('Approved')).toBeInTheDocument()
    expect(
      screen.getByText(/Open reusable context review to inspect the full suggestion/i)
    ).toBeInTheDocument()
    expect(screen.getByText(/review the suggestion before agents can use it/i)).toBeInTheDocument()
    expect(screen.queryByText(/Context queue/i)).toBeNull()
  })

  test('labels unknown suggestion types without pretending they are memories', () => {
    render(
      <ContextCandidatesList
        title="Suggested memory updates"
        kind="memory"
        candidates={[
          candidate({
            id: 'candidate-unknown-kind',
            itemKind: 'future_context_kind' as never,
            proposedPreview: {
              content_preview: 'Review this generated suggestion before reuse.',
            },
          }),
        ]}
      />
    )

    expect(screen.getByText('Suggested context item')).toBeInTheDocument()
    expect(screen.getByText('Suggestion needs review')).toBeInTheDocument()
    expect(screen.getByText(/review this suggestion before agents can reuse it/i)).toBeInTheDocument()
    expect(screen.queryByText(/future context kind/i)).toBeNull()
  })
})
