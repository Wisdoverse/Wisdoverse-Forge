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
  test('turns unauthorized catalog loads into a sign-in step', () => {
    expect(skillHttpErrorMessage('load', 401)).toBe('Sign in again, then load skills. Code: 401.')
  })

  test('turns create permission failures into an admin role step', () => {
    expect(skillHttpErrorMessage('create', 403)).toBe(
      'You do not have permission to create workspace skills. Ask an admin to update your role. Code: 403.'
    )
  })

  test('keeps validation details as support detail after the operator action', () => {
    expect(
      skillHttpErrorMessage('create', 422, { error: { message: 'content is required' } })
    ).toBe(
      'Check the skill name, trigger pattern, and content, then try again. Code: 422. Details: content is required'
    )
  })
})

describe('useSkillsStore errors', () => {
  test('stores beginner guidance when skill loading fails', async () => {
    fetchMock.mockResolvedValue(response(503, { error: { message: 'database unavailable' } }))

    await useSkillsStore.getState().loadSkills()

    expect(useSkillsStore.getState().error).toBe(
      'The skills service had a server problem. Try again after the backend is healthy. Code: 503. Details: database unavailable'
    )
  })

  test('stores a connection recovery step when skill loading cannot reach the server', async () => {
    fetchMock.mockRejectedValue(new TypeError('Failed to fetch'))

    await useSkillsStore.getState().loadSkills()

    expect(useSkillsStore.getState().error).toBe(
      'Skills could not load because the browser could not reach the server. Check your connection and refresh the page.'
    )
  })

  test('throws beginner guidance when skill creation fails validation', async () => {
    fetchMock.mockResolvedValue(response(422, { message: 'trigger is invalid' }))

    await expect(
      useSkillsStore.getState().createSkill({
        name: 'review',
        trigger_pattern: '[',
        content: 'Review the task',
      })
    ).rejects.toThrow(
      'Check the skill name, trigger pattern, and content, then try again. Code: 422. Details: trigger is invalid'
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
      'The skill could not be created because the browser could not reach the server. Check your connection and try again.'
    )
  })
})
