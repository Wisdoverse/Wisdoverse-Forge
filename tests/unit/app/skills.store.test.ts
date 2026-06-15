import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { skillHttpErrorMessage, useSkillsStore } from '@app/shared/model/skills.store'

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
      'Sign in again, then refresh Saved instructions.'
    )
  })

  test('turns create permission failures into an admin role step', () => {
    expectBeginnerMessage(
      skillHttpErrorMessage('create', 403),
      'Ask an owner or admin to let you create saved instructions. Your account cannot create workspace instructions yet.'
    )
  })

  test('turns validation details into a field-specific next step', () => {
    expectBeginnerMessage(
      skillHttpErrorMessage('create', 422, { error: { message: 'content is required' } }),
      'Enter the saved instructions, then try again.'
    )
  })
})

describe('useSkillsStore errors', () => {
  test('stores beginner guidance when skill loading fails', async () => {
    fetchMock.mockResolvedValue(response(503, { error: { message: 'database unavailable' } }))

    await useSkillsStore.getState().loadSkills()

    expect(useSkillsStore.getState().error).toBe(
      'Refresh Saved instructions to load the list. If it still fails, ask an owner or admin to check instruction setup.'
    )
    expect(useSkillsStore.getState().error).not.toContain('service is temporarily unavailable')
  })

  test('stores a connection recovery step when skill loading cannot reach the server', async () => {
    fetchMock.mockRejectedValue(new TypeError('Failed to fetch'))

    await useSkillsStore.getState().loadSkills()

    expect(useSkillsStore.getState().error).toBe(
      'Check your connection, then refresh Saved instructions to load the list.'
    )
    expect(useSkillsStore.getState().error).not.toContain('Failed to fetch')
  })

  test('stores retry guidance when the saved-instructions response is not ok', async () => {
    fetchMock.mockResolvedValue(response(200, { ok: false, error: 'database parser detail' }))

    await useSkillsStore.getState().loadSkills()

    expect(useSkillsStore.getState().error).toBe('Refresh Saved instructions to load the list.')
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
    ).rejects.toThrow('Check the matching words, then try again.')
  })

  test('throws a connection recovery step when skill creation cannot reach the server', async () => {
    fetchMock.mockRejectedValue(new TypeError('Failed to fetch'))

    await expect(
      useSkillsStore.getState().createSkill({
        name: 'review',
        content: 'Review the task',
      })
    ).rejects.toThrow(
      'Check your connection, then create the instruction again. Forge could not connect while creating it.'
    )
  })
})
