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
  const previousMemoryIdeaLabel = ['Memory', 'idea'].join(' ')
  const previousInstructionIdeaLabel = ['Instruction', 'idea'].join(' ')
  const previousUntitledInstructionIdea = ['Untitled', 'instruction', 'idea'].join(' ')
  const previousUnknownIdeaTitle = ['Idea', 'needs', 'review'].join(' ')
  const previousReviewIdeaCopy = new RegExp(['review', 'this', 'idea'].join('\\s+'), 'i')
  const previousFullIdeaCopy = ['full', 'idea'].join(' ')

  test('does not render an empty candidate section', () => {
    const { container } = render(
      <ContextCandidatesList title="Suggested notes to check" kind="memory" candidates={[]} />
    )

    expect(container).toBeEmptyDOMElement()
  })

  test('explains suggested memories as check-only before reuse', () => {
    render(
      <ContextCandidatesList
        title="Suggested notes to check"
        kind="memory"
        candidates={[candidate()]}
      />
    )

    expect(screen.getByText('Suggested notes to check')).toBeInTheDocument()
    expect(
      screen.getByText(/suggested notes from this task.*saving it for future tasks/i)
    ).toBeInTheDocument()
    expect(screen.getByText('Release memory')).toBeInTheDocument()
    expect(screen.getByText('Suggested note')).toBeInTheDocument()
    expect(screen.getByText('Needs your check')).toBeInTheDocument()
    expect(screen.queryByText('Waiting for review')).toBeNull()
    expect(
      screen.getByText(/open Context.*check the wording before saving it for future tasks/i)
    ).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /open context for release memory/i })).toHaveAttribute(
      'href',
      '/context'
    )
    expect(screen.getByText('From this task')).toBeInTheDocument()
    expect(screen.queryByText('Suggested from this task')).toBeNull()
    expect(screen.queryByText(previousMemoryIdeaLabel)).toBeNull()
  })

  test('explains instruction suggestions as check-only before agents can follow them', () => {
    render(
      <ContextCandidatesList
        title="Suggested guidance to check"
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

    expect(screen.getByText('Suggested guidance to check')).toBeInTheDocument()
    expect(screen.queryByText('Suggested instructions to check')).toBeNull()
    expect(
      screen.getByText(/suggested guidance from this task.*before agents can follow it/i)
    ).toBeInTheDocument()
    expect(screen.getByText('Release operator')).toBeInTheDocument()
    expect(screen.getByText('Suggested guidance')).toBeInTheDocument()
    expect(screen.queryByText('Suggested instruction')).toBeNull()
    expect(screen.getByText('Saved')).toBeInTheDocument()
    expect(screen.queryByText('Approved')).toBeNull()
    expect(
      screen.getByText(/Open Context and read the full suggestion before using it/i)
    ).toBeInTheDocument()
    expect(screen.queryByText(/No preview yet/i)).toBeNull()
    expect(screen.queryByText(/inspect the full suggestion/i)).toBeNull()
    expect(
      screen.getByText(/open Context.*check this guidance before agents can follow it/i)
    ).toBeInTheDocument()
    expect(
      screen.getByRole('link', { name: /open context for release operator/i })
    ).toHaveAttribute('href', '/context')
    expect(screen.queryByText(/Context queue/i)).toBeNull()
    expect(screen.queryByText(/instruction suggestions/i)).toBeNull()
    expect(screen.queryByText(previousInstructionIdeaLabel)).toBeNull()
    expect(screen.queryByText(new RegExp(previousFullIdeaCopy, 'i'))).toBeNull()
  })

  test('uses saved-instruction wording when an instruction suggestion has no title', () => {
    render(
      <ContextCandidatesList
        title="Suggested guidance to check"
        kind="skill"
        candidates={[
          candidate({
            id: 'candidate-untitled-skill',
            itemKind: 'skill',
            proposedPreview: {
              content_preview: 'Review this reusable instruction before saving it.',
            },
          }),
        ]}
      />
    )

    expect(screen.getByText('Untitled suggested guidance')).toBeInTheDocument()
    expect(screen.getByText('Suggested guidance')).toBeInTheDocument()
    expect(screen.queryByText('Untitled suggested instruction')).toBeNull()
    expect(screen.queryByText('Suggested instruction')).toBeNull()
    expect(screen.queryByText('Suggested saved instruction')).toBeNull()
    expect(screen.queryByText(new RegExp(['Suggested', 'skill'].join('\\s+')))).toBeNull()
    expect(screen.queryByText(previousUntitledInstructionIdea)).toBeNull()
  })

  test('labels unknown suggestion types without pretending they are memories', () => {
    render(
      <ContextCandidatesList
        title="Suggested notes to check"
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

    expect(screen.getAllByText('Check suggested item').length).toBeGreaterThan(0)
    expect(
      screen.getByText(/open Context.*check this suggestion before agents can reuse it/i)
    ).toBeInTheDocument()
    expect(screen.queryByText('Suggested context item')).toBeNull()
    expect(screen.queryByText(/future context kind/i)).toBeNull()
    expect(screen.queryByText(previousUnknownIdeaTitle)).toBeNull()
    expect(screen.queryByText(previousReviewIdeaCopy)).toBeNull()
  })
})
