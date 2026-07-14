import { describe, expect, test } from 'vitest'
import { agentGroupErrorMessage } from '@app/features/agents/model/agentGroupErrorMessage'

describe('agentGroupErrorMessage', () => {
  test('turns permission failures into an owner or admin next step', () => {
    expect(agentGroupErrorMessage(new Error('HTTP 403: Forbidden'))).toBe(
      'Ask an owner or admin to let you set up the place for new tasks in this project. The place was not created.'
    )
  })

  test('turns structured permission failures into an owner or admin next step', () => {
    const message = agentGroupErrorMessage({
      statusCode: '403',
      detail: 'owner role required',
    })

    expect(message).toBe(
      'Ask an owner or admin to let you set up the place for new tasks in this project. The place was not created.'
    )
    expect(message).not.toContain('owner role required')
  })

  test('turns role-required failures into an owner or admin next step', () => {
    const message = agentGroupErrorMessage('owner role required')

    expect(message).toBe(
      'Ask an owner or admin to let you set up the place for new tasks in this project. The place was not created.'
    )
    expect(message).not.toContain('owner role required')
  })

  test('maps nested role-required failures into an owner or admin next step', () => {
    const message = agentGroupErrorMessage({
      error: { message: 'owner role required' },
    })

    expect(message).toBe(
      'Ask an owner or admin to let you set up the place for new tasks in this project. The place was not created.'
    )
    expect(message).not.toContain('owner role required')
  })

  test('explains naming conflicts without leaking raw API wording', () => {
    expect(agentGroupErrorMessage(new Error('API 409 lane conflict'))).toBe(
      'Use a different name, then create the place again. A place with this name may already exist.'
    )
  })

  test('explains structured naming conflicts without leaking raw API wording', () => {
    const message = agentGroupErrorMessage({
      code: '409',
      reason: 'lane conflict',
    })

    expect(message).toBe(
      'Use a different name, then create the place again. A place with this name may already exist.'
    )
    expect(message).not.toContain('lane conflict')
  })

  test('maps missing projects to a navigable Agents step', () => {
    const message = agentGroupErrorMessage({ status: 404 })

    expect(message).toBe(
      'Open Agents, choose the project again, then set up the place for new tasks. The place was not created because the selected project may have changed or been removed.'
    )
    expect(message).not.toContain('Refresh this page')
  })

  test('gives a connection recovery path for network failures', () => {
    const message = agentGroupErrorMessage(new TypeError('Failed to fetch'))

    expect(message).toBe(
      'Check your connection, then create the place again. Forge could not connect while setting up the place.'
    )
    expect(message).not.toContain('Failed to fetch')
  })

  test('gives a safe retry step for service failures', () => {
    const message = agentGroupErrorMessage(new Error('Server error 503: database unavailable'))

    expect(message).toBe(
      'Wait a few minutes, then set up the place for new tasks again. Forge could not create the place right now. If it still fails, ask an owner or admin to check places in this project.'
    )
    expect(message).not.toContain('Server error')
    expect(message).not.toContain('platform')
  })

  test('keeps unformatted service failures on the task queue recovery path', () => {
    const message = agentGroupErrorMessage(new Error('database unavailable while creating group'))

    expect(message).toBe(
      'Wait a few minutes, then set up the place for new tasks again. Forge could not create the place right now. If it still fails, ask an owner or admin to check places in this project.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('Create the task queue again')
  })

  test('gives a safe retry step for structured service failures', () => {
    const message = agentGroupErrorMessage({
      status: '503',
      message: 'database unavailable',
    })

    expect(message).toBe(
      'Wait a few minutes, then set up the place for new tasks again. Forge could not create the place right now. If it still fails, ask an owner or admin to check places in this project.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('503')
  })

  test('uses direct retry actions for sign-in, busy, and fallback cases', () => {
    expect(agentGroupErrorMessage(new Error('HTTP 401'))).toBe(
      'Sign in again, choose the project, and set up the place for new tasks again. The place was not created.'
    )
    expect(agentGroupErrorMessage(new Error('API 429'))).toBe(
      'Wait a minute, then create the place again. Too many place changes are happening right now.'
    )
    expect(agentGroupErrorMessage('unexpected lane parser detail')).toBe(
      'Create the place again. If it still fails, ask an owner or admin to check places in this project. The place was not created.'
    )
  })
})
