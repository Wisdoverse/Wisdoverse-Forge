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
      <ContextCandidatesList title="Save ideas from this run" kind="memory" candidates={[]} />
    )

    expect(container).toBeEmptyDOMElement()
  })

  test('explains suggested memories as review-only before reuse', () => {
    render(
      <ContextCandidatesList
        title="Save ideas from this run"
        kind="memory"
        candidates={[candidate()]}
      />
    )

    expect(screen.getByText('Save ideas from this run')).toBeInTheDocument()
    expect(
      screen.getByText(/draft notes from the run.*saving it for future tasks/i)
    ).toBeInTheDocument()
    expect(screen.getByText('Release memory')).toBeInTheDocument()
    expect(screen.getByText('Memory idea')).toBeInTheDocument()
    expect(screen.getByText('Waiting for review')).toBeInTheDocument()
    expect(
      screen.getByText(/review the wording before saving it for future tasks/i)
    ).toBeInTheDocument()
    expect(screen.getByText('From this task')).toBeInTheDocument()
    expect(screen.queryByText('Suggested from this task')).toBeNull()
  })

  test('explains instruction suggestions as review-only before agents can follow them', () => {
    render(
      <ContextCandidatesList
        title="Instruction ideas from this run"
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

    expect(screen.getByText('Instruction ideas from this run')).toBeInTheDocument()
    expect(
      screen.getByText(/draft instructions from the run.*before agents can follow it/i)
    ).toBeInTheDocument()
    expect(screen.getByText('Release operator')).toBeInTheDocument()
    expect(screen.getByText('Instruction idea')).toBeInTheDocument()
    expect(screen.getByText('Approved')).toBeInTheDocument()
    expect(screen.getByText(/Open saved item review to inspect the full idea/i)).toBeInTheDocument()
    expect(
      screen.getByText(/review this instruction before agents can follow it/i)
    ).toBeInTheDocument()
    expect(screen.queryByText(/Context queue/i)).toBeNull()
    expect(screen.queryByText(/instruction suggestions/i)).toBeNull()
  })

  test('uses saved-instruction wording when an instruction suggestion has no title', () => {
    render(
      <ContextCandidatesList
        title="Instruction ideas from this run"
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

    expect(screen.getByText('Untitled instruction idea')).toBeInTheDocument()
    expect(screen.getByText('Instruction idea')).toBeInTheDocument()
    expect(screen.queryByText('Suggested saved instruction')).toBeNull()
    expect(screen.queryByText(new RegExp(['Suggested', 'skill'].join('\\s+')))).toBeNull()
  })

  test('labels unknown suggestion types without pretending they are memories', () => {
    render(
      <ContextCandidatesList
        title="Save ideas from this run"
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

    expect(screen.getAllByText('Idea needs review').length).toBeGreaterThan(0)
    expect(screen.getByText(/review this idea before agents can reuse it/i)).toBeInTheDocument()
    expect(screen.queryByText('Suggested context item')).toBeNull()
    expect(screen.queryByText('Suggestion needs review')).toBeNull()
    expect(screen.queryByText(/future context kind/i)).toBeNull()
  })
})
