import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { skillHttpErrorMessage, useSkillsStore } from '@app/entities/skill'

const fetchMock = vi.fn()

vi.stubGlobal('fetch', fetchMock)

beforeEach(() => {
  fetchMock.mockReset()
  useSkillsStore.getState().reset()
})

afterEach(() => {
  vi.clearAllMocks()
})

function response(status: number, body: unknown): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => body,
  } as Response
}

describe('skillHttpErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Details:')
  }

  test('turns unauthorized catalog loads into a sign-in step', () => {
    expectBeginnerMessage(
      skillHttpErrorMessage('load', 401),
      'Sign in again, then open Skills again.'
    )
  })

  test('turns create permission failures into team space access guidance', () => {
    expectBeginnerMessage(
      skillHttpErrorMessage('create', 403),
      'Ask an owner or admin to let you save reusable guidance for this team space, then save the skill again.'
    )
    expect(skillHttpErrorMessage('create', 403)).not.toContain('workspace instructions')
  })

  test('turns catalog permission failures into team space access guidance', () => {
    expectBeginnerMessage(
      skillHttpErrorMessage('load', 403),
      'Ask an owner or admin to update your team space access, then open Skills again. You do not have access to skills for this team space.'
    )
    expect(skillHttpErrorMessage('load', 403)).not.toContain('workspace instructions')
  })

  test('turns validation details into a field-specific next step', () => {
    expectBeginnerMessage(
      skillHttpErrorMessage('create', 422, { error: { message: 'content is required' } }),
      'Enter the reusable guidance, then save the skill again.'
    )
  })

  test('turns duplicate saved guidance errors into a specific check step', () => {
    const message = skillHttpErrorMessage('create', 409)

    expectBeginnerMessage(
      message,
      'Open Skills to check for a similar item, then change the name or matching words and save the skill again.'
    )
    expect(message).not.toContain('Review the existing instructions')
  })

  test('turns busy saved guidance saves into a plain wait step', () => {
    const message = skillHttpErrorMessage('create', 429)

    expectBeginnerMessage(
      message,
      'Wait a moment, then save the skill again. Forge is busy with skills right now.'
    )
    expect(message).not.toContain('Instruction setup')
  })

  test('turns busy saved guidance loads into a plain wait step', () => {
    const message = skillHttpErrorMessage('load', 429)

    expectBeginnerMessage(
      message,
      'Wait a moment, then open Skills again. Forge is busy with skills right now.'
    )
    expect(message).not.toContain('Instruction setup')
  })

  test('uses a check step for unknown create failures', () => {
    const message = skillHttpErrorMessage('create', 418)

    expectBeginnerMessage(message, 'Check the required fields, then save the skill again.')
    expect(message).not.toContain('Review the fields')
  })
})

describe('useSkillsStore errors', () => {
  test('normalizes missing saved guidance source names for team spaces', async () => {
    fetchMock.mockResolvedValue(
      response(200, {
        ok: true,
        skills: [
          {
            id: 'skill-team-space',
            organization_id: 'org-1',
            name: 'release-check',
            content: 'Check release notes',
          },
        ],
      })
    )

    await useSkillsStore.getState().loadSkills()

    const [skill] = useSkillsStore.getState().skills
    expect(skill?.plugin).toBe('Team space skills')
    expect(skill?.plugin).not.toContain('Workspace')
  })

  test('uses scope kind before missing organization id when labeling saved guidance', async () => {
    fetchMock.mockResolvedValue(
      response(200, {
        ok: true,
        skills: [
          {
            id: 'skill-team-scope',
            scope_kind: 'team',
            name: 'handoff-check',
            content: 'Check the handoff',
          },
        ],
      })
    )

    await useSkillsStore.getState().loadSkills()

    const [skill] = useSkillsStore.getState().skills
    expect(skill?.plugin).toBe('Team space skills')
    expect(skill?.marketplace).toBe('workspace')
  })

  test('labels project-scoped saved guidance as project saved guidance', async () => {
    fetchMock.mockResolvedValue(
      response(200, {
        ok: true,
        skills: [
          {
            id: 'skill-project-scope',
            scope_kind: 'project',
            name: 'release-check',
            content: 'Check the release',
          },
        ],
      })
    )

    await useSkillsStore.getState().loadSkills()

    const [skill] = useSkillsStore.getState().skills
    expect(skill?.plugin).toBe('Project skills')
    expect(skill?.marketplace).toBe('project')
  })

  test('stores beginner guidance when skill loading fails', async () => {
    fetchMock.mockResolvedValue(response(503, { error: { message: 'database unavailable' } }))

    await useSkillsStore.getState().loadSkills()

    expect(useSkillsStore.getState().error).toBe('Open Skills again to load the list.')
    expect(useSkillsStore.getState().error).not.toContain('service is temporarily unavailable')
    expect(useSkillsStore.getState().error).not.toContain('database unavailable')
  })

  test('stores a connection recovery step when skill loading cannot reach the server', async () => {
    fetchMock.mockRejectedValue(new TypeError('Failed to fetch'))

    await useSkillsStore.getState().loadSkills()

    expect(useSkillsStore.getState().error).toBe(
      'Check your connection, then open Skills again to load the list.'
    )
    expect(useSkillsStore.getState().error).not.toContain('Failed to fetch')
  })

  test('stores retry guidance when the saved-instructions response is not ok', async () => {
    fetchMock.mockResolvedValue(response(200, { ok: false, error: 'database parser detail' }))

    await useSkillsStore.getState().loadSkills()

    expect(useSkillsStore.getState().error).toBe('Open Skills again to load the list.')
    expect(useSkillsStore.getState().error).not.toContain('database parser detail')
  })

  test('throws beginner guidance when skill creation fails validation', async () => {
    fetchMock.mockResolvedValue(response(422, { message: 'trigger is invalid' }))

    await expect(
      useSkillsStore.getState().createSkill({
        name: 'review',
        trigger_pattern: '[',
        content: 'Review the task',
      })
    ).rejects.toThrow('Check the matching words, then save the skill again.')
  })

  test('throws access guidance when saved guidance create responses carry role details', async () => {
    fetchMock.mockResolvedValue(response(200, { ok: false, error: 'owner role required' }))

    await expect(
      useSkillsStore.getState().createSkill({
        name: 'release-check',
        content: 'Check release notes',
      })
    ).rejects.toThrow(
      'Ask an owner or admin to let you save reusable guidance for this team space, then save the skill again.'
    )
  })

  test('throws a connection recovery step when skill creation cannot reach the server', async () => {
    fetchMock.mockRejectedValue(new TypeError('Failed to fetch'))

    await expect(
      useSkillsStore.getState().createSkill({
        name: 'review',
        content: 'Review the task',
      })
    ).rejects.toThrow(
      'Check your connection, then save the skill again. Forge could not connect while saving it.'
    )
  })
})
